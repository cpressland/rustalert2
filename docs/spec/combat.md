# Combat

**Status:** Spec'd from observation (implementation next)
**Crate:** `ra2-sim`
**Issue:** #41

## Scope

Firing, projectiles and damage. Target choice belongs to missions.

## Sources

- Parity reference logs of the three `*-hunts-mcv` scenarios.
- Rules: `Damage=`, `ROF=`, `Verses=` (see rules.md).

## Behaviour

- Damage per hit is the weapon's `Damage` × its warhead's `Verses` for the
  target's armor, truncated (see rules.md for the exact double
  arithmetic). The Grizzly's 105mm takes 65 off an MCV per hit, the
  Rhino's 120mm 90, the GI's M60 3. `[observed]`
- The first hit lands a few frames after the shooter stops: 11 frames for
  the Grizzly, 4 for the Rhino, 2 for the GI. `[observed]` (turning,
  aiming and the projectile's flight; to trace)
- Hits then come every `ROF` frames plus a small, varying extra: 60–62
  frames for the Grizzly (ROF 60), 64–67 for the Rhino. Recordings with
  different seeds differ here by 0–2 frames. `[observed]` (a random part
  of the reload, drawn from the game's generator)

## Open questions

- The reload's random part and the order of draws from the generator.
- Projectile flight time and turret rotation before the first shot.
