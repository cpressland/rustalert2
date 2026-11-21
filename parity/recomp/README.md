# The recomp observer

The parity lab records the original game through
[redalert2-recomp](https://github.com/sp00nznet/redalert2-recomp), the static
recompilation of `gamemd.exe`, with a small observer added. This folder holds
that addition. It's our own code: it reads the game's memory and writes JSON
lines. Nothing in it comes from the game.

| File | What |
|---|---|
| `observe.c` | the observer; goes in the recomp at `src/runtime/observe.c` |
| `observe.patch` | the observer plus its two hooks: one call from `hdvox.c` per battle frame, and the CMake entry |
| `pcrecomp-disk-clamp.patch` | a one-line change to pcrecomp's `GetDiskFreeSpaceExA` (see below) |

## Building

```bash
cd ~/dev/github.com/recomps/redalert2-recomp
git apply ~/dev/github.com/rustalert2/parity/recomp/observe.patch
PCRECOMP=/path/to/pcrecomp BUILD_DIR=build-observe ./build-macos.sh
```

A fresh build compiles all the lifted C, which takes about 25 minutes on an M1
Pro. After that, editing `observe.c` and rebuilding takes seconds.

### The disk-space clamp

Since pcrecomp `1e2177a` (28 October 2026), `GetDiskFreeSpaceExA` reports
the host's real free space. Yuri's Revenge turns 63 GB free into "Free disk
space is 0 Mb", refuses to start, and dies with "String Manager failed to
initialize", because its strings aren't loaded yet. pcrecomp's own
`GetDiskFreeSpaceA` already clamps to 2 GB ("a 1990s game multiplies these
in 32 bits"). `pcrecomp-disk-clamp.patch` does the same for the `Ex` call.
Apply it to a pcrecomp checkout used for this build, or report it upstream.

## Running

The observer reads environment variables:

| Variable | Meaning |
|---|---|
| `RA2_OBSERVE=path` | write the log to `path` |
| `RA2_OBSERVE_EVERY=N` | log every Nth frame (default: every frame) |
| `RA2_OBSERVE_END=N` | log frame N, then exit the game |
| `RA2_OBSERVE_DUMP=N` | at frame N, also dump raw house and object memory, and every cell's zone record (`zone <index> <level 0> <level 1> <level 2> <w3> <w4>`, index `y × (w + h + 1) + x`), to `path.dump` |

`parity record` sets these, lays out a game folder and drives the menus, so
normally you don't run the recomp by hand:

```bash
cargo run --release -p ra2-parity -- record parity/scenarios/grizzly-hunts-mcv.toml \
    --recomp ~/dev/github.com/recomps/redalert2-recomp/build-observe/ra2 --out /tmp/ghm.jsonl
```

The log format, the field offsets and how scenarios start are in
[docs/spec/parity.md](../../docs/spec/parity.md).
