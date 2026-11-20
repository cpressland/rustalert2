# Theaters and TMP terrain tiles

**Status:** Implemented
**Crates:** `ra2-formats` (`tmp`), `ra2-vfs` (`theater`)
**Issue:** #17

## Scope

The six theaters, the archives, INI and palettes each brings in, how a map's
tile number names a file, and the TMP tile format. Drawing terrain with
height, lighting and ramps is M3.

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs.
- Community: the TS/RA2 TMP layout.
- Observation: `ra2tool tmp check` and `tmp png` on the Steam install.

## Behaviour

### The theater table

`[recomp 0x007E1B78]`: six rows of 0x70 bytes. The name is at `+0x00`, the
root at `+0x30`, the tile archive at `+0x3A`, its YR base at `+0x44`, the
extension at `+0x4E`, and the letter at `+0x56`.

| Name (map `Theater=`) | Root | Tile archive | YR base | Extension | Letter |
|---|---|---|---|---|---|
| `TEMPERATE` | `TEMPERAT` | `ISOTEMP` | `ISOTEM` | `TEM` | T |
| `SNOW` | `SNOW` | `ISOSNOW` | `ISOSNO` | `SNO` | A |
| `URBAN` | `URBAN` | `ISOURB` | `ISOURB` | `URB` | U |
| `DESERT` | `DESERT` | `ISODES` | `ISODES` | `DES` | D |
| `NEWURBAN` | `URBANN` | `ISOUBN` | `ISOUBN` | `UBN` | N |
| `LUNAR` | `LUNAR` | `ISOLUN` | `ISOLUN` | `LUN` | L |

The rest of each row holds floats (1.0 and 1.6, or 0.8 and 1.1 for snow) and
a double (1.0, or 0.75 for snow), which belong to the lighting note. The
letter is what's swapped into theater-specific art names (`GAPOWR` becomes
`GTPOWR`) `[community]`; that belongs to the art note.

### What a theater loads

At theater init (`[recomp 0x005349C0]`), the game builds five names with
`%s.MIX` and `%sMD.MIX`, then mounts them in this order
(`0x00534AC0`–`0x00534BD0`, traced through the stack buffers):

1. `{root}MD.MIX` (`TEMPERATMD.MIX`)
2. `{root}.MIX` (`TEMPERAT.MIX`)
3. `{ext}.MIX` (`TEM.MIX`)
4. `{YR base}MD.MIX` (`ISOTEMMD.MIX`)
5. `{tile archive}.MIX` (`ISOTEMP.MIX`)

The same function also handles:

- **Palettes:** `{root}.PAL` (`0x00534BF3`), `UNIT{ext}.PAL` (`0x00534CB2`)
  and `ISO{ext}.PAL` (`0x0054548D`, for tiles);
- **Theater INI:** `{root}MD.INI` (`0x005454FE`), for example
  `TEMPERATMD.INI`;
- **Slope art:** `SLOP01Z.{ext}` (`0x00545300`).

These mounts are added after the startup archives, so the earliest-mounted-
wins rule ([vfs.md](vfs.md)) puts them last.

### Tile sets

- The theater INI has `[TileSet0000]`, `[TileSet0001]` and so on, each with
  `SetName`, `FileName` and `TilesInSet`. `[observed]`
- A map's tile number counts through the sets in order. Tile *n* of a set is
  the file `{FileName}{n:02}.{ext}`, numbered from 01. `[community]`
- Sets with `NonMarbleMadness=` are the map editor's "Marble Madness"
  previews, and `AllowToPlace=no` marks sets that can't be placed. Most of
  these have no files in the game's archives. `[observed]`

### TMP

All integers are little-endian.

- **Header**: `u32` template width and height in cells, then `u32` cell
  width and height in pixels (60 and 30). Then `width × height` `u32`
  offsets, where 0 is an empty cell.
- **Cell header**, 52 bytes:

  | Offset | Field |
  |---:|---|
  | 0 | `i32` x |
  | 4 | `i32` y (pixels in the template) |
  | 8 | `u32` extra art offset |
  | 12 | `u32` depth offset |
  | 16 | `u32` extra depth offset (all three relative to the cell) |
  | 20 | `i32` extra x |
  | 24 | `i32` extra y |
  | 28 | `u32` extra width |
  | 32 | `u32` extra height |
  | 36 | `u32` flags: bit 0 extra art, bit 1 depth, bit 2 damaged art |
  | 40 | `u8` height |
  | 41 | `u8` terrain type |
  | 42 | `u8` ramp type |
  | 43 | radar left colour (3 bytes) |
  | 46 | radar right colour (3 bytes) |
  | 49 | 3 bytes padding |

- **Image**: the diamond's palette indices, `w × h / 2` bytes, right after
  the header. Rows widen by 4 to the middle and then narrow by 4:
  - for 60×30, rows 0 to 14 are 4, 8, … 60 wide;
  - rows 15 to 29 are 56, 52, … 0 wide;
  - each row is centred.

  `[observed]` Tiles draw correctly this way: a clear tile is a clean
  diamond, and a cliff's cells and extra art line up.
- **Depth**: one byte per image pixel. **Extra art**: `width × height` bytes,
  with optional depth.
- A cell is drawn raised by `height × 15` pixels (`tmp png`), the height step
  in [the coordinates note](README.md) (M3).

## Results on a real install

`ra2tool tmp check` mounts each theater after the startup archives and parses
every tile its INI lists. The `{root}MD.MIX` archive is absent except for
snow (`snowmd.mix`).

| Theater | Sets | Tiles listed | Parse | Cells | With extra art | Absent (editor-only) | Absent (other) |
|---|---:|---:|---:|---:|---:|---:|---:|
| Temperate | 82 | 838 | 511 | 2,628 | 551 | 327 | 0 |
| Snow | 83 | 964 | 563 | 2,843 | 755 | 379 | 22 |
| Urban | 111 | 1,081 | 712 | 3,998 | 712 | 369 | 0 |
| Desert | 82 | 726 | 726 | 3,458 | 595 | 0 | 0 |
| New Urban | 122 | 1,175 | 806 | 3,412 | 963 | 369 | 0 |
| Lunar | 85 | 198 | 154 | 310 | 49 | 44 | 0 |

No tile fails to parse, and every cell has depth data. Snow's 22 other
absences are the Marble Madness ramp and height sets (`mslop`, `hyte`), which
`snowmd.ini` lists without the editor-only keys.

## Open questions

- What the game does with a map cell whose tile file is missing. This is
  checked with maps (#18).
- Damaged art (flag bit 2): where is it stored? No cell in the survey sets it.
- The meaning of terrain and ramp types belongs to the movement and terrain
  notes (M4).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Diamond rows, cells, extra art, tile numbering, synthetic | unit tests | exact |
| Every theater mounts; every listed tile that exists parses | `ra2tool tmp check`, `tests/real_install.rs` | exact |
| Terrain renders like the original | M3 frame comparison | per pixel (later) |
