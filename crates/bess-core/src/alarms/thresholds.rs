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
    /// Converter internal temperature, °C: the warning ahead of the
    /// derating knee (M3 phase 3).
    pub pcs_temperature_c: Band,
    /// Transformer hot-spot temperature, °C: the warning at IEC 60076-7's
    /// normal cyclic limit (M3 phase 4).
    pub transformer_hot_spot_c: Band,
    /// Transformer hot-spot temperature at which the kernel trips the
    /// site, °C: IEC 60076-7's long-term emergency limit.
    pub transformer_trip_c: f64,
    /// The frame a closed quarter-hour's mean may deviate from its
    /// schedule by, as a share of the site rating, before
    /// `site.schedule_deviation` raises (M4 phase 2 PR2). The TSOs leave
    /// the frame to the connection agreement; 5 % is a labeled estimate.
    pub schedule_frame: f64,
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
            // 5 K under the SG2500U's knee on the node (50 °C ambient plus
            // the 20 K rated rise), clearing 3 K lower.
            pcs_temperature_c: Band {
                raise: 65.0,
                clear: 62.0,
            },
            // IEC 60076-7's hot-spot limits for a power transformer:
            // 120 °C normal cyclic, 140 °C long-term emergency.
            transformer_hot_spot_c: Band {
                raise: 120.0,
                clear: 110.0,
            },
            transformer_trip_c: 140.0,
            schedule_frame: 0.05,
        }
    }
}
