//! What the plant owes the grid at its frequency (M4 phase 1): the
//! limited-frequency-sensitive modes of the connection codes and the FCR
//! droop a balancing product arms.
//!
//! Both act on the site target as a fast term the converters do not ramp:
//! the setpoint follower (`setpoint.rs`) shapes third-party commands at
//! the grid code's 10 %/min, while a frequency response has to be there
//! in seconds. The fast term is shared evenly over the dispatchable blocks
//! and clamped at each converter's permitted power, so neither mode can
//! push a block past what it may deliver.
//!
//! Rational arithmetic only.

use serde::{Deserialize, Serialize};

use crate::state::EmsState;

/// The connection code's frequency characteristics, per plant.
#[derive(Debug, Clone, PartialEq)]
pub struct GridCode {
    /// Nominal frequency, Hz.
    pub nominal_hz: f64,
    /// Above this the active power falls (LFSM-O), Hz.
    pub lfsm_o_hz: f64,
    /// Below this the active power rises (LFSM-U), Hz.
    pub lfsm_u_hz: f64,
    /// Share of rated power per hertz beyond the threshold.
    pub gradient_per_hz: f64,
    /// Above this the plant disconnects, Hz.
    pub disconnect_over_hz: f64,
    /// Below this the plant disconnects, Hz.
    pub disconnect_under_hz: f64,
}

impl Default for GridCode {
    /// VDE-AR-N 4110's figures: 50.2 Hz and 40 %/Hz up to the 51.5 Hz
    /// disconnection; the under-frequency mirror at 49.8 Hz with the same
    /// gradient (the project's reading, labeled in CALIBRATION.md) down
    /// to 47.5 Hz.
    fn default() -> Self {
        Self {
            nominal_hz: 50.0,
            lfsm_o_hz: 50.2,
            lfsm_u_hz: 49.8,
            gradient_per_hz: 0.40,
            disconnect_over_hz: 51.5,
            disconnect_under_hz: 47.5,
        }
    }
}

impl GridCode {
    /// The LFSM cut at `frequency_hz`, W, signed like active power: a
    /// negative number above the over-frequency threshold (deliver less,
    /// or absorb more), a positive one below the under-frequency
    /// threshold, zero inside the band. Bounded at the rating.
    pub fn lfsm_w(&self, frequency_hz: f64, rated_w: f64) -> f64 {
        let over = (frequency_hz - self.lfsm_o_hz).max(0.0);
        let under = (self.lfsm_u_hz - frequency_hz).max(0.0);
        ((under - over) * self.gradient_per_hz * rated_w).clamp(-rated_w, rated_w)
    }

    /// Whether `frequency_hz` is outside the band the plant stays
    /// connected in.
    pub fn disconnects(&self, frequency_hz: f64) -> bool {
        frequency_hz > self.disconnect_over_hz || frequency_hz < self.disconnect_under_hz
    }
}

/// The FCR droop block: the SOGL activation rules as the German and
/// Swiss prequalification conditions state them for Continental Europe.
#[derive(Debug, Clone, PartialEq)]
pub struct FcrDroop {
    /// Nominal frequency, Hz.
    pub nominal_hz: f64,
    /// Combined insensitivity and dead band, Hz: no delivery inside.
    pub deadband_hz: f64,
    /// Deviation at which the full prequalified power is delivered, Hz.
    pub full_hz: f64,
    /// Seconds to reach the full power on a full step: the output ramps
    /// at `prequalified / activation_s` per second at least.
    pub activation_s: f64,
    /// Over-fulfilment the SoC manager may ask for, as a factor on the
    /// characteristic (1.0 is none; the German rules allow up to 1.2).
    pub overfulfil: f64,
}

impl Default for FcrDroop {
    /// ±10 mHz dead band, ±200 mHz full activation, 30 s, no
    /// over-fulfilment (phase 3's SoC manager drives it).
    fn default() -> Self {
        Self {
            nominal_hz: 50.0,
            deadband_hz: 0.010,
            full_hz: 0.200,
            activation_s: 30.0,
            overfulfil: 1.0,
        }
    }
}

impl FcrDroop {
    /// The characteristic: the power the block asks for at `frequency_hz`
    /// for a prequalified `pq_w`, W, positive for a low frequency
    /// (deliver). Linear from the dead band's edge to the full deviation,
    /// held at the full power beyond it.
    pub fn target_w(&self, frequency_hz: f64, pq_w: f64) -> f64 {
        let df = self.nominal_hz - frequency_hz;
        let beyond = (df.abs() - self.deadband_hz).max(0.0);
        let span = self.full_hz - self.deadband_hz;
        let share = (beyond / span).min(1.0) * self.overfulfil;
        df.signum() * share * pq_w
    }

    /// Advance the block one tick: the output moves toward the
    /// characteristic's target at the activation rate. Leaves the output
    /// in `ems.fcr_output_w` and returns it. A disarmed block (no
    /// prequalified power) rests at zero.
    pub fn advance(&self, ems: &mut EmsState, frequency_hz: f64, dt_s: f64) -> f64 {
        let pq_w = ems.fcr_prequalified_w;
        if pq_w <= 0.0 {
            ems.fcr_output_w = 0.0;
            return 0.0;
        }
        let target = self.target_w(frequency_hz, pq_w);
        let step = pq_w / self.activation_s * dt_s;
        let y = ems.fcr_output_w;
        ems.fcr_output_w = target.clamp(y - step, y + step);
        ems.fcr_output_w
    }
}

