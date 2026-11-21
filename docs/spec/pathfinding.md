# Pathfinding

**Status:** Implemented for open ground (every route the original
planned in the recorded starts is reproduced; zones on real terrain and
the threat term are still to trace)
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
  first on ties) is kept aside and the rest go on a binary heap. The node
  kept aside is expanded next; the heap is drawn on only when an
  expansion adds nothing. `[observed]` (the code at `0x0042A1B3` reads as
  if the heap's top could win when better, but the game's own search
  dumps match the always-kept-aside rule in every reached cell and cost)
- The heap moves an entry up only while its parent is strictly worse, and
  down only to a strictly better child, preferring the left. `[recomp]`
- The search ends when it makes the goal's node, and gives up after
  65,527 expansions. `[recomp]`

We hold the single-precision values as their bits and do the adds, the
square root and the roundings in integers, so the routes match the
game's bit for bit without floating point in the simulation.

### The whole plan, as implemented

1. **Destination.** A unit attacking goes to the target's cell plus its
   weapon range less half a cell (infantry: the full range), toward the
   attacker, each axis cut toward zero. `[observed]` (fits the Grizzly,
   Rhino and GI from seven starts)
2. **Corridor.** At each zone level from the top (8×8, 4×4, 2×2 blocks on
   open ground), a search without an estimate (cost 1 plus the direction
   bias per step) runs over that level's zones, kept to the zones inside
   the level above's route. `[observed]` (fits every dumped corridor;
   `0x0042C290` also adds a threat term and a per-connection weight, both
   nil on open ground, and has no estimate)
3. **Cell search** inside the level-0 corridor, as above. The start's
   neighbours outside the corridor are recorded as reached but not
   expanded. `[observed]` (every reached cell and cost matches the game's)
4. **Tidying.** Two diagonal steps a right angle apart become two straight
   steps; then everything after the first step is re-laid as straight
   steps first, then diagonals. `[observed]` (every stored route matches;
   the passes are `0x0042B210` and `0x0042B7F0`)
5. **Storing.** The unit keeps about 12 steps and drives them, taking each
   one off a step ahead for the drive tracks; it doesn't re-plan on the
   way. `[observed]`

All eight routes the original stored (the Grizzly from seven starts and
four facings, and the Rhino) come out identical, and the hunt scenarios,
including a new start from the south (`grizzly-hunts-mcv-north`), pass
every check with units driving these routes.

### Checked against the game's own searches

The observer can dump, right after a unit plans, the zones its search
marked at each level and every cell its cell search reached with its
cost (`RA2_OBSERVE_DUMP`). Inside the dumped corridor our cell search
reaches the same cells with the same costs, bit for bit.

### What the logs show

No single fixed corridor of those blocks gives the observed route with
the traced cell search: one that stops the early diagonal also rules out
a cell the unit later crosses. `[observed]`

Dumping the unit every two frames shows how the route is held: a list of
directions in the unit (`UnitClass +0x5E0`, ended by −1), planned once at
the start and **kept to about 12 steps**: NW, N, N, then nine NW, ending
at (37,38), which is the first 12 steps of a 15-step route to the MCV's
cell. Steps come off the front as the unit drives, one ahead of the one
in progress (for the drive tracks). It isn't re-planned on the way.
`[observed]`

So the N, N early in the route comes from the planning itself, most
likely the tidying pass `0x0042B7F0`, which walks the route in windows of
up to 20 steps and re-solves stretches through `0x0042BCA0` and
`0x0042BE20`. `[hypothesis]`


Without the corridor, the A* from (47,50) to (35,35) on open ground goes
twelve steps NW and then three N. The original's units go NW, N, N, then
eleven NW and N: the same search run with the corridor in force, which
rules out the cells the first route takes early on.

## Open questions

- How zones split on real terrain (cliffs, water, buildings), and the zone
  search's threat term and connection weights.
- The tidying passes in full: the rule above fits every route recorded
  but `0x0042B7F0` handles loops and windows of up to 20 steps.
- When a unit re-plans (a route longer than it stores, a blocked cell).
- The move-check results for each terrain, unit and occupant.
