//! The alarm journal: the events the viewer has seen, newest first, in a
//! bounded ring.
//!
//! View state only. The kernel keeps two numbers about its log (count and
//! digest) and hands each tick's events to whoever stepped it; the viewer
//! keeps the last few thousand so a person can read back what happened.
//! Each entry carries the kernel's own number (`seq`, from 1) and the
//! simulation time of the tick that shows its effect (`ts`), the same pair
//! MQTT and the stream publish, so a row here can be matched against a
//! message there. Cleared whenever the plant is replaced, because the
//! numbering starts over with it.

use std::collections::{HashMap, VecDeque};

use bess_core::alarms::AlarmNode;
use bess_core::kernel::Event;

/// Entries the ring keeps. A clean replayed day emits a few hundred events
/// and the longest library case a few thousand; the native shell sizes its
/// event backlog on the same figure.
pub const CAPACITY: usize = 4_096;

/// One event as the journal keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The kernel's number for it, from 1: the count of the event log once
    /// it was folded in.
    pub seq: u64,
    /// Simulation time of the tick that shows its effect, unix seconds.
    pub unix_time_s: i64,
    /// What happened.
    pub event: Event,
}

/// The bounded ring, and when each bit still set was raised.
#[derive(Debug, Clone, Default)]
pub struct Journal {
    entries: VecDeque<Entry>,
    /// Raise time of every bit whose last edge the journal saw was a raise.
    /// Kept apart from the ring so that a raise the ring has since dropped
    /// still dates its row in the summary.
    raised: HashMap<(AlarmNode, u8), i64>,
}

impl Journal {
    /// An empty journal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one tick's events. `log_count` is the kernel's event count
    /// after the tick (`SiteState::event_log.count`), `unix_time_s` the
    /// simulation time after it.
    pub fn record(&mut self, events: &[Event], log_count: u64, unix_time_s: i64) {
        let first = (log_count + 1).saturating_sub(events.len() as u64);
        for (seq, event) in (first..).zip(events) {
            match *event {
                Event::AlarmRaised { node, bit, .. } => {
                    self.raised.insert((node, bit), unix_time_s);
                }
                Event::AlarmCleared { node, bit, .. } => {
                    self.raised.remove(&(node, bit));
                }
                Event::PcsStateChanged { .. }
                | Event::FaultApplied(_)
                | Event::SocResnapped { .. } => {}
            }
            if self.entries.len() == CAPACITY {
                self.entries.pop_front();
            }
            self.entries.push_back(Entry {
                seq,
                unix_time_s,
                event: *event,
            });
        }
    }

    /// Forget everything: the plant was replaced.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.raised.clear();
    }

    /// Entries held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry `i` places from the newest (0 is the newest).
    pub fn newest(&self, i: usize) -> Option<&Entry> {
        self.entries
            .len()
            .checked_sub(i + 1)
            .and_then(|j| self.entries.get(j))
    }

    /// Every entry, newest first.
    pub fn iter_newest_first(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev()
    }

    /// Events the ring has dropped: the oldest entry's number says how many
    /// came before it.
    pub fn dropped(&self) -> u64 {
        self.entries.front().map_or(0, |e| e.seq - 1)
    }

    /// When a bit that is set was raised, if the journal saw the raise.
    pub fn raised_at(&self, node: AlarmNode, bit: u8) -> Option<i64> {
        self.raised.get(&(node, bit)).copied()
    }
}

/// How far a person has read: the event count when they last had the alarm
/// section open. The count is the kernel's (`event_log.count`), so this is
/// no second tally, only a bookmark in the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeenMark {
    count: u64,
}

impl SeenMark {
    /// Events since the mark. A restart starts the kernel's count over;
    /// the viewer resets the mark with it.
    pub fn unseen(self, log_count: u64) -> u64 {
        log_count.saturating_sub(self.count)
    }

    /// Move the mark to now.
    pub fn mark(&mut self, log_count: u64) {
        self.count = log_count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bess_core::alarms::Severity;
    use bess_core::kernel::FaultAction;

    const NODE: AlarmNode = AlarmNode::Block { block: 2 };

    fn raise(bit: u8) -> Event {
        Event::AlarmRaised {
            node: NODE,
            bit,
            severity: Severity::of_bit(bit),
        }
    }

    fn clear(bit: u8) -> Event {
        Event::AlarmCleared {
            node: NODE,
            bit,
            severity: Severity::of_bit(bit),
        }
    }

    #[test]
    fn entries_carry_the_kernels_numbering_newest_first() {
        let mut j = Journal::new();
        // Tick one: two events, the log now counts 2.
        j.record(
            &[
                Event::FaultApplied(FaultAction::PcsTrip { block: 2 }),
                raise(8),
            ],
            2,
            100,
        );
        // Tick two: one more.
        j.record(&[clear(8)], 3, 101);
        let seqs: Vec<(u64, i64)> = j
            .iter_newest_first()
            .map(|e| (e.seq, e.unix_time_s))
            .collect();
        assert_eq!(seqs, vec![(3, 101), (2, 100), (1, 100)]);
        assert_eq!(j.newest(0).map(|e| e.event), Some(clear(8)));
        assert_eq!(j.newest(2).map(|e| e.seq), Some(1));
        assert_eq!(j.newest(3), None);
        assert_eq!(j.dropped(), 0);
    }

    #[test]
    fn past_capacity_the_oldest_go_and_the_numbering_says_how_many() {
        let mut j = Journal::new();
        let total = CAPACITY as u64 + 10;
        for n in 1..=total {
            let event = if n % 2 == 1 { raise(0) } else { clear(0) };
            j.record(&[event], n, n as i64);
        }
        assert_eq!(j.len(), CAPACITY);
        assert_eq!(j.dropped(), 10);
        assert_eq!(j.newest(0).map(|e| e.seq), Some(total));
        assert_eq!(j.newest(CAPACITY - 1).map(|e| e.seq), Some(11));
        // Consecutive entries differ by one.
        let seqs: Vec<u64> = j.iter_newest_first().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] == w[1] + 1));
    }

    #[test]
    fn a_set_bit_is_dated_by_its_last_raise_and_forgotten_on_clear() {
        let mut j = Journal::new();
        j.record(&[raise(0)], 1, 50);
        assert_eq!(j.raised_at(NODE, 0), Some(50));
        assert_eq!(j.raised_at(NODE, 1), None);
        j.record(&[clear(0)], 2, 60);
        assert_eq!(j.raised_at(NODE, 0), None);
        j.record(&[raise(0)], 3, 70);
        assert_eq!(j.raised_at(NODE, 0), Some(70));
        j.clear();
        assert!(j.is_empty());
        assert_eq!(j.raised_at(NODE, 0), None);
    }

    #[test]
    fn the_unseen_count_is_a_bookmark_in_the_kernels_count() {
        let mut seen = SeenMark::default();
        assert_eq!(seen.unseen(12), 12);
        seen.mark(12);
        assert_eq!(seen.unseen(12), 0);
        assert_eq!(seen.unseen(15), 3);
        // A restarted plant counts from zero again; nothing goes negative.
        assert_eq!(seen.unseen(0), 0);
    }
}
