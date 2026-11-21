# Behaviour specs

Each system is written up here before it is implemented; see
[CONTRIBUTING.md](../../CONTRIBUTING.md) for the evidence tags, and start new
notes from [TEMPLATE.md](TEMPLATE.md).

**Status:** Not started → Spec'd → Implemented → Parity-checked.

## Tooling

| System | Note | Status |
|---|---|---|
| Parity lab (observer, scenarios, diff) | [parity.md](parity.md) | Implemented |

## Formats and data (M1)

| System | Note | Status |
|---|---|---|
| MIX archives | [mix.md](mix.md) | Implemented |
| File search order (VFS) | [vfs.md](vfs.md) | Implemented |
| INI reading | [ini.md](ini.md) | Implemented |
| CSF string tables | [csf.md](csf.md) | Implemented |
| Palettes and house colours | [palette.md](palette.md) | Implemented |
| SHP sprites | [shp.md](shp.md) | Implemented |
| VXL / HVA voxels | [voxel.md](voxel.md) | Implemented |
| TMP tiles and theaters | [tmp.md](tmp.md) | Implemented |
| Maps (MAP / YRM / MPR) | [map.md](map.md) | Implemented |
| Audio (AUD, IDX/BAG, WAV) | [audio.md](audio.md) | Implemented |
| Bink video (pure Rust, #87) | | Not started |
| Fonts and UI art | [font.md](font.md) | Implemented |

## Presentation (M3)

| System | Note | Status |
|---|---|---|
| Coordinates, cells and projection | [coords.md](coords.md) | Implemented |
| Terrain and height | [terrain.md](terrain.md) | Implemented |
| Sprites, objects and draw order | [objects.md](objects.md) | Implemented |
| Voxel lighting | [objects.md](objects.md#voxel-lighting) | Implemented |
| Map lighting | [lighting.md](lighting.md) | Implemented |
| Shroud and fog | [shroud.md](shroud.md) | Implemented |
| Presenter: window and scaling | [presenter.md](presenter.md) | Implemented |

The M3 exit check is `ra2tool map render-all`: every map the game ships
(287 in Yuri's Revenge, from the archives and the loose `.mmx`/`.yro`
files) is drawn through the whole composition, missions under their
start-of-game shroud. All draw, in about 30 s. A handful of buildings have
no art in some theaters (flags, guard shacks placed outside the theater
their art ships in); those are reported, not drawn.

## Simulation (M4)

| System | Note | Status |
|---|---|---|
| Ticks, game speed and RNG | [tick.md](tick.md) | Implemented |
| Rules tables | [rules.md](rules.md) | Implemented |
| Object model and missions | [objects-model.md](objects-model.md) | Started |
| Pathfinding and occupancy | | Not started |
| Locomotors | [locomotors.md](locomotors.md) | Spec'd |
| Combat | [combat.md](combat.md) | Spec'd |
| Economy | | Not started |
| Power and production | | Not started |
| Special abilities | | Not started |
| Superweapons | | Not started |
| Crates and veterancy | | Not started |

## AI, campaigns and multiplayer (M5 onwards)

| System | Note | Status |
|---|---|---|
| Skirmish AI | | Not started |
| Triggers, events and actions | | Not started |
| Mission teams and scripts | | Not started |
| Lockstep and desync detection | | Not started |
| Red Alert 2 differences | | Not started |
