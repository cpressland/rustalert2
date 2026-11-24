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
  nearest living object of another house. `[hypothesis]` (threat ratings and
  alliances not yet weighed)
- **Guard**: an armed unit on Guard (the default for map units and
  anything idle) takes the nearest living enemy within its weapon's
  range, keeps it while it lives and stays in range, and fires on it
  without moving. Enemies out of range are left alone, even within its
  sight. It can first take a target on frame 46 (infantry, which fire the
  frame after) or 47 (vehicles). `[observed]` (`guard-mcv`, three seeds:
  a GI 3 cells from an MCV hit it on 48, 71 and 94; a Grizzly 4 cells off
  on 68, after its turret turned a quarter and its shell flew 9 frames;
  Grizzlies 9 and 14 cells off didn't move) Where the 46 comes from (a
  scan interval, or the mission's first delay) isn't traced.
  - A computer's GI on Guard deploys after `AIAutoDeployFrameDelay=`
    frames on Guard (100 for the civilian houses) and fires its deployed
    weapon (abilities.md).
  - A player's units on Guard don't take on the civilian houses (Neutral,
    Special) by themselves; the civilians take on anyone. `[observed]`
    (`allied-gi-deploy`: a GI beside a sleeping Neutral Grizzly fired only
    when ordered)
  - A unit on Guard fires back at whoever last hurt it, within its range,
    even a civilian it wouldn't take on by itself. `[observed]`
    (`grizzly-crush-guard`: a player's Grizzly shot by a Neutral GI fired
    back, 16 a hit about every 62 frames) Ours keeps its current target
    first, then that attacker, then the nearest enemy. `[hypothesis]`
- **Infantry deaths**: infantry killed by a shot fall and are taken away
  15 frames later. `[observed]` (a spy shot dead on 1698, gone on 1713)
- **Turrets**: a turret (`Turret=yes`) turns toward the target at the
  body's `ROT=` × 256 a frame, from the body's facing at first, and the
  unit fires only once it's on the target. `[observed]` (a Grizzly's
  turret, 135° off its target, fired 20 frames after a forced attack:
  24,576 ÷ 1,280, rounded up; the shell landed the frame after) The facing
  toward a point is exact on the eight principal directions and an
  integer CORDIC elsewhere; the game's own arctangent isn't traced.
- **When**: turreted vehicles fire whenever the target is in range, moving
  or not: the Rhino (range 5.75) fires on frame 170, eight frames before
  it stops. Infantry fire only standing, from the frame after they stop.
  `[observed]`
- **Minimum range**: a unit doesn't fire on a target closer than its
  weapon's `MinimumRange=`, though it still closes to its range.
  `[observed]` (`yuri-magnetron`: a Grizzly dropped 1.4 cells from the
  Magnetron, whose beam has 3, wasn't lifted again where it lay;
  `allied-ifv-repair`: the IFV's HoverMissile, 1, fired from exactly a
  cell) Backing off to fire again isn't modelled.
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

## Death

- A destroyed vehicle's death weapon goes off where it stood, the frame
  after. `[recomp 0x0070D690]` `[observed]` (`grizzly-veteran`)
  - Its own `DeathWeapon=`, at that weapon's `Damage=` ×
    `DeathWeaponDamageModifier=` (default 1), truncated;
  - else its primary weapon, the same way;
  - else `[CombatDamage] DeathWeapon=` (`DefaultDeathWeapon`: warhead
    `DeathWH`, 100% against everything, `CellSpread=1.5`,
    `PercentAtMax=.5`) at half its `Strength=` (0.5 at `0x7E1738`).
- The blast falls off as any area damage does (superweapons.md). Infantry
  it kills fall and are taken away 15 frames later. `[observed]`
- A `Crewed=yes` vehicle's crew gets out, on the frame it's destroyed, if
  a draw from 0 to 2³¹ − 2, × 1 ÷ (2³¹ − 2), is below `CrewEscape=` (50%):
  one of the side's crew (`AlliedCrew=` E1, `SovietCrew=` E2, `ThirdCrew=`
  INIT) where the wreck stood, with 5 to half its strength at random.
  `[recomp 0x007381F7]` With the map's `CrewEscape=100%`, the MCV's
  crewman stepped out and the blast (500 at his feet) killed him on the
  next frame. `[observed]`
- Kills by a wreck's blast aren't credited to the wreck's killer.
- Not modelled yet: a building's crew (see repair.md, "Crews") and its
  death explosion; deaths by superweapons leave neither blast nor crew in
  our engine.

## Open questions

- The reload's random part and the order of draws from the generator.
- Projectile flight time.
- Whether turrets have their own rate (`TurretROT`-like) for other units.

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `*-hunts-mcv` | see locomotors.md | one shot, a few frames |
| `guard-mcv` | the MCV's first damage, its health at 60 and 110, the Grizzlies' positions at 300 | exact |
| `grizzly-veteran` | the kill, the crewman out and killed by the wreck's blast | ±2 frames |
