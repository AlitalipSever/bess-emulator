//! Layer traits: the extension mechanism of the emulator.
//!
//! Each plant layer sits behind one trait so it can be deepened milestone by
//! milestone without touching the others. Signals originate in the layer they
//! belong to; the kernel only orchestrates the causal chain between layers.

use crate::config::RackConfig;
use crate::state::{ContainerState, PcsState, RackState, SiteState, SubstationState};

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

/// Battery management logic for one rack.
pub trait BmsLogic: Send + Sync {
    /// Power limits for a rack right now (SoC window, derating).
    fn rack_limits(&self, rack: &RackState, cfg: &RackConfig) -> PowerLimits;
}

/// Thermal model of one container.
pub trait ThermalModel: Send + Sync {
    /// Advance one container by `dt_s` given the battery heat released
    /// inside it and the ambient temperature. Updates air temperature, HVAC
    /// state, and rack cell temperatures. Returns the HVAC electrical power
    /// drawn during the tick, W.
    fn step_container(
        &self,
        container: &mut ContainerState,
        heat_w: f64,
        ambient_c: f64,
        dt_s: f64,
    ) -> f64;
}

/// Power conversion system of one block.
pub trait PcsModel: Send + Sync {
    /// AC-side capability right now, given the DC-side limits aggregated
    /// over the block's racks.
    fn ac_capability_w(&self, dc_limits: &PowerLimits) -> PowerLimits;

    /// DC-side power request for an AC-side target (W, positive =
    /// discharge), respecting the AC rating and the DC limits.
    fn dc_request_w(&self, p_ac_target_w: f64, dc_limits: &PowerLimits) -> f64;

    /// Update PCS state from the DC power the racks actually delivered;
    /// returns the AC-side power, W.
    fn finalize(&self, pcs: &mut PcsState, p_dc_actual_w: f64) -> f64;
}

/// Plant-level dispatch strategy.
pub trait EmsStrategy: Send + Sync {
    /// Site active-power target at this instant, W (positive = discharge).
    fn site_target_w(&self, unix_time_s: i64, state: &SiteState) -> f64;
}

/// Substation and grid coupling.
pub trait GridInterface: Send + Sync {
    /// Propagate the total PCS AC power and the HVAC auxiliary load through
    /// the substation to the POI: transformer losses, station auxiliaries,
    /// POI measurements, energy meters, and grid frequency pass-through.
    fn step(
        &self,
        substation: &mut SubstationState,
        p_ac_total_w: f64,
        hvac_aux_w: f64,
        frequency_hz: f64,
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
    /// Substation / grid coupling.
    pub grid: Box<dyn GridInterface>,
}
