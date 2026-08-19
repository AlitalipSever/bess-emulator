//! Two-node container thermal model with a thermostat HVAC.

use bess_core::kernel::Weather;
use bess_core::state::ContainerState;
use bess_core::traits::{ThermalFlows, ThermalModel};

/// Two thermal nodes per container: the rack cell mass and the bulk air.
///
/// Battery heat enters the cells, flows from the cells into the air across a
/// fixed conductance, and leaves the air through the envelope or the HVAC
/// unit. Splitting the nodes is what gives cell temperature memory: it lags
/// air by tens of minutes instead of tracking it instantly, which is the
/// precondition for temperature derating in M2.
///
/// Solar gain is modeled as sol-air: instead of a separate radiation path,
/// irradiance raises the temperature the envelope sees (see
/// [`Self::sol_air_coefficient_m2_k_per_w`]). PCS heat is deliberately
/// absent: utility-scale PCS skids sit outside the battery container, so
/// their losses never reach this air node.
///
/// Staged HVAC operation and datasheet-calibrated capacities arrive in M1's
/// HVAC step; the single-stage thermostat here is still the M0 placeholder.
///
/// **Numerics.** The step is explicit Euler on two coupled nodes, stable
/// only while `dt_s` stays well below `2C/k` for each of them. At GW-01's
/// numbers that limit is about 500 s for the air node
/// (2.8e6 / (12 x 900 + 500)) and about 5200 s for a rack, so the kernel's
/// 1 s tick has three orders of margin. Past the limit temperatures
/// oscillate and then diverge while the energy balance keeps closing, since
/// a conservative scheme that is unstable still conserves.
///
/// **Provenance.** Every parameter here is an engineering estimate except
/// the sol-air coefficient, which comes from a standard reference. Each
/// field says which it is, and CALIBRATION.md's M1 section carries the same
/// list with the reasoning; PR7 pins them against measured sources. None of
/// these numbers is a measurement, and none of them pretends to be.
#[derive(Debug, Clone, PartialEq)]
pub struct LumpedThermal {
    /// Thermal capacitance of the container air node, J/K. Estimate:
    /// enclosure steel and rack frames dominate, roughly 6 t at the
    /// ~470 J/(kg K) of structural steel, plus a negligible 40 kJ/K of air.
    /// The cells are no longer part of this node; they are their own.
    pub air_heat_capacity_j_per_k: f64,
    /// Thermal capacitance of one rack's cells, J/K. Estimate, derived from
    /// what the site descriptor already fixes: a GW-01 rack is 418 kWh
    /// (416 cells x 314 Ah x 3.2 V), and the 314 Ah class sits near
    /// 180 Wh/kg at cell level, so a rack carries roughly 2330 kg of cells.
    /// At the ~1000 J/(kg K) reported for LFP cells that is 2.33e6 J/K. Both
    /// the energy density and the specific heat are the estimates; the cell
    /// count and rack energy are not.
    pub rack_heat_capacity_j_per_k: f64,
    /// Conductance from one rack's cells to the container air, W/K.
    /// Estimate, and a dependent one: it is sized so a rack sits about 9 K
    /// above the air it breathes at the ~8 kW it dissipates at full site
    /// power, and that 8 kW comes from the M0 equivalent-circuit
    /// resistances, which `cell.rs` describes as tuned rather than sourced
    /// and which M1 refines. When those move, this moves with them or the
    /// 9 K spread quietly becomes a different number. Tracked in
    /// CALIBRATION.md.
    pub rack_to_air_w_per_k: f64,
    /// Envelope conductance to ambient, W/K. Estimate, inherited from M0.
    pub ua_w_per_k: f64,
    /// Sol-air coefficient of the envelope, m2 K/W: solar absorptance over
    /// the outside surface film coefficient, `alpha / h_o`. Irradiance times
    /// this is how much hotter than ambient the envelope behaves, so the
    /// heat it lets in is `ua_w_per_k * sol_air_coefficient * irradiance`.
    ///
    /// 0.026 is the ASHRAE Handbook of Fundamentals value for a
    /// light-colored surface (0.052 for dark), which is what a white battery
    /// container is. Two documented simplifications: the long-wave sky
    /// correction is left out, so this is the optimistic end, and global
    /// horizontal irradiance is applied to the whole envelope rather than
    /// per surface with its own incidence.
    pub sol_air_coefficient_m2_k_per_w: f64,
    /// Air temperature at which cooling switches on, degrees Celsius.
    /// Estimate, inherited from M0.
    pub cool_on_c: f64,
    /// Air temperature at which cooling switches off, degrees Celsius.
    /// Estimate, inherited from M0.
    pub cool_off_c: f64,
    /// Thermal cooling capacity when running, W. M0 placeholder, and known
    /// to be undersized for this plant: see CALIBRATION.md.
    pub cooling_thermal_w: f64,
    /// Coefficient of performance of the cooling unit. M0 placeholder.
    pub cop: f64,
    /// Fan and control power while cooling runs, W. M0 placeholder.
    pub fan_w: f64,
    /// Controls standby power while cooling is off, W. M0 placeholder.
    pub standby_w: f64,
}

