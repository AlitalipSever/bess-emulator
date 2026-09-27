# M2 phase 1: BMS deepening, architecture

What exists after this phase: a BMS that derates on temperature, a rack
that can be out of balance, and balancing that slowly puts it right. All of
it behind `BmsLogic`, which widens but keeps its M0 shape.

## The trait

`BmsLogic` today has one method, `rack_limits(&rack, &cfg) -> PowerLimits`,
and it is pure: same rack, same limits. That stays, and derating happens
inside it, because `RackState` already carries `cell_temp_c`; temperature
derating needs no new plumbing, only a model that uses what is there.

What the trait gains is state advancement. Imbalance and balancing are
dynamics, not lookups, so the trait gets a second method:

```
fn step_rack(&self, rack: &mut RackState, cfg: &RackConfig, dt_s: f64) -> BmsFlows;
```

The kernel calls `step_rack` once per tick before it asks for limits, in
the same position of the tick where the cell model integrates. `BmsFlows`
reports what the step moved (balancing heat released into the rack's
thermal mass, balancing charge bled), so the energy conservation invariant
can hold the BMS to account the same way `ThermalFlows` holds the thermal
model.

## New state

`RackState` gains two fields:

- `cell_dv_v`: the spread between the highest and lowest cell voltage in
  the rack, the quantity a real BMS publishes as min/max. It is modeled at
  rack granularity as one scalar with dynamics: throughput widens it,
  balancing narrows it. There is no per-cell state behind it, per the
  non-goals; the register is the truth we are accountable to, and the
  register is a min/max.
- `balancing_active`: whether the bleed resistors are on. Status, not an
  alarm; the alarm phase reads the spread, not this flag.

## Where the energy goes

Balancing is passive: bleed resistors burn charge off the high cells. Two
consequences, stated so the invariants stay honest:

- The bled energy leaves the rack's stored energy and enters the rack's
  thermal mass as heat, through `BmsFlows`. It never touches the auxiliary
  inventory: the house-load rule is one consumer one row, and bleed
  resistors dissipate inside the rack, where the rack electronics item
  already lives. The wattage is small (hundreds of milliamps at cell
  voltage per rack); its home matters more than its size.
- The spread narrowing that balancing buys is bounded by the charge it
  bleeds. The model may not close spread faster than the bleed current
  explains.

## Derating shape

Charge and discharge limits get a temperature factor each, multiplied onto
the existing SoC taper. LFP charge acceptance collapses near freezing and
both directions taper at high temperature, so the factors are asymmetric:
charge tapers to zero approaching the cold limit, both taper above the warm
knee, and beyond the hot limit the limits are zero (the trip itself is an
alarm, wired in phase 2). Thresholds come from a public 314 Ah class LFP
datasheet, pinned in this phase's first PR; the shape (piecewise linear
taper bands, mirroring the SoC taper) is fixed here.

Cell temperature already has memory (M1 gave racks a thermal mass), which
is exactly what makes this derating meaningful: a hot afternoon derates the
plant minutes after the air peaks, not instantly.

## Causality this phase creates

After phase 1 the chain the architecture document promises exists up to the
PCS: hot afternoon, HVAC falls behind, cell temperature climbs, BMS derates,
block capability drops below the EMS setpoint, the PCS misses it. Phase 2
turns the links of that chain into alarms; this phase only has to make the
links real.

## Invariants

- Energy conservation extends over balancing: stored energy change equals
  cell-model flows minus bleed, bleed reappears as heat in the thermal
  node.
- `cell_dv_v >= 0` always; balancing strictly narrows, throughput strictly
  widens, neither may step the spread discontinuously.
- Derate factors are in [0, 1] and monotonic in temperature on each side of
  the window.
