//! Where the kernel's block and site bits raise and clear.
//!
//! Rack thresholds belong to the BMS model that evaluates them. These are
//! the kernel's own, for the two words it owns.

/// A hysteresis band: the bit raises at `raise` and clears past `clear`,
/// on the far side of `raise` from the alarm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Value at which the bit raises.
    pub raise: f64,
    /// Value the quantity has to get back past before the bit clears.
    pub clear: f64,
}

/// Thresholds of the block and site words.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockSiteThresholds {
    /// PCS setpoint miss as a share of PCS rating.
    pub setpoint_miss: Band,
    /// Seconds a miss above `setpoint_miss.raise` has to last before the
    /// bit raises. The M0 PCS has no ramp, so this is not sized against
    /// ramp lag; it keeps one tick of a block crossing its capability from
    /// reading as a failure to deliver.
    pub setpoint_deadband_s: f64,
    /// Container air temperature, degrees Celsius.
    pub container_air_c: Band,
    /// Site delivery short of the site setpoint, as a share of site
    /// rating.
    pub site_shortfall: Band,
}

impl Default for BlockSiteThresholds {
    /// The miss bands sit above the conversion efficiency's own noise and
    /// raise the site bit only once a shortfall is worth a dispatcher's
    /// attention, 2 MW at GW-01. The air band sits 11 K above the warmest
    /// air the M1 HVAC lets through on the replayed July day (29 C), and
    /// under the 45 C top of the EVE MB31 recommended range.
    fn default() -> Self {
        Self {
            setpoint_miss: Band {
                raise: 0.02,
                clear: 0.01,
            },
            setpoint_deadband_s: 10.0,
            container_air_c: Band {
                raise: 40.0,
                clear: 37.0,
            },
            site_shortfall: Band {
                raise: 0.02,
                clear: 0.01,
            },
        }
    }
}
