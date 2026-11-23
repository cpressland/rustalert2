# Power and production

**Status:** Parity-checked for a deploy, buildings, infantry, vehicles
and aircraft from one or two factories, queues, holding, cancelling,
selling, low power, lost prerequisites, free units and refused placements.
Naval yards (our generated maps have no water) and damaged-plant power
(traced) aren't recorded.
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
- The recomp: a factory starting and its affordability check
  (`0x004C9EA0`), the house re-timing its factories (`0x004CA6E0`) when
  it recalculates power (`0x00508C30`), a building's power (`0x0044E7B0`,
  `0x0044E880`), and the refund (`0x00711F60`).
- The recomp: the placement proximity check (`0x004A8EB0`) and the
  country multipliers' keys (`HouseTypeClass +0x134`…`+0x144`).
- Scenario `allied-air`: an Air Force Command and a Harrier.
- Scenario `allied-war-factory`: a refinery and its free Chrono Miner, a
  refused then valid placement, a GI held and resumed, a war factory and
  a Grizzly.
- Scenario `allied-queue-sell`: a queue of GIs, a refinery started, the
  power plant sold, a GI at low power, a GI cancelled.
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
- **Produce while busy:** infantry, vehicles and aircraft queue; the next
  starts the frame the one before leaves its factory. A building can't be
  ordered while another is building. `[observed]` (three GIs)
- **Hold:** the item stops after that frame's step; producing it again
  resumes it, its next step a rate after. Other factories keep going.
  `[observed]`
