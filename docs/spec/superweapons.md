# Superweapons

**Status:** Charging parity-checked (granting, recharge, holding on low
power). Firing, and each superweapon's effect, still to come.
**Crate:** `ra2-rules` (`production::recharge_frames`), `ra2-sim`
(`superweapon`)
**Issue:** #45

## Scope

Which superweapons a house has, how they charge, and (to come) firing them
and what each does. The AI's use of them is M5 (#47).

## Sources

- The recomp of `gamemd.exe` 1.001:
  - `SuperClass` (constructor `0x006CAEC0`, vtable `0x007F3FE8`) and its
    per-frame update (`0x006CBCA0`);
  - `SuperWeaponTypeClass` reading `RechargeTime=` (`0x006CED6E`);
  - the house's list of them, `HouseClass +0x254`: one per type, in
    `[SuperWeaponTypes]` order, whether the house has it or not.
- Scenario `allied-force-shield`: a Battle Lab (made cheap, with only the
  Construction Yard as a prerequisite, by the map's rules) gives the
  British player the Force Shield (made to recharge in half a minute). The
  power plant is sold while it charges and another built. The observer
  logs each house's superweapons every frame.

The recorder turns superweapons on for a scenario that asks
(`super_weapons = true`); the player's own settings have them off.

## Behaviour

| Field | `SuperClass` | Meaning |
|---|---|---|
| Type | `+0x28` | the `SuperWeaponTypeClass` |
| Owner | `+0x2C` | the house |
| Charge timer | `+0x30` start (−1 while held), `+0x38` frames | |
| Present | `+0x6D` | the house has it |
| Charged | `+0x6F` | ready to fire |
| On hold | `+0x70` | not charging |

- **Charge time:** `RechargeTime=` (minutes) × 900, truncated on the x87;
  5 minutes (4500 frames) if it's 0. `[recomp 0x006CED6E, 0x006CE5E9]`
- **Granting:** a building's `SuperWeapon=` (and `SuperWeapon2=`) comes to
  its house the frame after the building is placed, its charge timed from
  the frame it was placed. `[observed]`
- **Losing:** a house with none of a superweapon's buildings left loses
  it. `[hypothesis]` (not recorded yet)
- **Low power:** a superweapon with `IsPowered=yes` holds while its house
  draws more power than it makes. It keeps what was left as of the frame
  before. When the power is back it carries on, timed from the frame
  before. `[observed]`
- **Charged:** the frame after its time runs out. `[observed]`

## Open questions

- Firing (the `SPECIAL` event), and each superweapon's effect: Nuke,
  Weather Storm, Chronosphere, Iron Curtain, Force Shield, Genetic Mutator,
  Psychic Dominator, Paratroopers, Spy Plane.
- What happens to the charge when a superweapon's building is lost and
  rebuilt; `UseChargeDrain=` (the Force Shield drains).

## Parity

`parity/scenarios/allied-force-shield.toml`: 8 checks at zero tolerance
(the superweapon at seven frames through granting, the hold and the
recharge, and power). Credits, power, superweapons and buildings match the
original on all 2600 frames.
