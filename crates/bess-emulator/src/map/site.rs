//! The site's telemetry: grid connection, energy, weather, house load and
//! the run's own bookkeeping, at input 0 onward.

use bess_core::state::{BreakerState, EmsMode, SiteState, SwitchPosition};

use super::{point, Class, Encoding, Point, Space};

/// Site telemetry points. The alarm points at site addresses live with the
/// other alarm points, in `alarms`.
#[allow(clippy::too_many_lines)]
pub(super) fn points() -> Vec<Point> {
    use Class::{Fast, Medium, Slow};
    use Encoding::{I16, I32, U16, U32};
    use Space::Input;

    vec![
        point!(
            "site.poi.active_power_w",
            "W",
            Fast,
            I32,
            1.0,
            0,
            Input,
            |s: &SiteState| s.substation.poi_active_power_w
        ),
        point!(
            "site.poi.reactive_power_var",
            "var",
            Fast,
            I32,
            1.0,
            2,
            Input,
            |s: &SiteState| s.substation.poi_reactive_power_var
        ),
        point!(
            "site.poi.voltage_kv",
            "kV",
            Fast,
            U16,
            100.0,
            4,
            Input,
            |s: &SiteState| s.substation.poi_voltage_kv
        ),
        point!(
            "site.poi.frequency_hz",
            "Hz",
            Fast,
            U16,
            1000.0,
            5,
            Input,
            |s: &SiteState| s.substation.frequency_hz
        ),
        point!(
            "site.soc_pct",
            "%",
            Medium,
            U16,
            100.0,
            6,
            Input,
            // What the BMSs report, not the kernel's truth (map 0.4.0).
            |s: &SiteState| s.average_soc_reported() * 100.0
        ),
        point!("site.soh_pct", "%", Slow, U16, 100.0, 7, Input, |_| 100.0),
        point!(
            "site.available_discharge_kw",
            "kW",
            Fast,
            U32,
            1.0,
            8,
            Input,
            |s: &SiteState| s.ems.available_discharge_w / 1000.0
        ),
        point!(
            "site.available_charge_kw",
            "kW",
            Fast,
            U32,
            1.0,
            10,
            Input,
            |s: &SiteState| s.ems.available_charge_w / 1000.0
        ),
        point!(
            "site.meter.export_kwh",
            "kWh",
            Slow,
            U32,
            1.0,
            12,
            Input,
            |s: &SiteState| s.substation.export_wh / 1000.0
        ),
        point!(
            "site.meter.import_kwh",
            "kWh",
            Slow,
            U32,
            1.0,
            14,
            Input,
            |s: &SiteState| s.substation.import_wh / 1000.0
        ),
        // The house load's own meter while the HV breaker is open: no
        // current crosses an open breaker, so the two POI meters above stand
        // still and the station-service feed carries the site. It sits after
        // the alarm points at 42 and 43 because the meters' neighbourhood was
        // taken in M0; the map trades adjacency for stability.
        point!(
            "site.meter.station_service_kwh",
            "kWh",
            Slow,
            U32,
            1.0,
            44,
            Input,
            |s: &SiteState| s.substation.station_service_wh / 1000.0
        ),
        // The switchgear beside the breaker (M3 phase 4): the closing
        // sequence (0 idle, 1 closing), the line disconnectors and the
        // earthing switch, each a position. States, so exact.
        point!(
            "site.substation.sequence",
            "enum",
            Medium,
            U16,
            1.0,
            52,
            Input,
            |s: &SiteState| f64::from(s.substation.sequence.code())
        ),
        point!(
            "site.substation.disconnectors_closed",
            "bool",
            Medium,
            U16,
            1.0,
            53,
            Input,
            |s: &SiteState| f64::from(u8::from(
                s.substation.disconnectors == SwitchPosition::Closed
            ))
        ),
        point!(
            "site.substation.earthing_closed",
            "bool",
            Medium,
            U16,
            1.0,
            54,
            Input,
            |s: &SiteState| f64::from(u8::from(
                s.substation.earthing_switch == SwitchPosition::Closed
            ))
        ),
        point!(
            "site.substation.hv_breaker_closed",
            "bool",
            Medium,
            U16,
            1.0,
            16,
            Input,
            |s: &SiteState| f64::from(s.substation.hv_breaker == BreakerState::Closed)
        ),
        point!(
            "site.ems.mode",
            "enum",
            Medium,
            U16,
            1.0,
            17,
            Input,
            |s: &SiteState| { f64::from(s.ems.mode == EmsMode::External) }
        ),
        point!(
            "site.ems.setpoint_w",
            "W",
            Fast,
            I32,
            1.0,
            18,
            Input,
            |s: &SiteState| s.ems.site_setpoint_w
        ),
        point!(
            "site.weather.ambient_c",
            "degC",
            Medium,
            I16,
            10.0,
            20,
            Input,
            |s: &SiteState| s.weather.ambient_c
        ),
        point!(
            "site.weather.irradiance_wm2",
            "W/m2",
            Medium,
            U16,
            1.0,
            21,
            Input,
            |s: &SiteState| s.weather.irradiance_wm2
        ),
        point!(
            "site.aux_power_w",
            "W",
            Medium,
            U32,
            1.0,
            22,
            Input,
            |s: &SiteState| s.substation.aux_power_w
        ),
        point!(
            "site.transformer_loss_w",
            "W",
            Medium,
            U32,
            1.0,
            24,
            Input,
            |s: &SiteState| s.substation.transformer_loss_w
        ),
        point!(
            "site.sim.tick",
            "count",
            Fast,
            U32,
            1.0,
            26,
            Input,
            |s: &SiteState| { (s.tick % u64::from(u32::MAX)) as f64 }
        ),
        point!(
            "site.sim.unix_time_s",
            "s",
            Fast,
            U32,
            1.0,
            28,
            Input,
            |s: &SiteState| s.unix_time_s() as f64
        ),
        point!(
            "site.pcs_online_count",
            "count",
            Medium,
            U16,
            1.0,
            30,
            Input,
            |s: &SiteState| {
                s.blocks
                    .iter()
                    .filter(|b| b.pcs.op_state.is_converting())
                    .count() as f64
            }
        ),
        // The house load, item by item. `site.aux_power_w` above is the
        // total the substation meters; these five are what drew it, and they
        // sum to it. They start at 32 rather than beside the total, because
        // the total's address is published and moving it would be a breaking
        // change; the map trades adjacency for stability.
        point!(
            "site.aux.hvac_w",
            "W",
            Medium,
            U32,
            1.0,
            32,
            Input,
            |s: &SiteState| s.aux.hvac_w
        ),
        point!(
            "site.aux.bms_w",
            "W",
            Medium,
            U32,
            1.0,
            34,
            Input,
            |s: &SiteState| s.aux.bms_w
        ),
        point!(
            "site.aux.pcs_standby_w",
            "W",
            Medium,
            U32,
            1.0,
            36,
            Input,
            |s: &SiteState| s.aux.pcs_standby_w
        ),
        point!(
            "site.aux.controls_w",
            "W",
            Medium,
            U32,
            1.0,
            38,
            Input,
            |s: &SiteState| s.aux.controls_w
        ),
        point!(
            "site.aux.lighting_and_safety_w",
            "W",
            Medium,
            U32,
            1.0,
            40,
            Input,
            |s: &SiteState| s.aux.lighting_and_safety_w
        ),
    ]
}
