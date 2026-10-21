# M4 phase 2: external control and reporting, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the holding bank follows SunSpec's immediate-controls shape**
  (model 123 / DER model 704): a percent limit with an enable and a
  revert time, a reactive setpoint with a mode selector, a power
  factor. SunSpec is the one public register model every integrator has
  met; copying its names where the semantics match (`WMaxLimPct`,
  `OutPFSet`, `VArWMaxPct`) costs nothing and saves a mapping. The
  addresses are ours (the bank's next free words), the names on the map
  are ours (`control.p_limit_pct`, `control.p_limit_enable`,
  `control.p_limit_revert_s`, `control.q_mode`, `control.q_setpoint_kvar`,
  `control.cos_phi_x1000`, `control.command`), with the SunSpec name in the CSV's description column. *As built (PR1):*
  holding registers 3 to 11 as listed, `HOLDING_BANK_LEN` 12; the
  reactive registers write the operator's settings (`EmsState::
  reactive_cos_phi`, `reactive_under_excited`, `reactive_q_var`) and the
  mode selector builds the `ReactiveMode` from them, so every register
  reads back whichever mode is in force. The CSV has no description
  column; the SunSpec names are in `control.rs`'s module comment.
- **D2, a command register for the one-shot commands.** `control.command`
  (write-only in effect, reads zero): 1 site reset, 2 breaker open, 3
  breaker close, 4 disconnectors open, 5 close, 6 earthing open, 7 close,
  8 clear limit. A refused command leaves the plant unchanged and raises
  the next event-counter value with a `command_refused` event naming
  the interlock, so a Modbus client that cannot see REST's error body
  still sees the refusal. *As built (PR1):* `ControlCommand` in
  `kernel/limit.rs` with the eight codes, `Simulation::control_command`
  shared by Modbus, REST and tests; the event carries the register
  value, not the interlock's name (`Event` is `Copy + Eq` and
  deserializable, which a `&'static str` is not); the shell's payload
  adds the command's name. The REST routes answer 409 on an interlock
  and 400 on a value out of range.
- **D3, the gradient is the follower's.** The TSOs' May 2026 operational
  requirements for EHV-connected batteries put third-party power
  changes at 6 to 20 %/min of installed power; the M3 follower's 10 %/min
  sits inside. No second gradient: the plant has one way of moving.
  *As built (PR1):* as proposed; `apply_external_limit` caps the site
  target after the EMS and before the share, and `limit_binding` is the
  flag's source.
- **D4, the KPIs are the TSOs' two figures.** The same document binds
  the quarter-hour mean of feed-in or withdrawal to the schedule
  within a frame the operator sets, and asks for the state of charge
  and the full technical redispatch potential in the data exchange.
  The plant publishes the quarter-hour deviation (`site.kpi.schedule_
  deviation_kw`, signed, and `_pct`), the availability
  (`site.kpi.availability_pct`, discharge and charge), the tracking RMS
  (`site.kpi.tracking_rms_kw`), the period count, and the redispatch
  potential as the energy the plant could still move in each direction
  (`site.kpi.redispatch_up_mwh`, `_down_mwh`, from the SoC window and
  the available power). A `schedule_deviation` site bit (6, warning)
  raises when the last period's deviation exceeds a configured frame
  (default 5 % of rated, labeled; the document leaves the frame to the
  operator). *As built (PR2):* `KpiMeter` in `kernel/kpi.rs` on
  `SiteState::kpi`, booked every tick and closed on the fiscal meter's
  boundary; the scheduled mean is the site target after the external
  limit, so a written limit is schedule, not deviation; the redispatch
  potential is the racks' energy above the window's floor and below its
  ceiling; the frame is `BlockSiteThresholds::schedule_frame` (0.05).
  Points 63 to 71 (the deviation in kW and %, the two availabilities,
  the tracking RMS, the two potentials); site bit 6.
- **D5, checkpoint bump.** Format 18: the limit and its timer, the
  KPI accumulators and the last closed period. *As built (PR1):* format
  18 with `EmsState::external_limit`, `limit_binding` and the three
  reactive settings; PR2 adds the KPIs to the same format.
- **D6, points.** Holding registers 3 to 9 as named in D1 and D2; site
  inputs 62 to 69 for the KPIs; the two site bits. *As built:* holding
  registers 3 to 11 (the reactive setpoint is a pair and the direction
  of cos φ its own word), site inputs 63 to 71 (62 is the FCR output's
  second word), bits 5 and 6.

## Signal map impact

Seven holding registers, eight site inputs, two site bits, one event
kind (`command_refused`). Map 0.6.0.

## Checkpoint impact

One format bump (D5).

## Test plan

- **Unit: the limit** binds both directions, reverts on its timer,
  raises and clears the flag, and composes with the LFSM cut (the lower
  of the two wins).
- **Integration: Modbus end to end** (the closed-loop test extended):
  every holding register written over TCP, read back, and the plant's
  state checked; a refused command's event on the stream. *As built
  (PR1):* `every_control_register_is_written_end_to_end` in
  `modbus/tests.rs`; the plant tests in
  `bess-models/tests/external_control.rs`.
- **Unit: the KPIs** on a hand-built quarter-hour: scheduled mean,
  metered mean, deviation, availability with one block in maintenance.
  *As built (PR2):* `kpi.rs`'s unit tests on a hand-built period (the
  means, the frame verdict, a ramp's RMS) and
  `the_quarter_hour_report_follows_the_fiscal_meter` in the plant.
- **Integration:** a new library case `curtailment-order` (a 60 % limit
  written at 18:10 for 40 min during the evening discharge; the flag,
  the ramp down at 10 %/min, the revert, the ramp back); the
  `schedule_deviation` bit on the staggered-return case. *As built
  (PR1):* `curtailment-order` (taxonomy `controls`, `site.ems` with
  `limit` and a `revert` duration) and its lesson test; the
  `schedule_deviation` bit is PR2's.
