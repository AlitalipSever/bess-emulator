//! A written power limit, obeyed and flagged (M4 phase 2 PR1).
//!
//! A grid operator's curtailment or redispatch order arrives as a limit on
//! the exchange at the POI, a percentage of the rating with an enable and
//! an optional revert time, the shape SunSpec's immediate controls give it
//! (`WMaxLimPct`, `WMaxLimPct_Ena`, `WMaxLimPct_RvrtTms`). The kernel caps
//! the site target at the limit in both directions, from the tick after
//! the write, and reports `site.external_limitation` while the cap binds,
//! so an operator can tell a shortfall that is the order's from one that
//! is the plant's (`power_limited`). The converters' follower shapes the
//! step at the grid code's gradient, which sits inside the TSOs' 6 to
//! 20 %/min band for third-party changes.
//!
//! The one-shot commands a dispatch application sends through a command
//! register live here too, so Modbus, REST and a scenario share one
//! dispatcher and one numbering.

use serde::{Deserialize, Serialize};

use crate::alarms::ResetScope;

use super::{CommandError, Event, Simulation};

/// The external active-power limit as the operator left it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExternalLimit {
    /// The cap, percent of the site rating, 0 to 100.
    pub pct_of_rated: f64,
    /// Whether the cap is in force.
    pub enabled: bool,
    /// Seconds until the cap clears itself; zero means it stays until
    /// cleared.
    pub revert_s: f64,
}

impl ExternalLimit {
    /// No limit: the plant at its rating.
    pub fn none() -> Self {
        Self {
            pct_of_rated: 100.0,
            enabled: false,
            revert_s: 0.0,
        }
    }

    /// The cap in watts for a site rating, or the rating when disabled.
    pub fn cap_w(&self, rated_w: f64) -> f64 {
        if self.enabled {
            rated_w * (self.pct_of_rated / 100.0).clamp(0.0, 1.0)
        } else {
            rated_w
        }
    }

    /// Count the revert timer down one tick; the cap clears when it
    /// runs out. Returns whether it cleared this tick.
    pub fn tick(&mut self, dt_s: f64) -> bool {
        if !self.enabled || self.revert_s <= 0.0 {
            return false;
        }
        self.revert_s = (self.revert_s - dt_s).max(0.0);
        if self.revert_s == 0.0 {
            self.enabled = false;
            true
        } else {
            false
        }
    }
}

/// The one-shot commands of the command register, numbered as the
/// register carries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCommand {
    /// 1: the site reset.
    SiteReset,
    /// 2: open the HV breaker.
    BreakerOpen,
    /// 3: close the HV breaker, through its sequence.
    BreakerClose,
    /// 4: open the line disconnectors.
    DisconnectorsOpen,
    /// 5: close the line disconnectors.
    DisconnectorsClose,
    /// 6: open the earthing switch.
    EarthingOpen,
    /// 7: close the earthing switch.
    EarthingClose,
    /// 8: clear the external limit.
    ClearLimit,
}

impl ControlCommand {
    /// The register value.
    pub fn code(self) -> u16 {
        match self {
            Self::SiteReset => 1,
            Self::BreakerOpen => 2,
            Self::BreakerClose => 3,
            Self::DisconnectorsOpen => 4,
            Self::DisconnectorsClose => 5,
            Self::EarthingOpen => 6,
            Self::EarthingClose => 7,
            Self::ClearLimit => 8,
        }
    }

    /// The command a register value names, or `None` for a value the
    /// register does not know.
    pub fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            1 => Self::SiteReset,
            2 => Self::BreakerOpen,
            3 => Self::BreakerClose,
            4 => Self::DisconnectorsOpen,
            5 => Self::DisconnectorsClose,
            6 => Self::EarthingOpen,
            7 => Self::EarthingClose,
            8 => Self::ClearLimit,
            _ => return None,
        })
    }

    /// The name the surfaces publish.
    pub fn name(self) -> &'static str {
        match self {
            Self::SiteReset => "site_reset",
            Self::BreakerOpen => "breaker_open",
            Self::BreakerClose => "breaker_close",
            Self::DisconnectorsOpen => "disconnectors_open",
            Self::DisconnectorsClose => "disconnectors_close",
            Self::EarthingOpen => "earthing_open",
            Self::EarthingClose => "earthing_close",
            Self::ClearLimit => "clear_limit",
        }
    }
}

impl Simulation {
    /// Write the external limit: `pct_of_rated` of the rating in both
    /// directions, in force from the next tick, clearing itself after
    /// `revert_s` seconds when that is positive. A value outside 0 to 100
    /// is refused.
    pub fn set_external_limit(
        &mut self,
        pct_of_rated: f64,
        revert_s: f64,
    ) -> Result<(), CommandError> {
        if !(0.0..=100.0).contains(&pct_of_rated) || !revert_s.is_finite() || revert_s < 0.0 {
            return Err(CommandError::OutOfRange(
                "limit 0 to 100 % and a revert time of 0 s or more",
            ));
        }
        self.state.ems.external_limit = ExternalLimit {
            pct_of_rated,
            enabled: true,
            revert_s,
        };
        Ok(())
    }

    /// Clear the external limit. Returns whether one was in force.
    pub fn clear_external_limit(&mut self) -> bool {
        let was = self.state.ems.external_limit.enabled;
        self.state.ems.external_limit = ExternalLimit::none();
        was
    }

    /// Apply a command-register command between ticks. `Ok(true)` when
    /// the plant changed; a refusal leaves the plant unchanged and is
    /// recorded as a `command_refused` event on the next tick, so a
    /// Modbus client that cannot read an error body sees it on the
    /// event counter.
    pub fn control_command(&mut self, command: ControlCommand) -> Result<bool, CommandError> {
        let result = match command {
            ControlCommand::SiteReset => self
                .reset_alarms(ResetScope::Site)
                .map(|_| true)
                .map_err(|_| CommandError::OutOfRange("site reset")),
            ControlCommand::BreakerOpen => Ok(self.open_hv_breaker()),
            ControlCommand::BreakerClose => self.close_hv_breaker(),
            ControlCommand::DisconnectorsOpen => self.set_disconnectors(false),
            ControlCommand::DisconnectorsClose => self.set_disconnectors(true),
            ControlCommand::EarthingOpen => self.set_earthing_switch(false),
            ControlCommand::EarthingClose => self.set_earthing_switch(true),
            ControlCommand::ClearLimit => Ok(self.clear_external_limit()),
        };
        if result.is_err() {
            self.state.pending_events.push(Event::CommandRefused {
                command: command.code(),
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_follows_the_enable_and_the_percent() {
        let mut l = ExternalLimit::none();
        assert_eq!(l.cap_w(100.0e6), 100.0e6);
        l = ExternalLimit {
            pct_of_rated: 60.0,
            enabled: true,
            revert_s: 0.0,
        };
        assert!((l.cap_w(100.0e6) - 60.0e6).abs() < 1.0);
        assert!(!l.tick(1.0), "no revert time, no clearing");
        assert!(l.enabled);
        l.revert_s = 2.0;
        assert!(!l.tick(1.0));
        assert!(l.tick(1.0), "cleared on the second tick");
        assert!(!l.enabled);
        assert_eq!(l.cap_w(100.0e6), 100.0e6);
    }

    #[test]
    fn the_command_register_round_trips_its_codes() {
        for code in 1..=8u16 {
            let c = ControlCommand::from_code(code).expect("known");
            assert_eq!(c.code(), code);
            assert_ne!(c.name(), "");
        }
        assert!(ControlCommand::from_code(0).is_none());
        assert!(ControlCommand::from_code(9).is_none());
    }
}
