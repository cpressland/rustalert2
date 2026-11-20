# INI reading

**Status:** Implemented
**Crate:** `ra2-formats` (`ini`)
**Issue:** #12

## Scope

How the game turns INI text into sections and keys, and how it reads
integers, booleans, numbers and lists from values. What individual rules keys
*mean* belongs to the rules notes (M4). Fixed-point reading (`Get_Fixed`)
comes with the sim's number types.

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs.
  The loader is `INIClass::Load` at `0x00525A60`. Its RTTI names
  (`INIClass`, `INISection`, `INIEntry`) come from `.rdata` at
  `0x00825A98`–`0x00825B48`.
- The header of `rulesmd.ini` itself, which documents percentages.
- Community: ModEnc's notes on INI syntax and lists.
- Observation: `ra2tool ini stats` and `ini get` on the Steam install.

## Behaviour

### Lines

- Lines are read one byte at a time. `\n` ends a line, and `\r` is dropped
  wherever it appears. `[recomp 0x0065D5C0]`
- The line buffer is 512 bytes. Bytes past the 511th are **discarded**, and
  the rest of the line does not wrap onto a new one. `[recomp 0x0065D608]`
  Two lines in RA2's `eva.ini` (532 and 681 bytes) are cut this way. They're
  briefing text, so the cut changes only what that text says. `[observed]`
- **Trimming** strips every byte `<= 0x20` from both ends, not just spaces
  and tabs. `[recomp 0x00727CF0]`

### Sections

- A line is a section header if, after leading whitespace, it starts with
  `[` and has a `]` somewhere. The name is the text between them, kept as
  is: `[ Foo ]` names ` Foo `. Anything after the `]` is ignored.
  `[recomp 0x00525C0B]`
- Lines before the first section are not entries. Without the keep-comments
  flag the loader collects them, then frees them. `[recomp 0x00525AE8]`
- **A section that ends up with no entries is discarded** (it's freed rather
  than linked in). `[recomp 0x00526202]` This matters in the shipped data:
  `artmd.ini` and `art.ini` have an empty `[PARABOMB]` before the real one
  (`Rate=200`, `LoopStart=7`, `LoopCount=15`). The empty one never exists,
  so the real one isn't shadowed. `[observed]`

### Entries

For each line that isn't a section header: `[recomp 0x00525F69]`–`0x00526011`

1. cut it at the first `;`, then trim;
2. skip it if it's empty or starts with `;` or `=`;
3. split it at the first `=`, and trim the key and the value;
4. **skip it if either the key or the value is empty.** `Key=` stores
   nothing, so the reader's default applies, not an empty string.

`;` starts a comment anywhere, even inside what looks like a value. There's
no escaping.

### Names are case-sensitive

- Sections and keys are indexed by a CRC of their raw bytes
  (`[recomp 0x004A1DE0]`, no case folding), and lookups hash the name
  they're given the same way. `[recomp 0x005276D0]` So `[General]` and
  `[general]` are different sections. This is `[recomp]` evidence only; a
  case-mismatch test in the M2 hook would confirm it.

### Repeats

- In a fresh load, a repeated (non-empty) section becomes a second section
  object, and a repeated key a second entry. Each is appended to its index
  with no duplicate check. `[recomp 0x005260D9]`, `0x00526276`
- Lookups go through a sorted index (sorted with `qsort`, then
  binary-searched), so which copy a lookup finds depends on an unstable sort.
  We use the **first** copy and record every repeat in `Ini::duplicates` for
  tools to report. `[hypothesis]`
- After dropping empty sections, the shipped rules files have **no** repeated
  sections or keys (below), so the choice doesn't affect the stock game. It
  can affect mods and maps.

### Merging

- Loading into an INI that already has sections is a different path: each
  entry is written with `Put` (`[recomp 0x00528660]`). An existing key in an
  existing section takes the new value, a new key is added, and a new
  section is created when its first entry arrives. `[recomp 0x00525BFA]`
- Repeats can't arise when merging.

### Values

- **Integers** (`Get_Int`, `[recomp 0x005276D0]`):
  - a value starting with `$` is read with `sscanf("$%x")`;
  - a value whose last character is `h` (either case, through `tolower`) is
    read with `"%xh"`;
  - otherwise C `atoi` is used: leading whitespace, a sign, digits up to the
    first non-digit, and `0` if there are none. Overflow wraps on the
    original's 32-bit build.
- **Booleans:** a value starting with `y`, `t` or `1` (either case) is true;
  `n`, `f` or `0` is false; anything else leaves the default. `[community]`
  The writer uses `yes` (`0x00825BF8`). We haven't found the reader in the
  recomp yet.
- **Numbers:** C `atof`, divided by 100 if the value contains `%`. The header
  of `rulesmd.ini` says values "can be specified as either a simple floating
  point number (embed ".") or as a conventional percentage number (append
  "%")". `[observed]` We don't know whether `%` must come last or can be
  anywhere; we accept it anywhere. `[hypothesis]`
- **Lists** are split with `strtok(",")`: empty items are skipped and items
  are not trimmed, so `GI, E2` gives `GI` and ` E2`. `[community]` Some
  readers may trim; each is checked as the rules notes are written.
- Text is Windows-1252 bytes. We keep it as Latin-1 (one byte, one `char`),
  which is lossless.

## Results on a real install

`ra2tool ini stats` on the Steam install, after dropping empty sections:

| File | Sections | Entries | Repeats | Lines over 511 |
|---|---:|---:|---:|---:|
| `rulesmd.ini` (`expandmd01.mix`) | 1,478 | 23,176 | 0 | 0 |
| `artmd.ini` | 1,570 | 13,764 | 0 | 0 |
| `aimd.ini` | 388 | 6,625 | 0 | 0 |
| `soundmd.ini` | 821 | 3,765 | 0 | 0 |
| `evamd.ini` | 486 | 1,875 | 0 | 0 |
| `rules.ini` | 1,191 | 17,313 | 0 | 0 |
| `art.ini` | 1,278 | 10,339 | 0 | 0 |
| `eva.ini` | 357 | 1,480 | 0 | 2 |

The theater INIs, `thememd.ini`, `battlemd.ini`, `mapselmd.ini`,
`mpmodesmd.ini` and `keyboardmd.ini` also parse with no repeats.

## Quirks we match

- Empty values aren't stored, so `Key=` doesn't clear an inherited value.
- `;` inside a value cuts it.
- Over-long lines are cut, not wrapped.
- Names are case-sensitive.
- Empty sections are dropped, which is what makes `[PARABOMB]` work.

## Open questions

- Which copy of a repeated key or section the original's sorted index
  returns. We'd need to model its `qsort`, or observe it in M2.
- Where the boolean and float readers are in the recomp.
- When the keep-comments flag (the `Load` argument at `[esp+0x480]`) is set.
  It also keeps empty sections.
- How maps are read on top of rules (a merge into the rules INI, or a
  separate INI read by the same type readers). This belongs to the rules
  note (M4).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Parsing rules, synthetic | `ra2-formats` unit tests | exact |
| Real files parse, known values | `ra2tool ini stats`, `tests/real_install.rs` | exact |
| A sample of 1,000 keys read the same as the original | M2 observables hook | exact (not done yet) |
