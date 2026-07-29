//! Simple substation: constant-parameter transformer losses, station
//! auxiliaries, POI measurements and energy meters.

use bess_core::state::{BreakerState, SubstationState};
use bess_core::traits::GridInterface;

/// M0 grid coupling: transformer losses as no-load plus load-proportional
/// (quadratic in loading) terms lumped over the MV and HV stages, a constant
/// station auxiliary load, and monotonic import/export meters at the POI.
/// Breaker interlocks, OLTC behavior, transformer thermals, and the revenue
/// meter series arrive in M3.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleGrid {
    /// Site AC rating, W (base for the quadratic load-loss term).
    pub site_rated_w: f64,
    /// No-load (core) losses, W, present whenever the site is energized.
    pub no_load_loss_w: f64,
    /// Copper losses at rated power, W.
    pub load_loss_at_rated_w: f64,
    /// Constant station auxiliary load (controls, protection, lighting), W.
    pub station_aux_w: f64,
    /// Nominal POI voltage, kV.
    pub poi_nominal_kv: f64,
}

impl SimpleGrid {
    /// Defaults for a 100 MW / 110 kV connection: 100 kW core losses,
    /// 600 kW copper losses at rated, 150 kW station auxiliaries.
    pub fn new(site_rated_w: f64) -> Self {
        Self {
            site_rated_w,
            no_load_loss_w: 100.0e3,
            load_loss_at_rated_w: 600.0e3,
            station_aux_w: 150.0e3,
            poi_nominal_kv: 110.0,
        }
    }
}

impl GridInterface for SimpleGrid {
    fn step(
        &self,
        substation: &mut SubstationState,
        p_ac_total_w: f64,
        hvac_aux_w: f64,
        frequency_hz: f64,
        dt_s: f64,
    ) {
        substation.frequency_hz = frequency_hz;
        substation.poi_voltage_kv = self.poi_nominal_kv;
        substation.poi_reactive_power_var = 0.0;

        if substation.hv_breaker == BreakerState::Open {
            // Site dark: no POI flow, no metering. (M0 never opens the
            // breaker; the state machine with interlocks arrives in M3.)
            substation.poi_active_power_w = 0.0;
            substation.transformer_loss_w = 0.0;
            substation.aux_power_w = 0.0;
            return;
        }

        let loading = p_ac_total_w / self.site_rated_w;
        let transformer_loss_w =
            self.no_load_loss_w + self.load_loss_at_rated_w * loading * loading;
        let aux_w = hvac_aux_w + self.station_aux_w;
        let poi_w = p_ac_total_w - transformer_loss_w - aux_w;

        substation.transformer_loss_w = transformer_loss_w;
        substation.aux_power_w = aux_w;
        substation.poi_active_power_w = poi_w;

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

    fn substation() -> SubstationState {
        SubstationState {
            hv_breaker: BreakerState::Closed,
            poi_active_power_w: 0.0,
            poi_reactive_power_var: 0.0,
            poi_voltage_kv: 110.0,
            frequency_hz: 50.0,
            transformer_loss_w: 0.0,
            aux_power_w: 0.0,
            import_wh: 0.0,
            export_wh: 0.0,
        }
    }

    #[test]
    fn idle_site_imports_house_load() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        grid.step(&mut sub, 0.0, 8.0e3, 50.0, 3600.0);
        assert!(sub.poi_active_power_w < 0.0);
        assert!(sub.import_wh > 0.0);
        assert!(sub.export_wh.abs() < f64::EPSILON);
    }

    #[test]
    fn export_is_reduced_by_losses_and_aux() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        grid.step(&mut sub, 100.0e6, 300.0e3, 50.0, 1.0);
        let expected = 100.0e6 - (100.0e3 + 600.0e3) - (300.0e3 + 150.0e3);
        assert!((sub.poi_active_power_w - expected).abs() < 1.0e-6);
        assert!(sub.export_wh > 0.0);
    }

    #[test]
    fn meters_never_decrease() {
        let grid = SimpleGrid::new(100.0e6);
        let mut sub = substation();
        let mut last_import = 0.0;
        let mut last_export = 0.0;
        for step in 0..100 {
            let p = if step % 2 == 0 { 50.0e6 } else { -50.0e6 };
            grid.step(&mut sub, p, 0.0, 50.0, 1.0);
            assert!(sub.import_wh >= last_import);
            assert!(sub.export_wh >= last_export);
            last_import = sub.import_wh;
            last_export = sub.export_wh;
        }
    }
}
