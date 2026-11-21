# Combat

**Status:** Implemented for the Hunt scenarios (timings fitted to the reference logs)
**Crate:** `ra2-sim` (`hunt`)
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

## What we implement

- **Targets**: a hunter keeps its target while it lives, else takes the
  nearest object of another house. `[hypothesis]` (threat ratings and
  alliances not yet weighed)
- **When**: turreted vehicles fire whenever the target is in range, moving
  or not: the Rhino (range 5.75) fires on frame 170, eight frames before
  it stops. Infantry fire only standing, from the frame after they stop.
  `[observed]`
- **Flight**: an arcing shell lands `1 + distance / 128` frames after it's
  fired (distance in leptons, truncated): 11 frames at 1,280, 12 at 1,454.
  Other projectiles land the next frame. `[observed]`
- **Reload**: `ROF` plus a draw of 0–2 from the game's generator, and 2
  more for infantry. `[observed]` (the draw's place in the generator's
  sequence isn't traced, so individual shots differ by a frame or two)
- **Damage**: `Damage` × the warhead's `Verses` for the target's armor,
  with the game's double arithmetic. Objects at 0 hit points or fewer are
  removed.

All three scenarios match the original's first hit to the frame and its
hit points at every checked frame.

## Open questions

- The reload's random part and the order of draws from the generator.
- Projectile flight time and turret rotation before the first shot.
