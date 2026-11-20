# rustalert2

A new engine for *Command & Conquer: Red Alert 2* and *Yuri's Revenge*,
written in Rust. It aims to play like the original (the same rules, timings,
AI and outcomes) as a safe, native 64-bit program. It reads the data files of
your own copy of the game; nothing from the game is in this repository.

**Status: M0, foundations.** Nothing is playable yet. The plan, the
milestones and the decisions behind them are in [ROADMAP.md](ROADMAP.md) and
on the [project board](https://github.com/users/cpressland/projects/1).

macOS on Apple silicon comes first, then Linux, then Windows.

## Trying what's there

```bash
cargo run -p ra2tool -- doctor
```

This finds your install (`RA2_DATA`, `game_dir` in `config.toml`, Steam, the
EA App, Origin, or a CrossOver/Whisky bottle on macOS) and checks it has what
each game needs.

```bash
cargo run -p ra2tool -- mix ls ra2md.mix/localmd.mix
cargo run -p ra2tool -- mix x ra2md.mix/localmd.mix rulesmd.ini -o out/
cargo run -p ra2tool -- mix check
cargo run -p ra2tool -- vfs mounts
cargo run -p ra2tool -- vfs which rulesmd.ini artmd.ini
cargo run -p ra2tool -- ini get rulesmd.ini General BuildSpeed
cargo run -p ra2tool -- ini stats rulesmd.ini artmd.ini
cargo run -p ra2tool -- csf get Name:E1 NAME:GAPOWR
```

These list an archive (nested archives are separated by `/`), extract files
from it, open and verify every archive in your install, mount the archives the
way the game does at startup, show which archive each file comes from, and
read INI values and translated strings the way the game reads them.

## Documentation

- [ROADMAP.md](ROADMAP.md): the plan, the architecture and how parity is checked
- [docs/spec/](docs/spec/README.md): the behaviour notes the code is written from
- [CONTRIBUTING.md](CONTRIBUTING.md): the rules, and how to build and test
- [LEGAL.md](LEGAL.md): game data, provenance and trademarks

## Licence

MIT ([LICENSE](LICENSE)). *Red Alert 2* and *Yuri's Revenge* are © Electronic
Arts; this project is not affiliated with EA ([NOTICE.md](NOTICE.md)).
