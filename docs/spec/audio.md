# Audio formats: WAV, AUD, IDX/BAG

**Status:** Implemented (decoding). Mixing and playback rules are M5.
**Crate:** `ra2-formats` (`audio`)
**Issue:** #19

## Scope

Decoding the game's sound data to 16-bit PCM:

- the `audio.idx` / `audio.bag` sound bank (unit voices and effects);
- WAV files (speech, music);
- Westwood `.aud`.

Which sound plays when, priorities, limits, volumes and the EVA queue come
from `soundmd.ini`, `evamd.ini` and `thememd.ini`, and belong to the audio
note (M5).

## Sources

- Community: the GABA bank index, Westwood AUD, and the IMA ADPCM
  algorithm (the Interactive Multimedia Association's published step and
  index tables).
- Observation: `ra2tool audio check` on the Steam install.

## Behaviour

### Where sound lives

`[observed]`:

| What | Format | Where |
|---|---|---|
| Sound bank (voices, effects) | `audio.idx` + `audio.bag` | `audiomd.mix` (YR, 2,285 sounds), `audio.mix` (RA2, 1,153) |
| Speech and effects as files | WAV, IMA ADPCM, mono 22,050 Hz | `audiomd.mix` (738), `audio.mix` (459) |
| Music | WAV, IMA ADPCM, stereo 22,050 Hz | `thememd.mix` (10) |
| Old-style sounds | AUD type 99 | `local.mix` (5) |

Both banks are called `audio.idx` and `audio.bag`. Under the
earliest-mounted-wins rule ([vfs.md](vfs.md)), YR's in `audiomd.mix` hides
RA2's.

### The bank

- **`audio.idx`**: `"GABA"`, `u32` version 2, `u32` count, then 36-byte
  entries. The size must be exactly `12 + 36 × count`.

  | Offset | Field |
  |---:|---|
  | 0 | 16-byte name |
  | 16 | `u32` offset into the bag |
  | 20 | `u32` size |
  | 24 | `u32` sample rate |
  | 28 | `u32` flags |
  | 32 | `u32` IMA block size |

- **Flags** `[observed]`. The values in the data are:

  | Value | Meaning | Count |
  |---|---|---:|
  | `0xC` | mono IMA ADPCM, 512-byte blocks | 2,192 |
  | `0xD` | stereo IMA ADPCM, 1,024-byte blocks | 5 |
  | `0x6` | mono 16-bit PCM | 84 |
  | `0x7` | stereo 16-bit PCM | 4 |

  So bit 0 is stereo, bit 1 PCM, bit 2 16-bit, and bit 3 IMA ADPCM.
- **`audio.bag`** holds the sample data, with no headers of its own.

### IMA ADPCM, block form

Used by the bank and by WAV format `0x11`, the same as Microsoft's IMA ADPCM.
`[community]`

- Each block starts, per channel, with a 4-byte header: an `i16` first
  sample, a `u8` step index (0 to 88), and a `u8` 0.
- Then come 4-byte groups, for each channel in turn, of eight 4-bit codes,
  low nibble first.
- Each block resets the decoder state.
- We reject a header with an index above 88 or a non-zero fourth byte. No
  shipped sound has one.
- WAV's `fact` chunk gives the true sample count when the last block is
  short.

### IMA ADPCM, AUD form

- **Header**, 12 bytes: `u16` rate, `u32` packed size, `u32` unpacked size,
  `u8` flags (bit 0 stereo, bit 1 16-bit), `u8` compression.
- **Chunks**: a `u16` packed size, a `u16` unpacked size, `u32` `0x0000DEAF`,
  then data.
- Type 99 is IMA ADPCM with **one state carried across the whole file**,
  low nibble first. `[community]`
- Type 1 (Westwood's own ADPCM) isn't implemented, because no shipped file
  uses it.

### The IMA step

The standard algorithm:

- 89-entry step table and 8-entry index table;
- `diff = step/8`, plus `step`, `step/2` and `step/4` for bits 2, 1 and 0;
- bit 3 is the sign;
- the predictor is clamped to 16 bits and the index to 0–88.

## Results on a real install

`ra2tool audio check`:

| | |
|---|---|
| Decoded | **1,207** WAVs, **5** AUDs, **3,438** bank sounds |
| Duration | 199 minutes |
| Failures | 0 |
| Samples at full scale | 0.006% |

The last figure is a cheap check that the decode is sane: a wrong decoder
clips constantly. `ra2tool audio export <name> -o out.wav` writes any sound as
plain PCM to listen to.

## Open questions

- Whether the game's own IMA decoder rounds the same way. It probably does,
  as the step is integer-only. This matters only to bit-exact audio
  comparison, which behavioural parity doesn't need.
- What decides which sounds come from the bank and which from loose WAVs.
  `soundmd.ini` names them; this belongs to M5.

## Parity

| Check | How | Tolerance |
|---|---|---|
| IMA step, blocks, WAV, AUD, bank, synthetic | unit tests | exact |
| Everything shipped decodes, sanely | `ra2tool audio check`, `tests/real_install.rs` | exact |
| Sounds match the original's output | M5 listening and spectral comparison | perceptual |
