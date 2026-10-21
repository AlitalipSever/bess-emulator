//! Layer traits: the extension mechanism of the emulator.
//!
//! Each plant layer sits behind one trait so it can be deepened milestone by
//! milestone without touching the others. Signals originate in the layer they
//! belong to; the kernel only orchestrates the causal chain between layers.

use crate::config::RackConfig;
use crate::kernel::{Awards, Weather};
use crate::state::{AuxPower, ContainerState, PcsState, RackState, SiteState, SubstationState};

/// Result of stepping one rack for one tick.
#[derive(Debug, Clone, Copy)]
pub struct RackStepResult {
    /// DC terminal power actually delivered, W (positive = discharge).
    pub p_dc_w: f64,
    /// Heat released inside the rack, W. Defined as chemical power minus
    /// terminal power, so the site energy balance closes exactly.
    pub heat_w: f64,
}

/// Power limits, W. Both values are magnitudes (>= 0).
#[derive(Debug, Clone, Copy, Default)]
pub struct PowerLimits {
    /// Maximum charge power (absorbing), W.
    pub max_charge_w: f64,
    /// Maximum discharge power (delivering), W.
    pub max_discharge_w: f64,
}

/// Electrochemical model of one rack.
pub trait CellModel: Send + Sync {
    /// Advance one rack by `dt_s` given a requested DC terminal power
    /// (W, positive = discharge). The request is already BMS-limited;
    /// implementations additionally clamp to their own electrical limits
    /// (current limit, SoC hard bounds 0..1) and report what was actually
    /// delivered.
    fn step_rack(
        &self,
        rack: &mut RackState,
        cfg: &RackConfig,
        p_request_w: f64,
        dt_s: f64,
    ) -> RackStepResult;

    /// Chemical energy currently stored in the rack, Wh (integral of the OCV
    /// curve over charge). Must be consistent with `step_rack`; the energy
    /// conservation invariant is checked against this.
    fn stored_energy_wh(&self, rack: &RackState, cfg: &RackConfig) -> f64;
}

/// What one BMS step moved.
///
/// Passive balancing burns charge off the high cells in bleed resistors
/// inside the rack. The energy leaves storage and reappears as heat in the
/// rack's own thermal mass, so the step reports both halves: the energy
/// conservation invariant holds the interface, not one implementation, the
/// same way `ThermalFlows` holds the thermal model.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BmsFlows {
    /// Energy the step took out of the rack's stored energy, Wh.
    pub bled_wh: f64,
    /// Heat released into the rack during the step, W. Equals `bled_wh`
    /// spread over the step: bleed resistors have nowhere else to put it.
    pub heat_w: f64,
    /// The estimator corrected its reported SoC by a jump this step; the
    /// kernel turns it into an event. `None` on an ordinary tick.
    pub soc_resnap: Option<SocResnap>,
}

/// One correction jump of the reported SoC, in per mille of SoC: what a
/// register would carry, and exact, so the event log stays comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SocResnap {
    /// Reported SoC before the jump, per mille.
    pub from_permille: u16,
    /// Reported SoC after the jump, per mille.
    pub to_permille: u16,
}

/// Battery management logic for one rack.
pub trait BmsLogic: Send + Sync {
    /// Power limits for a rack right now (SoC window, derating).
    fn rack_limits(&self, rack: &RackState, cfg: &RackConfig) -> PowerLimits;

    /// Advance the BMS's own dynamics by `dt_s` (cell spread, balancing).
    /// The kernel calls this once per tick, before it asks for limits.
    /// Named apart from `CellModel::step_rack` on purpose: two traits
    /// mutating one rack under one method name would make every mention
    /// ambiguous.
    fn step_bms(&self, rack: &mut RackState, cfg: &RackConfig, dt_s: f64) -> BmsFlows;

    /// The rack's alarm word after this tick, laid out in
    /// `alarms::layout::rack`. `rack.alarm_bits` holds the word as the last
    /// tick left it, which is what hysteresis and latching read; the kernel
    /// stores the result and turns the difference into events.
    fn rack_alarms(&self, rack: &RackState, cfg: &RackConfig) -> u32;
}

