# rustalert2: plan

A new engine for *Red Alert 2* and *Yuri's Revenge*, written in Rust. It runs
on the data files from the player's own copy of the game. The aim is for the
game to **behave like the original**: same rules, timings, AI and outcomes. It
is safe, 64-bit and native. It is not binary compatible: it does not load
`gamemd.exe`, its saves, or Ares/Phobos patches.

The model is [NVForever](https://github.com/LootJunkieAzza/NVForever): a Rust
workspace, the game's data read-only at runtime, nothing of the game in the
repository, and behaviour written down before it is coded. The difference is
the source of truth. The original source code is lost, so our reference is the
static recompilation in
[redalert2-recomp](https://github.com/sp00nznet/redalert2-recomp). It gives us
the lifted C, the RTTI class map, and a runnable oracle we can instrument.

## Decisions so far

| | Decision |
|---|---|
| Parity bar | **Behavioural parity.** Rules, timings and outcomes match the original within stated tolerances. Our sim is still deterministic, since our own lockstep multiplayer needs that, but it isn't required to match the original's CRCs, and it doesn't interoperate with original clients. |
| Provenance | **Spec notes, then code.** Behaviour goes into `docs/spec/` as plain-language notes. Each claim is tagged `[observed]` (seen in the running game), `[recomp]` (read from the lifted code, citing the address) or `[hypothesis]`. Rust is written from the notes. Lifted C is never pasted into this repo. |
| Stack | **Custom wgpu + winit + cpal.** The sim is a pure Rust crate with no engine dependency. The frontend is thin. |
| First game | **Yuri's Revenge** (`gamemd.exe` 1.001, the build the recomp and YRpp map). RA2 comes later as a mode. |
| Platforms | **macOS (Apple silicon) first**, shipped as a `.app` with Sparkle updates like tpw-recomp. Then Linux (Flatpak/AppImage), then Windows. |

## Architecture

```
crates/
  ra2-formats   MIX (incl. encrypted headers), INI, CSF, PAL, SHP, VXL/HVA,
                TMP, MAP/YRM, AUD/IDX/BAG, FNT. Pure, no I/O policy, fuzzed.
  ra2-vfs       Mount order of MIX files and loose files, mod overlay folders.
  ra2-rules     Typed rules: rulesmd/artmd/aimd/soundmd/thememd/eva parsed
                into *Type tables (with CCINIClass semantics: overrides,
                defaults, list parsing quirks).
  ra2-sim       The game: fixed tick, deterministic RNG, leptons/cells,
                objects, houses, locomotors, pathfinding, combat, economy,
                production, superweapons, triggers, AI. No floats in the
                state that sync depends on; no graphics, no audio, no clocks.
  ra2-render    wgpu: iso terrain with height, SHP sprites, palettes and house
                remaps, voxels, lighting, shroud/fog, the UI layer.
  ra2-audio     cpal mixer: AUD/WAV, EVA and unit voice queues, music.
  ra2-video     Bink 1 (BIKi) playback for intros and briefings: a pure Rust
                decoder, no ffmpeg (#20).
  ra2-ui        Shell menus, sidebar, radar, tooltips, input mapping.
  ra2-net       Our own lockstep protocol, LAN discovery, replays.
  ra2-game      Wires sim + frontend; the binary.
  ra2tool       CLI: list/extract MIX, dump rules, render a map, run a
                headless scenario.
parity/         Scenario runner and observables comparison against the recomp.
platform/macos  .app bundle, Sparkle, DMG, appcast, install.sh.
docs/spec/      The behaviour notes the code is written from.
```

The sim exposes `step(orders) -> events`. Rendering and audio read sim state
and interpolate between ticks. Headless runs are the same binary with no
frontend.

## How parity is checked

1. **Format goldens.** The recomp's loaders, instrumented, dump what they
   parsed: MIX directory, INI values as the game read them, decoded SHP
   frames. `ra2-formats` must match.
2. **Scenario parity.** A scenario is a map, a setup, a seed and a scripted
   list of orders. It runs in the recomp (with a hook that logs observables)
   and in `ra2-sim` headless. Observables include build-complete frames,
   credits over time, unit HP and position at checkpoints, kills and their
   frames, and AI team creation. They're compared within tolerances that are
   written down per scenario. Tolerances are never edited just to make a test
   pass.
3. **Rules unit tests** from `docs/spec` claims: speed, ROF, Verses, range,
   cost and build time arithmetic.
4. **Playtest suite** like the recomp's: scripted input through menus and
   skirmishes, checked with logs and frame sheets.

## Milestones

| | Milestone | Done when |
|---|---|---|
| M0 | Foundations | Workspace, CI (macOS), licence, LEGAL, game-data guard, contributing rules, game-data locator. |
| M1 | Formats & VFS | Every MIX in a YR install mounts. `ra2tool` lists, extracts and decodes every format. Goldens pass. |
| M2 | Parity lab | Recomp observables hook plus scenario runner. One scenario recorded and replayed headless. |
| M3 | Renderer & map viewer | Any shipped YR map renders statically (terrain, overlays, buildings, units, lighting), panning at 120 fps on an M1. |
| M4 | Simulation core | Units move, fight, harvest and build in a headless skirmish. First scenarios pass parity. |
| M5 | Playable skirmish | Main menu → skirmish setup → game vs AI → score screen, with sidebar, radar, audio and EVA. |
| M6 | macOS release | Signed `.app`, DMG, curl installer, Sparkle appcast on GitHub Releases, first-run game-data setup. |
| M7 | Campaigns | Triggers, tags, actions, teams, briefings, movies, save/load. Both YR campaigns playable. |
| M8 | Multiplayer | Lockstep over UDP, LAN lobby, desync detection, replays. |
| M9 | Red Alert 2 mode | RA2's rules, data and the behavioural differences from YR. RA2 campaigns. |
| M10 | Linux | Flatpak and AppImage, like tpw-recomp. |
| M11 | Windows | Installer and updater. |
| — | Remaster (rolling) | Widescreen/hi-res, HD voxels, modern controls, mod folders, rebindable keys. |

M2 runs alongside M1 and M3. Everything in M4 onwards lands with parity
scenarios.

## Out of scope

- Distributing game files, or anything generated from them, including the
  lifted C.
- Running Ares/Phobos/CnCNet-spawner patched executables or their INI
  extensions. These may come later as opt-in mods, separately specified.
- Westwood Online.
