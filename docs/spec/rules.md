# Rules tables

**Status:** Implemented (the core keys of each table; more keys arrive
with the systems that read them)
**Crate:** `ra2-rules`
**Issue:** #37

## Scope

Turning `rulesmd.ini` (with a map's overrides loaded over it) and
`artmd.ini` into typed tables the simulation indexes by number. What each
value does in play belongs to the notes of the system that uses it.

## Sources

- The shipped `rulesmd.ini` and `artmd.ini`; ModEnc for key meanings
  `[community]`; the recomp where noted.

## Behaviour

### Reading

- Values are read as the game reads them (see ini.md): `atoi` for ints,
  `atof` (with `%` meaning /100) for numbers, the first letter for
  booleans, `strtok` lists. A map's own sections are loaded over the rules
  before the tables are built.
- `[BuildingTypes]`, `[VehicleTypes]`, `[InfantryTypes]` and
  `[AircraftTypes]` are read in order; an ID listed twice counts once.
  `[community]`
- Weapons, projectiles and warheads are allocated when first named, in
  the order they're met (the game's `FindOrAllocate`), so a weapon named
  by a unit exists even if it has no section. `[Warheads]` lists warheads
  up front. `[community]`
- A techno's art section is its `Image=` (else its ID); buildings take
  `Foundation=` (`WxH`, default 1×1) from there.

### Other tables

- `[Animations]` lists animations; their keys are in the **art** file:
  `Rate`, `LoopStart`, `LoopEnd`, `LoopCount` (−1 forever), `Next`,
  `Damage` (a double) and `Warhead`, `Translucent`, `Shadow`. An
  animation's warhead joins the warhead table. `[community]`
- `[SuperWeaponTypes]`: `Name`, `Type` (what it does), `RechargeTime` (in
  minutes, a double), `IsPowered`, `WeaponType` (joins the weapon table,
  read before the techno weapons) and `Range` (cells, a double).
  `[community]`
- `[ParticleSystems]`: `HoldsWhat` (a `[Particles]` type), `BehavesLike`,
  `ParticleCap` and `SparkSpawnFrames`. `[community]`

### Units of measure

| Key | Stored as |
|---|---|
| `Range=`, `MinimumRange=`, `CellSpread=` | leptons: cells × 256, truncated `[community]` |
| `Verses=`, `PercentAtMax=`, `ProneDamage=`, `ConditionYellow/Red=` | the double the game reads, kept as its bits |
| `ROF=` | frames |
| `Speed=` (technos) | as written, and as the game keeps it: clamped 0–100, × 256 / 100, capped at 255 `[recomp 0x0071465A]` |
| `Sight=` | cells |

- Factors the game keeps as doubles are applied by multiplying in double
  precision and truncating toward zero. The simulation does this with
  integers on the double's exact bits, so results match bit for bit,
  including the game's own rounding: `29%` of 100 is 28. `[recomp]` (the
  game's damage maths runs on the x87 at 53-bit precision; to confirm per
  call site)

### Defaults

| Key | Default |
|---|---|
| `TechLevel` | −1 (unbuildable) `[community]` |
| `Armor` | `none` `[community]` |
| `Burst` | 1 `[community]` |
| `Verses` | 100% for every armor type not listed `[hypothesis]` |
| `PercentAtMax`, `ProneDamage` | 100% `[hypothesis]` |
| `AG` (projectile hits ground) | yes `[hypothesis]` |
| `ConditionYellow`, `ConditionRed` | 50%, 25% `[community]` |
| Ore and gem value (`[Riparius]`/`[Cruentus]` `Value=`) | 25, 50 |
| Animation `Rate`, `LoopEnd`, `LoopCount` | 1, 1, 1 `[hypothesis]` |
| Superweapon `RechargeTime`, `IsPowered` | 5 minutes, yes `[hypothesis]` |

Armor types, in `Verses=` order: none, flak, plate, light, medium, heavy,
wood, steel, concrete, special_1, special_2. Unknown names read as none.
`[community]`

## Open questions

- Which keys the game reads with which reader (int, number, leptons) for
  every key we add; each is confirmed as its system lands.
- The defaults marked `[hypothesis]`, against the type constructors in
  the recomp.
