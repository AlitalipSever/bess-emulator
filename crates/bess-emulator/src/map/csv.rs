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
        "space,address,words,encoding,scale,name,unit,class,access"
    )?;
    for p in &points {
        let space = match p.space {
            Space::Input => "input",
            Space::Holding => "holding",
        };
        let access = if p.writable { "rw" } else { "ro" };
        writeln!(
            out,
            "{space},{},{},{},{},{},{},{},{access}",
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
