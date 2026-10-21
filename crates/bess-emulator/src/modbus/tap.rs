//! The Modbus tap: registers as a lying device serves them, and a wire
//! that drops (phase 5 design D1).
//!
//! A register bank has no timestamp, no NaN and no replay, so three of the
//! faults are refused at load; the four that remain are a freeze (the
//! selected registers hold what they read when it began), a unit error
//! (a quantity scaled by a power of ten, saturating at its encoding, the
//! pegged register being the audit trap), a chatter (the lowest bit of the
//! selected alarm words flaps) and a dropout. A dropout is the wire: while
//! it is open, every session resets on its next request and a new
//! connection is accepted and closed at once, which is what a device that
//! fell off the network looks like from a SCADA poller.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bess_scenario::Surface;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::watch;

use crate::map::{self, Point, Space};
use crate::sim::Snapshot;

/// The input bank as the device serves it at `tick`: the snapshot's bank
/// with every selected point re-encoded from what the tap says it reads,
/// and every flapped word's bit flipped.
pub fn input_bank(points: &[Point], snap: &Snapshot, tick: u64) -> Vec<u16> {
    let mut bank = snap.input_regs.clone();
    for (i, point) in points.iter().enumerate() {
        if point.space != Space::Input {
            continue;
        }
        let truth = snap.measured[i];
        let value = snap.taps.value(Surface::Modbus, i, tick, truth);
        if value.to_bits() != truth.to_bits() {
            map::write_point(point, value, &mut bank);
        }
        if snap.taps.flipped(Surface::Modbus, i, tick) {
            // The low word of the encoding carries bit 0.
            let low = point.addr as usize + point.encoding.words() as usize - 1;
            bank[low] ^= 1;
        }
    }
    bank
}

/// Whether the device is off the network at the latest snapshot.
pub fn offline(snapshot: &watch::Receiver<Arc<Snapshot>>) -> bool {
    let snap = snapshot.borrow();
    snap.taps.dropout(Surface::Modbus, snap.state.tick)
}

/// A session's TCP stream, which resets while a dropout is open.
pub struct Wire {
    inner: TcpStream,
    snapshot: watch::Receiver<Arc<Snapshot>>,
}

impl Wire {
    /// Wrap a session.
    pub fn new(inner: TcpStream, snapshot: watch::Receiver<Arc<Snapshot>>) -> Self {
        Self { inner, snapshot }
    }
}

/// What a poll answers while the wire is cut.
fn reset<T>() -> Poll<io::Result<T>> {
    Poll::Ready(Err(io::Error::from(io::ErrorKind::ConnectionReset)))
}

impl AsyncRead for Wire {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if offline(&self.snapshot) {
            return reset();
        }
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if offline(&self.snapshot) {
            return reset();
        }
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
