# Superweapons

**Status:** Charging and firing parity-checked; of the effects, the Force
Shield's blackout. The other superweapons' effects are still to come.
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

### Firing

The player's click sends `SPECIAL_PLACE` (event 18): the superweapon (its
type's index, `+0x98`), and the target cell. `[recomp 0x004C6B60,
0x004AC230]` It takes effect the frame after it's sent, like other orders.
A charged superweapon fired then starts charging again from the frame
before. `[observed]`

### Force Shield (`Type=ForceShield`)

- The house makes no power from the frame after it's fired until
  `ForceShieldBlackoutDuration=` (1000) frames after its charge restarted.
  Its powered superweapons hold meanwhile, the Force Shield included.
  `[observed]`
- The house's buildings within `ForceShieldRadius=` (4) cells of the target
  take no damage for `ForceShieldDuration=` (500) frames. `[hypothesis]`
  (nothing attacks them in the scenario yet)

## Open questions

- The other superweapons' effects: Nuke, Weather Storm, Chronosphere, Iron
  Curtain, Genetic Mutator, Psychic Dominator, Paratroopers, Spy Plane;
  and the Force Shield's protection, under fire.
- What happens to the charge when a superweapon's building is lost and
  rebuilt; `UseChargeDrain=` (the Force Shield drains).

## Parity

`parity/scenarios/allied-force-shield.toml`: 15 checks at zero tolerance
(the superweapon through granting, the hold, the recharge and firing;
power through the blackout). Credits, power, superweapons and buildings
match the original on all 3600 frames.
