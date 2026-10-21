//! Target paths: state-tree addresses, checked against the site.
//!
//! `site`, `block[N]`, `block[N].pcs`, `block[N].container[C].hvac` and
//! `block[N].container[C].rack[R]`, zero-based and unpadded as the tree and
//! `AlarmNode` count (design D2). This is not the surfaces' flattened point
//! spelling (`block02.pcs.p_ac_kw`); a refusal says what the site has at
//! the level that failed, which is what keeps the two from becoming a trap.

use std::fmt;

use bess_core::alarms::ResetScope;
use bess_core::kernel::FaultAction;
use bess_core::PlantConfig;

/// A node a physical fault acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// The site.
    Site,
    /// The site's substation: its switchgear (M3 phase 4).
    Substation,
    /// A block.
    Block {
        /// Block index.
        block: usize,
    },
    /// A block's PCS.
    Pcs {
        /// Block index.
        block: usize,
    },
    /// A container's HVAC unit.
    Hvac {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
    },
    /// A rack.
    Rack {
        /// Block index.
        block: usize,
        /// Container index within the block.
        container: usize,
        /// Rack index within the container.
        rack: usize,
    },
}

/// A path that does not resolve: the prefix that failed and what the site
/// has there.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{at}: {expected}")]
pub struct TargetError {
    /// The path up to and including the part at fault.
    pub at: String,
    /// What the site offers at that level.
    pub expected: String,
}

impl Target {
    /// Resolve a path against a site.
    pub fn parse(path: &str, cfg: &PlantConfig) -> Result<Self, TargetError> {
        let fail = |upto: usize, expected: String| {
            let parts: Vec<&str> = path.split('.').take(upto + 1).collect();
            Err(TargetError {
                at: parts.join("."),
                expected,
            })
        };
        let parts: Vec<&str> = path.split('.').collect();
        let roots = format!(
            "a path starts at `site` or `block[N]`, N from 0 to {}",
            cfg.blocks - 1
        );
        let block_has = format!(
            "a block has `pcs` and `container[0]` to `container[{}]`",
            cfg.containers_per_block - 1
        );
        let container_has = format!(
            "a container has `hvac` and `rack[0]` to `rack[{}]`",
            cfg.racks_per_container - 1
        );

        if parts == ["site"] {
            return Ok(Self::Site);
        }
        if parts == ["site", "substation"] {
            return Ok(Self::Substation);
        }
        let block = match indexed(parts[0], "block") {
            Some(Ok(b)) if b < cfg.blocks => b,
            Some(Ok(_)) => {
                return fail(
                    0,
                    format!("this site has block[0] to block[{}]", cfg.blocks - 1),
                )
            }
            Some(Err(why)) => return fail(0, why),
            None => return fail(0, roots),
        };
        let Some(&second) = parts.get(1) else {
            return Ok(Self::Block { block });
        };
        if second == "pcs" {
            return match parts.len() {
                2 => Ok(Self::Pcs { block }),
                _ => fail(2, "a PCS has no parts a scenario addresses".to_owned()),
            };
        }
        let container = match indexed(second, "container") {
            Some(Ok(c)) if c < cfg.containers_per_block => c,
            Some(Err(why)) => return fail(1, why),
            Some(Ok(_)) | None => return fail(1, block_has),
        };
        let Some(&third) = parts.get(2) else {
            return fail(
                1,
                "a container is not a target; name its `hvac` or a `rack[R]`".to_owned(),
            );
        };
        let target = if third == "hvac" {
            Self::Hvac { block, container }
        } else {
            match indexed(third, "rack") {
                Some(Ok(rack)) if rack < cfg.racks_per_container => Self::Rack {
                    block,
                    container,
                    rack,
                },
                Some(Err(why)) => return fail(2, why),
                Some(Ok(_)) | None => return fail(2, container_has),
            }
        };
        if parts.len() > 3 {
            return fail(
                3,
                format!("{} has no parts a scenario addresses", kind_name(target)),
            );
        }
        Ok(target)
    }

    /// The kind's name, as the (kind, fault) table spells it.
    pub fn kind(&self) -> &'static str {
        kind_name(*self)
    }

    /// The reset scope an `alarm_reset` on this target reaches, for the
    /// kinds that have an alarm word.
    pub fn reset_scope(&self) -> Option<ResetScope> {
        match *self {
            Self::Site => Some(ResetScope::Site),
            Self::Block { block } => Some(ResetScope::Block(block)),
            Self::Rack {
                block,
                container,
                rack,
            } => Some(ResetScope::Rack {
                block,
                container,
                rack,
            }),
            Self::Pcs { .. } | Self::Hvac { .. } | Self::Substation => None,
        }
    }

    /// The node an action acts on, so a resolved schedule can spell its
    /// targets the way the file did.
    pub fn of(action: &FaultAction) -> Self {
        match *action {
            FaultAction::HvacFailure { block, container }
            | FaultAction::HvacRepair { block, container } => Self::Hvac { block, container },
            FaultAction::PcsTrip { block } => Self::Pcs { block },
            FaultAction::ProtectionTrip | FaultAction::AlarmReset(ResetScope::Site) => Self::Site,
            FaultAction::HvBreakerOpen
            | FaultAction::HvBreakerClose
            | FaultAction::Disconnectors { .. }
            | FaultAction::EarthingSwitch { .. } => Self::Substation,
            FaultAction::BlockMaintenanceEnter { block }
            | FaultAction::BlockMaintenanceExit { block }
            | FaultAction::AlarmReset(ResetScope::Block(block)) => Self::Block { block },
            FaultAction::RackSelfDischarge {
                block,
                container,
                rack,
                ..
            }
            | FaultAction::RackIsolate {
                block,
                container,
                rack,
            }
            | FaultAction::RackRestore {
                block,
                container,
                rack,
            }
            | FaultAction::AlarmReset(ResetScope::Rack {
                block,
                container,
                rack,
            }) => Self::Rack {
                block,
                container,
                rack,
            },
        }
    }
}

/// The path as a file writes it: what `parse` reads back.
impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Site => f.write_str("site"),
            Self::Substation => f.write_str("site.substation"),
            Self::Block { block } => write!(f, "block[{block}]"),
            Self::Pcs { block } => write!(f, "block[{block}].pcs"),
            Self::Hvac { block, container } => {
                write!(f, "block[{block}].container[{container}].hvac")
            }
            Self::Rack {
                block,
                container,
                rack,
            } => write!(f, "block[{block}].container[{container}].rack[{rack}]"),
        }
    }
}

fn kind_name(target: Target) -> &'static str {
    match target {
        Target::Substation => "substation",
        Target::Site => "site",
        Target::Block { .. } => "block",
        Target::Pcs { .. } => "pcs",
        Target::Hvac { .. } => "hvac",
        Target::Rack { .. } => "rack",
    }
}

/// `name[N]` to `N`: `None` when the part is not `name[...]` at all,
/// `Some(Err)` when the index is not a plain unpadded number.
fn indexed(part: &str, name: &str) -> Option<Result<usize, String>> {
    let inner = part
        .strip_prefix(name)?
        .strip_prefix('[')?
        .strip_suffix(']')?;
    let plain = !inner.is_empty()
        && inner.bytes().all(|b| b.is_ascii_digit())
        && (inner == "0" || !inner.starts_with('0'));
    Some(if plain {
        inner
            .parse()
            .map_err(|_| format!("{inner} is not an index"))
    } else {
        Err(format!(
            "indices are plain zero-based numbers, as in {name}[2]"
        ))
    })
}

#[cfg(test)]
mod tests;
