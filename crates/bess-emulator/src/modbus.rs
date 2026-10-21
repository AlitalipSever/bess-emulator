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

use bess_core::kernel::ControlCommand;

use crate::map::{
    HOLDING_COMMAND_ADDR, HOLDING_COS_PHI_ADDR, HOLDING_COS_PHI_UNDER_ADDR,
    HOLDING_LIMIT_ENABLE_ADDR, HOLDING_LIMIT_PCT_ADDR, HOLDING_LIMIT_REVERT_ADDR,
    HOLDING_MODE_ADDR, HOLDING_Q_MODE_ADDR, HOLDING_Q_SETPOINT_ADDR, HOLDING_SETPOINT_ADDR,
};
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

    /// One holding register written: the control surface's single-word
    /// registers (M4 phase 2), each a command into the task. A value the
    /// register does not accept is an illegal data value; a register that
    /// is not writable one word at a time, an illegal address.
    fn write_single(&self, addr: u16, word: u16) -> Result<(), ExceptionCode> {
        let flag = |w: u16| match w {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ExceptionCode::IllegalDataValue),
        };
        let limit =
            |pct: Option<f64>, enable: Option<bool>, revert_s: Option<f64>| Command::SetLimit {
                pct,
                enable,
                revert_s,
                reply: None,
            };
        let reactive =
            |mode: Option<u16>, cos_phi: Option<f64>, under: Option<bool>| Command::SetReactive {
                mode,
                q_var: None,
                cos_phi,
                under_excited: under,
                reply: None,
            };
        let cmd = match addr {
            HOLDING_MODE_ADDR => match word {
                0 => Command::SetSetpointW(None),
                1 => Command::SetSetpointW(Some(self.current_setpoint_w())),
                _ => return Err(ExceptionCode::IllegalDataValue),
            },
            HOLDING_LIMIT_PCT_ADDR => {
                if word > 1000 {
                    return Err(ExceptionCode::IllegalDataValue);
                }
                limit(Some(f64::from(word) / 10.0), None, None)
            }
            HOLDING_LIMIT_ENABLE_ADDR => limit(None, Some(flag(word)?), None),
            HOLDING_LIMIT_REVERT_ADDR => limit(None, None, Some(f64::from(word))),
            HOLDING_Q_MODE_ADDR => {
                if word > 2 {
                    return Err(ExceptionCode::IllegalDataValue);
                }
                reactive(Some(word), None, None)
            }
            HOLDING_COS_PHI_ADDR => {
                if word > 1000 {
                    return Err(ExceptionCode::IllegalDataValue);
                }
                reactive(None, Some(f64::from(word) / 1000.0), None)
            }
            HOLDING_COS_PHI_UNDER_ADDR => reactive(None, None, Some(flag(word)?)),
            HOLDING_COMMAND_ADDR => match ControlCommand::from_code(word) {
                Some(command) => Command::Control {
                    command,
                    reply: None,
                },
                None => return Err(ExceptionCode::IllegalDataValue),
            },
            _ => return Err(ExceptionCode::IllegalDataAddress),
        };
        self.send(cmd)
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
                if words.len() != 2 {
                    return Err(ExceptionCode::IllegalDataAddress);
                }
                let raw = (u32::from(words[0]) << 16) | u32::from(words[1]);
                let value = f64::from(raw as i32);
                match addr {
                    HOLDING_SETPOINT_ADDR => self.send(Command::SetSetpointW(Some(value)))?,
                    HOLDING_Q_SETPOINT_ADDR => self.send(Command::SetReactive {
                        mode: None,
                        q_var: Some(value * 1000.0),
                        cos_phi: None,
                        under_excited: None,
                        reply: None,
                    })?,
                    _ => return Err(ExceptionCode::IllegalDataAddress),
                }
                Ok(Response::WriteMultipleRegisters(addr, 2))
            }
            Request::WriteSingleRegister(addr, word) => {
                self.write_single(addr, word)?;
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
