# Sprites, objects and draw order

**Status:** Implemented (static frames; animations and damage to come)
**Crate:** `ra2-render` (`scene`, `voxel`)
**Issues:** #30, #31, #32

## Scope

Drawing a map's overlays, smudges, terrain objects, buildings, infantry
and vehicles in their starting state. Animation is left to the game loop
(M4).

## Sources

- `rulesmd.ini` and `artmd.ini`, read through [ini.md](ini.md).
- [shp.md](shp.md), [voxel.md](voxel.md) and [palette.md](palette.md).
- `ra2tool map render` on the shipped maps.

## Behaviour

- **Image names.** An object's art is its `Image=` (or its ID if there is
  none). With `NewTheater=yes`, the second letter becomes the theater's
  letter, falling back to `g`. With `Theater=yes`, the file is
  `{image}.{theater ext}`. `[recomp]`
- **Palettes.**
  - Ore and gems use the theater palette.
  - Terrain objects and smudges use the iso palette.
  - Units and buildings use `unit{ext}.pal`, remapped to the owner's house
    colour.
  - Walls use the Neutral house's remap. `[observed]`
- **Overlays.** Overlays are `[OverlayTypes]` in list order. The frame is the
  cell's overlay data byte. `[recomp]`
- **Terrain objects.** Keys are `yyyxxx` cell numbers. `[recomp]`
- **Buildings.** A building is drawn at its foundation's centre. It sorts by
  the foundation's bottom corner. `[observed]`
- **Infantry.** Infantry draw the `Ready` frame of their art's `Sequence`
  for their facing. Each sub-cell offsets them within the cell. `[recomp]`
- **Shadows.** SHPs with shadows keep them in the second half of their
  frames. Shadows darken what is under them and draw before the body.
  `[recomp]`
- **Vehicles.** A vehicle is its hull voxel plus `tur` and `barl` parts at
  its facing (0–255, clockwise from north).
  - Each voxel is plotted 2 pixels wide with a depth test. `[recomp]`
  - Voxel scale is 256/36 leptons per voxel unit. `[hypothesis]`
  - Voxels are flat-shaded with `voxels.vpl` section 8, the section that
    leaves colours unchanged. `[hypothesis]` (stand-in for normal lighting)

## Open questions

- Lighting voxels by their normals (needs the game's normals tables), and
  the 256/36 scale, against recomp frames.
- Damaged frames, `ActiveAnim`/`IdleAnim`, bibs and building anims. These
  need the game loop's timers.
- Bridges, which are overlays drawn on their own layer.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Voxel projection | `ra2-render` unit test | exact |
| Objects match the original's frames | M3 exit comparison | visual |
