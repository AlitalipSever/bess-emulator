# M3 phase 4: substation depth, architecture

What exists after this phase: a substation with a sequence, a hot
transformer and a tap changer. The HV breaker stops being a bit that
flips: closing it requires the disconnectors closed and the earthing
switch open, and the sequence takes time; the transformer has a top-oil
and a hot-spot temperature driven by its load and the ambient, with the
alarm the standard names; the OLTC steps the MV bus voltage in
published steps, visible on the surfaces.

## The model

- **Interlocks and sequence.** `SubstationState` gains the disconnector
  and earthing-switch positions and a sequence state with its timer;
  the HV breaker closes only through the sequence; a trip opens it at
  once. The M2 staggered return waits for the sequence.
- **Transformer thermal.** IEC 60076-7's exponential model: top-oil
  rise with its time constant from the load factor, hot-spot rise above
  top oil with the winding time constant; published as two
  temperatures with a warning and a trip threshold from the standard's
  guidance.
- **OLTC.** A tap position with a range and a step, moved by a voltage
  regulator with a dead band and a delay, driven by the grid voltage
  the exogenous input carries (synthetic until M4) and the
  transformer's load; `site.poi.voltage_kv` keeps the grid side, a new
  MV bus point carries the stepped side.

## What it is not

- Not load flow, not protection relay internals: ARCHITECTURE.md's
  non-goals stand. The interlocks are a sequence, not a scheme.

## Invariants

- The breaker cannot be closed with the earthing switch closed, ever.
- The transformer's thermal balance closes; the hot spot never sits
  below the top oil.
- The tap never leaves its range; the regulator never moves more than
  one step per delay.
- With the sequence instant, the thermal model off and the tap fixed,
  the plant is the phase 3 plant byte for byte.
