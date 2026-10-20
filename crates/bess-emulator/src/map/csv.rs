//! The published reference: the table written out as CSV under `refmodel/`.

use std::io::Write as _;
use std::path::Path;

use bess_core::config::PlantConfig;

use super::{build_points, Space, MAP_VERSION};

/// Write the signal map reference as CSV (the artifact published under
/// `refmodel/`).
///
/// The first line is a `#` comment carrying [`MAP_VERSION`], so the published
/// artifact can say which version of the contract it is. Readers skip lines
/// starting with `#`.
pub fn dump_signal_map_csv(path: &Path) -> std::io::Result<()> {
    let cfg = PlantConfig::gw01();
    let points = build_points(&cfg);
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(out, "# signal-map-version: {MAP_VERSION}")?;
    writeln!(
        out,
        "space,address,words,encoding,scale,name,unit,class,access,instrument,accuracy_class"
    )?;
    for p in &points {
        let space = match p.space {
            Space::Input => "input",
            Space::Holding => "holding",
        };
        let access = if p.writable { "rw" } else { "ro" };
        // The instrument behind the point and its accuracy class, or
        // `exact` twice (M2.5 phase 3): a signal list that does not say how
        // good a number claims to be is leaving the reader to guess.
        let (instrument, accuracy) = crate::instruments::instrument_of(&p.name)
            .map_or(("exact", "exact"), |i| (i.name(), i.class().name));
        writeln!(
            out,
            "{space},{},{},{},{},{},{},{},{access},{instrument},{accuracy}",
            p.addr,
            p.encoding.words(),
            p.encoding.as_str(),
            p.scale,
            p.name,
            p.unit,
            p.class.as_str(),
        )?;
    }
    out.flush()
}
