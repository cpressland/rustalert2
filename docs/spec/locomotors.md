# Locomotors and movement

**Status:** Implemented for the Hunt scenarios (constants fitted to the reference logs; tracks and the pathfinder still to trace)
**Crate:** `ra2-sim` (`hunt`)
**Issue:** #40 (with pathfinding, #39)

## Scope

How units move once they have somewhere to go: speeds, turning, stopping.
Choosing the route is pathfinding (#39).

## Sources

- The recomp: how `Speed=` is stored (`0x0071465A`).
- Parity reference logs of `grizzly-hunts-mcv`, `rhino-hunts-mcv` and
  `gi-hunts-mcv` (flat clear map, one hunter driving to an idle MCV).

## Behaviour

- `Speed=` is clamped to 0–100 and stored as `Speed × 256 / 100`
  (integer), capped at 255: Grizzly (7) 17, Rhino (6) 15, GI (4) 10.
  `[recomp]`
- On clear ground a Drive unit covers about 1.6 leptons a frame per point
  of that stored value: the Grizzly's steps are mostly 27–28 leptons, the
  Rhino's 23–25. `[observed]`
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

## What we implement

Fitted so the three `*-hunts-mcv` scenarios pass every check; each
constant is `[observed]`, not traced:

- **Facing** is 16 bits, 0 north (−y), clockwise; a map facing `f` (0–255)
  is `f × 256`. Standing vehicles turn `ROT × 256` a frame toward their
  first step and move on the frame the turn completes (the Grizzly's
  135° turn takes 20 frames, so it sets off on frame 20). At corners they
  turn on the move. Infantry turn at once but set off the frame after
  choosing a target.
- **Speed**, in 1/256 leptons a frame: vehicles `speed × 406`, infantry
  `speed × 277`, where `speed` is the stored value above. Each leg (cell
  to cell) ends exactly on its point and the rest of the frame's step
  carries into the next frame.
- **Route** (`[hypothesis]`, fitted to one route): toward the cell beside
  the target along the major axis, first one diagonal step, then straight
  until the rest is diagonal, then diagonal. Vehicles stand on cell
  centres; infantry on sub-cell spot 2 of each cell.
- **Stopping**: at the first cell on the route with the target within
  weapon range.

Results: the Grizzly and Rhino are within 2 and 4 leptons of the original
at frame 120 and exact at 300; the GI within 53 and 30.

## Open questions

- Infantry are slower on straight legs (about 9.9 leptons a frame) than
  on diagonals (about 11); we use one speed.
- The route rule against more start positions, and the real pathfinder
  (#39).

- The drive track tables and how speed scales along them (the shorter
  steps).
- The constant behind 1.6 leptons per speed point, and terrain speed
  multipliers.
- The start delay: the hunt mission's scan interval or the turret/body
  turning first.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Hunter positions at frames 120 and 300 | `grizzly-hunts-mcv`, `rhino-hunts-mcv`, `gi-hunts-mcv` | 64 and 32 leptons |
