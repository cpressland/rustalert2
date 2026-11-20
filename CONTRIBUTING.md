# Contributing

Read [LEGAL.md](LEGAL.md) first. The rules below follow from it.

## The rules

1. **No game data in the repository.** That means no game files, nothing
   extracted from MIX archives, and nothing converted from them. CI checks
   this (`python3 -I tools/game_data_guard.py`). Fixtures are written by our
   own encoders at test time.
2. **Spec, then code.** Before a system is implemented, its behaviour goes in
   `docs/spec/<system>.md` (start from [the template](docs/spec/TEMPLATE.md)).
   Each claim is tagged with its evidence:
   - `[observed]`: seen in the running game (say how);
   - `[recomp 0xADDR]`: read from the recomp's lifted code, with the address
     and, where it has one, the function or class name;
   - `[community]`: ModEnc, YRpp or another write-up (link it);
   - `[hypothesis]`: our best guess, to be confirmed.

   The code is written from the spec, in our own design. **Don't paste lifted
   C or disassembly, and don't translate it line by line.**
3. **Parity rules are binding.** Never edit a golden or loosen a tolerance just
   to make a test pass. Never special-case a scenario by name. If the original
   behaves in a way we think is a bug, the spec says so, and we decide in
   review whether to match it.
4. **The sim stays deterministic.** `ra2-sim` has no floating point in its
   state, no clocks, no threads, and no `HashMap`/`HashSet` iteration. Its own
   `clippy.toml` enforces this. It may only depend on the crates listed in
   `crates/ra2-sim/allowed-deps.txt`.
5. **Sign off your commits** (`git commit -s`): the
   [Developer Certificate of Origin](https://developercertificate.org/).

## Building

You need Rust 1.99 or later (`rust-toolchain.toml` pins it for rustup users).

```bash
cargo build --workspace
cargo test --workspace
```

Before you push, run what CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
tools/check_sim_deps.sh
python3 -I tools/game_data_guard.py
```

Every parser has a fuzz target in `fuzz/` (see `cargo fuzz list`). CI runs
each for 30 seconds on nightly. Locally, with a nightly toolchain:

```bash
cd fuzz && cargo +nightly fuzz run shp -- -max_total_time=60
```

Tests that need a real install look for it the way the game does: `RA2_DATA`,
then `config.toml`, then the usual install locations. `cargo run -p ra2tool --
doctor` shows what it finds.

## Pull requests

- Keep each PR to one issue where you can, and link the issue.
- A PR that implements behaviour links the spec section it implements. Any
  parity scenario it makes pass gets ticked in `docs/spec/README.md`.
