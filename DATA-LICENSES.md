# Dataset licenses

Policy first, inventory second. Every dataset that enters this repository
is recorded here **before** it lands, with its license and redistribution
status. Sources whose terms do not permit redistribution are never bundled;
`bess-data` ships a fetch script instead and this file says so.

## Policy

1. Bundled data must be redistributable under terms compatible with this
   repository's licenses (MIT OR Apache-2.0), with attribution recorded
   here.
2. Restricted sources (for example exchange market data) are fetched by the
   user with a script, from the original source, under the user's own
   acceptance of the source's terms.
3. Each bundled dataset carries a version tag; the determinism contract
   includes the dataset version.

## Inventory

| Dataset | Used for | License / status | Bundled? |
|---|---|---|---|
| DWD hourly station observations, station 03015 Lindenberg (Mark), year 2024: 2 m air temperature + relative humidity (TU), global irradiance (ST), precipitation amount + form (RR), wind speed + direction (FF), total cloud cover (N) | M1 weather replay. Physics consumers: temperature and irradiance (thermal model, HVAC duty). Scenery-only consumers: cloud cover, precipitation amount and form, and wind speed and direction, read by the 3D view layer since M1.5 PR2. Humidity is compiled and carried without a consumer; it is kept because it arrives in the same DWD product as the temperature the physics does use, and dropping it would mean re-fetching to get it back | Deutscher Wetterdienst, Climate Data Center open data. Free re-use including processing under GeoNutzV / DL-DE->BY-2.0 with source attribution. Attribution: "Source: Deutscher Wetterdienst"; the bundled series are processed (hourly compilation, unit conversion, bounded gap filling; policy documented in `compile-weather` and its report below) | yes: compiled artifact `crates/bess-data/data/lindenberg-2024.bin`, FNV-1a pinned in `bess-data`; raw archives re-fetchable via `crates/bess-data/scripts/fetch-weather.sh` |
| Synthetic daily price curve (24 values, hard-coded) | M0 placeholder dispatch plan | Original to this project | yes (code constant) |
| Synthetic weather / frequency driver (sinusoids) | M0 placeholder inputs | Original to this project | yes (code) |
| LFP OCV curve shape (13-point table) | Cell model | Original parameterization informed by public datasheets and published OCV studies | yes (code constant) |
| PCS loss coefficients k0/k1/k2 (3 floats) + 7 reference efficiency points | `CurvePcs` model and its calibration gate test | Derived values. Fitted against the Sandia-model curve of the Sungrow SC2500UD-US entry in the CEC inverter database, as distributed with [NREL SAM](https://github.com/NREL/SAM) (BSD-3-Clause); underlying data published by the California Energy Commission. Attribution recorded here and in CALIBRATION.md | yes (code constants) |
| PCS night tare (1 float: 169.9 W at the 2.507 MW rating) | `InventoryAux` standby tare, 339.8 W per 5 MW block | Single published value, the `Pnt` field of the same Sungrow SC2500UD-US record as the row above, same distribution and license | yes (code constant) |
| Rack control and monitoring power (1 float, 287 W for 8 racks x 13 modules x 16 cell blocks) | `InventoryAux` rack electronics, scaled by monitored cell count to 71.8 W per rack | Single figure read from Table 3 of Schimpe et al., "Energy efficiency evaluation of a stationary lithium-ion battery container storage system via electro-thermal modeling and detailed component analysis", Applied Energy 210 (2018) 211-229. Accepted manuscript published open access under the DOE Public Access Plan ([OSTI 1409737](https://www.osti.gov/pages/biblio/1409737)). One number and its scaling argument, cited in CALIBRATION.md; no text or figures reproduced | yes (code constant) |

Real historical series (day-ahead prices, grid frequency, balancing
activations) keep arriving with `bess-data`; each will be added to this
table with its exact source and terms before the first commit that
references it.

### DWD Lindenberg 2024: compilation record

Compiled 2026-08-19 from the DWD CDC hourly archives current on that date.
The historical archives are re-issued periodically, so the committed
artifact and its pinned hash are the reproducibility anchor; the fetch
script resolves whatever file names are current. Gap handling in that
compilation (bounded; longer gaps fail the compile): physics series were
complete (temperature, humidity, irradiance: 0 hours filled); scenery
series carried one station outage of 37 h starting 2024-07-06 (wind speed
37 h linear, wind direction 36 h nearest, cloud cover 35 h nearest) plus
2 missing precipitation hours treated as no precipitation. Solar interval
sums are stamped in true solar time; the compiler re-tiles them onto UTC
hours by exact interval overlap. Annual figures of the compiled year:
mean temperature 11.7 C, global irradiance 1163 kWh/m2, precipitation
712 mm.