impl Default for LumpedThermal {
    fn default() -> Self {
        Self {
            air_heat_capacity_j_per_k: 2.8e6,
            rack_heat_capacity_j_per_k: 2.33e6,
            rack_to_air_w_per_k: 900.0,
            ua_w_per_k: 500.0,
            sol_air_coefficient_m2_k_per_w: 0.026,
            cool_on_c: 27.0,
            cool_off_c: 24.0,
            cooling_thermal_w: 40.0e3,
            cop: 3.0,
            fan_w: 1_500.0,
            standby_w: 200.0,
        }
    }
}

impl LumpedThermal {
    /// Effective solar aperture of the envelope, m2: the area that, times
    /// irradiance, gives the solar heat let in. Derived, never stored, so it
    /// cannot drift out of step with the envelope conductance it depends on.
    pub fn solar_aperture_m2(&self) -> f64 {
        self.ua_w_per_k * self.sol_air_coefficient_m2_k_per_w
    }
}

impl ThermalModel for LumpedThermal {
    /// All flows are evaluated at the temperatures the tick started with and
    /// the two nodes are updated afterwards, so the discrete step conserves
    /// energy exactly: the internal energy the nodes gained equals the net
    /// heat that entered, to the last bit.
    ///
    /// # Panics
    /// Panics unless `rack_heat_w` carries exactly one value per rack. The
    /// alternative, iterating over the shorter of the two, would leave the
    /// tail racks frozen and silently break the energy balance in release
    /// builds.
    fn step_container(
        &self,
        container: &mut ContainerState,
        rack_heat_w: &[f64],
        weather: Weather,
        dt_s: f64,
    ) -> ThermalFlows {
        assert_eq!(
            rack_heat_w.len(),
            container.racks.len(),
            "thermal step needs one heat value per rack"
        );
        let air_c = container.air_temp_c;

        if air_c >= self.cool_on_c {
            container.hvac.cooling_on = true;
        } else if air_c <= self.cool_off_c {
            container.hvac.cooling_on = false;
        }
        let hvac_thermal_w = if container.hvac.cooling_on {
            self.cooling_thermal_w
        } else {
            0.0
        };

        // Cells: their own heat in, conduction to the air they breathe out.
        // Each rack sees the bulk air plus its fixed airflow-position offset.
        let mut cells_to_air_w = 0.0;
        for (rack, &heat_w) in container.racks.iter_mut().zip(rack_heat_w) {
            let local_air_c = air_c + rack.temp_offset_c;
            let to_air_w = self.rack_to_air_w_per_k * (rack.cell_temp_c - local_air_c);
            rack.cell_temp_c += dt_s * (heat_w - to_air_w) / self.rack_heat_capacity_j_per_k;
            cells_to_air_w += to_air_w;
        }

        let envelope_gain_w = self.ua_w_per_k * (weather.ambient_c - air_c)
            + self.solar_aperture_m2() * weather.irradiance_wm2;
        container.air_temp_c = air_c
            + dt_s * (cells_to_air_w + envelope_gain_w - hvac_thermal_w)
                / self.air_heat_capacity_j_per_k;

        container.hvac.thermal_w = hvac_thermal_w;
        container.hvac.electrical_w = if container.hvac.cooling_on {
            hvac_thermal_w / self.cop + self.fan_w
        } else {
            self.standby_w
        };

        ThermalFlows {
            cells_to_air_w,
            envelope_gain_w,
            hvac_thermal_w,
            hvac_electrical_w: container.hvac.electrical_w,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::state::{HvacState, RackState};

    fn weather(ambient_c: f64, irradiance_wm2: f64) -> Weather {
        Weather {
            ambient_c,
            irradiance_wm2,
        }
    }

    fn container(air_temp_c: f64, racks: usize) -> ContainerState {
        ContainerState {
            air_temp_c,
            hvac: HvacState {
                cooling_on: false,
                electrical_w: 0.0,
                thermal_w: 0.0,
            },
            racks: (0..racks)
                .map(|i| RackState {
                    in_service: true,
                    soc: 0.5,
                    soh: 1.0,
                    voltage_v: 1331.0,
                    current_a: 0.0,
                    cell_temp_c: air_temp_c,
                    polarization_v: 0.0,
                    resistance_scale: 1.0,
                    // A spread of positions in the airflow, as the plant
                    // draws them: -1 K, 0 K, +1 K, ...
                    temp_offset_c: i as f64 - 1.0,
                    alarm_bits: 0,
                })
                .collect(),
        }
    }

    /// Internal energy of both node types, J above 0 C.
    fn stored_j(model: &LumpedThermal, c: &ContainerState) -> f64 {
        let cells: f64 = c
            .racks
            .iter()
            .map(|r| r.cell_temp_c * model.rack_heat_capacity_j_per_k)
            .sum();
        cells + c.air_temp_c * model.air_heat_capacity_j_per_k
    }

    #[test]
    fn heating_without_cooling_raises_temperature() {
        let model = LumpedThermal::default();
        let mut c = container(22.0, 1);
        let flows = model.step_container(&mut c, &[20.0e3], weather(22.0, 0.0), 60.0);
        assert!(c.racks[0].cell_temp_c > 22.0, "the cells take the heat");
        assert!(c.air_temp_c > 22.0, "and pass it to the air");
        assert!((flows.hvac_electrical_w - model.standby_w).abs() < f64::EPSILON);
    }

    #[test]
    fn thermostat_hysteresis_switches_cooling() {
        let model = LumpedThermal::default();
        let mut c = container(28.0, 1);
        model.step_container(&mut c, &[0.0], weather(20.0, 0.0), 1.0);
        assert!(c.hvac.cooling_on);
        c.air_temp_c = 23.0;
        model.step_container(&mut c, &[0.0], weather(20.0, 0.0), 1.0);
        assert!(!c.hvac.cooling_on);
    }

    #[test]
    #[should_panic(expected = "one heat value per rack")]
    fn a_short_heat_slice_is_refused() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 4);
        model.step_container(&mut c, &[1.0e3; 3], weather(20.0, 0.0), 1.0);
    }

