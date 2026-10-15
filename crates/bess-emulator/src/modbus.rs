//! Modbus TCP slave: input registers carry telemetry, holding registers are
//! the control surface. Modbus has no authentication by nature; the default
//! bind is localhost and the README says so plainly. A scenario's data
//! faults reach the registers and the wire through `modbus/tap.rs`.

mod tap;

use std::future;
use std::net::SocketAddr;

use bess_scenario::Surface;
use tokio::net::TcpListener;
use tokio_modbus::server::tcp::Server;
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
                let snap = self.handle.snapshot.borrow().clone();
                let tick = snap.state.tick;
                let bank = if snap.taps.any_open(Surface::Modbus, tick) {
                    tap::input_bank(&self.handle.points, &snap, tick)
                } else {
                    snap.input_regs.clone()
                };
                Self::read_bank(&bank, addr, count).map(Response::ReadInputRegisters)
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
    // A session runs over a wire the tap can cut; while a dropout is open
    // a new connection is accepted and closed at once.
    let on_connected = |stream, socket_addr| {
        let service = service.clone();
        async move {
            if tap::offline(&service.handle.snapshot) {
                info!("modbus: connection from {socket_addr} closed: the device is in a dropout");
                return Ok(None);
            }
            let wire = tap::Wire::new(stream, service.handle.snapshot.clone());
            Ok(Some((service, wire)))
        }
    };
    let on_process_error = |err| warn!("modbus: connection error: {err}");
    if let Err(err) = server.serve(&on_connected, on_process_error).await {
        error!("modbus: server stopped: {err}");
    }
}

#[cfg(test)]
mod tests;
