//! Bit positions of the three alarm words, frozen in M2 phase 2.
//!
//! Low byte warnings, high byte trips. Each constant says what raises the
//! bit and which of the three behaviors it has. The rack word lives in a
//! u32 for headroom, but only its low 16 bits are laid out, because the
//! per-block Modbus register that folds rack words with OR is a u16.

/// Rack word (`RackState::alarm_bits`), evaluated by `BmsLogic`.
pub mod rack {
    /// Cell temperature approaching the hot shoulder. Hysteresis.
    pub const OVER_TEMP_WARNING: u8 = 0;
    /// Cell temperature approaching the cold limit. Hysteresis.
    pub const UNDER_TEMP_WARNING: u8 = 1;
    /// SoC above the operating window. Hysteresis.
    pub const SOC_HIGH: u8 = 2;
    /// SoC below the operating window. Hysteresis.
    pub const SOC_LOW: u8 = 3;
    /// Cell voltage spread wide. Hysteresis.
    pub const IMBALANCE_WARNING: u8 = 4;
    /// Temperature is holding either direction below full rate. Hysteresis.
    pub const DERATE_ACTIVE: u8 = 5;
    /// Rack disconnected from the DC bus. Mirrors `in_service`.
    pub const ISOLATED: u8 = 6;
    /// Cell temperature at the hot limit. Latched.
    pub const OVER_TEMP_TRIP: u8 = 8;
    /// Cell voltage spread past the fault level. Latched.
    pub const IMBALANCE_TRIP: u8 = 9;
    /// Cell temperature at the cold limit. Latched.
    pub const UNDER_TEMP_TRIP: u8 = 10;

    /// Every laid-out bit and the name the surfaces publish it under.
    pub const NAMES: &[(u8, &str)] = &[
        (OVER_TEMP_WARNING, "over_temp_warning"),
        (UNDER_TEMP_WARNING, "under_temp_warning"),
        (SOC_HIGH, "soc_high"),
        (SOC_LOW, "soc_low"),
        (IMBALANCE_WARNING, "imbalance_warning"),
        (DERATE_ACTIVE, "derate_active"),
        (ISOLATED, "isolated"),
        (OVER_TEMP_TRIP, "over_temp_trip"),
        (IMBALANCE_TRIP, "imbalance_trip"),
        (UNDER_TEMP_TRIP, "under_temp_trip"),
    ];
}

/// Block word (`BlockState::alarm_bits`), evaluated by the kernel.
pub mod block {
    /// The PCS has missed its setpoint for longer than the deadband.
    /// Hysteresis.
    pub const SETPOINT_NOT_MET: u8 = 0;
    /// Air in one of the block's containers is too warm. Hysteresis.
    pub const CONTAINER_OVER_TEMP: u8 = 1;
    /// One of the block's container HVAC units has failed. Mirrors
    /// `HvacState::failed`.
    pub const HVAC_FAILURE: u8 = 2;
    /// The PCS is in fault. Mirrors `PcsOpState::Fault`, a state that
    /// itself only leaves through an operator reset, hence the trip byte.
    pub const PCS_FAULT: u8 = 8;

    /// Every laid-out bit and the name the surfaces publish it under.
    pub const NAMES: &[(u8, &str)] = &[
        (SETPOINT_NOT_MET, "setpoint_not_met"),
        (CONTAINER_OVER_TEMP, "container_over_temp"),
        (HVAC_FAILURE, "hvac_failure"),
        (PCS_FAULT, "pcs_fault"),
    ];
}

/// Site word (`SiteState::alarm_bits`), evaluated by the kernel.
pub mod site {
    /// The blocks together deliver less than the site setpoint. Hysteresis.
    pub const POWER_LIMITED: u8 = 0;
    /// Some block or rack is out of the plant: a PCS in fault or a rack
    /// isolated. Mirrors state.
    pub const PARTIAL_AVAILABILITY: u8 = 1;
    /// The HV breaker is open. Mirrors `SubstationState::hv_breaker`.
    pub const HV_BREAKER_OPEN: u8 = 2;
    /// Protection has tripped the site. Latched. Laid out now so the word
    /// is frozen whole; raised from phase 3, which builds the trip.
    pub const PROTECTION_TRIP: u8 = 8;

    /// Every laid-out bit and the name the surfaces publish it under.
    pub const NAMES: &[(u8, &str)] = &[
        (POWER_LIMITED, "power_limited"),
        (PARTIAL_AVAILABILITY, "partial_availability"),
        (HV_BREAKER_OPEN, "hv_breaker_open"),
        (PROTECTION_TRIP, "protection_trip"),
    ];
}

use super::AlarmNode;

/// Published name of a bit, `word.name` (`rack.derate_active`), or `None`
/// for a position the layout leaves free.
pub fn name(node: AlarmNode, bit: u8) -> Option<String> {
    let (word, names) = match node {
        AlarmNode::Site => ("site", site::NAMES),
        AlarmNode::Block { .. } => ("block", block::NAMES),
        AlarmNode::Rack { .. } => ("rack", rack::NAMES),
    };
    names
        .iter()
        .find(|(b, _)| *b == bit)
        .map(|(_, n)| format!("{word}.{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bit is laid out once, in the byte its behavior belongs to, and a
    /// name is never shared: two alarms on one bit is the thing a frozen
    /// layout exists to rule out.
    #[test]
    fn every_word_names_each_bit_once() {
        for names in [rack::NAMES, block::NAMES, site::NAMES] {
            for (i, (bit, name)) in names.iter().enumerate() {
                assert!(*bit < 16, "{name} outside the published u16");
                for (other_bit, other_name) in &names[i + 1..] {
                    assert_ne!(bit, other_bit, "{name} and {other_name} share a bit");
                    assert_ne!(name, other_name);
                }
            }
        }
        assert_eq!(
            name(AlarmNode::Site, site::POWER_LIMITED).as_deref(),
            Some("site.power_limited")
        );
        assert_eq!(name(AlarmNode::Site, 7), None);
    }
}
