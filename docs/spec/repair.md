# Repair, self-healing, veterancy and crates

**Status:** Parity-checked (self-healing, building repair, the Service
Depot, engineers repairing, experience from kills, veterancy's effects on
an attacker, crates: picking up, Money, Veteran, Unit and Speed; the
IFV's repair turret). Still to come: the other powerups.
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

### Engineers

- An engineer ordered onto its own side's damaged building (`MEGAMISSION`,
  mission Capture) walks to it and goes in on reaching one of its cells.
  The building is whole at once, at no cost, and the engineer is used up.
  `[observed]` (`yuri-engineer-repair`: a Bio Reactor at 500 of 700, whole
  on the frame the engineer reached its top-left cell; the house's credits
  didn't change)
- As with repair, the house's power shows it a frame later. `[observed]`
- An engineer ordered onto an enemy's building captures it the same way:
  on reaching one of its cells the building becomes its house's, as it
  stands, and the engineer is used up. Both houses' power is counted again
  on that frame. `[observed]` (`allied-engineer-capture`: a Neutral power
  plant; the player's output went from 200 to 400 on the frame the
  engineer went in, the Neutral house's to nothing)
- We walk it to the building's nearest cell.

### The Service Depot

- A unit ordered into a building with `UnitRepair=yes` (`MEGAMISSION`,
  mission Enter) drives onto its centre cell. `[observed]`
  (`allied-depot-repair`: an IFV on a depot at (37,38) parked on (38,39))
- It's repaired there, `RepairStep=` (8) hit points a step, each costing
  cost × `RepairPercent=` × `RepairStep=` ÷ `Strength=`, truncated: 3
  credits for an IFV (600, 200 hit points). `[observed]`
- The steps run on the depot's own timer, not the repair clock: the first
  4 frames after the unit docks, the next 85 frames after that (likely the
  depot's animation), then one every 15. 15 is the first whole frame
  count of at least `URepairRate=` × 900 (0.016 gives 14.4). The depot
  keeps a count it compares with that product. `[observed]` `[recomp
  0x0044BD38]`
- 12 frames after it's whole the unit drives off the pad, to the cell
  below the depot's left column. `[observed]`
- What happens when the house can't pay isn't recorded. We send it off.

A forced attack (`MEGAMISSION`, mission Attack, on one's own unit) is how
the scenario damages the IFV. The original's Grizzly fired its first shell
about 20 frames after the order, its turret turning (combat.md). Ours
turns too, but our IFV parks on the Grizzly's cell (see below), so the
first shell's frame differs.

### The IFV's repair turret

- Infantry ordered into a transport (`Passengers=` above 0, its own
  side's, with room) walk to its cell and get in. `[observed]`
  (`allied-ifv-repair`: an engineer into an IFV)
- A gunner (`Gunner=yes`, the IFV) fires its `Weapon<n>`, n its first
  passenger's `IFVMode=` + 1, or `Weapon1=` empty: an engineer (mode 1)
  gives it `RepairBullet` (`Damage=-50`, `Range=1.8`). `[community]`
  `[observed]`
- A negative damage mends, up to full strength: one shot took a Grizzly
  from 268 to 300. `[observed]` An attack with a mending weapon ends once
  the target is whole. `[hypothesis]`
- The original's IFV mended only once it had stopped beside the Grizzly;
  ours mends as soon as it's in range on the move, 9 frames sooner.
- Not modelled yet: a gunner on Guard taking on damaged friends by
  itself, other passengers' weapons beyond the table, and unloading.

### Crates

- A crate is overlay `CRATE` (index 179 in `[OverlayTypes]`; `WCRATE`,
  242, on water). `[observed]`
- A skirmish drops the map's crates and puts its own at random, one per
  human player (`[CrateRules] CrateMinimum=`). It puts a new one at random
  as soon as one is taken. `[observed]` `[recomp 0x005FD474]`
- The game keeps them in `MapClass +0x158`: 256 entries of 16 bytes, a
  timer then the cell at `+0xC`. `[recomp 0x0056BF83]` The recorder moves
  the game's crate to the scenario's cell and sets its powerup the same
  way.
- A crate's overlay data names its powerup: below 19, that one of
  `ra2_rules::POWERUPS` (Tiberian Sun's order: Money 0, Unit 1, HealBase
  2, … Veteran 14). 19 and up means a pick by `[Powerups]` weight: a
  number from 1 to their sum, and the first powerup whose running total
  reaches it. A random crate's data is 255. `[recomp 0x00481ACE]`
- A unit takes it on the frame it sets off into the crate's cell; the
  pathfinder steers round a crate that isn't the destination. `[observed]`
  (`mcv-crates`)
- What each gives (`[Powerups]` is `weight,anim,water,data`):
  - **Money**: data + 0 to 900 credits, at random (2000 gave 2879 and
    2412). `[recomp 0x00482484]` `[observed]`
  - **Veteran**: data levels for each trainable unit of the house within
    `CrateRadius=` (3 cells); with none, it's money. `[observed]` (an MCV
    alone got money) The levels' arithmetic is `[hypothesis]`.
  - **Unit**: a unit of `[CrateRules] UnitCrateType=` (random if `none`,
    not modelled) on the crate's cell. The unit that took it stops a cell
    short. `[observed]`
  - **Speed**: × data (1.2) on the house's units within the radius. It
    multiplies the stored speed, truncated, before veterancy: an MCV's 10
    became 12, 19 leptons a frame. `[recomp 0x004DB1D5]` `[observed]`
  - **Armor** and **Firepower**: ÷ damage taken, × damage dealt, the same
    way. `[hypothesis]`
  - **HealBase**: the house's buildings whole. `[hypothesis]`
  - the rest (cloak, explosions, reveal, darkness, ICBM…) aren't modelled
    yet; we give money.
- Our random numbers don't yet follow the original's, so a random
  crate's powerup and a money crate's random part differ from the
  original's.

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

### Crews

- A destroyed vehicle's crew, and the wreck's blast: see combat.md
  ("Death"). Modelled.
- A destroyed `Crewed=yes` building lets out its refund ÷ the side's
  `AlliedSurvivorDivisor=` (500), `SovietSurvivorDivisor=` (250) or
  `ThirdSurvivorDivisor=` (750), twice the divisor if captured, between 1
  and 5. `[recomp 0x00451330]` (a Construction Yard, 1500 back: three GIs
  `[observed]`) Their health was 39 to 120 of 125, at random.
  Not modelled yet, nor a building's death explosion: none of the yard's
  three survived in one recording, all three in another.

## Open questions

- Two units leaving a war factory: the original sent the first a cell
  further on, so the second had the door cell; ours send both to the
  door cell.
- A building's crew and death explosion (see "Crews").
- When a unit attacks a building, the original fires while still driving
  up, and stops elsewhere than our hunt does. A Grizzly shelling a
  Construction Yard fired 5 frames sooner than ours.
- Crates: where new ones appear, the random parts, a random unit crate's
  unit, and the powerups not yet modelled.

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `allied-ifv-repair` | The Grizzly's health after the IFV's missiles and after one RepairBullet, where the IFV ends, the engineer aboard | exact |
| `mcv-crates` | Credits after a money crate and a veteran crate (money without a trainable unit), the unit crate's Grizzly and where the MCV stops, and its speed after a speed crate | credits ±900 a crate (the random part); positions exact, one ±64 |
| `allied-depot-repair` | The IFV's health after the shell, after the first step and once whole, the house's credits at each, and where it parks after | exact |
| `allied-engineer-capture` | The engineer going in, the plant's owner and health after, both houses' power | exact; the engineer's arrival ±40 frames (infantry speed) |
| `yuri-engineer-repair` | The reactor's health before and after, the engineer going in, the house's power and credits after | exact; the engineer's arrival ±30 frames |
| `grizzly-veteran-hunts-mcv` | A veteran from the map: its experience, its position at 120, the MCV's first damage and its health at 600 and 900 | as `grizzly-hunts-mcv`: one shell (71), 3 frames |
| `grizzly-elite-hunts-mcv` | Elite from the map: its experience, its health (self-healing) at 140 and 440, the MCV's first damage, its health at 300 and when it's destroyed | health exact; one shell, 3 frames |
| `grizzly-veteran` | The MCV's health at 500, the frame it's destroyed, and the Grizzly's experience at 530 and 560 | exact; the kill ±2 frames |
| `yuri-heal-repair` | The War Miner's health at 1000 and 2600, and the Rhino's and Conscript's at 2600. The reactor's health at 1138, 1303 and 1639. The house's power at 1138, 1303 and 2600. Its credits at 1303 and 2600. | exact |
