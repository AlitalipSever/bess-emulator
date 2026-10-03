//! The control surface: the holding registers a dispatch application writes.

use bess_core::state::{EmsMode, SiteState};

use super::{point, Class, Encoding, Point, Space};

/// Holding register: site external setpoint, W, i32 (write switches the EMS
/// to external mode).
pub const HOLDING_SETPOINT_ADDR: u16 = 0;
/// Holding register: EMS mode (0 = follow internal plan, 1 = external).
pub const HOLDING_MODE_ADDR: u16 = 2;

/// The writable points, read back from the state they command.
pub(super) fn points() -> Vec<Point> {
    use Class::Fast;
    use Encoding::{I32, U16};
    use Space::Holding;

    vec![
        point!(
            "control.site_setpoint_w",
            "W",
            Fast,
            I32,
            1.0,
            HOLDING_SETPOINT_ADDR,
            Holding,
            |s: &SiteState| s.ems.external_setpoint_w
        ),
        point!(
            "control.ems_mode",
            "enum",
            Fast,
            U16,
            1.0,
            HOLDING_MODE_ADDR,
            Holding,
            |s: &SiteState| f64::from(s.ems.mode == EmsMode::External)
        ),
    ]
}
