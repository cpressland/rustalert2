# Ticks, game speed and the RNG

**Status:** Implemented (RNG, speed table, state hash and the step API;
orders beyond a stand-in come with the object model)
**Crate:** `ra2-sim` (`rng`, `tick`)
**Issue:** #36

## Scope

The simulation's clock, its random numbers and the outer API every other
system plugs into. What a frame does inside (missions, movement, combat)
is in the notes for those systems.

## Sources

- `gamemd.exe` 1.001 through the recomp: the generator's seeding
  (`0x0065C6D0`), raw draw (`0x0065C780`) and ranged draw (`0x0065C7E0`).
- Numerical Recipes' `psdes` for the seeding hash, whose constants the
  game carries at `0x839644` and `0x839694`.

## Behaviour

### The random number generator

- State: two indices and a table of 250 32-bit entries. `[recomp]`
- Seeding with `s`: entry `n` (0–249) is the pseudo-DES hash of (`s`, `n`):
  four Feistel rounds in which the right half is XORed with a round
  constant, split into 16-bit halves `lo` and `hi`, mixed as
  `~(hi²) + lo²`, its halves swapped, XORed with a second constant, added
  to `hi × lo` and XORed with the left half. The halves are split with an
  arithmetic shift, so a negative `hi` carries its sign into the swap,
  unlike the published `psdes`. `[recomp]`
- The indices start at 0 and 103. `[recomp]`
- A raw draw XORs the entry at the second index into the entry at the
  first, returns that entry, and steps both indices on, wrapping at 250.
  `[recomp]`
- A ranged draw from `a` to `b` (either order, inclusive): if `a == b` it's
  `a`, without drawing. Otherwise, with `lo`/`hi` the smaller and larger
  and `r = hi − lo`, raw draws are masked to the bit width of `r` and
  redrawn until one is no more than `r`; the result is `lo` plus it.
  `[recomp]`

### Game speed

| Setting | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| Logic frames per second | 60 | 45 | 30 | 20 | 15 | 12 | 10 |

`[community]` (to confirm against the recomp's frame limiter)

### The step API

- `Sim::step(orders)` advances one logic frame, applying the frame's
  orders in order, and returns what happened as events.
- `Sim::hash()` is a 64-bit FNV-1a hash over the state in a fixed order:
  the frame count, then the generator's indices and table, then (as they
  arrive) houses and objects. It's for our own lockstep desync checks; the
  game's own sync check isn't reproduced.

## Quirks we match

- The arithmetic shift in the seeding hash.

## Quirks we don't match

- A ranged draw whose range is wider than `i32` (e.g. from `i32::MIN` to
  `i32::MAX`) masks every draw to 0 in the game and never returns. We
  return the lower bound.

## Open questions

- Where the game seeds its generator for a skirmish and a campaign mission
  (spawn `Seed=`, the scenario), and whether any system keeps a second
  generator.
- The speed table, from the frame limiter.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Same seed and orders, same hashes on every platform | pinned hash in `tick` tests, run on macOS and Linux CI | exact |
| Draw sequence against the original | a parity scenario logging the generator state (with #97) | exact |
