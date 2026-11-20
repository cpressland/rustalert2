# Fonts and UI art

**Status:** Implemented (decoding). Text layout and the shell's screens are M5.
**Crate:** `ra2-formats` (`fnt`, `pcx`)
**Issue:** #21

## Scope

The game's Unicode bitmap font (`game.fnt`) and the PCX images the shell
uses. The shell's SHP art is covered by [shp.md](shp.md). How the UI lays out
text (wrapping, colours, shadows) belongs to M5.

## Sources

- `gamemd.exe` 1.001 references `GAME.FNT` (string at `0x00818B98`).
- Observation: `ra2tool font check`, `font render` and `font pcx` on the
  Steam install.

## Behaviour

### `game.fnt`

`[observed]`: every size below is confirmed by the file adding up exactly.

- **Header**, 28 bytes:

  | Offset | Field | Value |
  |---:|---|---|
  | 0 | `"fonT"` | |
  | 4 | `u32` widest glyph | 20 |
  | 8 | `u32` bytes per glyph row | 3 |
  | 12 | `u32` rows per glyph | 16 |
  | 16 | `u32` line height | 17 |
  | 20 | `u32` glyph count | 29,655 |
  | 24 | `u32` bytes per glyph | 49 |

  The bytes per glyph equal `1 + stride × rows`.
- **The map**: 65,536 `u16`s, one per UTF-16 code unit. The map is
  **1-based**: 0 means no glyph, and *k* means glyph *k* − 1. With a 0-based
  reading every letter shows as the next one ("Yuri" renders as "Zvsj").
  With a 1-based one the largest value equals the glyph count, space is 4 px
  wide and `!` is 2 px.
- **Glyphs**: a `u8` advance width, then the rows, 1 bit per pixel, most
  significant bit first.
- The file is exactly `28 + 131,072 + count × 49` bytes: 1,584,195 for the
  shipped font.

On the Steam install the font maps **34,112** characters to **29,655**
glyphs. The widest glyph used is 16 px, against the header's 20. Latin,
accented Latin and CJK render correctly; Cyrillic isn't mapped.

### PCX

`[community]` The standard ZSoft PCX, version 5, RLE-encoded, 8 bits per
plane. A byte of `0xC0` or more repeats the next byte `b & 0x3F` times, with
runs crossing rows and planes.

- **One plane**: palette indices, followed by `12` and a 768-byte 8-bit
  palette at the end of the file.
- **Three planes**: each row is the red, green and blue lines in turn, giving
  24-bit colour.

On the Steam install, **200** paletted and **12** RGB PCX images decode in
`local.mix`, `localmd.mix` and `expandmd01.mix`, with 0 failures.

## Open questions

- Where the game draws text, and how it handles a character with no glyph:
  skipped, a box, or `?`. This belongs to M5.
- What the header's widest-glyph value (20) is used for, given no glyph is
  wider than 16.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Font and PCX layouts, synthetic | unit tests | exact |
| The shipped font maps cleanly; every PCX decodes | `ra2tool font check`, `font pcx`, `tests/real_install.rs` | exact |
| Text renders like the original | M5 frame comparison | per pixel (later) |
