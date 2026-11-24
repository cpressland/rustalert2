# Locomotors and movement

**Status:** Implemented for the Hunt and Harvest scenarios: acceleration and slowing traced; the per-unit lepton rate and the drive tracks (curves, corner stops) still fitted or open
**Crate:** `ra2-sim` (`hunt`, `harvest`)
**Issue:** #40 (with pathfinding, #39)

## Scope

How units move once they have somewhere to go: speeds, turning, stopping.
Choosing the route is pathfinding (#39).

## Sources

- The recomp: how `Speed=` is stored (`0x0071465A`); the drive locomotor
  (`DriveLocomotionClass`, its `ILocomotion` table at `0x007E7EB0`:
  `Process` `0x004B0500`, the start of each move `0x004B0F20`, moving
  `0x004B2630`); `TechnoTypeClass` defaults (`0x00710BB2`, `0x00711651`)
  and reading (`0x0071247C`, `0x00715402`); a foot's current speed
  (`0x004DB1A0`) and `Set_Speed` (`0x004D3710`).
- Parity reference logs of `grizzly-hunts-mcv`, `rhino-hunts-mcv` and
  `gi-hunts-mcv` (flat clear map, one hunter driving to an idle MCV), and
  `harv-hunts-mcv` and `harvester-ore` (a War Miner, which accelerates).

## Behaviour

- `Speed=` is clamped to 0–100 and stored as `Speed × 256 / 100`
  (integer), capped at 255: Grizzly (7) 17, Rhino (6) 15, GI (4) 10.
  `[recomp]`
- On clear ground a Drive unit covers about 1.6 leptons a frame per point
  of that stored value: the Grizzly's steps are mostly 27–28 leptons, the
  Rhino's 23–25. `[observed]`
- A foot keeps a speed fraction (a double, `FootClass +0x578`) and its
  locomotor a target fraction (normally 1). Each frame a Drive unit sets
  the fraction: `[recomp 0x004B0F69]`
  - not `Accelerates=` (default yes; the Grizzly and Rhino set no): the
    target, at once;
  - within `SlowdownDistance=` leptons (default 500) of where it's going:
    less `speed × DeaccelerationFactor=` (default 0.002), but not below
    0.3;
  - else below the target: plus `AccelerationFactor=` (default 0.03), up
    to the target.
  Not while it's on a turning track (index 64 or more), nor for a unit
  type with `Passive=yes`.
- At or below `[AudioVisual] ConditionYellow=` health (50%) the fraction
  is × 0.75, before the truncation below. `[recomp 0x004B3DDF]`
  (`grizzly-crush-guard`: a Grizzly at 140 of 300 moved 19 leptons a
  frame, its 17 units × 0.75 truncated to 12; 27 unhurt) `[observed]`
- What it moves on a frame is the stored speed × that fraction (× house
  and veterancy multipliers), truncated: a War Miner (stored 10) moves 3
  points at a crawl and gains a point about every 3 frames. `[recomp
  0x004DB1A0]`
- An accelerating unit sets off at the crawl, 0.3. `[observed]` (the War
  Miner's first steps; where the fraction is set at the start isn't
  traced)
- Infantry (Walk) are slower than the stored value suggests: the GI's
  steps are mostly 10–11 leptons. `[observed]`
- Drive units follow the cell grid, not a straight line: the Grizzly's
  run from (47,50) toward (35,35) goes diagonally, then a stretch due
  north, then diagonally again, with a shorter step about every 13
  frames. `[observed]` (the game's drive tracks; to trace)
- A unit hunting a target out of range starts moving 19 frames into the
  game (vehicles) or 1 (the GI). `[observed]`
- It stops at the centre of the first cell from which the target is in
  weapon range: the Grizzly and Rhino stop at (38,39), exactly 5 cells
  from the MCV's cell. `[observed]`

- A unit ordered to move (`MEGAMISSION`, mission Move, to a cell) plans a
  route there and drives it, stopping on the cell's centre. `[observed]`
  (`mcv-move`) Our Move mission drives as harvesters do (see below). It
  ends exactly where the original's does, but on the way:
  - the original's MCV set off 16 frames after the order landed, against
    our 12 (its turn to the east). It then gathered speed from a
    standstill, about half a lepton a frame each frame. We start it at the
    crawl, as observed for the War Miner, so we run up to 150 leptons
    ahead. That a standing MCV starts from nothing and a hunting miner at
    the crawl points to the drive tracks, not yet traced;
  - from (42,35) to (37,30), a pure diagonal, the original drove straight
    there. Our corridor of 2×2 zones leaves out the cells beside the
    diagonal, so our route steps north first (pathfinding.md);
  - from (37,30) to (33,39) the original went SW, S×4, SW×3, S; we go
    SW, S×5, SW×3.

## Jumpjets

What's recorded of the jumpjet locomotor (`{92612C46-…}`,
`JumpjetLocomotionClass`, Process dispatching on its state at `+0x50`
through six handlers at `0x0054B980`…`0x0054CA90`, not traced through).
Its defaults are `[JumpjetControls]`; a unit's own `Jumpjet…` keys override
them.

- **The Siege Chopper** (`JumpJet=yes`, `JumpjetClimb=10`,
  `JumpjetHeight=500`). `[observed]` (`soviet-siege-chopper`)
  - Out of the war factory it climbs 10 a frame from 46 frames after it
    appears at the factory's centre. The original rises over the factory's
    roof to 1030, sets off east once at about 890 (gaining 2 a frame), and
    comes down to 500 over the cell east of the door, 212 frames after it
    appeared. Ours drives out on the ground while it climbs, and is on the
    cell at 500 well before.
  - Ordered to deploy (`IsSimpleDeployer=yes`), it comes down 10 a frame
    from 3 frames after the order, and is deployed on landing: it fires its
    `Secondary=` (`DeployFire=yes`): 160mm, 45 a hit on a Grizzly every
    100 to 102 frames, the first 28 frames after the attack order (the
    original's 29). It faces `DeployDir=` (2, east) deployed. `[observed]`
    for the firing; the facing from the rules.
- **Things the Magnetron lifts** move as jumpjets too (abilities.md).

## What we implement

Fitted so the three `*-hunts-mcv` scenarios pass every check; each
constant is `[observed]`, not traced:

- **Facing** is 16 bits, 0 north (−y), clockwise; a map facing `f` (0–255)
  is `f × 256`. Standing vehicles turn `ROT × 256` a frame toward their
  first step and move on the frame the turn completes (the Grizzly's
  135° turn takes 20 frames, so it sets off on frame 20). At corners they
  turn on the move. Infantry turn at once but set off the frame after
  choosing a target.
- **Speed**, in 1/256 leptons a frame: vehicles `points × 406`, where
  `points` is the stored speed × the fraction above, truncated (we keep
  the fraction in millionths, not as a double, so a frame now and then may
  be a point off); infantry `speed × 277`. Each leg (cell
  to cell) ends exactly on its point and the rest of the frame's step
  carries into the next frame.
- **Route**: the game's planner (pathfinding.md), planned once toward the
  attack position and driven step by step. Vehicles stand on cell
  centres; infantry on sub-cell spot 2 of each cell.
- **Stopping**: at the first cell on the route with the target within
  weapon range.

Results: the Grizzly and Rhino are within 2 and 4 leptons of the original
at frame 120 and exact at 300; the GI within 53 and 30.

## Open questions

- Infantry are slower on straight legs (about 9.9 leptons a frame) than
  on diagonals (about 11); we use one speed.
- Infantry's stored route (it lives elsewhere in `InfantryClass`), to
  confirm they plan like vehicles.

- The drive track tables and how speed scales along them (the shorter
  steps).
- The constant behind 1.6 leptons per speed point, and terrain speed
  multipliers.
- The start delay: the hunt mission's scan interval or the turret/body
  turning first. The War Miner on Hunt sets off 8 frames later than the
  Grizzly does.
- Corners: the original mostly curves through them (drive tracks), but
  sometimes stops and turns on the spot for about 5 frames (the War Miner
  at (50,46) and (52,45) in `harvester-ore`); we always turn on the move,
  so our miner runs up to about 20 frames ahead by its first unload.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Hunter positions at frames 120 and 300 | `grizzly-hunts-mcv`, `rhino-hunts-mcv`, `gi-hunts-mcv` | 64 and 32 leptons |
| An accelerating hunter slowing and standing, at 300 and 400 | `harv-hunts-mcv` | 64 leptons |
| A harvester's round, cell hops at a crawl | `harvester-ore` (economy.md) | see there |
| The Siege Chopper hovering, coming down to deploy, and firing deployed | `soviet-siege-chopper` | exact; the first hit ±1 frame |
