# Coordinates, cells and projection

**Status:** Implemented
**Crate:** `ra2-sim` (`coords`), used by `ra2-render`
**Issue:** #27

## Scope

How positions are measured (cells, leptons, height levels) and how they
project to screen pixels. Bridges and their layers are left to the
simulation (M4), where they first matter.

## Sources

- The recomp's `CoordStruct`/`CellStruct` use, and its projection in the
  tactical view code (`gamemd.exe` 1.001).
- Every shipped map drawn with `ra2tool map render`. Tiles, cliffs and
  objects line up only with the numbers below.
- A recomp frame of `XMP30S6` with the MCV at cell (38,108), level 5.

## Behaviour

- A cell is 256 leptons on a side. A height level is 104 leptons.
  `[recomp]`
- A cell is drawn as a 60×30 diamond. Each height level raises it by
  15 pixels. `[recomp]` `[observed]`
- Projection of a cell's top corner at level `h`:
  `x = (cx − cy) × 30`, `y = (cx + cy) × 15 − h × 15`. `[observed]` (tiles
  tessellate; cliff extras meet their neighbours)
- Projection of a point in leptons:
  `x = (X − Y) × 30 / 256`, `y = (X + Y) × 15 / 256 − Z × 15 / 104`.
  `[observed]`
- The cell containing a point is `floor(X / 256), floor(Y / 256)`.
  `[recomp]`
- A cell's centre is 128 leptons in from its corner on each axis. Its `Z`
  is its level × 104. `[recomp]`
- Map packs store cells in these same iso coordinates. The playable
  rectangle is a separate `[Map] LocalSize`. `[observed]`

## Draw order

- Terrain: base diamonds go first, in row order (`cx + cy`), then height.
  Cliff and ramp "extra" art goes second, in the same order, so it overlaps
  later diamonds. `[observed]`
- Objects use painter's order: the screen `y` of the object's base, then its
  `x`, then a layer within the object (shadow, body, turret). A building's
  base is its foundation's bottom corner. `[observed]` (walls, trees and
  buildings overlap correctly on the shipped maps)
- The game's own order is a Z buffer for voxels and a sorted object list.
  Painter's order on the base gives the same result except for objects that
  straddle each other (open questions).

## Quirks we match

- None yet.

## Open questions

- Exact sort keys for tall objects next to raised cells, and how bridges
  sort. To check against recomp frames in the M3 exit comparison.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Neighbouring cells tessellate; lepton and cell projections agree | `ra2-sim` unit tests | exact |
| Shipped maps line up | `ra2tool map render` on every map | visual |
