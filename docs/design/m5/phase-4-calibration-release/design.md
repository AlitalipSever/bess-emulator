# M5 phase 4: calibration and release, design

Each decision is proposed here and confirmed or revised in its PR.

## Decisions

- **D1, the field band is derived, not read off.** No public multi-year
  field record of a utility-scale LFP plant's capacity was found with a
  licence and a number (the field-data surveys list grid-scale releases
  of NMC, lead-acid and second-life packs, and the operators' own fleet
  studies are not public). The closest public measurement over years is
  Figgener et al. 2024 on LFP home storage: 2.2 percentage points a year
  at about 200 cycles, 0.7 per 100 cycles, a 2-point spread. Scaling
  the cycle share to GW-01's 280 cycles and keeping the spread gives 1.5
  to 3.5 points a year; five years of it is a state of health between
  0.825 and 0.925, rounded outward to 0.82 and 0.93. Named a field band
  because a home system is not a container plant, and the band's width
  says so.
- **D2, the laboratory band is Preger's range.** 2,500 to 9,000 cycles
  to 80 % across LFP cells at 15 to 35 °C, 0.5C to 3C and three
  depth windows; the two-cycle-a-day trajectory at 25 °C and 0.9 depth is
  the condition inside that matrix closest to a plant's, and its
  crossing has to fall inside.
- **D3, the datasheet claim is reported beside, not gated.** The EVE
  MB31's 8,000 cycles to 70 % is a test over about two years at 0.5P;
  the model's 8,000-cycle figure includes the calendar of the eleven
  years a plant takes to make them and is expected lower; CALIBRATION.md
  prints both.
- **D4, the M1 band is the fourth check**, unchanged: the aging year's
  round trip stays inside it (the resistance growth costs a fraction of
  a point).
- **D5, version 0.10.0.** Pre-1.0, the minor moves with the milestone as
  it has since M0; the map is 0.7.0 and the checkpoint 20.

## Test plan

- `bands.rs`: each band refuses a value outside it and accepts the
  record's.
- `check_aging` reads the committed file and passes in CI.
- The release PR regenerates every record and the digests, and the
  library records move in their version string only.