/// The heat flows one container tick moved, W.
///
/// Every implementation reports its flows, so "the thermal energy balance
/// closes" is a property of this interface rather than of one model: what
/// the container's thermal masses gained must equal the heat that entered
/// minus what the HVAC removed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ThermalFlows {
    /// Heat carried from the rack thermal masses into the container air.
    pub cells_to_air_w: f64,
    /// Net heat entering through the envelope: ambient leakage plus solar
    /// gain. Negative when the container is warmer than the outside air the
    /// envelope sees.
    pub envelope_gain_w: f64,
    /// Heat removed by the HVAC unit.
    pub hvac_thermal_w: f64,
    /// Electrical power the HVAC unit drew. This is the one the kernel
    /// meters as an auxiliary load.
    pub hvac_electrical_w: f64,
}

/// Thermal model of one container.
pub trait ThermalModel: Send + Sync {
    /// Advance one container by `dt_s`.
    ///
    /// `rack_heat_w` carries the heat each rack released this tick, in rack
    /// order, so a model can put that heat into the rack's own thermal mass
    /// instead of straight into the air. `weather` is what the envelope
    /// sees. Updates air temperature, rack cell temperatures and HVAC state,
    /// and reports the flows it moved.
    fn step_container(
        &self,
        container: &mut ContainerState,
        rack_heat_w: &[f64],
        weather: Weather,
        dt_s: f64,
    ) -> ThermalFlows;
}

/// Power conversion system of one block.
pub trait PcsModel: Send + Sync {
    /// Advance the converter's own thermal state by `dt_s` (M3 phase 3):
    /// its node from the last tick's loss toward `air_c`, its overload
    /// budget from the last tick's AC power, and from both the AC power
    /// it permits this tick, left in `pcs.ac_limit_w`. A converter without
    /// a thermal model leaves the temperature and sets the limit to its
    /// rating.
    fn step_thermal(&self, pcs: &mut PcsState, air_c: f64, dt_s: f64);

    /// AC-side capability right now, given the DC-side limits aggregated
    /// over the block's racks and what the converter sees and permits this
    /// tick (`pcs.dc_voltage_v`, the efficiency surface's second axis, and
    /// `pcs.ac_limit_w`).
    fn ac_capability_w(&self, pcs: &PcsState, dc_limits: &PowerLimits) -> PowerLimits;

    /// DC-side power request for the AC-side target the converter is
    /// following (`pcs.setpoint_followed_w`, W, positive = discharge),
    /// respecting the permitted AC power and the DC limits, at the DC bus
    /// voltage.
    fn dc_request_w(&self, pcs: &PcsState, dc_limits: &PowerLimits) -> f64;

    /// Update PCS state from the DC power the racks actually delivered;
    /// returns the AC-side power, W. Converts at `pcs.dc_voltage_v`.
    fn finalize(&self, pcs: &mut PcsState, p_dc_actual_w: f64) -> f64;

    /// The converter's apparent-power rating, VA: the radius of the
    /// circle its reactive power lives on (M4 phase 1 PR2).
    fn apparent_rating_va(&self) -> f64;

    /// The reactive power the converter delivers for a target
    /// `q_target_var`, var, positive over-excited: the target bounded by
    /// what the circle leaves beside this tick's active power
    /// (`pcs.p_ac_w`), the full rating at zero active power.
    fn reactive(&self, pcs: &PcsState, q_target_var: f64) -> f64 {
        let s = self.apparent_rating_va();
        let p = pcs.p_ac_w.abs().min(s);
        let room = (s * s - p * p).sqrt();
        q_target_var.clamp(-room, room)
    }
}

/// The prices the revenue meter books at this instant (M4 phase 3 PR3),
/// EUR/MWh. Positive is money to the plant for energy delivered (day
/// ahead, aFRR positive) or, in the negative direction, for energy
/// absorbed; the imbalance price settles the schedule deviation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MarketPrices {
    /// The day-ahead price of the hour, EUR/MWh.
    pub day_ahead_eur_mwh: f64,
    /// What delivered aFRR energy is paid, EUR/MWh.
    pub afrr_pos_eur_mwh: f64,
    /// What absorbed aFRR energy is paid, EUR/MWh (negative: the plant
    /// pays to absorb).
    pub afrr_neg_eur_mwh: f64,
    /// The imbalance price, EUR/MWh: a long quarter-hour earns it, a short
    /// one pays it.
    pub imbalance_eur_mwh: f64,
}

