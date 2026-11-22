# Economy

**Status:** Parity-checked for one harvester on a field of ore (search,
bails, depletion, a full load, docking at a Soviet refinery, payment, the
trip back). Growth and spread, gems, slave miners and dock queues are still
to come.
**Crate:** `ra2-sim` (`harvest`, `world::Ore`)
**Issue:** #42

## Scope

Ore on the map, the Harvest mission, and how a load becomes credits.
Route planning is in pathfinding.md; driving is in locomotors.md.

## Sources

- The recomp of `gamemd.exe` 1.001: `UnitClass::Mission_Harvest`
  (`0x0073E5E0`), `UnitClass::Harvesting` (`0x0073D450`),
  `FootClass::Goto_Tiberium` (`0x004DCFE0`), the ore search
  (`0x004DD0A0`, the unit's virtual `+0x338`) and its cell test
  (`0x004DCE80`), `CellClass::Reduce_Tiberium` (`0x00480A80`), a cell's
  ore worth (`0x00485020`), and `RulesClass` reading and defaults
  (`0x00670299`, `0x00667638`, `0x006671CD`).
- Scenario `harvester-ore`: a Neutral War Miner on Harvest beside a Neutral
  Soviet refinery and a 5×5 field of full ore (`TIB01`, data 11). The
  observer logs every ore cell and each object's cargo every frame; the
  field cells' raw `CellClass` bytes were dumped every 10 frames to find
  the overlay data and land type.

## Behaviour

### Ore on the map

- A cell holds ore or gems when its overlay is one of `GEM01`–`GEM12`
  (27–38), `TIB01`–`TIB20` (102–121) or `TIB2_01`–`TIB3_20` (127–166).
  `[observed]` (the map's `OverlayPack`; the game marks such cells with land
  type 5, `CellClass +0xEC`)
- The overlay's data (`CellClass +0x11E`, from `OverlayDataPack`) is the
  bails it gives: each bail takes one; a cell at 0 gives none, and the next
  try removes the overlay. `[observed]` (11 bails from a data-11 cell)
- A bail is worth `[Riparius] Value=` (25) for ore and `[Cruentus] Value=`
  for gems. `[observed]` (40 bails paid 1,000)

### Looking for ore

`Goto_Tiberium` (radius in cells): if the harvester is already driving
somewhere, nothing. Otherwise it searches; if what it finds is the cell it
stands on, it's there (true); else it drives there (false). `[recomp
0x004DCFE0]`

The search: `[recomp 0x004DD0A0]`

1. The harvester's own cell, if it has ore.
2. Otherwise rings of cells round it, radius 1 up to the scan less one.
   For each step `i` from −r to r, in turn: (x+i, y−r), (x+i, y+r),
   (x−r, y+i), (x+r, y+i).
3. A cell counts if it's on the map, has ore, and the harvester could
   enter it (no other object there), and could reach it (not yet checked: our
   scenarios are open ground). Its worth is a
   bail's value × (data + 1). `[recomp 0x004DCE80, 0x00485020]`
4. The first cell of the highest worth wins; the first ring with any
   such cell ends the search.

Every cell the recorded War Miner chose, on both trips, is the one this
order picks. `[observed]`

The scans: `TiberiumShortScan=` (6 cells) while working a field, and
`TiberiumLongScan=` (48 in `rulesmd.ini`; the game's default is 32) when
starting out. `[recomp 0x00670299]`

### The mission

The mission keeps a status. When it isn't returning 1 (run again next
frame) it returns `[Harvest] Rate=` × 900, truncated (14 frames), plus a
random 0–2 from the game's generator. `[recomp 0x0073EF77]`

- **Looking:** full → off home. On ore (long scan) → start the harvest
  stage and start gathering. Driving to ore → look again after the usual
  delay. Nothing found → idle.
- **Gathering**, every frame: the harvest stage counts up one every
  `HarvesterLoadRate=` frames (default 2) and is ready at 9, so a bail
  comes every 19 frames. When ready, it tries to take a bail (below). If
  it can't: full → remember where to come back to (the short-scan search
  from where it stands, so its own cell if any ore is left) and go home;
  else look for the next cell (short scan) and drive to it; nothing found
  → go home with nothing to come back to. `[recomp 0x0073E931,
  0x0073E994]`
- Taking a bail (`Harvesting`): busy while driving; not when full; not on a
  cell without ore (the stage stops); else take min(1, room) bails off the
  cell and restart the stage. `[recomp 0x0073D450]`

The stage keeps counting while it drives to the next cell, so it takes a
bail the frame after it arrives. `[observed]` (542 → 543, 845 → 846)

The first look is at frame 9. `[observed]`

### Docking and unloading `[observed]`

Fitted to the one recorded visit to a Soviet refinery, until the
refinery's side of the radio conversation is traced:

1. 3 frames after the mission turns home it sets off for the refinery's
   queueing cell (art `QueueingCell=`, from the foundation's top-left).
2. There it turns on the spot to face away from the pad (the cell west of
   the queueing cell), and backs onto it at 1,150/256 leptons a frame.
3. 40 frames after it's on the pad the whole load is paid into its house's
   credits at once.
4. 31 frames later it drives back to the cell it remembered, and looks for
   ore again.

### Driving

As locomotors.md describes: the War Miner accelerates (it doesn't set
`Accelerates=no`), so it sets off at a crawl, speeds up, and slows within
500 leptons of the cell it's going to. A one-cell hop between ore cells is
all crawl. It turns on the spot before setting off, at `ROT=` × 256 a
frame.

## Quirks we match

- A cell's last bail-try gives nothing: a data-11 cell gives 11 bails over
  12 tries, and the harvester stands there 19 frames longer.

## Open questions

- Ore growth and spread (`Growth=`, `Spread=`, `GrowthRate=`), gems, and
  the Slave Miner.
- The refinery's side of docking (radio messages, a queue of harvesters,
  Allied refineries), and the refinery's own `Storage=`.
- The drive locomotor's real speed and turning rules (#40): the War Miner
  stops at corners for about 5 frames; we turn on the move.
- What the mission does with a target (`+0x218`) or a weeder.

## Parity

`parity/scenarios/harvester-ore.toml`: 12 checks — cargo at four frames,
positions on the first and second loads and on the pad, ore left on the
map, and credits before and after the payment. Our timing runs up to about 20
frames ahead of the original by the first unload (1509 against 1530),
from the corner stops we don't make yet. The Russian AI's own War Miner reaches the field after frame
2200, so checks stop there.
