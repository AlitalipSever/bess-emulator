# M2 phase 1: BMS deepening, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, derating thresholds from one named datasheet.** The temperature
  window and taper bands come from a single public 314 Ah class LFP cell
  datasheet, the same sourcing pattern as the STULZ WXUC5 for the HVAC.
  Typical LFP values (charge 0 to 55 C, discharge minus 20 to 55 C, taper
  knees around 10 C and 45 C) are the expectation, but the pinned numbers
  are whatever the chosen datasheet says; candidates are the EVE and
  Hithium 314 Ah cells, falling back to the EVE LF280K if no 314 Ah sheet
  is publicly retrievable. Recorded in CALIBRATION.md sources with a
  retrieval date. Rationale: GW-01's cells are declared as 314 Ah class in
  ARCHITECTURE.md; the derating story should come from the cell the plant
  claims to have.
  **Confirmed in PR1:** EVE MB31, PBRI-MB31-D06-01 rev A, Tables 5
  (charging) and 7 (discharging), linear between listed points. The
  expectation above was wrong on the warm side: the table holds full power
  to 55 C, not 45 C. The table's level (0.5P) is not carried over, only its
  shape, because GW-01 racks are rated 1C; that mismatch is recorded as an
  open question in the milestone README.
- **D2, derating multiplies the SoC taper.** The temperature factor and the
  existing SoC taper combine by multiplication, not minimum. Rationale:
  both mechanisms are real and independent (a cold rack at high SoC charges
  worse than either alone suggests); multiplication is also what keeps the
  factor differentiable across the band edges, which the property tests
  lean on. The alternative (min of factors) matches some vendor manuals but
  makes the interaction invisible; revisit if a public BMS manual pins the
  combination rule.
- **D3, one scalar spread per rack.** `cell_dv_v` follows
  `d(spread)/dt = alpha * |I| / Q - bleed effect when balancing`, with the
  growth coefficient sized from published manufacturing spread statistics
  (capacity and resistance sigma of 0.5 to 1 percent in public cell
  characterization studies) and the bleed effect from a typical passive
  balancing current of 100 to 200 mA. Both numbers are pinned in the PR
  from named studies; the dossier behind this phase lists candidates.
  Rationale: SCADA publishes min/max, so a min/max is what we model, per
  "model to the interface". Per-cell state (200k cells at GW-01) buys
  nothing observable and is a non-goal.
  **Revised in PR2.** The formula above is a SoC-spread law (|I|/Q is a
  SoC rate), so the state became `cell_dsoc` and `cell_dv_v` its OCV
  reading (architecture.md, New state). Growth has two terms, not one:
  coulombic-efficiency mismatch per unit throughput (100 ppm, an
  estimate between the 10 ppm measurable and Nuvation's deliberately high
  830 ppm) and self-discharge mismatch per unit time (EVE MB31's
  3 %/month at Nuvation's 10 % spread, about 6 sigma across 416 cells:
  1.8 %/month). Manufacturing capacity sigma, the number this decision
  first reached for, sets where cells start, not how fast they drift, so
  it does not enter the law. Bleed: 240 mA per cell (TI BQ79616; Nuvation
  about 250 mA), half the string bleeding (estimate). Sources in
  CALIBRATION.md.
- **D4, balancing policy.** Bleed runs when the rack is near the top of the
  window and the spread exceeds a threshold, and while the rack is idle or
  charging; discharge interrupts it. This mirrors the top-balancing
  behavior of real passive BMS designs (bleed the high cells at the
  charge-voltage shoulder). The policy lives in `BasicBms` parameters, not
  in the trait.
  **Confirmed in PR2:** bleed at SoC 0.90 and up (Orion: a cell within 5
  to 10 % of full), on at 30 mV and off at 20 mV (a published LFP BMS
  parameter sheet), interrupted above 1 A of discharge. The thresholds
  read the voltage spread, as a real BMS does; at SoC 0.95 our OCV curve
  turns 30 mV into about 2.7 % SoC.
- **D5, `BmsFlows` is the accountability surface.** Like `ThermalFlows` in
  M1: the step reports bleed power and heat so the invariant tests hold the
  interface, not one implementation.
- **D6, checkpoint format 4.** Three new rack fields (`cell_dsoc`,
  `cell_dv_v`, `balancing_active`) change the schema. Old files are rejected by version,
  release notes say so, no migration pre-1.0.

## Signal map impact

None in this phase. The spread and the derate status become visible in
phase 2's map delta (new block address range), so the map moves once, not
twice. Until then the new quantities are state, Parquet, and Prometheus
material only; Prometheus gains `bess_rack_cell_dv_volts` min/max and a
balancing count gauge, which is hand-written exposition per the
architecture's Prometheus rule and needs no map version.

## Checkpoint impact

Format 3 to 4 (D6), in the PR that adds the fields.

## Test plan

- **Property: energy closes over balancing.** Stored energy delta equals
  cell flows minus bleed; bleed equals heat delivered to the rack thermal
  node, within tolerance, for arbitrary dispatch.
- **Property: derate monotonicity.** For fixed SoC, limits are
  non-increasing as temperature leaves the comfort band in either
  direction; factors stay in [0, 1]; charge factor reaches zero at the cold
  limit while discharge is still positive.
- **Property: spread dynamics.** Never negative, widens under throughput
  with balancing off, narrows under balancing no faster than the bleed
  current explains.
- **Unit: threshold fidelity.** The pinned datasheet numbers appear in the
  model's parameter set verbatim (a CI test against constants, the M0.5
  pattern).
- **Golden snapshot:** regenerated once, in the PR that adds the rack
  fields, called out there.
- **Annual run:** derating will move the annual record slightly (hot weeks
  lose some throughput). The committed record is regenerated in this phase
  and the delta explained in the PR; the round-trip band must still hold.
