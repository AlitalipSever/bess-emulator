//! The control surface: the holding registers a dispatch application writes.
//!
//! Since M4 phase 2 the bank carries what SunSpec's immediate controls
//! (model 123, DER model 704) carry where the semantics match, under this
//! map's names: a percent limit with an enable and a revert time
//! (`WMaxLimPct`, `WMaxLimPct_Ena`, `WMaxLimPct_RvrtTms`), a reactive mode
//! with its setpoints (`VArPct_Mod`, `VArWMaxPct`, `OutPFSet`), and a
//! command register for the one-shot commands REST carries. Every register
//! reads back from the state it commands.

use bess_core::kernel::ControlCommand;
use bess_core::state::{EmsMode, SiteState};

use super::{point, Class, Encoding, Point, Space};

/// Holding register: site external setpoint, W, i32 (write switches the EMS
/// to external mode).
pub const HOLDING_SETPOINT_ADDR: u16 = 0;
/// Holding register: EMS mode (0 = follow internal plan, 1 = external).
pub const HOLDING_MODE_ADDR: u16 = 2;
/// Holding register: external active-power limit, tenths of a percent of
/// the site rating (M4 phase 2).
pub const HOLDING_LIMIT_PCT_ADDR: u16 = 3;
/// Holding register: whether the limit is in force (0 or 1).
pub const HOLDING_LIMIT_ENABLE_ADDR: u16 = 4;
/// Holding register: seconds until the limit clears itself (0 keeps it).
pub const HOLDING_LIMIT_REVERT_ADDR: u16 = 5;
/// Holding register: reactive mode (0 fixed cos φ, 1 fixed Q, 2 Q(U)).
pub const HOLDING_Q_MODE_ADDR: u16 = 6;
/// Holding register pair: fixed reactive setpoint, kvar, i32, positive
/// over-excited.
pub const HOLDING_Q_SETPOINT_ADDR: u16 = 7;
/// Holding register: displacement factor, thousandths (900 to 1000).
pub const HOLDING_COS_PHI_ADDR: u16 = 9;
/// Holding register: 1 under-excited (absorbing vars), 0 over-excited.
pub const HOLDING_COS_PHI_UNDER_ADDR: u16 = 10;
/// Holding register: the command register, reads zero (M4 phase 2).
pub const HOLDING_COMMAND_ADDR: u16 = 11;

/// The writable points, read back from the state they command.
pub(super) fn points() -> Vec<Point> {
    let mut points = active_points();
    points.extend(reactive_points());
    points
}

/// The active-power registers: setpoint, mode, the limit (M4 phase 2).
fn active_points() -> Vec<Point> {
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
        point!(
            "control.p_limit_pct",
            "%",
            Fast,
            U16,
            10.0,
            HOLDING_LIMIT_PCT_ADDR,
            Holding,
            |s: &SiteState| s.ems.external_limit.pct_of_rated
        ),
        point!(
            "control.p_limit_enable",
            "bool",
            Fast,
            U16,
            1.0,
            HOLDING_LIMIT_ENABLE_ADDR,
            Holding,
            |s: &SiteState| f64::from(s.ems.external_limit.enabled)
        ),
        point!(
            "control.p_limit_revert_s",
            "s",
            Fast,
            U16,
            1.0,
            HOLDING_LIMIT_REVERT_ADDR,
            Holding,
            |s: &SiteState| s.ems.external_limit.revert_s.min(65_535.0)
        ),
    ]
}

/// The reactive registers and the command register (M4 phase 2).
fn reactive_points() -> Vec<Point> {
    use Class::Fast;
    use Encoding::{I32, U16};
    use Space::Holding;

    vec![
        point!(
            "control.q_mode",
            "enum",
            Fast,
            U16,
            1.0,
            HOLDING_Q_MODE_ADDR,
            Holding,
            |s: &SiteState| f64::from(s.ems.reactive_mode.code())
        ),
        point!(
            "control.q_setpoint_kvar",
            "kvar",
            Fast,
            I32,
            1.0,
            HOLDING_Q_SETPOINT_ADDR,
            Holding,
            |s: &SiteState| s.ems.reactive_q_var / 1000.0
        ),
        point!(
            "control.cos_phi_x1000",
            "ratio",
            Fast,
            U16,
            1000.0,
            HOLDING_COS_PHI_ADDR,
            Holding,
            |s: &SiteState| s.ems.reactive_cos_phi
        ),
        point!(
            "control.cos_phi_under_excited",
            "bool",
            Fast,
            U16,
            1.0,
            HOLDING_COS_PHI_UNDER_ADDR,
            Holding,
            |s: &SiteState| f64::from(s.ems.reactive_under_excited)
        ),
        point!(
            "control.command",
            "enum",
            Fast,
            U16,
            1.0,
            HOLDING_COMMAND_ADDR,
            Holding,
            |_: &SiteState| f64::from(
                ControlCommand::from_code(0).map_or(0u16, ControlCommand::code)
            )
        ),
    ]
}
