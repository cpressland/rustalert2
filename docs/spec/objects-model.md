# Object model: houses and objects

**Status:** Spec'd and started (houses, objects placed from a map, state
hash; missions, ownership changes and the class hierarchy's behaviour come
with the systems that need them)
**Crates:** `ra2-rules` (`scenario`), `ra2-sim` (`world`)
**Issue:** #38

## Scope

What the simulation holds: houses, and the buildings, vehicles, infantry
and aircraft they own, as created at the start of a game. How they move,
fight and build is in the notes for those systems.

## Sources

- Parity reference logs from the original (`parity record`), frame 1 of
  each scenario: positions and hit points of pre-placed objects and of
  the players' base units.
- ModEnc for the map line formats `[community]`.

## Behaviour

### Houses

- A skirmish's houses are the players first (the human, then the AIs),
  then the map's own houses (`Neutral`, `Special`). `[observed]`
- The parity lab's default skirmish has the human as British with 10,000
  credits and one AI as Russians with 50,000. `[observed]`
- Each player starts with its side's `[General] BaseUnit=` (Allied
  first, then Soviet, then Yuri) at its start point's cell centre.
  `[observed]` (AMCV and SMCV in every reference log)

### Map objects

- `[Infantry]`, `[Units]`, `[Aircraft]` and `[Structures]` lines are
  `owner,type,health,x,y,…`:
  - infantry continue `sub-cell,mission,facing,…`;
  - units and aircraft continue `facing,mission,…`;
  - structures continue `facing,…`. `[community]`
- In a skirmish the map's objects are read before the player houses
  exist, so only `Neutral` and `Special` can own them; objects naming any
  other house aren't created. `[observed]` (see the parity scenarios)
- Position: units and aircraft at their cell's centre; infantry at their
  sub-cell (0 is the centre `[observed]`; 1–4 the quarters
  `[hypothesis]`); buildings at their foundation's centre `[hypothesis]`.
  Height is the cell's level × 104 leptons.
- Hit points: `Strength × health / 256`, truncated, at least 1.
  `[hypothesis]` (full health gives `Strength`, `[observed]`)
- Objects get IDs in creation order. IDs are ours; the original's
  are addresses and aren't compared.

### State hash

The world feeds the state hash: house count, then each house's index,
name and credits; object count, then each object's ID, type, owner,
position, hit points and facing.

## Open questions

- The order the game creates map objects in (we take infantry, units,
  aircraft, structures).
- Sub-cells 1–4, and a building's reference point.
- How health below 256 rounds.
