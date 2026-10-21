//! What the plant reports about its own tracking and availability, per
//! settlement quarter-hour (M4 phase 2 PR2).
//!
//! The German TSOs' operational requirements for batteries at the
//! extra-high-voltage grid bind the quarter-hour mean of feed-in or
//! withdrawal to the schedule within a frame the operator sets, and ask
//! for the state of charge and the full technical redispatch potential in
//! the data exchange. The plant publishes both sides of that: per closed
//! quarter-hour the scheduled mean (the site target as the controller
//! pursued it, integrated), the metered mean (the fiscal meter's period),
//! their difference in kilowatts and in percent of the rating, the
//! availability (the power the plant could deliver over its rating,
//! time-averaged, each direction), the tracking error's root mean square
//! over the period, and the energy it could still move each way. The
//! period closes on the fiscal meter's boundary, so the three agree on
//! what a quarter-hour is. A `schedule_deviation` bit raises when the
//! last period's deviation exceeds the frame.

use serde::{Deserialize, Serialize};

use crate::state::RevenueMeter;

/// The accumulators of the open period and the figures of the last
/// closed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KpiMeter {
    /// Seconds booked to the open period.
    pub open_s: f64,
    /// Scheduled energy of the open period, Wh, signed (export positive).
    pub open_scheduled_wh: f64,
    /// Metered energy of the open period, Wh, signed.
    pub open_metered_wh: f64,
    /// Sum of the squared tracking error over the open period, W²·s.
    pub open_error_sq_ws: f64,
    /// Time-weighted available discharge power over the open period, W·s.
    pub open_avail_discharge_ws: f64,
    /// Time-weighted available charge power over the open period, W·s.
    pub open_avail_charge_ws: f64,
    /// Last closed period: scheduled mean, W.
    pub last_scheduled_w: f64,
    /// Last closed period: metered mean, W.
    pub last_metered_w: f64,
    /// Last closed period: metered minus scheduled, W.
    pub last_deviation_w: f64,
    /// Last closed period: the deviation as a share of the rating.
    pub last_deviation_share: f64,
    /// Last closed period: tracking error RMS, W.
    pub last_tracking_rms_w: f64,
    /// Last closed period: availability to discharge, 0 to 1.
    pub last_availability_discharge: f64,
    /// Last closed period: availability to charge, 0 to 1.
    pub last_availability_charge: f64,
    /// Energy the plant could still deliver to the grid right now, Wh:
    /// what the racks hold above the window's floor, at the available
    /// discharge power or less. Updated every tick.
    pub redispatch_up_wh: f64,
    /// Energy the plant could still absorb, Wh, likewise.
    pub redispatch_down_wh: f64,
    /// Whether the last closed period's deviation exceeded the frame.
    pub over_frame: bool,
}

/// What one tick hands the meter.
#[derive(Debug, Clone, Copy)]
pub struct KpiTick {
    /// The site target the controller pursued this tick, W.
    pub scheduled_w: f64,
    /// The POI active power this tick, W.
    pub metered_w: f64,
    /// Available discharge power this tick, W.
    pub available_discharge_w: f64,
    /// Available charge power this tick, W.
    pub available_charge_w: f64,
    /// Energy above the window's floor, Wh.
    pub energy_above_floor_wh: f64,
    /// Energy below the window's ceiling, Wh.
    pub energy_below_ceiling_wh: f64,
}

