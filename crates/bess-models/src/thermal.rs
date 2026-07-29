//! Lumped container thermal model with a thermostat HVAC.

use bess_core::state::ContainerState;
use bess_core::traits::ThermalModel;

/// One thermal node per container: battery heat and ambient leakage push on
/// the bulk air temperature, a hysteresis thermostat switches active cooling.
/// Rack cell temperatures follow container air plus each rack's fixed
/// airflow-position offset. Staged HVAC operation and realistic auxiliary
/// calibration arrive in M1.
#[derive(Debug, Clone, PartialEq)]
pub struct LumpedThermal {
    /// Thermal capacitance of the container contents, J/K.
    pub heat_capacity_j_per_k: f64,
    /// Envelope conductance to ambient, W/K.
    pub ua_w_per_k: f64,
    /// Air temperature at which cooling switches on, degrees Celsius.
    pub cool_on_c: f64,
    /// Air temperature at which cooling switches off, degrees Celsius.
    pub cool_off_c: f64,
    /// Thermal cooling capacity when running, W.
    pub cooling_thermal_w: f64,
    /// Coefficient of performance of the cooling unit.
    pub cop: f64,
    /// Fan and control power while cooling runs, W.
    pub fan_w: f64,
    /// Controls standby power while cooling is off, W.
    pub standby_w: f64,
}

impl Default for LumpedThermal {
    fn default() -> Self {
        Self {
            heat_capacity_j_per_k: 3.0e7,
            ua_w_per_k: 500.0,
            cool_on_c: 27.0,
            cool_off_c: 24.0,
            cooling_thermal_w: 40.0e3,
            cop: 3.0,
            fan_w: 1_500.0,
            standby_w: 200.0,
        }
    }
}

impl ThermalModel for LumpedThermal {
    fn step_container(
        &self,
        container: &mut ContainerState,
        heat_w: f64,
        ambient_c: f64,
        dt_s: f64,
    ) -> f64 {
        let t = container.air_temp_c;
        if t >= self.cool_on_c {
            container.hvac.cooling_on = true;
        } else if t <= self.cool_off_c {
            container.hvac.cooling_on = false;
        }
        let q_cool_w = if container.hvac.cooling_on {
            self.cooling_thermal_w
        } else {
            0.0
        };

        let leakage_w = self.ua_w_per_k * (ambient_c - t);
        container.air_temp_c =
            t + dt_s * (heat_w + leakage_w - q_cool_w) / self.heat_capacity_j_per_k;

        container.hvac.thermal_w = q_cool_w;
        container.hvac.electrical_w = if container.hvac.cooling_on {
            q_cool_w / self.cop + self.fan_w
        } else {
            self.standby_w
        };

        for rack in &mut container.racks {
            rack.cell_temp_c = container.air_temp_c + rack.temp_offset_c;
        }
        container.hvac.electrical_w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::state::{HvacState, RackState};

    fn container(air_temp_c: f64) -> ContainerState {
        ContainerState {
            air_temp_c,
            hvac: HvacState {
                cooling_on: false,
                electrical_w: 0.0,
                thermal_w: 0.0,
            },
            racks: vec![RackState {
                in_service: true,
                soc: 0.5,
                soh: 1.0,
                voltage_v: 1331.0,
                current_a: 0.0,
                cell_temp_c: 20.0,
                polarization_v: 0.0,
                resistance_scale: 1.0,
                temp_offset_c: 1.0,
                alarm_bits: 0,
            }],
        }
    }

    #[test]
    fn heating_without_cooling_raises_temperature() {
        let model = LumpedThermal::default();
        let mut c = container(22.0);
        let elec = model.step_container(&mut c, 20.0e3, 22.0, 60.0);
        assert!(c.air_temp_c > 22.0);
        assert!((elec - model.standby_w).abs() < f64::EPSILON);
    }

    #[test]
    fn thermostat_hysteresis_switches_cooling() {
        let model = LumpedThermal::default();
        let mut c = container(28.0);
        model.step_container(&mut c, 0.0, 20.0, 1.0);
        assert!(c.hvac.cooling_on);
        c.air_temp_c = 23.0;
        model.step_container(&mut c, 0.0, 20.0, 1.0);
        assert!(!c.hvac.cooling_on);
    }

    #[test]
    fn rack_temperature_follows_air_plus_offset() {
        let model = LumpedThermal::default();
        let mut c = container(25.0);
        model.step_container(&mut c, 0.0, 25.0, 1.0);
        let air = c.air_temp_c;
        assert!((c.racks[0].cell_temp_c - (air + 1.0)).abs() < 1.0e-12);
    }
}
