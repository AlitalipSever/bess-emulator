//! Which instrument reads which point, and what each one claims.

use bess_core::config::PlantConfig;

/// An instrument in the plant, with the accuracy class it is built to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    /// The fiscal meter at the POI, IEC 62053-22 class 0.2S, behind its
    /// own 0.2S instrument transformers.
    Revenue,
    /// The station-service and house-load meters, class 0.5S.
    Auxiliary,
    /// A SCADA analog through the substation's CT/VT chain or a PCS's own
    /// measurement: class 0.5 overall.
    Analog,
    /// A resistance thermometer, IEC 60751 class B, read through the BMS or
    /// the container controller: ±0.5 K at the temperatures the plant sees.
    Thermometer,
}

/// What a class promises: the relative error limit at rated load and at
/// 1 % of rated load (the S-curve's two ends), or an absolute limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassSpec {
    /// The class as the standard names it.
    pub name: &'static str,
    /// Relative limit at and above 5 % of rated load.
    pub rated: f64,
    /// Relative limit at 1 % of rated load.
    pub low: f64,
    /// Absolute limit instead, in the point's unit, for instruments whose
    /// class is absolute (thermometers).
    pub absolute: Option<f64>,
}

impl Instrument {
    /// The class the instrument is built to. Sources in CALIBRATION.md.
    pub fn class(self) -> ClassSpec {
        match self {
            // IEC 62053-22 class 0.2S: ±0.2 % from 0.05 In to Imax, ±0.4 %
            // from 0.01 In, at unity power factor.
            Self::Revenue => ClassSpec {
                name: "0.2S",
                rated: 0.002,
                low: 0.004,
                absolute: None,
            },
            // Class 0.5S: ±0.5 % and ±1.0 %.
            Self::Auxiliary => ClassSpec {
                name: "0.5S",
                rated: 0.005,
                low: 0.010,
                absolute: None,
            },
            // IEC 61869-2 class 0.5 CT: ±0.5 % at 20 to 120 %, ±0.75 % at
            // 5 %, ±1.5 % at 1 % of rated current; the chain is taken at
            // the CT's figure, the VT (class 0.5, ±0.5 %) folded in.
            Self::Analog => ClassSpec {
                name: "0.5",
                rated: 0.005,
                low: 0.015,
                absolute: None,
            },
            // IEC 60751 class B: ±(0.30 + 0.005 |t|) K, 0.5 K at 40 C.
            Self::Thermometer => ClassSpec {
                name: "Pt100 B",
                rated: 0.0,
                low: 0.0,
                absolute: Some(0.5),
            },
        }
    }

    /// The inventory's name for the instrument, as the signal map prints it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Revenue => "revenue_meter",
            Self::Auxiliary => "auxiliary_meter",
            Self::Analog => "scada_analog",
            Self::Thermometer => "thermometer",
        }
    }

    /// A stable small integer for the derivation hash.
    pub fn id(self) -> u64 {
        match self {
            Self::Revenue => 1,
            Self::Auxiliary => 2,
            Self::Analog => 3,
            Self::Thermometer => 4,
        }
    }
}

/// The instrument behind a published point, or `None` for an exact point.
///
/// The list is the inventory, not a heuristic (design D4): counters,
/// states, enumerations, configuration, the BMS's own estimates (SoC, the
/// cell spread), derived ratios (efficiency), the sun's irradiance and the
/// grid frequency stay exact. Frequency because a plant frequency meter
/// resolves the point's own LSB (1 mHz); irradiance and efficiency as
/// known gaps named in CALIBRATION.md.
pub fn instrument_of(name: &str) -> Option<Instrument> {
    let block_point = name.strip_prefix("block").and_then(|r| r.get(3..));
    match name {
        "site.revenue.period_import_kwh" | "site.revenue.period_export_kwh" => {
            Some(Instrument::Revenue)
        }
        "site.aux.hvac_w"
        | "site.aux.bms_w"
        | "site.aux.pcs_standby_w"
        | "site.aux.controls_w"
        | "site.aux.lighting_and_safety_w"
        | "site.aux_power_w"
        | "site.meter.station_service_kwh" => Some(Instrument::Auxiliary),
        "site.poi.active_power_w"
        | "site.poi.reactive_power_var"
        | "site.poi.voltage_kv"
        | "site.meter.import_kwh"
        | "site.meter.export_kwh" => Some(Instrument::Analog),
        "site.weather.ambient_c" => Some(Instrument::Thermometer),
        _ => match block_point {
            Some("pcs.p_ac_kw" | "pcs.p_dc_kw" | "pcs.dc_voltage_v") => Some(Instrument::Analog),
            Some("cell_temp_min_c" | "cell_temp_max_c" | "container.air_temp_c") => {
                Some(Instrument::Thermometer)
            }
            _ => None,
        },
    }
}

/// Full scale of an instrumented point, in the point's own unit: what the
/// load-dependent limit is taken against. Site power against the site
/// rating, a block's against its PCS rating, energy registers against one
/// hour at rating (the class curve is applied to the power behind them),
/// the auxiliary items against a 1 MW house-load meter, voltage against
/// nominal.
pub fn full_scale(name: &str, cfg: &PlantConfig) -> f64 {
    let site_w = cfg.grid.site_rated_w;
    let block_point = name.strip_prefix("block").and_then(|r| r.get(3..));
    match name {
        "site.poi.active_power_w" | "site.poi.reactive_power_var" => site_w,
        "site.poi.voltage_kv" => cfg.grid.poi_nominal_kv,
        "site.meter.import_kwh"
        | "site.meter.export_kwh"
        | "site.revenue.period_import_kwh"
        | "site.revenue.period_export_kwh" => site_w / 1000.0,
        "site.meter.station_service_kwh" => 1.0e3,
        "site.weather.ambient_c" => 1.0,
        n if n.starts_with("site.aux") => 1.0e6,
        _ => match block_point {
            Some("pcs.p_ac_kw" | "pcs.p_dc_kw") => cfg.pcs_rated_w / 1000.0,
            Some("pcs.dc_voltage_v") => cfg.rack.nominal_v(),
            // Temperatures carry an absolute class; their scale is the unit.
            _ => 1.0,
        },
    }
}
