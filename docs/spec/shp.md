# SHP sprites (TS/RA2)

**Status:** Implemented
**Crate:** `ra2-formats` (`shp`)
**Issue:** #15

## Scope

The sprite format used for infantry, buildings, animations, cameos, the
sidebar and the mouse cursor. This note covers decoding frames to palette
indices. Which frames a unit uses (`[Sequence]`, facings, shadows in the
second half) and how they're drawn belong to the rendering notes (M3).

## Sources

- Community: the TS/RA2 SHP layout as documented for modding tools (the
  header, frame header and compression types).
- Observation: `ra2tool shp check`, which decodes every SHP in the archives
  mounted at startup on the Steam install. We haven't located the game's own
  decoder in the recomp yet (open questions).

## Behaviour

All integers are little-endian.

### File

- **Header**, 8 bytes:
  - `u16` 0, which marks the TS/RA2 flavour (TD/RA SHPs differ);
  - `u16` canvas width;
  - `u16` canvas height;
  - `u16` frame count.
- **Frame headers**, 24 bytes each, straight after the header:

  | Offset | Field |
  |---:|---|
  | 0 | `u16` x on the canvas |
  | 2 | `u16` y on the canvas |
  | 4 | `u16` width |
  | 6 | `u16` height |
  | 8 | `u8` flags (compression); 3 more bytes, unused |
  | 12 | 4 bytes: radar colour (R, G, B, unused) |
  | 16 | `u32`, unused |
  | 20 | `u32` offset of the frame's data from the start of the file, or **0 for an empty frame** |

- A frame is a rectangle on the canvas. Everything outside it is transparent.
- Palette index 0 is transparent.

### Compression

| Flags | Count in shipped data | Storage |
|---|---:|---|
| `0` | 948 frames | raw: `width × height` bytes |
| `1` | 4,288 frames | raw |
| `2` | 89 frames | each row is a `u16` length (including the two length bytes), then exactly `width` bytes |
| `3` | 28,270 frames | each row is a `u16` length (including itself), then bytes in which `00 n` means *n* transparent pixels and any other byte is a pixel |

So bit 1 means rows with lengths, and with it, bit 0 means zero runs.
`[community]` The counts are `[observed]`.

### Over-long runs

- **The last zero run of a row often counts past the row's width.** For
  example, a 76-pixel row encoded as 51 transparent, 4 pixels, then 22
  transparent, which makes 77. `[observed]` 604 of the 1,009 files have rows
  like this.
- Transparent pixels past the edge draw nothing, so we allow a run to
  overshoot. A *literal* pixel past the edge is still an error, and so is a
  row that leaves pixels uncovered or doesn't use all its bytes.
  `[hypothesis]` We believe the game's decoder clips rather than spilling
  into the next row. A frame comparison in M3 confirms it.

## Results on a real install

`ra2tool shp check` on the Steam install covers the 3,449 files in the 27
archives mounted at startup:

- **1,009** parse as SHP;
- **50,867** frames decode;
- **0** failures.

The other 24 files with a leading zero word are MIX archives, whose flags read
as a 1-pixel-wide canvas. They're rejected because their "frames" lie off it.

Theater-specific SHPs (buildings, terrain objects) are in archives mounted
when a scenario loads. They're surveyed with the theaters (#17).

`ra2tool shp png gi.shp --house DarkBlue --count 8` draws the GI in its eight
standing facings, in a house colour.

## Quirks we match

- Over-long trailing runs are accepted.

## Open questions

- Where the game decodes SHP frames (the blitters around the `Surface` and
  `Blit*` classes), and whether it clips over-long runs as we assume.
- Whether the radar colour and the unused fields are ever read.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Every compression, empty frames, damage, synthetic | `ra2-formats` unit tests, including a mutation test that must never panic | exact |
| Every SHP in the mounted game decodes | `ra2tool shp check`, `tests/real_install.rs` | exact |
| Decoded frames match the original's | M2 hook dumping decoded frames, by hash | exact (not done yet) |
