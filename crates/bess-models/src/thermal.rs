//! Two-node container thermal model with a thermostat HVAC.

use bess_core::kernel::Weather;
use bess_core::state::{ContainerState, HvacMode};
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
/// The HVAC unit is staged: two cooling units, as the reference container
/// carries, plus an electric heater for the cold end. A minimum run and
/// minimum off time keep the compressors from chattering. That protection
/// covers compressors and only compressors: a unit already running may bring
/// its neighbour in at once, since that is a second machine and the
/// container is climbing; a stopped compressor waits out its interval
/// however hot it gets, because a real one cannot restart against
/// unequalized pressure; and electric heating, being a coil and a contactor,
/// is not gated at all.
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
    /// Thermal cooling capacity of one unit, W. Referenced: the STULZ WXUC5,
    /// a BESS-dedicated unit, is a 35 kW monoblock and a 40 ft battery
    /// container carries one on each short side. That container holds about
    /// 3.14 MWh, GW-01's holds 5.02 MWh, so the same two-unit topology
    /// scaled by container energy gives 56 kW per unit. Scaling by energy
    /// assumes comparable C-rates, which is the assumption to argue with.
    pub unit_cooling_thermal_w: f64,
    /// Air temperature at which the first cooling unit starts, degrees
    /// Celsius. The reference installation holds 25 C inside; the bands sit
    /// around that.
    pub cool1_on_c: f64,
    /// Air temperature at which the last cooling unit stops, degrees Celsius.
    pub cool1_off_c: f64,
    /// Air temperature at which the second cooling unit joins, degrees
    /// Celsius. Estimate.
    pub cool2_on_c: f64,
    /// Air temperature at which the second cooling unit drops out, degrees
    /// Celsius. Estimate.
    pub cool2_off_c: f64,
    /// Coefficient of performance while cooling. Estimate: container
    /// datasheets publish capacity but not input power, and 3.0 is mid-range
    /// for a packaged direct-expansion unit at these conditions.
    pub cop: f64,
    /// Electric heating capacity, W. Referenced: the same installation heats
    /// with 13 kW of multi-stage electric heaters, needed while the battery
    /// stands by. Electric resistance, so its coefficient of performance is
    /// exactly 1 and needs no estimate. Modeled as one stage rather than
    /// several, which overstates the heater's on/off swing and not its
    /// energy.
    pub heating_thermal_w: f64,
    /// Air temperature at which heating starts, degrees Celsius. Estimate:
    /// heating exists to keep cells out of the range where charging an LFP
    /// cell damages it, not to make the container comfortable.
    pub heat_on_c: f64,
    /// Air temperature at which heating stops, degrees Celsius. Estimate.
    pub heat_off_c: f64,
    /// Fan power per running cooling unit, W. Estimate.
    pub unit_fan_w: f64,
    /// Controls power, drawn in every mode, W. Estimate, inherited from M0.
    pub standby_w: f64,
    /// Minimum seconds a compressor stage runs before the controller may
    /// step it down. Estimate: three minutes is the usual anti short-cycle
    /// interval for scroll compressors.
    pub min_run_s: f64,
    /// Minimum seconds the unit stays off before restarting. Estimate.
    pub min_off_s: f64,
}

