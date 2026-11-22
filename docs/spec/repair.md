# Repair, self-healing, veterancy and crates

**Status:** Parity-checked (self-healing, building repair). Veterancy is
traced but not yet modelled; crates and repair by depots, IFVs and
engineers are still to come.
**Crate:** `ra2-rules` (`production::repair_frames`,
`production::repair_step_cost`), `ra2-sim` (`production::heal`)
**Issue:** #46

## Scope

How damaged things get their health back: self-healing, a house repairing
its buildings, and, to come, the Service Depot, the IFV's repair turret and
engineers. Also veterancy and crates. Area damage to buildings is covered
here too, because the repair scenario damages its building that way.

## Sources

- The recomp of `gamemd.exe` 1.001:
  - `[General] RepairRate=`, `RepairStep=` and `RepairPercent=`;
  - veterancy, below.
- Scenario `yuri-heal-repair`:
  - three neutral units start at half health, far from anything: a War
    Miner (`SelfHealing=yes`), a Rhino and a Conscript;
  - the Yuri player builds a Bio Reactor and a Psychic Dominator (the map
    makes it cheap and quick to charge);
  - the player fires the Dominator seven cells east of the reactor, so its
    blast catches one reactor cell;
  - at 1300 the player orders the damaged reactor repaired (event 21,
    `REPAIR`).

## Behaviour

### The repair clock

- Repairs and self-healing tick together, on frames where (frame − 1) is
  a multiple of `RepairRate=` × 900 (0.016 × 900 = 14), worked out on the
  x87 and truncated (at least 1). `[observed]` The War Miner gained a hit
  point at 501, 515, 529 and so on, and the reactor's repair, ordered at
  1300, first landed at 1303.

### Self-healing

- An object with `SelfHealing=yes` gains 1 hit point on each tick, up to
  its `Strength=`. `[observed]` The Rhino and the Conscript didn't heal.

### Repairing buildings

- Each tick, a building being repaired gains `RepairStep=` (8) hit points.
  `[observed]`
- Each step costs cost × `RepairPercent=` × `RepairStep=` ÷ `Strength=`,
  truncated. For the Bio Reactor that's 600 × 0.15 × 8 ÷ 700 = 1 credit.
  `[observed]`
- Repair stops once the building is whole. The reactor went from 500 to
  700 in 25 steps, 1303 to 1639, for 25 credits. `[observed]`
- What happens when the house can't pay isn't recorded. We stop the
  repair. `[hypothesis]`

### Power while damaged and repaired

- A power plant's output scales with its health (see
  [production.md](production.md)).
- Damage recounts its house's power on the same frame. `[observed]` The
  reactor dropped from 150 to 107 when the blast hit.
- A repair step shows a frame later: the house's power is counted before
  repairs land. `[observed]`

### Area damage to buildings

- A blast round a cell hurts a unit by its distance from the cell's
  centre. The damage is the full amount after `Verses=`, × (1 − (1 −
  `PercentAtMax=`) × distance ÷ `CellSpread=`), truncated.
- A building is hurt once for each of its foundation cells within the
  spread, each at that cell's distance, and the shares add up.
  `[observed]`
  - The Dominator took 200 from the Bio Reactor: one cell at 7 cells.
  - It took 82 from a Construction Yard: five cells, 18 + 18 + 18 + 16 +
    12, at 6% against concrete.

### Veterancy (traced, not yet modelled)

- A unit's experience is a float at `TechnoClass +0x150`. `[recomp]`
- A kill adds the victim's cost ÷ (the killer's cost × `VeteranRatio=`),
  capped at `VeteranCap=`. `[recomp 0x0074FF50]`
- It reaches veteran at 1 and elite at 2. `[community]`

## Open questions

- Repair by the Service Depot, the IFV and engineers needs move and enter
  orders, which the scripted orders can't give yet.
- Veterancy needs kills credited to their shooter, and then a scenario
  where a unit makes enough kills to be promoted.
- Crates depend on the game's random numbers: what's in each one, and
  where new ones appear.

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `yuri-heal-repair` | The War Miner's health at 1000 and 2600, and the Rhino's and Conscript's at 2600. The reactor's health at 1138, 1303 and 1639. The house's power at 1138, 1303 and 2600. Its credits at 1303 and 2600. | exact |
