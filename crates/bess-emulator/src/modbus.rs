//! Modbus TCP slave: input registers carry telemetry, holding registers are
//! the control surface. Modbus has no authentication by nature; the default
//! bind is localhost and the README says so plainly.

use std::future;
use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio_modbus::server::tcp::{accept_tcp_connection, Server};
use tokio_modbus::{ExceptionCode, Request, Response};
use tracing::{error, info, warn};

use crate::map::{HOLDING_MODE_ADDR, HOLDING_SETPOINT_ADDR};
use crate::sim::{Command, SimHandle};

#[derive(Clone)]
struct PlantService {
    handle: SimHandle,
}

impl PlantService {
    fn read_bank(bank: &[u16], addr: u16, count: u16) -> Result<Vec<u16>, ExceptionCode> {
        let start = addr as usize;
        let end = start + count as usize;
        if count == 0 || count > 125 || end > bank.len() {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        Ok(bank[start..end].to_vec())
    }

    fn current_setpoint_w(&self) -> f64 {
        let snap = self.handle.snapshot.borrow();
        let hi = snap.holding_regs[HOLDING_SETPOINT_ADDR as usize];
        let lo = snap.holding_regs[HOLDING_SETPOINT_ADDR as usize + 1];
        f64::from(((u32::from(hi) << 16) | u32::from(lo)) as i32)
    }

    fn send(&self, cmd: Command) -> Result<(), ExceptionCode> {
        self.handle.commands.try_send(cmd).map_err(|err| {
            warn!("command channel full or closed: {err}");
            ExceptionCode::ServerDeviceBusy
        })
    }

    fn handle_request(&self, req: Request<'static>) -> Result<Response, ExceptionCode> {
        match req {
            Request::ReadInputRegisters(addr, count) => {
                let snap = self.handle.snapshot.borrow();
                Self::read_bank(&snap.input_regs, addr, count).map(Response::ReadInputRegisters)
            }
            Request::ReadHoldingRegisters(addr, count) => {
                let snap = self.handle.snapshot.borrow();
                Self::read_bank(&snap.holding_regs, addr, count).map(Response::ReadHoldingRegisters)
            }
            Request::WriteMultipleRegisters(addr, words) => {
                if addr == HOLDING_SETPOINT_ADDR && words.len() == 2 {
                    let raw = (u32::from(words[0]) << 16) | u32::from(words[1]);
                    let setpoint_w = f64::from(raw as i32);
                    self.send(Command::SetSetpointW(Some(setpoint_w)))?;
                    Ok(Response::WriteMultipleRegisters(addr, 2))
                } else {
                    Err(ExceptionCode::IllegalDataAddress)
                }
            }
            Request::WriteSingleRegister(addr, word) => {
                if addr != HOLDING_MODE_ADDR {
                    return Err(ExceptionCode::IllegalDataAddress);
                }
                match word {
                    0 => self.send(Command::SetSetpointW(None))?,
                    1 => self.send(Command::SetSetpointW(Some(self.current_setpoint_w())))?,
                    _ => return Err(ExceptionCode::IllegalDataValue),
                }
                Ok(Response::WriteSingleRegister(addr, word))
            }
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }
}

impl tokio_modbus::server::Service for PlantService {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = future::Ready<Result<Response, ExceptionCode>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        future::ready(self.handle_request(req))
    }
}

/// Run the Modbus TCP server until the task is aborted.
pub async fn serve(addr: SocketAddr, handle: SimHandle) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            error!("modbus: cannot bind {addr}: {err}");
            return;
        }
    };
    info!("modbus: listening on {addr}");
    let server = Server::new(listener);
    let service = PlantService { handle };
    let on_connected = |stream, socket_addr| {
        let service = service.clone();
        async move { accept_tcp_connection(stream, socket_addr, |_addr| Ok(Some(service.clone()))) }
    };
    let on_process_error = |err| warn!("modbus: connection error: {err}");
    if let Err(err) = server.serve(&on_connected, on_process_error).await {
        error!("modbus: server stopped: {err}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bess_core::alarms::has_bit;
    use bess_core::alarms::layout::{block, rack, site};
    use tokio_modbus::client::{tcp, Context, Reader, Writer};
    use tokio_modbus::slave::Slave;

    use super::serve;
    use crate::cli::Args;
    use crate::fixtures::{self, HOT_BLOCK};
    use crate::map::{HOLDING_MODE_ADDR, HOLDING_SETPOINT_ADDR};
    use crate::sim;

    fn test_args() -> Args {
        Args {
            seed: 42,
            start_unix: 1_767_225_600,
            speed: sim::MAX_SPEED,
            modbus: "127.0.0.1:15502".parse().unwrap(),
            http: "127.0.0.1:18080".parse().unwrap(),
            mqtt_host: "127.0.0.1".into(),
            mqtt_port: 1883,
            no_mqtt: true,
            dump_signal_map: None,
        }
    }

    /// End-to-end closed loop over real TCP: read telemetry, write a
    /// charge setpoint, watch the plant obey, hand control back.
    #[tokio::test(flavor = "multi_thread")]
    async fn modbus_read_write_closed_loop() {
        let args = test_args();
        let (handle, _sim_task) = sim::spawn(&args);
        tokio::spawn(serve(args.modbus, handle));
        tokio::time::sleep(Duration::from_millis(300)).await;

        let mut ctx = tcp::connect_slave(args.modbus, Slave(1)).await.unwrap();

        // Telemetry: frequency register near 50 Hz (x1000).
        let regs = ctx.read_input_registers(5, 1).await.unwrap().unwrap();
        assert!((49_900..=50_100).contains(&regs[0]), "freq reg {}", regs[0]);

        // Control: write -20 MW (charge) as an i32 pair, high word first.
        let setpoint: i32 = -20_000_000;
        let words = [
            (setpoint as u32 >> 16) as u16,
            (setpoint as u32 & 0xFFFF) as u16,
        ];
        ctx.write_multiple_registers(HOLDING_SETPOINT_ADDR, &words)
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        // The site setpoint register (input 18-19) now reflects the command.
        let sp = ctx.read_input_registers(18, 2).await.unwrap().unwrap();
        let echoed = ((u32::from(sp[0]) << 16) | u32::from(sp[1])) as i32;
        assert_eq!(echoed, setpoint);
        // And the holding bank mirrors external mode.
        let mode = ctx
            .read_holding_registers(HOLDING_MODE_ADDR, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mode[0], 1);

        // Hand control back to the internal plan.
        ctx.write_single_register(HOLDING_MODE_ADDR, 0)
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mode = ctx
            .read_holding_registers(HOLDING_MODE_ADDR, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mode[0], 0);

        // Out-of-map access is rejected, not silently served.
        let err = ctx.read_input_registers(60_000, 2).await.unwrap();
        assert!(err.is_err());
    }

    async fn read(ctx: &mut Context, addr: u16) -> u16 {
        ctx.read_input_registers(addr, 1).await.unwrap().unwrap()[0]
    }

    /// The phase's acceptance check on Modbus: a poller watching the chain
    /// plant sees the hot block's rack fold (base+6) go nonzero and the event
    /// counter advance, and the chain's three links appear on their own
    /// words in causal order and minutes apart: derate on the racks, the
    /// setpoint miss on the block, power limited on the site.
    ///
    /// The poller records when it first *saw* each bit, so it has to start
    /// before the first one raises (tick 471). At 600x the 300 ms start-up
    /// wait is about 180 ticks, a 5 ms poll about 3, and the links stay
    /// seconds apart in wall time even on a loaded machine. At full speed
    /// the first poll landed after the derate and a few dozen milliseconds
    /// before the miss, and load pushed them into the same poll.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_poller_sees_the_chain_on_the_alarm_registers() {
        let addr = "127.0.0.1:15503".parse().unwrap();
        let (handle, _sim_task) = sim::start(fixtures::hot_plant(), 600.0);
        tokio::spawn(serve(addr, handle));
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut ctx = tcp::connect_slave(addr, Slave(1)).await.unwrap();

        let rack_fold = 1000 + 10 * HOT_BLOCK as u16 + 6;
        let block_word = 2000 + 10 * HOT_BLOCK as u16;
        let first_counter = read(&mut ctx, 43).await;
        assert_ne!(read(&mut ctx, rack_fold).await, 0, "hot racks warn at once");
        assert!(
            has_bit(
                u32::from(read(&mut ctx, block_word).await),
                block::HVAC_FAILURE
            ),
            "the failed units show on the block word"
        );

        // Simulated tick at which each link first showed.
        let (mut derate, mut miss, mut limited) = (None, None, None);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while limited.is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "chain incomplete: derate {derate:?}, miss {miss:?}"
            );
            let t = ctx.read_input_registers(26, 2).await.unwrap().unwrap();
            let tick = (u32::from(t[0]) << 16) | u32::from(t[1]);
            let fold = u32::from(read(&mut ctx, rack_fold).await);
            let word = u32::from(read(&mut ctx, block_word).await);
            let site_word = u32::from(read(&mut ctx, 42).await);
            if derate.is_none() && has_bit(fold, rack::DERATE_ACTIVE) {
                derate = Some(tick);
            }
            if miss.is_none() && has_bit(word, block::SETPOINT_NOT_MET) {
                miss = Some(tick);
            }
            if has_bit(site_word, site::POWER_LIMITED) {
                limited = Some(tick);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let (derate, miss, limited) = (derate.unwrap(), miss.unwrap(), limited.unwrap());
        assert!(
            derate + 60 <= miss && miss + 60 <= limited,
            "order and spacing: derate {derate}, miss {miss}, limited {limited}"
        );
        assert_ne!(
            read(&mut ctx, 43).await.wrapping_sub(first_counter),
            0,
            "the event counter advanced"
        );
    }
}