impl Default for LumpedThermal {
    fn default() -> Self {
        Self {
            air_heat_capacity_j_per_k: 2.8e6,
            rack_heat_capacity_j_per_k: 2.33e6,
            rack_to_air_w_per_k: 900.0,
            ua_w_per_k: 500.0,
            sol_air_coefficient_m2_k_per_w: 0.026,
            unit_cooling_thermal_w: 56.0e3,
            cool1_on_c: 26.0,
            cool1_off_c: 23.5,
            cool2_on_c: 29.0,
            cool2_off_c: 26.5,
            cop: 3.0,
            heating_thermal_w: 13.0e3,
            heat_on_c: 10.0,
            heat_off_c: 15.0,
            unit_fan_w: 1_200.0,
            standby_w: 200.0,
            min_run_s: 180.0,
            min_off_s: 180.0,
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

    /// Total cooling capacity with every unit running, W.
    pub fn full_cooling_thermal_w(&self) -> f64 {
        2.0 * self.unit_cooling_thermal_w
    }

    /// The mode the controller wants next.
    ///
    /// `compressor_free` says whether the protection interval has run out,
    /// and it gates exactly one thing: compressors starting or stopping.
    /// A running unit may bring its neighbour in at once, since that is a
    /// second machine and the container is climbing. A stopped compressor
    /// waits however hot it gets, because a real one physically cannot
    /// restart against unequalized pressure. Electric heating is not gated
    /// at all: it is a coil and a contactor, so it follows its band.
    fn next_mode(&self, mode: HvacMode, air_c: f64, compressor_free: bool) -> HvacMode {
        // Stepping up while a compressor already runs never waits.
        if cooling_units(mode) > 0.0 && air_c >= self.cool2_on_c {
            return HvacMode::Cool2;
        }
        match mode {
            HvacMode::Cool2 => {
                if air_c <= self.cool2_off_c && compressor_free {
                    HvacMode::Cool1
                } else {
                    HvacMode::Cool2
                }
            }
            HvacMode::Cool1 => {
                if air_c <= self.cool1_off_c && compressor_free {
                    HvacMode::Off
                } else {
                    HvacMode::Cool1
                }
            }
            HvacMode::Heat => {
                if air_c >= self.cool1_on_c {
                    // Cooling demand ends heating immediately, whether or
                    // not a compressor may start yet.
                    self.cooling_start(air_c, compressor_free)
                } else if air_c >= self.heat_off_c {
                    HvacMode::Off
                } else {
                    HvacMode::Heat
                }
            }
            HvacMode::Off => {
                if air_c <= self.heat_on_c {
                    HvacMode::Heat
                } else if air_c >= self.cool1_on_c {
                    self.cooling_start(air_c, compressor_free)
                } else {
                    HvacMode::Off
                }
            }
        }
    }

    /// Which cooling mode to start from a stopped compressor, or `Off` while
    /// the protection interval still has time on it.
    fn cooling_start(&self, air_c: f64, compressor_free: bool) -> HvacMode {
        if !compressor_free {
            HvacMode::Off
        } else if air_c >= self.cool2_on_c {
            HvacMode::Cool2
        } else {
            HvacMode::Cool1
        }
    }

    /// Protection time left after this transition, s. The interval tracks
    /// compressors only: starting or changing a cooling stage arms the
    /// minimum run time, dropping cooling altogether arms the minimum off
    /// time, and anything the heater does leaves the clock running.
    fn next_hold_s(&self, from: HvacMode, to: HvacMode, hold_s: f64, dt_s: f64) -> f64 {
        let was_cooling = cooling_units(from) > 0.0;
        let is_cooling = cooling_units(to) > 0.0;
        if is_cooling && to != from {
            self.min_run_s
        } else if was_cooling && !is_cooling {
            self.min_off_s
        } else {
            (hold_s - dt_s).max(0.0)
        }
    }

    /// Heat the unit moves in a mode, W. Positive removes heat from the
    /// container, negative adds it.
    fn mode_thermal_w(&self, mode: HvacMode) -> f64 {
        match mode {
            HvacMode::Heat => -self.heating_thermal_w,
            _ => cooling_units(mode) * self.unit_cooling_thermal_w,
        }
    }

    /// Electrical power a mode draws, W. Controls run in every mode; each
    /// cooling unit adds its compressor and its fan; the heater is
    /// resistance, so it draws exactly what it delivers, plus one fan to
    /// move the warm air.
    fn mode_electrical_w(&self, mode: HvacMode) -> f64 {
        let cooling_w =
            cooling_units(mode) * (self.unit_cooling_thermal_w / self.cop + self.unit_fan_w);
        let heating_w = match mode {
            HvacMode::Heat => self.heating_thermal_w + self.unit_fan_w,
            _ => 0.0,
        };
        self.standby_w + cooling_w + heating_w
    }
}

/// Cooling units running in a mode. The single place that knows a container
/// carries two, so capacity and electrical draw cannot drift apart.
fn cooling_units(mode: HvacMode) -> f64 {
    match mode {
        HvacMode::Cool1 => 1.0,
        HvacMode::Cool2 => 2.0,
        HvacMode::Off | HvacMode::Heat => 0.0,
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

        let previous_mode = container.hvac.mode;
        let compressor_free = container.hvac.compressor_hold_s <= 0.0;
        let mode = self.next_mode(previous_mode, air_c, compressor_free);
        container.hvac.mode = mode;
        container.hvac.compressor_hold_s =
            self.next_hold_s(previous_mode, mode, container.hvac.compressor_hold_s, dt_s);
        let hvac_thermal_w = self.mode_thermal_w(mode);

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
        container.hvac.electrical_w = self.mode_electrical_w(mode);

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
                mode: HvacMode::Off,
                compressor_hold_s: 0.0,
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

    /// Drive the container to a temperature the controller cannot argue
    /// with, and let it react. Time passes at 1 s per call, so hold timers
    /// behave as they would in the plant.
    fn hold_at(model: &LumpedThermal, c: &mut ContainerState, air_c: f64, seconds: u32) {
        for _ in 0..seconds {
            c.air_temp_c = air_c;
            model.step_container(c, &[0.0], weather(20.0, 0.0), 1.0);
        }
    }

    /// The ladder: warmer air brings units on one at a time, cooler air
    /// takes them off the same way, and the bands do not overlap.
    #[test]
    fn cooling_stages_follow_the_ladder() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        assert_eq!(c.hvac.mode, HvacMode::Off);

        hold_at(&model, &mut c, 26.5, 1);
        assert_eq!(c.hvac.mode, HvacMode::Cool1, "first band starts one unit");

        hold_at(&model, &mut c, 29.5, 1);
        assert_eq!(c.hvac.mode, HvacMode::Cool2, "second band adds the other");

        hold_at(&model, &mut c, 26.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Cool1, "falling back through stage 1");

        hold_at(&model, &mut c, 23.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Off);
    }

    /// Between the on and off bands the controller does nothing, whichever
    /// way it arrived there.
    #[test]
    fn the_dead_band_holds_whatever_is_running() {
        let model = LumpedThermal::default();
        let mut warming = container(20.0, 1);
        hold_at(&model, &mut warming, 25.0, 600);
        assert_eq!(warming.hvac.mode, HvacMode::Off, "not warm enough to start");

        let mut cooling = container(20.0, 1);
        hold_at(&model, &mut cooling, 27.0, 600);
        hold_at(&model, &mut cooling, 25.0, 600);
        assert_eq!(
            cooling.hvac.mode,
            HvacMode::Cool1,
            "already running, and 25 C is above the off band"
        );
    }

    /// The anti short-cycle rule: a stage that just started keeps running
    /// through a brief dip, instead of chattering.
    #[test]
    fn a_started_stage_holds_through_a_short_dip() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 26.5, 1);
        assert_eq!(c.hvac.mode, HvacMode::Cool1);

        hold_at(&model, &mut c, 20.0, 60);
        assert_eq!(
            c.hvac.mode,
            HvacMode::Cool1,
            "a minute is inside the minimum run time"
        );

        hold_at(&model, &mut c, 20.0, 130);
        assert_eq!(c.hvac.mode, HvacMode::Off, "past it, the unit may stop");
    }

