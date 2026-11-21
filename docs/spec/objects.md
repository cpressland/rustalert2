# Sprites, objects and draw order

**Status:** Implemented (static frames, voxel lighting; animations and damage to come)
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
  - Voxels are lit by their normals; see "Voxel lighting" below.

## Voxel lighting

- **Normals.** The normal sets live in `gamemd.exe`, not in the data files.
  Pointers to the sets are at `0x8469E0 + mode×4` and their counts at
  `0x8469F4 + mode×4`. The counts are 16, 36, 64 and 245 for modes 1–4.
  `[recomp 0x00758670]`
  - We read them from the player's own `gamemd.exe` 1.001 when a map is
    drawn. Nothing from the executable is kept in the repository.
  - Without the executable, voxels are flat-shaded with `voxels.vpl` section
    8, which leaves colours unchanged.
- **The light.** `(−1/√2, −1/√2, 0)` rotated by 0.785375 rad (`0x7E1E68`)
  about y gives `(−0.5, −0.7071, 0.5)`. `[recomp 0x00754C00]` Units turn
  it into their own frame with the inverse of their matrix.
  `[recomp 0x00753D00]`
- **The view vector** is the model's up axis `(0, 0, 1)`. `[recomp 0x00753D00]`
- **Shade table.** Once per render, the game builds a 256-entry table that
  maps normal index to shade level. `[recomp 0x007586F0]` For each normal `n`
  in the section's mode:
  - `d = max(n·L, 0)`;
  - `H = normalize(L + V)`, `s = n·H`;
  - `spec = s / (k − k·s + s)` with `k = 3.0`, or 0 if negative;
  - level `= trunc(16 × (d + spec))`.

  Indices past the set keep 0. Indices 253–255 are always 16.
- **Colour.** A voxel's colour is `voxels.vpl[level][palette index]`.
  `[recomp 0x007DFAB0]`
- **HVA rotations.** A section's HVA rotation moves its voxels but doesn't
  turn the light. The table is built once for the whole unit. `[recomp]`

## Building details

- **Bib.** `BibShape=` names an SHP laid flat under the building at the
  same centre. Bibs are drawn before every other object. `[community]`
- **Damage.** A building at or below `[AudioVisual] ConditionYellow`
  (default 0.5) of its health (map health is out of 256) shows frame 1 of
  its SHP and of its bib. `[community]`
- **Active anims.** `ActiveAnim`, `ActiveAnimTwo`, `…Three`, `…Four` name
  anims drawn over the building at its centre plus `…X`/`…Y`, in the
  building's house palette, frame 0. When damaged, `…Damaged` replaces
  each. `[hypothesis]` (palette, frame; `…ZAdjust` and `…YSort` aren't
  used yet)
- `IdleAnim`, production and door anims need the game loop (M4).

## Voxel shadows

- A unit's shadow is a second render of its voxels (`0x00706BD0` →
  `0x00707280`), blitted by a converter that halves each pixel under it
  (`(c >> 1) & 0x7BEF` in 16-bit colour). `[recomp]`
- We render it as every voxel flattened to the ground (z = 0) under the
  same facing, plotted two pixels wide, and halve what's beneath.
  `[hypothesis]` (flattening fits what shows: an edge around the unit)
- The shadow pass also takes a vector set next to the light:
  `(Lx × −6, 0, 0)` at `0x887420` (`0x00754C00`), = (3, 0, 0). It likely
  shifts the shadow along the light; we don't apply it yet.

## Open questions

- What the shadow vector at `0x887420` does to the shadow's position, and
  whether turrets and barrels cast shadows (we draw all parts).

- Which world axes the light's x and y are. We take map x and y; the
  shading looks right (lit tops, darker camera-facing sides), but a
  close-up recomp frame would confirm it. The same goes for the 256/36
  voxel scale.
- A level of 32 (full diffuse and full specular) indexes past `voxels.vpl`'s
  32 sections. We clamp it to 31; the game reads whatever follows the table.
- Building anims' timing, `IdleAnim`, and `ZAdjust`/`YSort` ordering.
  These need the game loop's timers.
- Bridges, which are overlays drawn on their own layer.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Voxel projection | `ra2-render` unit test | exact |
| Objects match the original's frames | M3 exit comparison | visual |
