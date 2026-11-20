# Map lighting

**Status:** Implemented (normal lighting; ion storm, dominator and lamps to come)
**Crate:** `ra2-render` (`lighting`)
**Issue:** #33

## Scope

How a map's `[Lighting]` section tints and brightens the palettes the map is
drawn with.

## Sources

- `LightConvertClass` construction in the recomp (`0x00555DA0`).
- A recomp frame of `XMP30S6` (MCV at 38,108, level 5), compared pixel for
  pixel with `ra2tool map render --around 38,108`.

## Behaviour

- `[Lighting]` gives `Ambient`, `Red`, `Green`, `Blue`, `Ground` and
  `Level`. Missing keys default to 1, 1, 1, 1, 0 and 0. `[recomp]`
- For height level `h`, the intensity is `Ambient + Level × h`. Each channel's
  factor is that intensity times its tint, clamped to 0–2. `[recomp]`
  (tints are kept in thousandths and clamped at 2,000)
- Every palette entry except 0 (transparent) is multiplied by its channel's
  factor, rounded and clamped to 255. `[hypothesis]` (exact rounding not
  traced)
- Terrain uses the theater's iso palette lit per level. Objects use the
  palette their kind draws with, lit at the object's cell level. `[recomp]`

## Quirks we match

- None yet.

## Open questions

- The brightest 10% of pixels in the comparison frame match. The median band
  is a little darker in ours, which suggests the game also applies `Ground`
  or a per-cell ramp term.
- `IonAmbient` and the other ion storm and dominator overrides, and lamp
  posts (`LightIntensity`, `LightRed`…) that light nearby cells.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Tint and level arithmetic | `ra2-render` unit test | exact |
| A lit frame against the recomp | `XMP30S6` frame, top-decile brightness | ±2 levels |
