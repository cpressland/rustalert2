# Repair, self-healing, veterancy and crates

**Status:** Parity-checked (self-healing, building repair, experience
from kills, veterancy's effects on an attacker). Still to come: crates, and
repair by depots, IFVs and engineers.
**Crate:** `ra2-rules` (`production::repair_frames`,
`production::repair_step_cost`, `production::experience_after_kill`),
`veterancy`), `ra2-sim` (`production::heal`, `hunt::credit`)
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
- Scenario `grizzly-veteran`:
  - a Neutral Grizzly on Hunt destroys the human player's MCV, weakened by
    the map's rules;
  - the observer logs each techno's experience.

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

### Veterancy

- A unit's experience is a float at `TechnoClass +0x150`. `[recomp]` It's
  a veteran from 1 and elite from 2. `[recomp 0x0074FF90]`
- When a kill is credited, the killer gains the victim's cost ÷ (its own
  cost × `VeteranRatio=`). The game does this on the x87, adds it to the
  float, caps it at `VeteranCap=` and stores it as a float.
  `[recomp 0x0074FF50]`
  - A Grizzly (700) that killed an MCV (3000) had 1.42857146.
    `[observed]`
  - It killed a GI (200) next, and had 1.52380955. `[observed]`
- Some kills are worth more or nothing. `[recomp 0x00702D50]`
  - A veteran victim is worth twice its cost, and an elite one three
    times.
  - A kill of the killer's ally is worth nothing. We count only a kill
    of a house's own units as an ally's.
  - Only a `Trainable=` killer gains. By default buildings aren't, and
    everything else is.
  - Both costs are worked out with the victim's house's cost multiplier.
    We use the plain cost.
- A map's `Veterancy=` field (after the tag, on units and infantry)
  starts a unit with experience of that ÷ 100: 100 a veteran, 200 elite.
  `[observed]`

### What veterancy does

Each ability in the type's `VeteranAbilities=` (and for an elite unit its
`EliteAbilities=` too) changes one number by a `[General]` factor, truncated
on the x87. `[recomp 0x0070D0D0]`

| Ability | What it changes | By |
|---|---|---|
| `FIREPOWER` | the damage of each shot it fires | × `VeteranCombat=` (1.1) |
| `STRONGER` | the damage it takes, before `Verses=` (at least 1) | ÷ `VeteranArmor=` (1.5) |
| `ROF` | its reload after a burst, random part included | × `VeteranROF=` (0.6) |
| `FASTER` | its full speed (`Speed=` × 256 ÷ 100) | × `VeteranSpeed=` (1.2) |
| `SELF_HEAL` | heals as `SelfHealing=yes` does | |

- An elite unit fires its `ElitePrimary=`, if it has one. `[community]`
  `[observed]` (the elite Grizzly's `105mmE` fired in pairs)
- Shots within a `Burst=` come 3 to 5 frames apart, at random. After the
  last, the reload is (`ROF=` + 0 to 2) × the `ROF` factor.
  `[recomp 0x006FD084]`
- What we saw with a Grizzly hunting an MCV (`[observed]`):
  - **Veteran** (`STRONGER,FIREPOWER,SIGHT,FASTER`):
    - 71 a shell, against 65 for a rookie;
    - a rookie's reload;
    - in place and firing by frame 149, against 170 for a rookie (speed
      20 rather than 17).
  - **Elite** (adds `SELF_HEAL,STRONGER,FIREPOWER,ROF`):
    - pairs of 71s, 3 to 5 frames apart, every 30 or 31 frames;
    - a hit point back on each frame of the repair clock from frame 1.
- `STRONGER` hasn't been seen in a recording yet: the victims so far were
  rookies. That it comes before `Verses=` follows the order of the
  original's damage code. `[recomp 0x00701952]`
- `SIGHT` waits for sight in the simulation.

## Open questions

- Repair by the Service Depot, the IFV and engineers needs move and enter
  orders, which the scripted orders can't give yet.
- The crew that steps out of a destroyed vehicle or building, and the
  blast that can kill it, aren't modelled yet.
- When a unit attacks a building, the original fires while still driving
  up, and stops elsewhere than our hunt does. A Grizzly shelling a
  Construction Yard fired 5 frames sooner than ours.
- Crates depend on the game's random numbers: what's in each one, and
  where new ones appear.

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `grizzly-veteran-hunts-mcv` | A veteran from the map: its experience, its position at 120, the MCV's first damage and its health at 600 and 900 | as `grizzly-hunts-mcv`: one shell (71), 3 frames |
| `grizzly-elite-hunts-mcv` | Elite from the map: its experience, its health (self-healing) at 140 and 440, the MCV's first damage, its health at 300 and when it's destroyed | health exact; one shell, 3 frames |
| `grizzly-veteran` | The MCV's health at 500, the frame it's destroyed, and the Grizzly's experience at 530 and 560 | exact; the kill ±2 frames |
| `yuri-heal-repair` | The War Miner's health at 1000 and 2600, and the Rhino's and Conscript's at 2600. The reactor's health at 1138, 1303 and 1639. The house's power at 1138, 1303 and 2600. Its credits at 1303 and 2600. | exact |
