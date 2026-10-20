//! The Modbus projection: physical values into register banks.

use super::{Encoding, Point, Space};

/// Encode one physical value into registers at `point.addr`.
pub(crate) fn write_point(point: &Point, value: f64, bank: &mut [u16]) {
    let scaled = value * point.scale;
    let addr = point.addr as usize;
    match point.encoding {
        Encoding::U16 => {
            bank[addr] = scaled.round().clamp(0.0, f64::from(u16::MAX)) as u16;
        }
        Encoding::I16 => {
            let v = scaled
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            bank[addr] = v as u16;
        }
        Encoding::U32 => {
            let v = scaled.round().clamp(0.0, f64::from(u32::MAX)) as u32;
            bank[addr] = (v >> 16) as u16;
            bank[addr + 1] = (v & 0xFFFF) as u16;
        }
        Encoding::I32 => {
            let v = scaled
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32 as u32;
            bank[addr] = (v >> 16) as u16;
            bank[addr + 1] = (v & 0xFFFF) as u16;
        }
    }
}

/// Project the published values, one per point in map order, into the
/// Modbus register banks. The values arrive already measured (the
/// instrument inventory) so the banks carry what a device would.
pub fn write_banks(points: &[Point], values: &[f64], input: &mut [u16], holding: &mut [u16]) {
    for (point, &value) in points.iter().zip(values) {
        match point.space {
            Space::Input => write_point(point, value, input),
            Space::Holding => write_point(point, value, holding),
        }
    }
}
