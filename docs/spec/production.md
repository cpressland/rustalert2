# Power and production

**Status:** Parity-checked for a deploy, buildings and infantry from
one or two factories at full power. Low power is traced, not yet
recorded. Queues, cancel and hold, placement rules, build-up, selling,
and vehicles and aircraft leaving their factories are still to come.
**Crate:** `ra2-rules` (`production`), `ra2-sim` (`production`)
**Issue:** #43

## Scope

How the human player's build orders turn into objects and spent credits,
and each house's power. The sidebar (#48) and the AI's choices (#47) are
elsewhere; here an order arrives as the event a click would send.

## Sources

- The recomp of `gamemd.exe` 1.001:
  - build time: `TechnoClass::Time_To_Build` (`0x006F47A0`), a type's base
    time (`0x00711EE0`), the house's power ratio (`0x004FCE30`) and its
    per-kind multiplier (`0x0050C0A0`);
  - rules: reading `[General]` (`0x0066EB60`…`0x0066EBCE`) and
    `BuildTimeMultiplier=` (`0x0071437C`);
  - events: the sidebar's `PRODUCE` click (`0x006AB4CC`), placing
    (`0x004ABB0A`), and the event constructors (`0x004C65E0`,
    `0x004C6970`, `0x004C6AE0`).
- Scenario `allied-base`: the British player's orders, queued by the
  observer: deploy the MCV, build and place a power plant, a barracks, a
  GI, a second barracks and two more GIs. The log has the house's credits
  and power every frame.

## Behaviour

### Orders

An order sent on frame `f` takes effect on `f + 1`, as the original
executes its events. `[observed]` (every object below appears on `f + 1`)

- **Deploy:** the MCV is replaced by its `DeploysInto=` building 26 frames
  later, with the building's top-left half its foundation (rounded down)
  up and left of the MCV's cell: the AMCV at (35,35) gives a Construction
  Yard at (34,34). `[observed]`
- **Produce:** if the house has a factory for that kind (`Factory=` on a
  building it owns), the object is made at once but kept off the map (in
  limbo; logged at 0,0) while it's built. `[observed]`
- **Place:** a finished building goes on the map with its top-left at the
  cell given. `[observed]`

### Build time `[recomp 0x006F47A0]`

1. `Cost × BuildSpeed × 0.9`, truncated (`BuildSpeed=` 0.7: a power plant
   at 800 takes 504 frames);
2. × the country's multiplier for that kind, truncated (1 for every
   country so far);
3. × `BuildTimeMultiplier=` (a float), truncated;
4. ÷ the production speed, truncated. The speed is 1 unless power is
   short: then 1 − (1 − output ÷ drain) × `LowPowerPenaltyModifier=`, at
   least `MinLowPowerProductionSpeed=` and at most
   `MaxLowPowerProductionSpeed=`. The ratio is a single-precision float, so
   60% power builds a 126-frame GI in 209 frames, not 210;
5. × `MultipleFactory=` (0.8) once for each factory of that kind beyond the
   first, truncating each time: a GI from two barracks takes 100 frames;
6. walls × `WallBuildSpeedCoefficient=`.

The game does this in doubles and singles; so do we, in `ra2-rules`. IEEE
arithmetic gives the same result on every platform, and the game runs the
x87 at 53-bit precision. Only the integer result reaches the simulation.

### Steps and payment `[observed]`

A factory counts 54 steps, one every `time ÷ 54` frames (truncated, at
least 1), from the frame the order takes effect. On each of the first 53
steps it takes what's still owed ÷ the payments left, rounded down. So a
power plant pays 15 for 48 steps, then 16 for 5. On the 54th step it's
done. Recorded for three costs and step lengths of 9, 5, 2 and 1 frames.

### Leaving the factory `[observed]`

A finished GI appears the frame after the 54th step. It comes out of the
house's first barracks at its top-left + (320, 448) leptons, and stands a
frame. Then it walks to sub-spot (64, 192) of the first cell along the row
below the barracks that nobody stands on or is heading to.

### Power `[observed]`

A house's power output is the `Power=` of its buildings that make power,
counted from the frame after each is placed. Its drain is the `Power=` of
those that use it, counted from the frame each is placed. Production reads
the house's output and drain when an order starts.

## Quirks we match

- The power ratio is rounded to a float before it slows production (see
  above).

## Open questions

- Queues (more than one item per kind), cancelling, holding, and what
  happens when credits run out.
- Placement rules (adjacency, `Adjacent=`, clear ground) and the build-up
  animation; selling.
- Vehicles and aircraft leaving their factories; which factory is primary.
- Power output from damaged buildings, and low power recorded.
- The country multipliers (`HouseTypeClass +0x134`…`+0x144`) and the keys
  that set them.
- The GI's exact walking path out (we're within a few leptons on the way,
  exact on arrival).

## Parity

`parity/scenarios/allied-base.toml`, 12 checks, all at zero tolerance:
- the yard's creation frame;
- credits at six frames through the power plant and the GIs from one and
  two barracks;
- power at two frames;
- every building's and GI's position at the end.
Credits and power match the original on every frame of the run.
