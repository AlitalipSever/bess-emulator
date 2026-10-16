//! Point selectors: which points a data fault corrupts (phase 5 design D2).
//!
//! `*` is the whole surface. Otherwise a selector is a tree path, spelled
//! as targets are, naming the part of the plant whose points are affected:
//! `site`, `block[N]`, `block[N].pcs`. The surfaces publish nothing finer
//! than a block's PCS and container, so a container or a rack is refused
//! by name. The shell turns a selector into the published prefix
//! (`block02.pcs.`); this crate only says what was selected.

use std::fmt;

use bess_core::PlantConfig;
use serde::{Serialize, Serializer};

use super::target::{Target, TargetError};

/// What a data fault's `points` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selector {
    /// Every point of the surface.
    All,
    /// The site's own points.
    Site,
    /// Every point of one block.
    Block {
        /// Block index.
        block: usize,
    },
    /// One block's PCS points.
    Pcs {
        /// Block index.
        block: usize,
    },
}

impl Selector {
    /// Resolve a selector against a site.
    pub fn parse(text: &str, cfg: &PlantConfig) -> Result<Self, TargetError> {
        if text == "*" {
            return Ok(Self::All);
        }
        match Target::parse(text, cfg)? {
            Target::Site => Ok(Self::Site),
            Target::Block { block } => Ok(Self::Block { block }),
            Target::Pcs { block } => Ok(Self::Pcs { block }),
            finer => Err(TargetError {
                at: text.to_owned(),
                expected: format!(
                    "points are selected with `*`, `site`, `block[N]` or `block[N].pcs`; \
                     the surfaces publish nothing per {}",
                    finer.kind()
                ),
            }),
        }
    }
}

/// As the file writes it.
impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::All => f.write_str("*"),
            Self::Site => Target::Site.fmt(f),
            Self::Block { block } => Target::Block { block }.fmt(f),
            Self::Pcs { block } => Target::Pcs { block }.fmt(f),
        }
    }
}

impl Serialize for Selector {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use bess_core::PlantConfig;

    use super::Selector;

    fn parse(text: &str) -> Result<Selector, String> {
        Selector::parse(text, &PlantConfig::gw01()).map_err(|e| e.to_string())
    }

    #[test]
    fn the_four_forms_resolve_and_spell_themselves_back() {
        for (text, selector) in [
            ("*", Selector::All),
            ("site", Selector::Site),
            ("block[19]", Selector::Block { block: 19 }),
            ("block[2].pcs", Selector::Pcs { block: 2 }),
        ] {
            assert_eq!(parse(text), Ok(selector));
            assert_eq!(selector.to_string(), text);
        }
    }

    #[test]
    fn finer_paths_and_bad_paths_are_refused() {
        assert_eq!(
            parse("block[2].container[0].hvac"),
            Err(
                "block[2].container[0].hvac: points are selected with `*`, `site`, `block[N]` \
                 or `block[N].pcs`; the surfaces publish nothing per hvac"
                    .to_owned()
            )
        );
        assert_eq!(
            parse("block[25]"),
            Err("block[25]: this site has block[0] to block[19]".to_owned())
        );
        assert_eq!(
            parse("block02.pcs"),
            Err("block02: a path starts at `site` or `block[N]`, N from 0 to 19".to_owned())
        );
    }
}