/// Plant-level dispatch strategy.
pub trait EmsStrategy: Send + Sync {
    /// Site active-power target at this instant, W (positive = discharge).
    fn site_target_w(&self, unix_time_s: i64, state: &SiteState) -> f64;

    /// The balancing products the plant holds at this instant (M4 phase
    /// 3): the awarded power per product and the capacity price it is
    /// paid. A strategy that sells nothing holds nothing.
    fn awards_at(&self, _unix_time_s: i64, _state: &SiteState) -> Awards {
        Awards::NONE
    }

    /// The prices the revenue meter books at this instant (M4 phase 3
    /// PR3). A strategy without a market books nothing.
    fn prices_at(&self, _unix_time_s: i64, _state: &SiteState) -> MarketPrices {
        MarketPrices::default()
    }

    /// What the strategy does with the current block (M4 phase 4): 0
    /// arbitrage, 1 FCR, 2 aFRR. A plan without a market arbitrages.
    fn block_use_at(&self, _unix_time_s: i64, _state: &SiteState) -> u16 {
        0
    }
}

/// What the site is asking its auxiliary systems to supply this tick.
///
/// Counts rather than states: the auxiliary model owns what a rack's
/// electronics or an idle converter draws, the kernel owns how many of each
/// there are.
#[derive(Debug, Clone, Copy, Default)]
pub struct AuxDemand {
    /// HVAC electrical draw already computed by the thermal layer, W.
    pub hvac_w: f64,
    /// Racks on site, energized whether or not they are in service.
    pub racks: usize,
    /// PCS units energized but not converting. A tripped unit counts: a trip
    /// opens the AC side and leaves the controls, cooling and communications
    /// alive, which is what the tare pays for. A unit in a block under
    /// maintenance does not: it is locked out with its disconnects open.
    pub pcs_not_converting: usize,
}

/// The site's auxiliary loads: everything the plant consumes to run itself.
///
/// Behind a trait like every other layer, because the M1 gate makes this a
/// measured category rather than a constant, and later milestones deepen it
/// (loads that follow temperature, standby strategies, aux supply faults).
pub trait AuxiliaryModel: Send + Sync {
    /// Auxiliary draw for one tick, itemized.
    fn step_site(&self, demand: AuxDemand) -> AuxPower;
}

/// What the grid and the sky hand the substation this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridConditions {
    /// Grid frequency at the POI, Hz.
    pub frequency_hz: f64,
    /// Grid voltage at the POI, per unit of nominal.
    pub voltage_pu: f64,
    /// Ambient air at the transformer, °C.
    pub ambient_c: f64,
}

/// Substation and grid coupling.
pub trait GridInterface: Send + Sync {
    /// Propagate the total PCS AC power, the converters' reactive power
    /// and the site auxiliary load through the substation to the POI:
    /// transformer losses and temperatures, its reactive consumption, the
    /// tap changer and the MV bus, POI measurements, energy meters, and
    /// grid frequency and voltage pass-through. The auxiliary total
    /// arrives itemized from the auxiliary model; the substation meters it
    /// rather than inventing it.
    fn step(
        &self,
        substation: &mut SubstationState,
        p_ac_total_w: f64,
        q_ac_total_var: f64,
        aux_w: f64,
        grid: GridConditions,
        dt_s: f64,
    );
}

/// The full model bundle the kernel drives. Boxed trait objects: dispatch
/// cost is a few hundred virtual calls per tick, irrelevant next to the
/// arithmetic, and it keeps the kernel monomorphization-free.
pub struct Models {
    /// Electrochemical rack model.
    pub cell: Box<dyn CellModel>,
    /// Battery management logic.
    pub bms: Box<dyn BmsLogic>,
    /// Container thermal model.
    pub thermal: Box<dyn ThermalModel>,
    /// Power conversion system model.
    pub pcs: Box<dyn PcsModel>,
    /// Dispatch strategy.
    pub ems: Box<dyn EmsStrategy>,
    /// Auxiliary load inventory.
    pub aux: Box<dyn AuxiliaryModel>,
    /// Substation / grid coupling.
    pub grid: Box<dyn GridInterface>,
}
