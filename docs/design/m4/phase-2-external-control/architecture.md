# M4 phase 2: external control and reporting, architecture

What exists after this phase: a control surface a dispatch application
can drive end to end, and telemetry that says how well the plant did
what it was told. The holding bank grows from two registers to a bank:
site setpoint, mode, active-power limit (percent of rated, with an
enable and a revert timer, the SunSpec immediate-controls shape),
reactive mode and its setpoints, and a command register for the
switchgear and resets that REST already carries. A written limit is
obeyed at the third-party gradient and reported as an external
limitation. Each closed quarter-hour the plant publishes its schedule
deviation and its availability.

## The model

- **The limit.** `EmsState::external_limit { pct_of_rated, enabled,
  revert_s }`: applied after the EMS target and the grid-code block,
  before the share is cut; a limit lowers both discharge and charge
  magnitudes (a curtailment order bounds the exchange at the POI). When
  the revert timer runs out the limit clears, as SunSpec's `WMaxLimPct_RvrtTms`
  does. The follower (M3, 10 %/min) shapes the step, which sits inside
  the TSOs' 6 to 20 %/min band for third-party changes.
- **The flag.** Site word bit `external_limitation` (5, warning byte):
  raised while the limit binds, that is while the unlimited target
  exceeds the limit; mirrored as `site.ems.limit_active`. `power_limited`
  (bit 0) keeps its meaning (the plant could not deliver what was
  asked); the two together tell an operator whether the shortfall is
  the plant's or the order's.
- **The quarter-hour KPIs.** `kernel/kpi.rs`: per quarter-hour the
  scheduled mean (the EMS target integrated), the metered mean (the
  fiscal meter's period), the deviation in kW and in percent of rated,
  the availability (available power over contracted power, time-
  averaged, both directions), and the tracking error's RMS over the
  period; closed on the fiscal meter's boundary, published as the last
  closed period's values, like the revenue meter.
- **Switchgear on the shell.** REST `POST /api/v1/substation/{breaker,
  disconnectors,earthing}` and the Modbus command register, both
  calling the M3 kernel commands and returning the interlock's name
  when refused.

## What it is not

- Not a schedule interface: the schedule is the EMS target as the plant
  pursued it; a schedule file (PROD/VERB series) is M4 phase 4's or
  later.
- Not a penalty model: the deviation is measured, not priced, until
  phase 3's reBAP settlement.

## Invariants

- A limit written over Modbus and the same limit written over REST leave
  the plant in the same state.
- Every holding register reads back what it commands, from the state,
  as the two M0 registers do.
- The KPI's scheduled and metered means are both exactly the period's
  integrals (no sampling); with no limit and no shortfall the deviation
  is the conversion chain's only.
