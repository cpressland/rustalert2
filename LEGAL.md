# Legal

This is not legal advice. It sets out what this project does and doesn't do,
and the rules contributions have to follow.

## What this is

rustalert2 is a new engine, written from scratch in Rust, that runs on the
data files of the player's own copy of *Command & Conquer: Red Alert 2* and
*Yuri's Revenge*. It contains no code from Westwood or Electronic Arts and
ships none of their content. It is a non-commercial project and is not sold.

The engine's own code, tools and documentation are under the MIT licence
([LICENSE](LICENSE)). That licence covers none of the game's content.

## Game data

- **No game files are in this repository, its releases, or its CI caches.**
  That covers the game's own files, files extracted from its MIX archives,
  and anything converted or generated from them (PNGs of its sprites, dumps of
  its rules, decoded audio). `tools/game_data_guard.py` enforces this on every
  pull request.
- The engine reads the player's install read-only. It never writes into the
  game's folder.
- We never download or host the game for the player. They point the engine at
  a copy they own.
- Test fixtures are synthetic, made at test time by our own encoders. Tests
  that need a real install read it from `RA2_DATA` on the developer's own
  machine, and anything they produce stays outside the repository.

## Where behaviour comes from

The original source code was lost. What we know about the game's behaviour
comes from:

- **observation** of the running game;
- the **static recompilation** in
  [redalert2-recomp](https://github.com/sp00nznet/redalert2-recomp), read and
  instrumented on a developer's own machine, built from their own copy;
- **community documentation** (ModEnc, the YRpp headers, format write-ups).

The rule is **spec, then code**. Behaviour is first written up in plain
language under `docs/spec/`, citing where each claim came from. Rust is then
written from that description, in our own design. **Never commit lifted C,
disassembly, or a line-by-line translation of either.** Short citations of
addresses and names (`TechnoClass`, `0x0049F5C0`) are fine.

The engine is not binary compatible with the original. It does not load
`gamemd.exe` or `game.exe`, does not patch them, and does not circumvent any
copy protection.

## Trademarks

*Command & Conquer*, *Red Alert* and *Yuri's Revenge* are trademarks of
Electronic Arts Inc. This project is not affiliated with or endorsed by
Electronic Arts or Westwood Studios. The names are used only to say which game
the engine runs.

## Contributions

Contributions are accepted under the MIT licence and need a DCO sign-off
(`git commit -s`); see [CONTRIBUTING.md](CONTRIBUTING.md).
