# Maps: MAP, YRM and MPR

**Status:** Implemented (packed sections). Objects, triggers and the rest
are read by their systems.
**Crate:** `ra2-formats` (`map`, `codec`)
**Issue:** #18

## Scope

A scenario is an INI ([ini.md](ini.md)). This note covers:

- `[Map]` (size, local size, theater);
- the packed sections: `[IsoMapPack5]` (terrain), `[OverlayPack]` and
  `[OverlayDataPack]` (walls, ore, bridges and so on), and `[PreviewPack]`;
- the encodings those use: base64, block framing, LZO1X and LCW.

Units, structures, waypoints, triggers and the other sections are kept in the
INI for M4 and M7.

## Sources

- Community: the TS/RA2 map packs as documented for map editors and tools,
  and the LZO1X and Westwood LCW (Format80) formats.
- Observation: `ra2tool map check` decodes every map in the Steam install.

## Behaviour

### `[Map]`

- `Size=x,y,width,height` and `LocalSize=` (the playable area), as four
  integers.
- `Theater=`, one of the names in [tmp.md](tmp.md).

### Packed sections

- A packed section's values are base64 text, split over keys `1`, `2`, … We
  concatenate them in numeric key order. Whitespace is ignored and padding
  is optional.
- The decoded bytes are **blocks**: a `u16` compressed size, a `u16`
  decompressed size, then the compressed bytes. A block must decompress to
  exactly its stated size.

| Section | Compression | Decoded |
|---|---|---|
| `IsoMapPack5` | LZO1X | 11-byte cells: `i16` x, `i16` y, `i32` tile number, `u8` sub-tile, `u8` level, `u8` ice |
| `OverlayPack` | LCW | 262,144 bytes (512 × 512): overlay type per cell, `0xFF` for none |
| `OverlayDataPack` | LCW | 262,144 bytes: overlay frame or growth stage per cell |
| `PreviewPack` | LZO1X | `[Preview] Size=` width × height × 3 bytes, RGB |

- **Every shipped iso pack ends with 4 zero bytes** after the last whole
  cell. `[observed]` We keep them as `iso_trailing` and don't read them as a
  cell.
- A tile number counts through the theater's tile sets ([tmp.md](tmp.md)).
  `0xFFFF` and negative numbers mean clear ground. `[community]`

### LZO1X

The standard LZO1X stream: an optional first literal run (first byte > 17),
then instructions. `[community]` We wrote the decoder from the format
description, and it checks every bound:

- `0`–`15` are literal runs, extended by zero bytes;
- `16`–`31` are far matches (M4), where distance 0 is the end of the
  stream;
- `32`–`63` are medium matches (M3);
- `64` and up are near matches (M2);
- a short match (M1) follows a match's trailing literals, or, at a far
  distance, a literal run.

The stream must end exactly at the block's size.

### LCW (Format80)

`[community]` These are the commands, with absolute positions for the long
forms, as TS/RA2 write them:

| Bytes | Meaning |
|---|---|
| `0cccpppp pppppppp` | copy *c* + 3 bytes from *p* back |
| `10cccccc` | *c* literals; `0x80` alone ends the stream |
| `11cccccc p16` | copy *c* + 3 bytes from absolute position *p* |
| `0xFE n16 v` | fill *n* bytes with *v* |
| `0xFF n16 p16` | copy *n* bytes from absolute position *p* |

That every overlay pack in the game decodes to exactly 262,144 bytes confirms
the absolute form. `[observed]`

## Results on a real install

`ra2tool map check` covers the archives mounted at startup, plus the `.mmx`
and `.yro` map archives in the game folder:

| | |
|---|---|
| Maps | **287**, all decoding |
| Terrain cells | 4,761,557 |
| With overlays | 287 (both packs, exactly 512 × 512) |
| With previews | 239 (sizes exact) |
| By theater | temperate 118, snow 69, urban 51, new urban 28, desert 19, lunar 2 |

### Tile numbers against the theaters

- **21,115 cells use Marble Madness tile numbers** whose files aren't in the
  archives (ZMM roads, cliffs and so on). The game must draw the set named
  by `NonMarbleMadness=` at the same offset, rather than leave a hole.
  `[hypothesis]` This needs confirming in M3.
- **217 cells use the editor's "MM Height" sets** (`hyte`), which have no
  files. They're probably invisible height markers. `[hypothesis]`
- **5,318 cells in snow maps** (YR's own `multimd.mix` maps among them) name
  tiles 964 to 1,012, past the 964 tiles `snowmd.ini` defines. What the game
  draws there is open.
- Every other cell names a tile whose file exists.

## Open questions

- Out-of-range tile numbers (the snow case): are they clamped, treated as
  clear, or wrapped?
- Is Marble Madness substitution done at load or at draw time?
- What `ice` and the 4 trailing bytes are for.
- How the game reads multiplayer `.mpr`/`.yrm` headers and `[Basic]`, which
  belongs to the shell (M5).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Base64, LZO, LCW, blocks, overlays, synthetic | unit tests, including noise tests | exact |
| Every shipped map decodes, sizes exact | `ra2tool map check`, `tests/real_install.rs` | exact |
| Terrain and overlays look like the original | M3 frame comparison | per pixel (later) |
