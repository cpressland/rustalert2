# rustalert2

A new engine for *Command & Conquer: Red Alert 2* and *Yuri's Revenge*,
written in Rust. It aims to play like the original (the same rules, timings,
AI and outcomes) as a safe, native 64-bit program. It reads the data files of
your own copy of the game; nothing from the game is in this repository.

**Status: M2, the parity lab.** Nothing is playable yet. The plan, the
milestones and the decisions behind them are in [ROADMAP.md](ROADMAP.md) and
on the [project board](https://github.com/users/cpressland/projects/1).

macOS on Apple silicon comes first, then Linux, then Windows.

## Trying what's there

`ra2tool` finds your install (`RA2_DATA`, `game_dir` in `config.toml`, Steam,
the EA App, Origin, or a CrossOver/Whisky bottle on macOS), mounts it the way
the game does, and reads every format the engine has so far:

| Command | What it does |
|---|---|
| `doctor` | find the install and check it's complete |
| `mix ls / x / check` | list, extract (by name, or by the ID `ls` prints) and verify archives (`ra2md.mix/localmd.mix`) |
| `vfs mounts / which` | the startup mount order, and where a file comes from |
| `ini get / stats` | read INI values as the game reads them |
| `csf get / stats` | translated strings |
| `pal png / ramp` | palettes and house colours |
| `shp png / check` | sprites |
| `vxl info / png / check` | voxel models and animations |
| `tmp png / check` | theaters and terrain tiles |
| `map info / preview / check / render-all` | scenarios; `render-all` draws every shipped map |
| `map render / view` | draw a map (from the game, or a file on disk) to a PNG, or open it in a window (pan, zoom, layer toggles, scaling modes, fullscreen) |
| `audio export / check` | sounds, music and the sound bank |
| `font render / check / pcx` | the bitmap font and PCX art |

```bash
cargo run --release -p ra2tool -- doctor
cargo run --release -p ra2tool -- shp png gi.shp --house DarkBlue --count 8 -o /tmp/gi.png
cargo run --release -p ra2tool -- map view all01umd.map
```

`cargo run --release -p ra2-game -- parity/scenarios/grizzly-hunts-mcv.toml`
plays a scenario live: the simulation at game speed, drawn as it runs.

The parity lab (`cargo run -p ra2-parity -- record | run | diff | events | summary`)
records scenarios in the original game through the recomp and compares them
with our engine; see [docs/spec/parity.md](docs/spec/parity.md).

Images and sounds go wherever you point `-o`. Never commit them: pictures
and recordings of game data are game data.

## Documentation

- [ROADMAP.md](ROADMAP.md): the plan, the architecture and how parity is checked
- [docs/spec/](docs/spec/README.md): the behaviour notes the code is written from
- [CONTRIBUTING.md](CONTRIBUTING.md): the rules, and how to build and test
- [LEGAL.md](LEGAL.md): game data, provenance and trademarks

## Licence

MIT ([LICENSE](LICENSE)). *Red Alert 2* and *Yuri's Revenge* are © Electronic
Arts; this project is not affiliated with EA ([NOTICE.md](NOTICE.md)).