    /// The protection guards compressors, and a resistance heater is not
    /// one. It stops the moment its band says so, whatever the interval has
    /// left on it.
    #[test]
    fn the_heater_stops_on_its_band_not_on_the_compressor_timer() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 5.0, 1);
        assert_eq!(c.hvac.mode, HvacMode::Heat);

        hold_at(&model, &mut c, 20.0, 1);
        assert_eq!(
            c.hvac.mode,
            HvacMode::Off,
            "13 kW into a container that passed its off band a second ago"
        );
    }

    /// And the other way: a container that gets cold enough starts heating
    /// straight away, since there is no compressor to protect.
    #[test]
    fn the_heater_starts_without_waiting_for_the_compressor_interval() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        // Run a cooling stage and stop it, which arms the interval.
        hold_at(&model, &mut c, 26.5, 1);
        hold_at(&model, &mut c, 20.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Off);
        hold_at(&model, &mut c, 26.5, 1);
        assert!(c.hvac.compressor_hold_s > 0.0, "interval must be running");

        hold_at(&model, &mut c, 5.0, 1);
        assert_eq!(c.hvac.mode, HvacMode::Heat);
    }

    /// A compressor that just stopped stays stopped, however hot the
    /// container gets. This is the one case where the plant has to wait:
    /// the machine physically cannot restart yet.
    #[test]
    fn a_stopped_compressor_waits_even_when_the_container_is_hot() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 26.5, 1);
        hold_at(&model, &mut c, 20.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Off);

        hold_at(&model, &mut c, 30.0, 60);
        assert_eq!(
            c.hvac.mode,
            HvacMode::Off,
            "a restart one minute after stopping is not physical"
        );
        hold_at(&model, &mut c, 30.0, 130);
        assert_eq!(c.hvac.mode, HvacMode::Cool2, "past the interval, both run");
    }

    /// Escalation is the exception, and it is narrow: a unit that is already
    /// running may start its neighbour at once, because that neighbour has
    /// been sitting still.
    #[test]
    fn escalation_ignores_the_hold() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 26.5, 1);
        assert!(
            c.hvac.compressor_hold_s > 0.0,
            "the protection must actually be active"
        );
        hold_at(&model, &mut c, 30.0, 1);
        assert_eq!(c.hvac.mode, HvacMode::Cool2);
    }

    /// Restarting after a stop waits for the off timer, so a container that
    /// keeps crossing the start band cannot cycle a compressor every second.
    #[test]
    fn restarting_waits_for_the_off_timer() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 26.5, 1);
        hold_at(&model, &mut c, 20.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Off);

        hold_at(&model, &mut c, 26.5, 60);
        assert_eq!(c.hvac.mode, HvacMode::Off, "still inside the off timer");
        hold_at(&model, &mut c, 26.5, 130);
        assert_eq!(c.hvac.mode, HvacMode::Cool1);
    }

    /// The cold end: heating runs on its own band, draws exactly what it
    /// delivers because it is resistance, and adds heat rather than removing
    /// it.
    #[test]
    fn heating_runs_at_the_cold_end() {
        let model = LumpedThermal::default();
        let mut c = container(20.0, 1);
        hold_at(&model, &mut c, 5.0, 1);
        assert_eq!(c.hvac.mode, HvacMode::Heat);
        assert!(c.hvac.thermal_w < 0.0, "heating adds heat to the container");
        let expected_w = model.standby_w + model.heating_thermal_w + model.unit_fan_w;
        assert!((c.hvac.electrical_w - expected_w).abs() < 1.0e-9);

        hold_at(&model, &mut c, 16.0, 200);
        assert_eq!(c.hvac.mode, HvacMode::Off);
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
