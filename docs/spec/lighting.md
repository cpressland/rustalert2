# Map lighting

**Status:** Implemented (normal lighting and lamps; ion storm and dominator need the game loop)
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
- Values are kept as integer thousandths: `Ambient` and the tints are read
  ×100 (+0.01, truncated) and then ×10; `Ground` and `Level` ×1000
  (+0.01, truncated). `[recomp]` (0x0068A8xx)
- Each cell's light is computed on its own (`0x00484180`): `[recomp]`
  - start from `Ambient` and the three tints;
  - for each lamp whose centre is within its range of the cell's centre
    (both in leptons, in x and y only), add `f × LightIntensity` to the
    intensity and `f × Light…Tint` to each tint, where
    `f = (range − distance) × 1000 / range`, all integer;
  - add `Level × h − Ground` to the intensity, for height level `h`, and
    cap it at 2,000.
- A lamp is a building in `[Structures]` whose type has `LightVisibility=`
  (its range, in leptons), with `LightIntensity`, `LightRedTint`,
  `LightGreenTint` and `LightBlueTint`. Its centre is its foundation's
  centre. `[hypothesis]` (centre; all lamps on, without power checks)
- Each channel's factor is intensity × tint, clamped to 0–2.
- Every palette entry except 0 (transparent) is multiplied by its channel's
  factor, rounded and clamped to 255. `[hypothesis]` (exact rounding not
  traced)
- Terrain uses the theater's iso palette lit by its cell's light. Objects
  use the palette their kind draws with, lit by the light of their cell.
  `[recomp]`
- Buildings with `InvisibleInGame=yes` (the invisible lamp posts) aren't
  drawn. `[community]`

## Quirks we match

- None yet.

## Open questions

- The same routine computes a second value with `h + 4` in place of `h`.
  Four levels is a high bridge's height, so we take it as the light on a
  bridge deck and use it for high bridge overlays. `[hypothesis]` Comparing again with lamps and `Ground` in is part of the
  M3 exit check (#35).
- Ramps take their cell's level; no per-slope term has turned up in the
  cell lighting routine.
- `IonAmbient` and the other ion storm and dominator sets replace the
  normal values while those are active (the routine checks the ion storm,
  dominator and another mode first). They're set off by superweapons and
  triggers, so they come with the game loop (M4).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Tint and level arithmetic | `ra2-render` unit test | exact |
| A lit frame against the recomp | `XMP30S6` frame, top-decile brightness | ±2 levels |
