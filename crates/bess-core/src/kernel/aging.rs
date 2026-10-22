//! The daily aging pass: fill every rack's ledger with the tick, and hand
//! a rack whose day closed to the aging model (M5 phase 1).
//!
//! The stored energy a capacity loss writes off is booked to the energy
//! accounting's `aging_wh` row, so the site balance keeps closing: a rack
//! that holds less at the same state of charge did not leak it through
//! its terminals, and the waterfall says where it went.

use crate::config::PlantConfig;
use crate::state::aging::StressLedger;
use crate::state::SiteState;
use crate::traits::Models;

/// One tick of every rack's ledger, and the daily pass on the racks whose
/// ledger is full. Returns how many racks aged this tick.
pub(super) fn observe_and_age(
    models: &Models,
    cfg: &PlantConfig,
    state: &mut SiteState,
    dt_s: f64,
) -> usize {
    let nominal_ah = cfg.rack.cell_capacity_ah;
    let mut aged = 0;
    let mut written_off_wh = 0.0;
    for rack in state
        .blocks
        .iter_mut()
        .flat_map(|b| b.containers.iter_mut())
        .flat_map(|c| c.racks.iter_mut())
    {
        rack.age
            .ledger
            .observe(rack.soc, rack.cell_temp_c, rack.current_a, nominal_ah, dt_s);
        if !rack.age.ledger.is_full() {
            continue;
        }
        let day = rack.age.ledger.close(nominal_ah);
        let before_wh = models.cell.stored_energy_wh(rack, &cfg.rack);
        models.aging.age_rack(&mut rack.age, &cfg.rack, &day);
        rack.age.days += day.days;
        rack.age.efc += day.efc;
        rack.soh = rack.age.soh();
        written_off_wh += before_wh - models.cell.stored_energy_wh(rack, &cfg.rack);
        rack.age.ledger = StressLedger::open(rack.soc);
        rack.age.last_day = Some(day);
        aged += 1;
    }
    state.energy.aging_wh += written_off_wh;
    aged
}