/// What the grid-code block is doing this tick, for the published point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridCodeState {
    /// Inside every band, no droop armed or at rest.
    Normal,
    /// The over-frequency characteristic is cutting the target.
    LfsmOver,
    /// The under-frequency characteristic is raising it.
    LfsmUnder,
    /// The FCR droop is delivering.
    Fcr,
}

impl GridCodeState {
    /// The register value: 0 normal, 1 lfsm_o, 2 lfsm_u, 3 fcr.
    pub fn code(self) -> u16 {
        match self {
            Self::Normal => 0,
            Self::LfsmOver => 1,
            Self::LfsmUnder => 2,
            Self::Fcr => 3,
        }
    }

    /// The name the surfaces publish.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::LfsmOver => "lfsm_o",
            Self::LfsmUnder => "lfsm_u",
            Self::Fcr => "fcr",
        }
    }

    /// From the controller's state: an LFSM cut outranks the droop.
    pub fn of(ems: &EmsState) -> Self {
        if ems.lfsm_cut_w < 0.0 {
            Self::LfsmOver
        } else if ems.lfsm_cut_w > 0.0 {
            Self::LfsmUnder
        } else if ems.fcr_output_w != 0.0 {
            Self::Fcr
        } else {
            Self::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::EmsMode;

    const RATED_W: f64 = 100.0e6;

    fn ems(pq_w: f64) -> EmsState {
        EmsState {
            mode: EmsMode::FollowPlan,
            site_setpoint_w: 0.0,
            external_setpoint_w: 0.0,
            available_discharge_w: 0.0,
            available_charge_w: 0.0,
            blocks_released: 20,
            next_release_s: 0.0,
            fcr_prequalified_w: pq_w,
            fcr_output_w: 0.0,
            lfsm_cut_w: 0.0,
            reactive_mode: crate::state::ReactiveMode::unity(),
            q_site_target_var: 0.0,
            reactive_cos_phi: 1.0,
            reactive_under_excited: false,
            reactive_q_var: 0.0,
            external_limit: crate::kernel::ExternalLimit::none(),
            limit_binding: false,
            awards: crate::kernel::Awards::NONE,
            awards_pinned: false,
            reservoir: crate::kernel::Reservoir::default(),
        }
    }

    /// 40 %/Hz beyond 50.2 Hz: at 50.5 Hz the cut is 12 % of rated; the
    /// mirror below 49.8 Hz; nothing in between; bounded at the rating.
    #[test]
    fn lfsm_is_the_standards_gradient() {
        let gc = GridCode::default();
        assert!((gc.lfsm_w(50.5, RATED_W) + 0.12 * RATED_W).abs() < 1.0);
        assert!((gc.lfsm_w(49.5, RATED_W) - 0.12 * RATED_W).abs() < 1.0);
        assert_eq!(gc.lfsm_w(50.19, RATED_W), 0.0);
        assert_eq!(gc.lfsm_w(49.81, RATED_W), 0.0);
        assert_eq!(gc.lfsm_w(55.0, RATED_W), -RATED_W);
        assert!(gc.disconnects(51.51) && gc.disconnects(47.49));
        assert!(!gc.disconnects(51.5) && !gc.disconnects(47.5));
    }

    /// The SOGL shape: on a −200 mHz step the output is at least 50 % of
    /// the prequalified power at 15 s and 100 % at 30 s, linear; a
    /// −100 mHz step is half; inside ±10 mHz nothing; disarmed nothing.
    #[test]
    fn the_droop_meets_the_activation_shape() {
        let droop = FcrDroop::default();
        let pq = 20.0e6;
        let mut e = ems(pq);
        let mut at = Vec::new();
        for _ in 0..30 {
            at.push(droop.advance(&mut e, 49.8, 1.0));
        }
        assert!(at[14] >= 0.5 * pq - 1.0, "at 15 s: {}", at[14]);
        assert!((at[29] - pq).abs() < 1.0, "at 30 s: {}", at[29]);
        for w in at.windows(2) {
            assert!((w[1] - w[0] - pq / 30.0).abs() < 1.0, "not linear");
        }
        let half = droop.target_w(49.9, pq);
        assert!((half - pq * (0.1 - 0.01) / 0.19).abs() < 1.0);
        assert_eq!(droop.target_w(50.009, pq), 0.0);
        assert_eq!(droop.target_w(49.991, pq), 0.0);
        assert!((droop.target_w(50.3, pq) + pq).abs() < 1.0);
        let mut off = ems(0.0);
        off.fcr_output_w = 5.0e6;
        assert_eq!(droop.advance(&mut off, 49.0, 1.0), 0.0);
        let mut over = ems(pq);
        let droop_over = FcrDroop {
            overfulfil: 1.2,
            ..FcrDroop::default()
        };
        for _ in 0..60 {
            droop_over.advance(&mut over, 49.7, 1.0);
        }
        assert!((over.fcr_output_w - 1.2 * pq).abs() < 1.0);
    }

    #[test]
    fn the_state_names_what_is_acting() {
        let mut e = ems(1.0);
        assert_eq!(GridCodeState::of(&e), GridCodeState::Normal);
        e.fcr_output_w = 1.0;
        assert_eq!(GridCodeState::of(&e), GridCodeState::Fcr);
        e.lfsm_cut_w = -1.0;
        assert_eq!(GridCodeState::of(&e), GridCodeState::LfsmOver);
        e.lfsm_cut_w = 1.0;
        assert_eq!(GridCodeState::of(&e).code(), 2);
        assert_eq!(GridCodeState::LfsmUnder.name(), "lfsm_u");
    }
}
