//! Simple substation: constant-parameter transformer losses, POI
//! measurements and energy meters.

pub mod thermal;

use bess_core::state::{BreakerState, SubstationState};
use bess_core::traits::{GridConditions, GridInterface};

pub use thermal::{TapChanger, TransformerState, TransformerThermal};

/// M0 grid coupling: transformer losses as no-load plus load-proportional
/// (quadratic in loading) terms lumped over the MV and HV stages, and
/// monotonic import/export meters at the POI, with a third meter on the
/// station-service feed that carries the house load while the breaker is
/// open (M2). These are the SCADA meters; the fiscal quarter-hour meter
/// beside them is kernel state (`RevenueMeter`), fed the same POI power
/// after this step. M3 added the transformer's IEC 60076-7 temperatures
/// and the tap changer here (`thermal.rs`); the breaker's interlocks and
/// closing sequence live in the kernel (`kernel/switchgear.rs`).
///
/// The station auxiliary load used to live here as a 150 kW constant. Since
/// M1 it is an itemized inventory of its own (`aux.rs`); the substation
/// meters the total it is handed and no longer invents one.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleGrid {
    /// Site AC rating, W (base for the quadratic load-loss term).
    pub site_rated_w: f64,
    /// No-load (core) losses, W, present whenever the site is energized.
    pub no_load_loss_w: f64,
    /// Copper losses at rated power, W.
    pub load_loss_at_rated_w: f64,
    /// Nominal POI voltage, kV.
    pub poi_nominal_kv: f64,
    /// Nominal MV bus voltage, kV (M3 phase 4).
    pub mv_nominal_kv: f64,
    /// Resistive short-circuit voltage of the transformer, per unit: the
    /// load-dependent drop the MV bus sees at unity power factor, the
    /// copper losses over the rating (M3 phase 4).
    pub resistive_drop_pu: f64,
    /// Reactive short-circuit voltage of the transformer, per unit: the
    /// vars it consumes at rated current, over its rating (M4 phase 1
    /// PR2). 12 % for a 100 MVA HV/MV unit, a labeled estimate.
    pub reactance_pu: f64,
    /// The transformer's two temperatures (M3 phase 4).
    pub thermal: TransformerThermal,
    /// The on-load tap changer and its regulator (M3 phase 4).
    pub tap: TapChanger,
}

impl SimpleGrid {
    /// Defaults for a 100 MW / 110 kV connection: 100 kW core losses,
    /// 600 kW copper losses at rated.
    pub fn new(site_rated_w: f64) -> Self {
        Self {
            site_rated_w,
            no_load_loss_w: 100.0e3,
            load_loss_at_rated_w: 600.0e3,
            poi_nominal_kv: 110.0,
            mv_nominal_kv: 33.0,
            resistive_drop_pu: 600.0e3 / 100.0e6,
            reactance_pu: 0.12,
            thermal: TransformerThermal::onaf_100_mva(),
            tap: TapChanger::default(),
        }
    }
}