    /// The whole point of the second node: a rack that starts dissipating
    /// warms up long before the air it sits in does.
    #[test]
    fn cell_temperature_lags_the_air() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 12);
        let heat = vec![8.0e3; 12];
        for _ in 0..600 {
            model.step_container(&mut c, &heat, weather(20.0, 0.0), 1.0);
        }
        let cell = c.racks[1].cell_temp_c; // the rack with a zero offset
        assert!(cell > c.air_temp_c, "cells lead the air they heat");
        // Ten minutes in, a rack is still far from its steady-state rise of
        // roughly 8000 / 900 = 8.9 K over the air.
        let rise = cell - c.air_temp_c;
        assert!((1.0..8.0).contains(&rise), "rack to air spread {rise} K");
    }

    #[test]
    fn cell_temperature_settles_at_the_conduction_spread() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        // Hold the air still by cancelling every flow into it: only the
        // cell node is under test here.
        for _ in 0..20_000 {
            model.step_container(&mut c, &[4.5e3], weather(20.0, 0.0), 1.0);
            c.air_temp_c = 20.0;
        }
        let expected = 20.0 - 1.0 + 4.5e3 / model.rack_to_air_w_per_k;
        assert!(
            (c.racks[0].cell_temp_c - expected).abs() < 0.05,
            "settled at {} C, expected {expected} C",
            c.racks[0].cell_temp_c
        );
    }

    #[test]
    fn the_solar_aperture_follows_the_envelope() {
        let mut model = LumpedThermal::default();
        assert!((model.solar_aperture_m2() - 13.0).abs() < 1.0e-9);
        // A better insulated container lets less sun in, without anyone
        // having to remember to re-derive a second constant.
        model.ua_w_per_k = 250.0;
        assert!((model.solar_aperture_m2() - 6.5).abs() < 1.0e-9);
    }

    #[test]
    fn sunshine_warms_the_container() {
        let model = LumpedThermal::default();
        let mut dark = container(20.0, 4);
        let mut sunlit = container(20.0, 4);
        // Flat airflow offsets, so the only thing moving the air is the sun.
        for c in [&mut dark, &mut sunlit] {
            for rack in &mut c.racks {
                rack.temp_offset_c = 0.0;
            }
        }
        let heat = [0.0; 4];
        for _ in 0..3600 {
            model.step_container(&mut dark, &heat, weather(20.0, 0.0), 1.0);
            model.step_container(&mut sunlit, &heat, weather(20.0, 900.0), 1.0);
        }
        let gain_k = sunlit.air_temp_c - dark.air_temp_c;
        // 13 m2 x 900 W/m2 = 11.7 kW against the envelope conductance,
        // damped by an hour of the air node's time constant.
        assert!((3.0..12.0).contains(&gain_k), "solar warming {gain_k} K");
        assert!((dark.air_temp_c - 20.0).abs() < 1.0e-9, "no sun, no drift");
    }

    /// Energy balance, the M1 acceptance property: over a long run with
    /// every mechanism active (heat, sun, leakage, cooling cycles), the
    /// internal energy both nodes gained equals the net heat that entered.
    #[test]
    fn the_thermal_energy_balance_closes() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 12);
        let heat = vec![6.0e3; 12];
        let outside = weather(31.0, 700.0);
        let start_j = stored_j(&model, &c);
        let mut net_in_j = 0.0;
        let mut cooled_ticks = 0u32;

        for _ in 0..7_200 {
            let flows = model.step_container(&mut c, &heat, outside, 1.0);
            let heat_in_w: f64 = heat.iter().sum();
            net_in_j += heat_in_w + flows.envelope_gain_w - flows.hvac_thermal_w;
            cooled_ticks += u32::from(flows.hvac_thermal_w > 0.0);
        }

        assert!(cooled_ticks > 0, "the run must exercise the HVAC");
        let stored_j = stored_j(&model, &c) - start_j;
        let residual = (stored_j - net_in_j).abs();
        assert!(
            residual / net_in_j.abs() < 1.0e-12,
            "thermal energy residual {residual} J on {net_in_j} J in"
        );
    }
}
