# Shroud and fog

**Status:** Implemented (hard edges; soft edges and fog to come)
**Crate:** `ra2-render` (`compose`)
**Issue:** #34

## Scope

What a player sees of the map at the start, and how unseen cells are
drawn. Revealing as units move belongs to the simulation (M4).

## Sources

- `shroud.shp`: 96 frames of 60×30, in two halves. `[observed]`
- `Sight=` in the rules.

## Behaviour

- The player house is `[Basic] Player=`. At the start it sees every cell
  within each of its objects' `Sight=` (in cells, default 4) of that
  object's cell. `[hypothesis]` (circle test on cell offsets)
- Unseen cells are black. Seen cells next to unseen ones (on the four sides)
  are darkened to 55%. `[hypothesis]` (stand-in for the soft edge frames)
- Shroud covers the ground diamond of each cell, at level 0.

## Open questions

- How `shroud.shp`'s frames are chosen. The game picks an edge shape from
  a cell's neighbour mask, and the table needs tracing in the recomp.
- Fog of war (shroud that returns), gap generators and the reveal-all
  debug toggle (planned for the M4 game loop).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Start-of-game shroud against the recomp | M3 exit comparison | visual |
