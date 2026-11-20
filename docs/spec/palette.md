# Palettes and house colours

**Status:** Implemented
**Crate:** `ra2-formats` (`pal`)
**Issue:** #14

## Scope

The `.pal` format, converting it to 8-bit colour, and how a house's colour
(`[Colors]` in the rules) becomes the 16 colours in palette slots 16 to 31
that units are drawn with. Lighting and tinting of palettes (theater
lighting, ion storms) belong to the lighting note (M3).

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs.
- Observation: the shipped palettes (`unittem.pal`, `temperat.pal`,
  `anim.pal` and others in `cache.mix`) and `[Colors]` in `rulesmd.ini`.

## Behaviour

### `.pal`

- 768 bytes: 256 entries of red, green and blue, each a 6-bit value from 0 to
  63. `[observed]` The largest value in `unittem.pal` is 63.
- To get 8-bit colour, each value is shifted left by 2 (63 becomes 252).
  `[community]` The game draws to 16-bit surfaces through `ConvertClass`
  (`[recomp 0x0048E740]`), so what reaches the screen has 5 or 6 bits per
  channel. That conversion belongs to the renderer note.
- Slot 0 is transparent in sprites. That belongs to the SHP note.

### House colours

- `[Colors]` maps a name to `H,S,V`, each 0 to 255. For example,
  `DarkRed=0,230,255` and `NeonGreen=0,0,0` (labelled "Black" in the file).
  `[observed]` The rules reader parses each triple with `"%d,%d,%d"`
  (`0x008189B0`, read at `0x00474C70` from `0x0066D3A0`).
- **The ramp** `[recomp 0x0068C3B0]`. For step *i* from 0 to 15, palette
  slot 16 + *i* gets:
  - hue: unchanged;
  - saturation: `trunc(sin(50° + i × 2.667°) × S)`, using constants
    `0.8726646` and `0.0465421` rad at `0x007F0E80` and `0x007F0E88`;
  - value: `trunc(cos(20° + i × 4.667°) × V)`, using `0.3490659` and
    `0.0814487` rad at `0x007F0E90` and `0x007F0E98`. **For i = 0 the angle
    is forced to 90°** (`0x0068C41D`), so the value is 0.

  The resulting HSV is converted to RGB (below).
- So **slot 16 is black**, slots 17 to 31 darken from bright to black, and
  slot 31 is black too (20° + 15 × 4.667° = 90°). This doesn't look like the
  artist-made default ramp in `unittem.pal`, which runs from 63 at slot 16
  down to 8 at slot 31. We follow the code. **We need to check it against a
  rendered frame** (open questions).
- `sin` and `cos` are the game's fast versions: a table of 8,192 `float`
  sines over a full turn, extended by a quarter turn so `cos` is `sin` with a
  2,048-step offset (`0x004CACB0`, `0x004CAD00`, table at `0x0084F084`).
  - The angle is multiplied by `2607.5945` (`0x008223B0`), truncated, then
    halved, rounding a remainder up.
  - The table holds `table[0] = 0` and `table[2048] = 1`.
  - `_ftol` truncates the products toward zero.
- The scheme's palette is then handed to `LightConvertClass` (`0x00555DA0`),
  which belongs to the lighting note.

### HSV to RGB

`[recomp 0x00517440]` Integer arithmetic, with divisions by 255 compiled as a
multiply by `0x80808081`. For hue *H*, saturation *S* and value *V*:

- `sector = 6H / 255` and `f = 6H mod 255`;
- `p = V(255 − S) / 255`;
- `q = V(255 − fS/255) / 255`;
- `t = V(255 − (255 − f)S/255) / 255`;
- the channels are `(V,t,p)`, `(q,V,p)`, `(p,V,t)`, `(p,q,V)`, `(t,p,V)` or
  `(V,p,q)` for sectors 0 to 5. Sector 6 (H = 255) is treated as 0.

The order of channels by sector is the textbook one `[community]`. In the
recomp it's computed with branchless arithmetic we haven't fully traced. The
terms `p`, `q` and `t` match the code.

## Results on a real install

`ra2tool pal ramp` prints and draws the ramp for all 21 `[Colors]` entries in
`rulesmd.ini`. Each is its hue, bright at slot 17 and fading to black at slot
31, with slot 16 black. `ra2tool pal png unittem.pal --house DarkRed` draws a
unit palette with the ramp applied.

## Quirks we match

- Slots 16 and 31 of every house ramp are black, as the code computes. This
  is pending confirmation (below).

## Open questions

- **Slot 16.** Does the game really draw remap slot 16 as black? We'll
  confirm by comparing a rendered unit against a recomp frame in M3. If the
  scheme palette isn't what the unit draw path uses, this note changes.
- How 6-bit palette values become 8-bit. Is it `<< 2`, or `<< 2 | >> 4`?
  Where does the game do it (`PaletteClass` load or `ConvertClass`)?
- The channel order in the HSV-to-RGB tail at `0x005174E6`.
- `0x0068C500` is a second ramp builder with the same constants, which works
  on a copied 768-byte palette. When is it used instead?

## Parity

| Check | How | Tolerance |
|---|---|---|
| Parsing, HSV to RGB, trig table, ramp shape, synthetic | `ra2-formats` unit tests | exact |
| Real palettes parse; every `[Colors]` ramp builds | `ra2tool pal`, `tests/real_install.rs` | exact |
| Remapped unit colours match the original's frames | M3 frame comparison against the recomp | per-channel ±1 (not done yet) |
