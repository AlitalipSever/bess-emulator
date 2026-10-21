# M3 phase 3: converter thermal and the overload budget, architecture

What exists after this phase: a converter that gets warm from its own
losses, derates when it is hot, and can run above its rating for a
bounded time. Its temperature is published and alarmed; its overload
budget is published and spent.

## The model

- **One thermal node per PCS** fed by the tick's conversion loss and
  cooled toward the container air (or ambient for an outdoor skid) with
  a published time constant; the node's temperature is state.
- **Derating** from that temperature: a published curve of permitted
  power against heat-sink or ambient temperature, applied as a factor
  on the AC capability the same way the BMS's temperature factor
  applies on the DC side.
- **The overload budget**: the datasheet's short-term overload (a
  percentage for a duration, then a cool-down) as a bounded reservoir
  that spends while power exceeds rating and refills below it; the
  budget is state and a published point.

## What it is not

- Not a multi-node converter model (IGBT junctions, capacitors): the
  interface is one temperature and one factor, which is what the
  converter publishes.

## Invariants

- The thermal node's energy balance closes (loss in, cooling out,
  stored change).
- The budget never goes negative and never exceeds its full value.
- With the derating curve flat at 1.0 and the budget infinite the plant
  is the phase 2 plant byte for byte.
