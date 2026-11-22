# The parity lab

**Status:** Recording works against the original; `parity run` plays scenarios in our engine (objects are placed; movement and combat follow).
**Crate:** `ra2-parity` (the `parity` tool). The observer is in `parity/recomp/`.
**Issues:** #23 (observer), #24 (scenarios and runner), #25 (diff), #26 (first scenarios)

## Scope

How we check that our engine behaves like the original:

1. A **scenario** sets up a small situation: a map, how long to run, and what
   to compare.
2. The **original game** plays it in the recomp, with an observer logging
   what the simulation holds every frame.
3. **Our engine** plays the same scenario and writes the same log. This comes
   in M4.
4. `parity diff` compares the two under the scenario's checks and
   tolerances.

## The observer

`parity/recomp/observe.c` runs inside the recomp. It's called once per
rendered battle frame from the frame-copy hook (`0x004373B0` entry,
`hdvox.c`), on the game thread, so what it reads is consistent. It logs when
the game frame counter changes.

### What it reads

These were found from a raw memory dump of a running skirmish
(`RA2_OBSERVE_DUMP`), by looking for values that had to be there. They match
the community's YRpp headers. `[observed]`

| What | Where | How it was found |
|---|---|---|
| Game frame | `0x00A8ED84` | 950 references in the code; increases by 1 per logic frame |
| Houses | `0x00A80228`: `DynamicVectorClass<HouseClass*>`, items `+4`, count `+0x10` | RTTI of the vtable written there |
| Objects | `0x00A8EC78`: `DynamicVectorClass<TechnoClass*>` | RTTI |
| Object class | vtable → complete object locator → type descriptor name | RTTI, read at run time |
| Owner | `TechnoClass +0x21C` (`HouseClass*`) | the only offset holding a house pointer for every object (plus `+0x14C`, a second one) |
| Position | `ObjectClass +0x9C`: three `int`s in leptons | the only triple with lepton-sized x and y and heights in multiples of 104 for 96% of objects (the rest in limbo) |
| Hit points | `ObjectClass +0x6C` | every infantryman 125, the MCV 1,000, cars 300 and 400, buildings 750 to 6,000 |
| Type | `BuildingClass +0x520`, `UnitClass +0x6C4`, `InfantryClass +0x6C0` | shared by objects of one type; the rules ID is at `+0x24` of the type |
| House index, country, credits | `HouseClass +0x30`, `+0x34` (type), `+0x30C` | indices 0 to 7; the human's 10,000 untouched while the AIs' change |
| Power produced, drawn | `HouseClass +0x53A4`, `+0x53A8` | the power ratio the build time reads (`0x004FCE30`) |
| Cargo | `TechnoClass +0x33C`: four `float`s, bails of each ore kind | counts 1 to 40 a bail at a time as a War Miner harvests, back to 0 as the refinery pays |
| Cells | `MapClass` (`0x0087F7E8`) `+0x13C`: `CellClass*` per cell, index `y × 512 + x` | the lookup `0x005657A0` |
| Overlay, its data | `CellClass +0x44` (`int`, −1 for none), `+0x11E` (byte) | the overlay goes from 102 (`TIB01`) to −1 when its data has counted down from 11 to 0 |

### Log format

JSON lines, one per observed frame:

```json
{"frame":170,"houses":[{"index":0,"country":"British","credits":10000}],
 "objects":[{"id":376832016,"kind":"Unit","type":"AMCV","owner":0,"x":9088,"y":9088,"z":0,"hp":935}],
 "ore":[[52,44,102,11]]}
```

- `cargo` appears on objects carrying ore or gems: their bails, in total.
- `ore` lists every cell holding ore or gems: x, y, the overlay's index in
  `[OverlayTypes]`, and its data.

- `id` is the object's address. It's stable for the object's life within one
  log, but not comparable across logs or engines.
- Defeat isn't logged yet; the field is optional.
- Creation and removal events are derived from consecutive frames
  (`parity events`).

## Our engine

`parity run <scenario> -o candidate.jsonl` plays a scenario in our engine
and writes the same log shape: it builds the rules from the install's
`rulesmd.ini`/`artmd.ini` with the map's sections loaded over them, sets
up the skirmish as the recorder's (see objects-model.md), and steps to the
end frame, observing every frame from 1. Then
`parity diff <scenario> <reference> <candidate>` reports each check.