- **Placing** is refused (the order does nothing) unless some cell within
  `Adjacent=` (default 3) + 1 cells round the new foundation holds one of
  the house's buildings with `BaseNormal=` (default yes). `[recomp
  0x004A8EB0]` We also refuse a foundation over a building or a vehicle
  `[hypothesis]`; the game's terrain checks aren't traced.
- **Free units:** a building's `FreeUnit=` (a refinery's harvester)
  appears 50 frames after it's placed, likely the end of its build-up, in
  the cell up and left of its top-left. `[observed]`
- **Abandon:** the last queued one of that type goes; failing that, the
  one building stops and everything paid for it comes back. `[observed]`
- **Sell:** two frames in, a crew soldier (`AlliedCrew=` or
  `SovietCrew=`) comes out at the building's top-left + (455, 313) leptons
  and walks to sub-spot (192, 64) of the cell right of its top row. At 50
  frames the building goes and the refund is paid. `[observed]` (one
  soldier from a power plant; how many come out from others isn't traced)
- **Refund:** `Soylent=` if the type sets it, else cost × `RefundPercent=`
  (50%) for a human player, truncated. `[recomp 0x00711F60]`
- **Lost prerequisites:** when a house no longer has what something
  under construction needs, it's cancelled and refunded, on the frame the
  last such building goes. A building being sold still counts until it's
  gone. The generic prerequisites (`POWER`, `FACTORY`, `BARRACKS`,
  `RADAR`, `TECH`, `PROC`) are the buildings `[General]
  Prerequisite<Name>=` lists. `[observed]` (a refinery when its power plant
  was sold)

### Build time `[recomp 0x006F47A0]`

1. `Cost × BuildSpeed × 0.9`, truncated (`BuildSpeed=` 0.7: a power plant
   at 800 takes 503 frames, a Harrier at 1200 takes 755);
2. × the country's multiplier for that kind, truncated:
   `BuildTimeInfantryMult=`, `BuildTimeUnitsMult=`, `BuildTimeAircraftMult=`,
   `BuildTimeBuildingsMult=` or `BuildTimeDefensesMult=` (a building with
   `BuildCat=Combat`); no country sets them in the shipped rules
   `[recomp 0x0050C0A0]`;
3. × `BuildTimeMultiplier=` (a float), truncated;
4. ÷ the production speed, truncated. The speed is 1 unless power is
   short: then 1 − (1 − output ÷ drain) × `LowPowerPenaltyModifier=`, at
   least `MinLowPowerProductionSpeed=` and at most
   `MaxLowPowerProductionSpeed=`. The ratio is a single-precision float, so
   60% power builds a 125-frame GI in 208 frames;
5. × `MultipleFactory=` (0.8) once for each factory of that kind beyond the
   first, truncating each time: a GI (125 frames) from two barracks takes
   100;
6. walls × `WallBuildSpeedCoefficient=`.

The game does this on the x87 in extended precision (64-bit mantissas),
from doubles and singles. That matters: 1200 × 0.7 × 0.9 is 755.99… there
but exactly 756 in doubles, so in doubles a Harrier would take 14-frame
steps instead of 13. `ra2-rules` (`x87`) does the same multiplies and
divides in integers. Only the integer result reaches the simulation.
`[observed]` (`allied-air`) The refund and a building's power output are
worked out the same way.

### Steps and payment `[observed]`

A factory counts 54 steps, one every `time ÷ 54` frames (truncated, at
least 1 and at most 255 `[recomp 0x004C9F07]`), from the frame the order
takes effect. When the house's power changes, every factory's rate is
worked out again, but the step already under way keeps its timing; the
new rate applies from the next one. `[recomp 0x004CA6E0]` (`[observed]` in
`allied-queue-sell`: 2-frame steps become 4 when the power plant goes) A
house that can't afford a step's payment doesn't take the step: the
factory tries again a rate later, and so on until it can. The last step
pays whatever is still owed (nothing, by then). `[recomp 0x004C9B20]`
(running out isn't recorded; a unit test covers it) On each of the first 53
steps it takes what's still owed ÷ the payments left, rounded down. So a
power plant pays 15 for 48 steps, then 16 for 5. On the 54th step it's
done. Recorded for three costs and step lengths of 9, 5, 2 and 1 frames.

### Leaving the factory `[observed]`

A finished aircraft appears the frame after the 54th step on its
factory's first free pad: the factory's centre plus the art's
`DockingOffsetN=`. While it's being built it's logged at the flight level
(`FlightLevel=`, 1500).

A finished vehicle appears at its war factory's centre the frame after the
54th step. 48 frames later (the door), it drives out to the cell beside the
factory's middle row: a Grizzly from a war factory at (41,36) stops at
(46,37). It drives a little slower than on open ground there, so ours keeps
pace to within a few frames on the way and arrives on the same frame.

A finished GI appears the frame after the 54th step. It comes out of the
house's first barracks at its top-left + (320, 448) leptons, and stands a
frame. Then it walks to sub-spot (64, 192) of the first cell along the row
below the barracks that nobody stands on or is heading to.

### Power `[observed]`

A house's power output is the `Power=` of its buildings that make power,
× each one's health ratio, truncated (`[recomp 0x0044E7B0]`), counted
from the frame after each is placed. Its drain is the `Power=` of
those that use it, counted from the frame each is placed. Production reads
the house's output and drain when an order starts.

## Quirks we match

- The power ratio is rounded to a float before it slows production (see
  above).

## Open questions

- The rest of placement (terrain, other houses' units) and the build-up
  animation itself.
- Ships leaving a naval yard; which factory is primary when there are
  several (we use the oldest).
- Power output from damaged buildings, recorded.
- How many crew a sold building lets out, and where; which cell a vehicle
  leaving a war factory goes to (the game's pick varies between
  recordings, likely from its random numbers).
- Infantry leaving a barracks go to the first free cell of the row below a
  wide one (the Allied barracks, 3×2) or of the column east of a tall one
  (the Yuri barracks, 2×3, from its top row). `[observed]` (GIs, an
  engineer, a Yuri engineer and Yuri) Other barracks, and why, untraced.
- The GI's exact walking path out (we're within a few leptons on the way,
  exact on arrival).

## Parity

`parity/scenarios/allied-air.toml`: 6 checks at zero tolerance; credits,
power and every object, height included, match the original on all 3400
frames.

`parity/scenarios/allied-war-factory.toml`: 11 checks at zero tolerance
(credits through the refinery, the hold and the Grizzly; the free miner's
creation and spot; the placed barracks; power; the GI out of the door; the
Grizzly at its spot). Everything but the Grizzly's 37 frames driving out
matches the original on all 4300 frames.

`parity/scenarios/allied-queue-sell.toml`: 12 checks at zero tolerance
(credits through the queue, the sale, low power and the cancel; power;
removal frames; counts). Credits, power and which objects exist match the
original on all 2500 frames.

`parity/scenarios/allied-base.toml`, 12 checks, all at zero tolerance:
- the yard's creation frame;
- credits at six frames through the power plant and the GIs from one and
  two barracks;
- power at two frames;
- every building's and GI's position at the end.
Credits and power match the original on every frame of the run.
