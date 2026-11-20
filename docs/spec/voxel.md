# VXL and HVA voxels

**Status:** Implemented (format). Rendering is M3.
**Crate:** `ra2-formats` (`vxl`)
**Issue:** #16

## Scope

Decoding voxel models (`.vxl`) and their animations (`.hva`) into sections,
voxels and transforms. Lighting by normal (`voxels.vpl`), the normals tables,
projection and the rasterizers belong to the voxel renderer note (M3). The
recomp has mapped that path in its `docs/voxels.md` (draw at `0x00706640`,
finish at `0x00754510`, shade table loaded at `0x00753B70`).

## Sources

- Community: the VXL and HVA layouts as documented for TS/RA2 voxel tools.
- Observation: `ra2tool vxl check` over the archives mounted at startup on
  the Steam install.

## Behaviour

All integers are little-endian. The floats are IEEE `f32`.

### VXL

- **Header**, 802 bytes:

  | Offset | Field |
  |---:|---|
  | 0 | `"Voxel Animation\0"` (16 bytes) |
  | 16 | `u32` palette count (1) |
  | 20 | `u32` section count |
  | 24 | `u32` tailer count (equal to the section count) |
  | 28 | `u32` body size |
  | 32 | `u8` remap start (16) |
  | 33 | `u8` remap end (31) |
  | 34 | a 768-byte palette (the game draws with its own; not used) |

- **Section headers**, 28 bytes each: a 16-byte name, then three `u32`s
  (number, 1, 0).
- **Body**: `body size` bytes. Offsets in the tailers are relative to its
  start.
- **Tailers**, 92 bytes each, after the body:

  | Offset | Field |
  |---:|---|
  | 0 | `u32` offset of the column start table |
  | 4 | `u32` offset of the column end table |
  | 8 | `u32` offset of the span data |
  | 12 | `f32` scale |
  | 16 | 12 × `f32`: a 3×4 transform, row-major |
  | 64 | 3 × `f32` minimum bounds |
  | 76 | 3 × `f32` maximum bounds |
  | 88 | `u8` x size, `u8` y size, `u8` z size |
  | 91 | `u8` normals mode |

- **Columns**: `x × y` `i32` start offsets, relative to the span data, with
  −1 for an empty column. Column *c* is at `x = c mod xsize`,
  `y = c div xsize`.
- **Spans**, within a column until *z* reaches the z size:
  1. a `u8` count of empty voxels to skip;
  2. a `u8` count *n*;
  3. *n* pairs of (`u8` palette index, `u8` normal index);
  4. *n* again.

  The trailing repeat lets the column be walked backwards.
  - We require the repeat to match.
  - We require the spans to cover the column's height exactly.
  - We require no voxel above the top.
- **Normals mode**: 2 means Tiberian Sun's normal set, 4 means Red Alert 2's.
  Both appear in Yuri's Revenge (below).

### HVA

| Offset | Field |
|---:|---|
| 0 | 16-byte name |
| 16 | `u32` frame count |
| 20 | `u32` section count |
| 24 | 16-byte name per section |
| then | per frame, per section, a 3×4 `f32` transform, row-major |

The size is exactly `24 + 16·sections + 48·frames·sections`, which makes an
HVA easy to recognise. A model's HVA has as many sections as its VXL.

## Results on a real install

`ra2tool vxl check` on the Steam install:

- **221** VXLs parse, with 1,212,644 voxels and 0 failures.
- **220** files parse as HVA.
- Normals mode 2: 133 sections. Normals mode 4: 113 sections.
- The highest normal index used is 255 in both modes, more than either
  normal set has (open questions).
- Pairing by name: the images of every vehicle, aircraft, building and voxel
  animation in the rules, with `tur` and `barl` parts, give **112** VXL+HVA
  pairs, all with matching section counts.
- `ra2tool vxl info htnk` shows, for example, the Apocalypse hull: one
  section, 46×34×12, 4,816 voxels, scale 1/12, normals mode 2.

## Quirks we match

- None that affect decoding.

## Open questions

- Normal indices above the normal set's size. Is the game's normals table
  padded to 256, and what lighting do those voxels get? This belongs to the
  renderer (M3).
- Do the section header's three `u32`s and the column end table matter to
  the game?
- How the renderer combines the section transform, the HVA frame and the
  scale. This belongs to M3, guided by the recomp's `docs/voxels.md`.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Spans, empty columns, HVA, damage, synthetic | `ra2-formats` unit tests | exact |
| Every VXL and HVA parses; pairs agree | `ra2tool vxl check`, `tests/real_install.rs` | exact |
| Rendered units match the original's frames | M3 frame comparison | per pixel (later) |
