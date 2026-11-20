# CSF string tables

**Status:** Implemented
**Crate:** `ra2-formats` (`csf`)
**Issue:** #13

## Scope

The game's translated text: `ra2md.csf` for Yuri's Revenge and `ra2.csf` for
Red Alert 2. This note covers the file layout, how strings are decoded and
tidied, and how labels are looked up. How the UI lays out text belongs to the
font and UI notes.

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs.
  The loader is at `0x00734700` (header) and `0x00734990` (labels), and the
  lookup at `0x00734E60`.
- Community: the CSF format as documented for TS/RA2 tools (the magic
  numbers and the NOT encoding).
- Observation: `ra2tool csf` on the Steam install.

## Behaviour

### Layout

All integers are little-endian.

- **Header**, 24 bytes `[recomp 0x00734784]`:

  | Field | Notes |
  |---|---|
  | magic `" FSC"` | `[recomp 0x00734799]` |
  | version | |
  | label count | |
  | string count | |
  | reserved | |
  | language | used only if version ≥ 2, otherwise 0 `[recomp 0x007347B1]` |

- **Each label** `[recomp 0x00734A0A]`:
  - magic `" LBL"`;
  - a `u32` count of strings;
  - a `u32` name length, then the name in ASCII.
- **Each string** `[recomp 0x00734ADE]`:
  - magic `" RTS"`, or `"WRTS"` for a string with an extra value;
  - a `u32` length in UTF-16 units, then that many units, each bitwise
    NOT-ed;
  - for `WRTS`, a `u32` length and that many ASCII bytes: the **extra**
    value. In `ra2md.csf` the extras are sound names for spoken lines.
- **Label names** are copied into 32-byte slots, 40 bytes per label with the
  string count and the first string's index. `[recomp 0x00734A53]` A name of
  32 bytes or more would overflow. The longest in either shipped table is 31.
  `[observed]`
- A label can hold several strings. The loader stores them all, but lookup
  returns only the first. `[recomp 0x00734EB1]` No shipped label has more
  than one.

### Decoding and the clean-up

- Each unit is NOT-ed back, up to the first NUL. `[recomp 0x00734B35]`
- Then **every** string is tidied, before anything uses it.
  `[recomp 0x00734B4B]`–`0x00734BB4`
  - a space at the start of the string, or right after a newline or tab, is
    dropped;
  - a run of spaces becomes a single space;
  - a space just before a newline or tab, or at the end of the string, is
    dropped;
  - newlines and tabs are kept. Tabs are treated like line breaks here.
- So text in the game never has double spaces or trailing spaces, whatever
  the file holds.

### Lookup

- After loading, the labels are sorted with `qsort` using `_stricmp`, and
  looked up with `bsearch` using the same comparison. `[recomp 0x007348DB]`,
  `0x00734E93` `_stricmp` is at `0x007C8D20`: ASCII case folding. So
  **labels ignore case**.
- A label that isn't found gives the text `MISSING:'label'` (a wide format
  string at `0x00845820`), and the game logs
  `***NO_STRING*** '%s' -> file:%s line:%d`. `[recomp 0x00734EC2]`
- If no table was loaded, every lookup gives `***FATAL*** String Manager
  failed to initilaized properly`, with the original's spelling.
  `[recomp 0x00734E79]`

### Repeated labels

- Labels that differ only in case are the same label to the lookup. Which
  copy `bsearch` finds depends on the unstable sort. We keep the **first**
  in file order and record the rest. `[hypothesis]`
- `ra2md.csf` has none. `ra2.csf` has three, all case variants with
  different text. For example, `txt_structure_tab` is "Select Structures Tab"
  and `TXT_STRUCTURE_TAB` is "Structure Tab". This matters for RA2 mode (M9),
  and is an open question there.

### Languages

The language field indexes this table `[recomp 0x00845678]`:

| Id | Name |
|---|---|
| 0 | US |
| 1 | UK |
| 2 | German |
| 3 | French |
| 4 | Spanish |
| 5 | Italian |
| 6 | Japanese |
| 7 | Jabberwockie |
| 8 | Korean |
| 9 | Chinese |

Any other value is logged as `Unknown`.

## Results on a real install

| File | Version | Language | Labels | With extra values | Repeats |
|---|---:|---|---:|---:|---:|
| `ra2md.csf` (`langmd.mix`) | 3 | US | 5,211 | 1,204 | 0 |
| `ra2.csf` (`language.mix`) | 3 | US | 4,479 | 1,204 | 3 |

For example, `Name:E1` is "GI" and `NAME:GAPOWR` is "Power Plant". The
clean-up is idempotent on every string in both tables: tidying a tidied
string changes nothing.

## Quirks we match

- Whitespace is tidied, so the game never shows double or trailing spaces.
- Labels ignore case.
- A missing label shows `MISSING:'label'`.

## Open questions

- Which of `ra2.csf`'s repeated labels the original shows. We'd need to
  model its `qsort`, or observe it in RA2 mode.
- Whether anything reads a label's second and later strings.

## Parity

| Check | How | Tolerance |
|---|---|---|
| Format, decoding, clean-up, lookup, synthetic | `ra2-formats` unit tests | exact |
| Real tables parse, known labels | `ra2tool csf stats`, `tests/real_install.rs` | exact |
| Every label's text matches what the original shows | M2 observables hook, dumping the table after load | exact (not done yet) |