impl GridInterface for SimpleGrid {
    fn step(
        &self,
        substation: &mut SubstationState,
        p_ac_total_w: f64,
        q_ac_total_var: f64,
        aux_w: f64,
        grid: GridConditions,
        dt_s: f64,
    ) {
        substation.frequency_hz = grid.frequency_hz;
        substation.poi_voltage_kv = grid.voltage_pu * self.poi_nominal_kv;

        // The transformer's temperatures follow the through-power, or
        // cool toward the ambient when the breaker has de-energized it;
        // the tap changer regulates the MV bus the converters see.
        let energized = substation.hv_breaker == BreakerState::Closed;
        let loading = if energized {
            p_ac_total_w / self.site_rated_w
        } else {
            0.0
        };
        let mut t = TransformerState {
            top_oil_c: substation.top_oil_c,
            hot_spot_rise_1_k: substation.hot_spot_rise_1_k,
            hot_spot_rise_2_k: substation.hot_spot_rise_2_k,
        };
        self.thermal
            .step(&mut t, grid.ambient_c, loading.abs(), energized, dt_s);
        substation.top_oil_c = t.top_oil_c;
        substation.hot_spot_rise_1_k = t.hot_spot_rise_1_k;
        substation.hot_spot_rise_2_k = t.hot_spot_rise_2_k;
        substation.hot_spot_c = t.hot_spot_c();
        if energized {
            let mv_pu = grid.voltage_pu
                * self.tap.ratio(substation.tap_position)
                * (1.0 + self.resistive_drop_pu * loading);
            substation.mv_bus_voltage_kv = mv_pu * self.mv_nominal_kv;
            substation.tap_position = self.tap.regulate(
                substation.tap_position,
                &mut substation.tap_timer_s,
                mv_pu,
                dt_s,
            );
        } else {
            substation.mv_bus_voltage_kv = 0.0;
            substation.tap_timer_s = 0.0;
        }

        if substation.hv_breaker == BreakerState::Open {
            // No current crosses an open breaker: the POI reads zero, its
            // meters stand still, and the de-energized transformer loses
            // nothing. The house load carries on through the
            // station-service feed and is metered there. (The breaker state
            // machine with interlocks arrives in M3.)
            substation.poi_active_power_w = 0.0;
            substation.poi_reactive_power_var = 0.0;
            substation.poi_power_factor = 1.0;
            substation.transformer_loss_w = 0.0;
            substation.aux_power_w = aux_w;
            substation.station_service_wh += aux_w * dt_s / 3600.0;
            return;
        }

        let loading = p_ac_total_w / self.site_rated_w;
        let transformer_loss_w =
            self.no_load_loss_w + self.load_loss_at_rated_w * loading * loading;
        let poi_w = p_ac_total_w - transformer_loss_w - aux_w;

        // The transformer's reactive consumption, I²X on the apparent
        // loading: what the converters delivered less what the leakage
        // reactance took.
        let s_rated = self.site_rated_w;
        let apparent_sq = p_ac_total_w * p_ac_total_w + q_ac_total_var * q_ac_total_var;
        let reactive_loss_var = self.reactance_pu * apparent_sq / s_rated;
        let poi_var = q_ac_total_var - reactive_loss_var;
        let poi_s = (poi_w * poi_w + poi_var * poi_var).sqrt();

        substation.transformer_loss_w = transformer_loss_w;
        substation.aux_power_w = aux_w;
        substation.poi_active_power_w = poi_w;
        substation.poi_reactive_power_var = poi_var;
        substation.poi_power_factor = if poi_s > 0.0 {
            poi_w.abs() / poi_s
        } else {
            1.0
        };

        let wh = poi_w * dt_s / 3600.0;
        if wh >= 0.0 {
            substation.export_wh += wh;
        } else {
            substation.import_wh -= wh;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::state::RevenueMeter;

    fn substation() -> SubstationState {
        SubstationState {
            hv_breaker: BreakerState::Closed,
            disconnectors: bess_core::state::SwitchPosition::Closed,
            earthing_switch: bess_core::state::SwitchPosition::Open,
            sequence: bess_core::kernel::BreakerSequence::Idle,
            sequence_elapsed_s: 0.0,
            mv_bus_voltage_kv: 33.0,
            tap_position: 0,
            tap_timer_s: 0.0,
            top_oil_c: 20.0,
            hot_spot_c: 20.0,
            hot_spot_rise_1_k: 0.0,
            hot_spot_rise_2_k: 0.0,
            poi_active_power_w: 0.0,
            poi_reactive_power_var: 0.0,
            poi_power_factor: 1.0,
            poi_voltage_kv: 110.0,
            frequency_hz: 50.0,
            frequency_offset_mhz: 0,
            transformer_loss_w: 0.0,
            aux_power_w: 0.0,
            import_wh: 0.0,
            export_wh: 0.0,
            station_service_wh: 0.0,
            revenue: RevenueMeter::default(),
        }
    }

    #[test]
    fn idle_site_imports_house_load() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        grid.step(
            &mut sub,
            0.0,
            0.0,
            66.0e3,
            GridConditions {
                frequency_hz: 50.0,
                voltage_pu: 1.0,
                ambient_c: 20.0,
            },
            3600.0,
        );
        assert!(sub.poi_active_power_w < 0.0);
        assert!(sub.import_wh > 0.0);
        assert!(sub.export_wh.abs() < f64::EPSILON);
    }

    #[test]
    fn export_is_reduced_by_losses_and_aux() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        grid.step(
            &mut sub,
            100.0e6,
            0.0,
            450.0e3,
            GridConditions {
                frequency_hz: 50.0,
                voltage_pu: 1.0,
                ambient_c: 20.0,
            },
            1.0,
        );
        let expected = 100.0e6 - (100.0e3 + 600.0e3) - 450.0e3;
        assert!((sub.poi_active_power_w - expected).abs() < 1.0e-6);
        assert!(sub.export_wh > 0.0);
    }