impl KpiMeter {
    /// Book one tick of `dt_s` seconds ending at `now_s`, and close the
    /// period when the fiscal meter does. `rated_w` is the site rating
    /// and `frame_share` the deviation frame as a share of it.
    pub fn observe(
        &mut self,
        tick: KpiTick,
        rated_w: f64,
        frame_share: f64,
        dt_s: f64,
        now_s: i64,
    ) {
        let wh = dt_s / 3600.0;
        self.open_s += dt_s;
        self.open_scheduled_wh += tick.scheduled_w * wh;
        self.open_metered_wh += tick.metered_w * wh;
        let err = tick.metered_w - tick.scheduled_w;
        self.open_error_sq_ws += err * err * dt_s;
        self.open_avail_discharge_ws += tick.available_discharge_w * dt_s;
        self.open_avail_charge_ws += tick.available_charge_w * dt_s;
        self.redispatch_up_wh = tick.energy_above_floor_wh.max(0.0);
        self.redispatch_down_wh = tick.energy_below_ceiling_wh.max(0.0);
        if now_s.rem_euclid(RevenueMeter::PERIOD_S) == 0 && self.open_s > 0.0 {
            let hours = self.open_s / 3600.0;
            self.last_scheduled_w = self.open_scheduled_wh / hours;
            self.last_metered_w = self.open_metered_wh / hours;
            self.last_deviation_w = self.last_metered_w - self.last_scheduled_w;
            self.last_deviation_share = self.last_deviation_w / rated_w;
            self.last_tracking_rms_w = (self.open_error_sq_ws / self.open_s).sqrt();
            self.last_availability_discharge = self.open_avail_discharge_ws / self.open_s / rated_w;
            self.last_availability_charge = self.open_avail_charge_ws / self.open_s / rated_w;
            self.over_frame = self.last_deviation_share.abs() > frame_share;
            self.open_s = 0.0;
            self.open_scheduled_wh = 0.0;
            self.open_metered_wh = 0.0;
            self.open_error_sq_ws = 0.0;
            self.open_avail_discharge_ws = 0.0;
            self.open_avail_charge_ws = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START_S: i64 = 1_767_225_600;
    const RATED_W: f64 = 100.0e6;

    fn tick(scheduled_w: f64, metered_w: f64) -> KpiTick {
        KpiTick {
            scheduled_w,
            metered_w,
            available_discharge_w: 80.0e6,
            available_charge_w: 100.0e6,
            energy_above_floor_wh: 50.0e6,
            energy_below_ceiling_wh: 120.0e6,
        }
    }

    /// A quarter-hour scheduled at 100 MW and metered at 94 MW: the means,
    /// the 6 % deviation past a 5 % frame, the RMS of a constant error,
    /// and the availabilities as time averages.
    #[test]
    fn a_period_closes_with_its_means_and_the_frame_verdict() {
        let mut m = KpiMeter::default();
        for s in 1..=900 {
            m.observe(tick(100.0e6, 94.0e6), RATED_W, 0.05, 1.0, START_S + s);
            if s < 900 {
                assert_eq!(m.last_metered_w, 0.0, "closed early at {s}");
            }
        }
        assert!((m.last_scheduled_w - 100.0e6).abs() < 1.0);
        assert!((m.last_metered_w - 94.0e6).abs() < 1.0);
        assert!((m.last_deviation_w + 6.0e6).abs() < 1.0);
        assert!((m.last_deviation_share + 0.06).abs() < 1e-9);
        assert!((m.last_tracking_rms_w - 6.0e6).abs() < 1.0);
        assert!((m.last_availability_discharge - 0.8).abs() < 1e-9);
        assert!((m.last_availability_charge - 1.0).abs() < 1e-9);
        assert!(m.over_frame);
        assert_eq!(m.open_s, 0.0);
        assert_eq!(m.redispatch_up_wh, 50.0e6);
        assert_eq!(m.redispatch_down_wh, 120.0e6);
    }

    /// A ramp inside the period: the error's RMS exceeds the mean
    /// deviation's magnitude, and a period inside the frame clears it.
    #[test]
    fn a_ramp_shows_in_the_rms_and_a_good_period_clears_the_frame() {
        let mut m = KpiMeter::default();
        for s in 1..=900 {
            let metered = if s <= 450 { 90.0e6 } else { 110.0e6 };
            m.observe(tick(100.0e6, metered), RATED_W, 0.05, 1.0, START_S + s);
        }
        assert!(m.last_deviation_w.abs() < 1.0);
        assert!((m.last_tracking_rms_w - 10.0e6).abs() < 1.0);
        assert!(!m.over_frame);
    }
}
