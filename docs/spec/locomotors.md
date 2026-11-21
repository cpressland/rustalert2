# Locomotors and movement

**Status:** Spec'd from observation (implementation next)
**Crate:** `ra2-sim`
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

## Open questions

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