    /// The substation reports the auxiliary load it was handed, unchanged.
    /// It has no inventory of its own to add since M1.
    #[test]
    fn the_metered_aux_is_the_aux_it_was_given() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        grid.step(
            &mut sub,
            0.0,
            0.0,
            123.4e3,
            GridConditions {
                frequency_hz: 50.0,
                voltage_pu: 1.0,
                ambient_c: 20.0,
            },
            1.0,
        );
        assert!((sub.aux_power_w - 123.4e3).abs() < 1.0e-9);
    }

    /// CALIBRATION.md's M1 entry reads the annual measurement partly through
    /// this number: the no-load loss runs whether or not the plant works, so
    /// over 8760 hours it is more than half the transformer's annual total,
    /// while on a single cycle it disappears. That paragraph is arithmetic on
    /// the value below, so the value is pinned here rather than left free to
    /// move and quietly make the paragraph wrong.
    #[test]
    fn the_transformer_no_load_loss_is_what_the_record_says() {
        let grid = SimpleGrid::new(100.0e6);
        assert!((grid.no_load_loss_w - 100.0e3).abs() < 1.0);
        let over_a_year_mwh = grid.no_load_loss_w * 8_760.0 / 1.0e6;
        assert!((over_a_year_mwh - 876.0).abs() < 1.0);
    }

    /// An open breaker: nothing at the POI, the POI meters frozen, the house
    /// load on the station-service meter. Closing it again stops that meter.
    #[test]
    fn an_open_breaker_moves_the_house_load_to_station_service() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        sub.hv_breaker = BreakerState::Open;
        grid.step(
            &mut sub,
            0.0,
            0.0,
            66.0e3,
            GridConditions {
                frequency_hz: 50.0,
                voltage_pu: 1.0,
                ambient_c: 20.0,
            },
            3600.0,
        );
        assert!(sub.poi_active_power_w.abs() < f64::EPSILON);
        assert!(sub.transformer_loss_w.abs() < f64::EPSILON);
        assert!(sub.import_wh.abs() < f64::EPSILON);
        assert!(sub.export_wh.abs() < f64::EPSILON);
        assert!((sub.aux_power_w - 66.0e3).abs() < 1.0e-9);
        assert!((sub.station_service_wh - 66.0e3).abs() < 1.0e-9);

        sub.hv_breaker = BreakerState::Closed;
        grid.step(
            &mut sub,
            0.0,
            0.0,
            66.0e3,
            GridConditions {
                frequency_hz: 50.0,
                voltage_pu: 1.0,
                ambient_c: 20.0,
            },
            3600.0,
        );
        assert!((sub.station_service_wh - 66.0e3).abs() < 1.0e-9);
        assert!(sub.import_wh > 0.0);
    }

    #[test]
    fn meters_never_decrease() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        let mut last_import = 0.0;
        let mut last_export = 0.0;
        for step in 0..100 {
            let p = if step % 2 == 0 { 50.0e6 } else { -50.0e6 };
            grid.step(
                &mut sub,
                p,
                0.0,
                0.0,
                GridConditions {
                    frequency_hz: 50.0,
                    voltage_pu: 1.0,
                    ambient_c: 20.0,
                },
                1.0,
            );
            assert!(sub.import_wh >= last_import);
            assert!(sub.export_wh >= last_export);
            last_import = sub.import_wh;
            last_export = sub.export_wh;
        }
    }
}
