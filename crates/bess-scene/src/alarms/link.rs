//! Between a row and the scene: which object holds a node, and which
//! alarms an object holds.
//!
//! A rack lives in a container, a block word belongs to the PCS skid (the
//! converter is what the block word mostly speaks for), and the site word
//! to the transformer. An HVAC fault is the container's, a protection trip
//! the transformer's.

use bess_core::alarms::{has_bit, AlarmNode, ResetScope};
use bess_core::kernel::{Event, FaultAction};
use bess_core::state::SiteState;

use crate::layout::Selection;

use super::ActiveAlarm;

/// The scene object that holds a node.
pub fn object_of(node: AlarmNode) -> Selection {
    match node {
        AlarmNode::Site => Selection::Transformer,
        AlarmNode::Block { block } => Selection::Pcs { block },
        AlarmNode::Rack {
            block, container, ..
        } => Selection::Container { block, container },
    }
}

/// The scene object an event happened on.
pub fn object_of_event(event: &Event) -> Selection {
    match *event {
        Event::AlarmRaised { node, .. } | Event::AlarmCleared { node, .. } => object_of(node),
        Event::PcsStateChanged { block, .. } => Selection::Pcs { block },
        Event::SocResnapped {
            block, container, ..
        } => Selection::Container { block, container },
        Event::FaultApplied(action) => match action {
            FaultAction::HvacFailure { block, container }
            | FaultAction::HvacRepair { block, container }
            | FaultAction::RackSelfDischarge {
                block, container, ..
            }
            | FaultAction::RackIsolate {
                block, container, ..
            }
            | FaultAction::RackRestore {
                block, container, ..
            }
            | FaultAction::AlarmReset(ResetScope::Rack {
                block, container, ..
            }) => Selection::Container { block, container },
            FaultAction::PcsTrip { block }
            | FaultAction::BlockMaintenanceEnter { block }
            | FaultAction::BlockMaintenanceExit { block }
            | FaultAction::AlarmReset(ResetScope::Block(block)) => Selection::Pcs { block },
            FaultAction::ProtectionTrip | FaultAction::AlarmReset(ResetScope::Site) => {
                Selection::Transformer
            }
        },
    }
}

/// The short names of a word's set bits, `derate_active, soc_low`: what a
/// detail row says about its node, the word prefix being the row itself.
pub fn short_names(node: AlarmNode, word: u32) -> String {
    (0..32u8)
        .filter(|&bit| has_bit(word, bit))
        .map(|bit| {
            let name = ActiveAlarm { node, bit }.name();
            name.split_once('.')
                .map_or(name.clone(), |(_, n)| n.to_owned())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The word of the node an object holds directly: the PCS skid's block
/// word, the transformer's site word. A container holds rack words, one
/// per row of its detail table, so it has no single word.
pub fn word_of(state: &SiteState, sel: Selection) -> Option<(AlarmNode, u32)> {
    match sel {
        Selection::Container { .. } => None,
        Selection::Pcs { block } => state
            .blocks
            .get(block)
            .map(|b| (AlarmNode::Block { block }, b.alarm_bits)),
        Selection::Transformer => Some((AlarmNode::Site, state.alarm_bits)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::alarms::layout::{block, rack};
    use bess_core::PlantConfig;

    #[test]
    fn every_node_and_event_lands_on_an_object() {
        let rk = AlarmNode::Rack {
            block: 2,
            container: 1,
            rack: 5,
        };
        assert_eq!(
            object_of(rk),
            Selection::Container {
                block: 2,
                container: 1
            }
        );
        assert_eq!(
            object_of(AlarmNode::Block { block: 7 }),
            Selection::Pcs { block: 7 }
        );
        assert_eq!(object_of(AlarmNode::Site), Selection::Transformer);

        let hvac = Event::FaultApplied(FaultAction::HvacFailure {
            block: 3,
            container: 0,
        });
        assert_eq!(
            object_of_event(&hvac),
            Selection::Container {
                block: 3,
                container: 0
            }
        );
        let trip = Event::FaultApplied(FaultAction::ProtectionTrip);
        assert_eq!(object_of_event(&trip), Selection::Transformer);
        let reset = Event::FaultApplied(FaultAction::AlarmReset(ResetScope::Block(4)));
        assert_eq!(object_of_event(&reset), Selection::Pcs { block: 4 });
    }

    #[test]
    fn an_object_lists_the_short_names_of_its_word() {
        let mut state = SiteState::new(&PlantConfig::gw01(), 7, 1_767_225_600);
        state.blocks[7].alarm_bits = (1 << block::HVAC_FAILURE) | (1 << block::PCS_FAULT);
        let (node, word) = word_of(&state, Selection::Pcs { block: 7 }).expect("a word");
        assert_eq!(short_names(node, word), "hvac_failure, pcs_fault");
        let rk = AlarmNode::Rack {
            block: 0,
            container: 0,
            rack: 0,
        };
        assert_eq!(short_names(rk, 1 << rack::SOC_LOW), "soc_low");
        assert_eq!(short_names(rk, 0), "");
        assert_eq!(
            word_of(
                &state,
                Selection::Container {
                    block: 0,
                    container: 0
                }
            ),
            None
        );
    }
}
