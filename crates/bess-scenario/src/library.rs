//! The scenario library, embedded at build time.
//!
//! Every file of the repository's `scenarios/` directory, as written, so
//! the content hash a run carries is the file's and the browser plays the
//! same bytes CI does. The order is the library README's: grouped by
//! taxonomy, `bos`, `controls`, `cell`, `calendar`, `maintenance`, `grid`,
//! `market`.
//! A test
//! holds this list and the directory equal, so a case added to one and not
//! the other fails CI rather than quietly missing from the browser.

use bess_core::PlantConfig;
use serde_json::{json, Value};

use crate::{load, LoadError, Scenario};

/// One embedded case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    /// The file's `name`, which is also its file stem.
    pub name: &'static str,
    /// The file, byte for byte.
    pub yaml: &'static str,
}

macro_rules! case {
    ($name:literal) => {
        Case {
            name: $name,
            yaml: include_str!(concat!("../../../scenarios/", $name, ".yaml")),
        }
    };
}

/// The library, in the README's order.
pub const CASES: &[Case] = &[
    case!("hvac-failure-hot-week"),
    case!("hvac-failure-both-units"),
    case!("pcs-trip-and-reset"),
    case!("pcs-overload-budget"),
    case!("protection-trip-staggered-return"),
    case!("hvac-failure-winter-night"),
    case!("scada-dropout-with-backfill"),
    case!("frozen-scada-day"),
    case!("unit-error-audit-trap"),
    case!("alarm-chatter-storm"),
    case!("clock-drift-and-nan"),
    case!("curtailment-order"),
    case!("rack-self-discharge-drift"),
    case!("dst-spring-forward"),
    case!("dst-fall-back"),
    case!("block-maintenance-window"),
    case!("rack-isolation-and-restore"),
    case!("substation-earthing-work"),
    case!("frequency-event"),
    case!("fcr-block"),
    case!("afrr-activation-day"),
];

/// Longest a headline may be, characters: a paragraph, not a page.
pub const HEADLINE_MAX: usize = 600;

/// The case named `name`, if the library has it.
pub fn find(name: &str) -> Option<&'static Case> {
    CASES.iter().find(|c| c.name == name)
}

/// The library as the browser module's `scenarios()` publishes it: one
/// object per case that loads against `cfg`, in the library's order, with
/// `name`, `taxonomy`, `length_s` and `headline`.
pub fn catalogue(cfg: &PlantConfig) -> Value {
    Value::Array(
        CASES
            .iter()
            .filter_map(|case| {
                let scenario = case.load(cfg).ok()?;
                Some(json!({
                    "name": case.name,
                    "taxonomy": scenario.taxonomy.map(crate::Taxonomy::name),
                    "length_s": scenario.length_s,
                    "headline": case.headline(),
                }))
            })
            .collect(),
    )
}

impl Case {
    /// Parse and validate the case against a site.
    pub fn load(&self, cfg: &PlantConfig) -> Result<Scenario, LoadError> {
        load(self.yaml.as_bytes(), cfg)
    }

    /// What the case demonstrates: its leading comment block, which the
    /// library contract requires, joined into one line.
    pub fn headline(&self) -> String {
        self.yaml
            .lines()
            .map_while(|line| line.strip_prefix('#'))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headline_is_the_header_comment_on_one_line() {
        let case = find("hvac-failure-both-units").expect("in the library");
        let headline = case.headline();
        assert!(headline.starts_with("Both HVAC units of block 2 fail at once"));
        assert!(headline.ends_with("cools back into its window."));
        assert!(!headline.contains('#') && !headline.contains('\n'));
        assert_eq!(find("no-such-case"), None);
    }

    #[test]
    fn the_catalogue_lists_every_case_in_order_with_four_fields() {
        let catalogue = catalogue(&PlantConfig::gw01());
        let cases = catalogue.as_array().expect("an array");
        assert_eq!(cases.len(), CASES.len());
        assert_eq!(
            cases[1],
            json!({
                "name": "hvac-failure-both-units",
                "taxonomy": "bos",
                "length_s": 108_000,
                "headline": find("hvac-failure-both-units").unwrap().headline(),
            })
        );
    }
}
