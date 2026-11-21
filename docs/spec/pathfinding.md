# Pathfinding

**Status:** Started (the cell-level search is traced and implemented
bit-exact; the hierarchical zone corridor that limits it is still to trace,
#108)
**Crate:** `ra2-sim` (`pathfind`)
**Issue:** #39

## Scope

How a unit's route is found. Moving along it is in locomotors.md.

## Sources

- `gamemd.exe` 1.001 through the recomp: the find-path entry
  (`0x0042C900`), the hierarchical search (`0x0042C290`), the cell-level A*
  (`0x00429A90`), node creation (`0x0042A460`) and the step cost
  (`0x00429830`).
- The routes in the `*-hunts-mcv` parity logs.

## Behaviour

### Two passes

1. A hierarchical search over the map's zones marks a corridor of zones
   from start to goal (`0x0042C290`). `[recomp]`
2. A cell-level A* runs from the start cell to the goal cell, skipping any
   cell whose zone isn't in the corridor. If the hierarchical search fails
   the game logs "Hierarchical findpath failure" and runs the A* without
   it. `[recomp]`
3. The found route is then tidied (`0x0042B210` straightens diagonal
   zig-zags; `0x0042B7F0` does more). `[recomp]` (not yet traced)

Each cell has a 10-byte record in a map table (map object `+0x70`,
indexed `y × (w + h + 1) + x`) holding its zone at three levels. On open
ground, zones are aligned square blocks: 2×2 cells at level 0, 4×4 at
level 1 and 8×8 at level 2, clipped to the playable diamond. `[observed]`
(observer zone dump of a 50×50 flat map) How they split on real terrain,
and how the corridor is searched, is #108.

### The cell-level A*

- Neighbours are tried in the order N (−y), NE, E, SE, S, SW, W, NW, plus
  a ninth for bridges and tunnels. `[recomp]` (offset table `0x7E3774`)
- Entering a cell costs a per-result amount from the move check (1 for
  clear ground; 1000, 60, 20, 8 and 10,000 for worse results; 7 or more
  is impassable) times a multiplier, **plus a direction bias** of 0.001,
  0.005, 0.002, 0.006, 0.003, 0.007, 0.004, 0.008 for N … NW. Diagonals
  cost the same as straight steps; the bias decides between equally long
  routes, favouring straight steps. `[recomp]` (`0x81870C`, `0x81872C`)
- A node's cost `g` is its parent's plus the step, and its estimate is
  `g` plus the straight-line distance to the goal in cells. Both are
  single-precision; the square root and the sum are in double precision
  before the result is stored as a single. `[recomp]` (`0x0042A460`)
- The straight-line estimate exceeds the step count on diagonals, so the
  search hugs the straight line to the goal. `[recomp]`
- A cell already reached is skipped unless its recorded cost is at least
  the current node's cost + 1.009. `[recomp]`
- Of each expansion's new nodes, the best (strictly lowest estimate; the
  first on ties) is kept aside and the rest go on a binary heap. The next
  node expanded is the one kept aside, unless the heap's top is at least
  as good: then the heap's top is expanded and the kept node takes its
  place in the heap. `[recomp]`
- The heap moves an entry up only while its parent is strictly worse, and
  down only to a strictly better child, preferring the left. `[recomp]`
- The search ends when it makes the goal's node, and gives up after
  65,527 expansions. `[recomp]`

We hold the single-precision values as their bits and do the adds, the
square root and the roundings in integers, so the routes match the
game's bit for bit without floating point in the simulation.

### What the logs show

No single fixed corridor of those blocks gives the observed route with
the traced cell search: one that stops the early diagonal also rules out
a cell the unit later crosses. So the route is likely re-planned on the
way, or tidied by the passes above. `[hypothesis]`


Without the corridor, the A* from (47,50) to (35,35) on open ground goes
twelve steps NW and then three N. The original's units go NW, N, N, then
eleven NW and N: the same search run with the corridor in force, which
rules out the cells the first route takes early on. The hunters still use
the route rule fitted to that route (locomotors.md) until #108 lands.

## Open questions

- Zone building and the hierarchical search (#108).
- The route tidying in `0x0042B210` and `0x0042B7F0`.
- The move-check results for each terrain, unit and occupant.