With the Hunt mission (#38, #40, #41) and the game's route planner (#39),
all 20 checks of the four `*-hunts-mcv*` scenarios pass, and with the
Harvest mission (#42) all 12 of `harvester-ore`, and with production
(#43) all 12 of `allied-base`. The reference logs stay on the developer's
machine; CI runs `engine::hunt_tests`, which replays the Grizzly scenario
from rules values written into the test.

## Scenarios

A scenario is TOML in `parity/scenarios/`:

- **`name`**, **`description`**, **`end_frame`**.
- **`map`** and **`[generate]`**: a map the lab writes. Flat clear terrain
  of a given size, skirmish start points, and pre-placed units, infantry and
  structures with a mission (`Hunt`, `Guard`…). See `mapgen.rs`.
- **`human_start`**: the cell the human player must start at (see below).
- **`[[order]]`**: the human player's orders: `frame`, `what` (`deploy`,
  `produce` or `place`), `type`, and `x`/`y` for placing. The recorder
  passes them to the observer, which queues the event the sidebar or a
  click would (`RA2_OBSERVE_ORDERS`; see `parity/recomp/README.md`); our
  engine applies them on the same frames. Each takes effect the frame
  after it's sent.
- **`[[check]]`**: comparisons, each with a tolerance:
  - `count` of matching objects at a frame;
  - `credits` of a house at a frame;
  - `hp`: total hit points of matching objects at a frame;
  - `power`: a house's power produced and drawn at a frame;
  - `cargo`: total bails carried by matching objects at a frame;
  - `ore`: bails left on the map (every ore cell's data, summed) at a frame;
  - `position`: each object paired with its nearest counterpart; the worst
    distance must be within the tolerance, in leptons;
  - `first_damage`: the first frame matching objects lose hit points;
  - `event`: the first creation or removal of a matching object.

Tolerances may only be tightened, unless review agrees on a reason.

### How a scenario starts in the original

`parity record`:

1. Lays out a private game folder of hard links to the install.
2. Edits `RA2MD.INI`: the intro off, one hard AI in slot 1, the other slots
   closed (`1,-2,-2`), no starting units, `ScenIndex=9999`.
3. Writes the scenario map under the name the game loads for that index:
   `XMP29U2.MAP`, a loose file overriding the shipped map.
4. Runs the recomp headless, pressing Single Player, Skirmish and Start Game
   (dialogs `0xE2`, `0x100`, `0x102`, buttons 1667, 1401, 1559).
5. The observer ends the game at `end_frame`, about a minute of wall time for
   900 frames.

### What the original taught us getting there

- **In a skirmish, the map's objects are read before the player houses
  exist.** Units owned by a country (`Americans`), by a player's country
  (`Russians`) or by `<Player @ A>` are silently dropped. Only `Neutral` and
  `Special` can own them. `Neutral` is hostile to the players; `Neutral` and
  `Special` ignore each other. `[observed]`
- **Start points are assigned at random**, from a seed the game takes from the
  clock (logged as `Seed is …`). So two recordings can put the human in
  different corners. The recorder retries until the human starts at
  `human_start`, and logs the seed. Our engine will start from the
  reference's first frame and seed. `[observed]`
- **Map pack blocks must pack into fewer than 8,192 bytes.** A map whose
  stored LCW overlay blocks were 8,324 bytes crashed the game in the overlay
  reader (`0x005FD2E0`). We store 8,000-byte blocks. `[observed]`
- **The cell set of a W×H map** is `0 ≤ x − y + W − 1 < 2W − 1` and
  `0 ≤ x + y − W − 1 < 2H`. That's exactly the cells the campaign maps list.
  One more column crashed the game. `[observed]`
- **Shots are not perfectly regular.** With the same start, two recordings
  match position for position, but the gap between a Grizzly's shots varies
  from 60 to 62 frames with the seed. Damage checks allow about one shot, and
  timing checks a few frames. `[observed]`

## The first scenarios

Each sends one unit, owned by `Neutral` and on `Hunt`, from the middle of a
50×50 map to the idle human's MCV. The human starts at (35,35), and one AI in
the far corner.

| Scenario | First damage to the MCV | MCV hit points |
|---|---|---|
| `grizzly-hunts-mcv` (`MTNK`) | frame 170 | 480 at 600; 220 at 900 |
| `rhino-hunts-mcv` (`HTNK`) | frame 182 | 370 at 600; 10 at 900 |
| `gi-hunts-mcv` (`E1`) | frame 387 | 970 at 600; 892 at 1200 |

They fail until M4: our engine can't run them yet. #26 also listed a
harvester cycle, a power plant build, a Prism Tower chain and a Chrono
Legionnaire. Those need ore on the map, production and specific houses, and
come with the systems they test in M4.

## Settled with the lab

Two M1 hypotheses were tested by giving the GI 777 strength in a modified
`rulesmd.ini` and reading its hit points in frame 1 (normally 125). See
[vfs.md](vfs.md).

| Experiment | GI hit points | So |
|---|---|---|
| none | 125 | baseline |
| `rulesmd.ini` loose in the game folder | 777 | loose files override archives |
| `rulesmd.ini` in a new `expandmd99.mix` | 777 | the earliest-mounted archive wins (`expandmd99` is mounted before `expandmd01` and `localmd`) |

## Open questions

- Power, defeat and the current mission aren't logged yet.
- The game's own playback (`-play`/`-record`, `Seed is %08x`) might let a
  recording fix its seed, making repeat recordings identical.
- How our engine reads a reference's first frame as its starting state (M4).
