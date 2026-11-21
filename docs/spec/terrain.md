# Terrain and height

**Status:** Implemented (tile animations drawn at frame 0; their timing comes with the game loop)
**Crate:** `ra2-render` (`terrain`)
**Issue:** #29

## Scope

Drawing a map's `IsoMapPack5` cells from their theater's tiles. Tile
decoding is in [tmp.md](tmp.md), and the projection is in
[coords.md](coords.md).

## Sources

- [tmp.md](tmp.md) and [map.md](map.md) (tile sets, the pack format).
- `ra2tool map render` on every shipped map, compared with the original.

## Behaviour

- A cell's tile number counts through the theater INI's `[TileSetNN]`
  sections in order. Tile `k` of a set is `{FileName}{k+1:02}.{ext}`, and the
  cell's subtile picks a cell inside that template. `[recomp]`
- Tile `0xFFFF` and negative tiles are clear ground (tile 0). `[recomp]`
- **Marble Madness:** maps can name the editor-only Marble Madness tiles,
  which have no files. Those are drawn from the set named by
  `NonMarbleMadness`, at the same index. `[hypothesis]` (the shipped maps
  draw with no gaps)
- Any other tile without a file is drawn as clear ground. `[hypothesis]`
- A cell's diamond is drawn at its level ([coords.md](coords.md)). Its extra
  art is offset by the extra's position minus the cell's. `[observed]`
- Each height level gets its own lit palette ([lighting.md](lighting.md)).
  `[recomp]`

## Quirks we match

- Maps that name missing tiles still draw: they fall back as described
  above, and are never an error.

## Tile animations

- A theater INI section named after a tile set's `SetName` attaches an
  animation to tiles of that set: `TileNNAnim=` (art name),
  `TileNNXOffset`, `TileNNYOffset`, `TileNNAttachesTo` (the template's
  sub-tile it hangs off) and `TileNNZAdjust`. Waterfalls and tunnel tops
  use it. `[observed]` (temperatmd.ini)
- We draw frame 0 of each, with the objects, centred on the attaching
  cell's centre plus the offsets, in the iso palette (their art has
  `ShouldUseCellDrawer=yes`), sorted `YSortAdjust` pixels further down.
  `[hypothesis]` (anchor and palette)

## Open questions

- The anchor of tile animations: one shipped map has waterfalls and it's
  a night map, too dark to check placement by eye. The recomp comparison
  (#35) is the check. Their frame timing (`Rate`, `LoopStart`/`LoopEnd`)
  comes with the game loop.
- Water itself isn't animated in RA2; open water is static tiles.
  `[observed]`
- Ramps are lit by their cell's level alone (see lighting.md); a slope
  term would sit in the cell lighting routine and none has turned up.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Every cell of every shipped map resolves to a tile | `ra2tool map check` | exact |
| Terrain matches the original's frames | M3 exit comparison | visual |
