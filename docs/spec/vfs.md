# File search order (VFS)

**Status:** Implemented
**Crate:** `ra2-vfs` (`mount`)
**Issue:** #11

## Scope

How the game finds a file by name: loose files, the archives it mounts at
startup and their order, and how nested archives are reached. The archive
format is in [mix.md](mix.md). Archives mounted when a scenario loads
(theaters, sides) are listed here but specified with the systems that use
them.

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs.
- Observation: `ra2tool vfs mounts` and `ra2tool vfs which` on a Steam
  install of *The Ultimate Collection*.

## Behaviour

### The archive list

- Every opened archive is linked onto one global list, at the tail.
  `[recomp 0x005B3DE2]` (the `MixFileClass` constructor at `0x005B3C20`).
  The list object is at `0x00ABEFD8`, with head and tail sentinel nodes at
  `0x00ABEFDC` and `0x00ABEFE8`. Each node has `+4` next and `+8` prev.
  `[recomp 0x005B3AC0]`
- So the list is in **mount order**.
- A name resolves to the **first** archive in list order that holds it. The
  earliest-mounted archive wins. `[observed]` In the parity lab, a
  `rulesmd.ini` in an added `expandmd99.mix` (mounted before `expandmd01`
  and `localmd`, which both hold one) gave the GI the modified strength
  ([parity.md](parity.md)). Everything else points the same way:
  - the expansion archives are mounted first, and their purpose is to
    override;
  - `EXPANDMD##` is mounted counting *down* from 99, so a higher number is
    a later patch and wins;
  - the community describes `expandmd##.mix` as highest priority
    `[community]`;
  - on a real install, `rulesmd.ini` then resolves to `expandmd01.mix`
    (1.001's patched rules) rather than `localmd.mix` `[observed]`.
- An archive is opened by name through the same file lookup. That's how
  nested archives work: `localmd.mix` is found inside the already-mounted
  `ra2md.mix` and read in place. `[recomp 0x005303CD]` pushes only the
  name `LOCALMD.MIX`; there's no path. `[observed]`

### Loose files

- A loose file in the game folder overrides the copy in any archive.
  `[observed]` In the parity lab, a loose `rulesmd.ini` changed the GI's
  strength, and a loose `XMP29U2.MAP` replaced the shipped map of that name
  ([parity.md](parity.md)). The header of `rulesmd.ini` itself says so too:
  "If placed in game directory, it will override built in values". We
  haven't yet found the check in the recomp (`CCFileClass`).
- Names are case-insensitive. The game runs on Windows file systems, so we
  match loose files case-insensitively on every platform.
- Only the game folder itself is searched, not subfolders. The exception is
  the movie archives, which are also tried under `MIXFILES\` (see below).

### Startup order (Yuri's Revenge)

The sequence below is in execution order. "If present" means the step
checks that the file is available (`CCFileClass` `Is_Available`, vtable
`+0x14`) and skips it otherwise. `[recomp]`

| # | Archives | Where | Notes |
|---|---|---|---|
| 1 | `LANGMD.MIX`, `LANGUAGE.MIX` | WinMain `0x006BD7E8`, `0x006BD818` | before game init |
| 2 | `EXPANDMD99.MIX` … `EXPANDMD00.MIX` | `0x005301A0`, loop `0x005301BA`–`0x0053028A` | counts down; each one if present |
| 3 | `RA2MD.MIX`, `RA2.MIX` | `0x005302A3`, `0x005302CB` | |
| 4 | `CACHEMD.MIX`, `CACHE.MIX` | `0x0053031F`, `0x00530361` | nested in 3 |
| 5 | `LOCALMD.MIX`, `LOCAL.MIX` | `0x005303CD`, `0x005303F9` | nested in 3 |
| 6 | `AUDIOMD.MIX`, `AUDIO.MIX` | `0x00406B10` (called from `0x0052BBC7`) | nested in 1 |
| 7 | `ECACHE*.MIX`, then `ELOCAL*.MIX` | `0x00530000` (`FindFirstFile`) | loose files only |
| 8 | `CONQMD`, `GENERMD`, `GENERIC`, `ISOGENMD`, `ISOGEN`, `CONQUER`, `CAMEOMD`, `CAMEO` | `0x00530460`… | each if present |
| 9 | `MAPSMD*.MIX`, then `MAPS*.MIX` | `0x005307B0`, `0x005308EF` | wildcard search; see the open questions |
| 10 | `MULTIMD.MIX` | `0x00530B76` | |
| 11 | `THEMEMD.MIX`, **else** `THEME.MIX` | `0x00530C09`–`0x00530CE7` | one slot: `THEME.MIX` only if `THEMEMD.MIX` is missing |
| 12 | `MOVMD*.MIX`, `MOVIES*.MIX` | `0x00530D05`… | in the folder, else under `MIXFILES\` |

Steps 1 to 7 run in the game's init routine (`0x0052BA60`, called through
`0x0048CCC0` from WinMain). Step 8 onwards is the "secondary" set
(`0x00530460`, whose log string is `Init Secondary Mixfiles.....` at
`0x00825F80`).

- **Wildcards** use `FindFirstFile`/`FindNextFile`, which on NTFS return
  names in sorted order. We sort lower-cased names. `MAPS*.MIX` also matches
  `MAPSMD03.MIX`; the game would open it twice, and the second copy can
  never win a lookup, so we skip it.

### Scenario-time archives

Mounted when a scenario loads. These are specified with their systems:

- the theater archives, built from the theater's name with `%s.MIX` and
  `%sMD.MIX` patterns (the `ISO…` tile sets and the theater art);
- `NTRLMD.MIX`/`NEUTRAL.MIX` `[recomp 0x00534F49]`;
- `SIDEC%02dMD.MIX`/`SIDEC%02d.MIX` (sidebar art for the player's side)
  `[recomp 0x005350DB]`;
- `LOADMD.MIX`/`LOAD.MIX` (loading screens) `[recomp 0x00552CE7]`.

### Mods

Our addition: a mod folder laid over the game. A loose file in a mod folder
wins over the game folder's. Because archives are found through the same
lookup, a mod can also replace an archive, for example its own
`expandmd99.mix`.

## Results on a real install

`ra2tool vfs mounts` on the Steam install mounts 27 archives. The expansion
archive is `expandmd01.mix`. Some resolutions:

| File | Resolves to |
|---|---|
| `rulesmd.ini` | `expandmd01.mix` (the 1.001 patch) |
| `soundmd.ini` | `expandmd01.mix` |
| `artmd.ini`, `aimd.ini`, `evamd.ini` | `localmd.mix` (in `ra2md.mix`) |
| `rules.ini` | `local.mix` (in `ra2.mix`) |
| `ra2md.csf` | `langmd.mix` |
| `mouse.sha` | `conqmd.mix` |

## Quirks we match

- `THEME.MIX` isn't mounted when `THEMEMD.MIX` exists, so RA2's music isn't
  reachable in YR through the archives.

## Open questions

- **The lookup routine.** It's settled by observation; reading it in the
  recomp would add the citation.
- **The loose-file override.** It's settled by observation; `CCFileClass`'s
  disk check would add the citation.
- **Wildcard versus numbered maps and movies.** The map and movie steps
  branch on a flag at `0x0089E3A0` between the wildcard search and the
  numbered `MAPSMD%02d.MIX`/`MAPS%02d.MIX` and `MOVMD%02d`/`MOVIES%02d`
  names. We need to find out what sets the flag. We use the wildcard path,
  which mounts the same files on a normal install.
- **`MULTI.MIX`.** YR references it at `0x004794E4`, in a routine
  (`0x004790B0`) called before the bootstrap. Is RA2's multiplayer archive
  mounted in YR?
- **Red Alert 2's order.** `startup_sequence(Game::RedAlert2)` is YR's
  sequence with the YR archives removed, and is `[hypothesis]` until it's
  read from `game.exe` (M9).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Mount order and resolution, synthetic | `ra2-vfs` unit tests | exact |
| Real install: startup mounts, key files resolve | `ra2tool vfs mounts`, `tests/real_install.rs` | exact |
| Earliest-mounted archive wins; loose files win | parity lab experiments (`expandmd99.mix`, loose `rulesmd.ini`) | exact (done) |
