# MIX archives

**Status:** Implemented
**Crates:** `ra2-formats` (`mix`: the index), `ra2-vfs` (`archive`: reading from disk and nesting)
**Issues:** #9 (spec), #10 (reader)

## Scope

The MIX container: its two header layouts, the encrypted index, file IDs,
the optional checksum, nesting, and XCC's name database. The order the game
mounts archives in is a separate note (VFS, #11).

## Sources

- `gamemd.exe` 1.001 as recompiled by redalert2-recomp. Addresses are VAs
  (image base `0x00400000`; raw offsets equal RVAs).
- Community: the XCC Utilities documentation and the ModEnc *MIX* article.
- Observation: `ra2tool mix check` on a Steam install of *The Ultimate
  Collection* (YR 1.001, RA2 1.006). Every installed archive, and every
  archive nested in them under a known name, opened and verified (below).

## Behaviour

### File IDs

- An archive's index doesn't store names. Each file is filed under a 32-bit ID
  computed from its name. `[community]`
- **TS/RA2 IDs:** upper-case the name (ASCII). If its length isn't a multiple
  of 4, append one byte holding `len % 4`, then repeat the first byte of the
  last partial 4-byte group until the length is a multiple of 4. The ID is the
  standard CRC-32 (IEEE, as in zlib) of the result. `[community]`
  `[observed]` (all the archive and INI names in `ra2-vfs::names` resolve in
  the real archives, and `local mix database.dat` gives `0x366E051F`, the
  value XCC documents).
- **TD/RA IDs** (older archives): upper-case, then for each little-endian
  32-bit group of the name (the last one zero-padded),
  `id = rotl(id, 1) + group`. `[community]` We have no archive of this kind
  to test against, so it's tested only on our own encodings.
- Names are case-insensitive because of the upper-casing.

### Header layouts

All integers are little-endian.

- If the first `u16` is non-zero, it's the **classic** layout: that `u16` is
  the file count, then a `u32` body size, then the entries. `[community]`
- Otherwise the first `u32` is a **flags** word. `[community]`
  - `0x00010000`: a 20-byte SHA-1 of the body follows the body.
  - `0x00020000`: the index is encrypted.
  - We reject any other bit as not a MIX.
- Unencrypted extended layout: flags, `u16` count, `u32` body size, then the
  entries.
- An **entry** is 12 bytes: `u32 id`, `u32 offset`, `u32 size`. The offset is
  from the start of the body.
- The **body** starts right after the index (after the padded index, when
  encrypted).
- Westwood's archives list entries sorted by ID, as a signed 32-bit value.
  `[community]` We sort our own lookup table and don't depend on the file's
  order.
- Two entries with the same ID make the archive invalid. On real data, this
  is also what a wrong decryption key looks like.

### The encrypted index

- After the flags come 80 bytes of **key source**, then the index encrypted
  with Blowfish. `[community]`
- **Unwrapping the key:**
  - Westwood's public RSA key is held in the executable as text,
    `1=AihRvNoIbTn85FZRYNZRcT+i6KpU+maCsEqr3Q5q+LDB5tH7Tz2qQ38V`.
    `[recomp 0x007E1A8C]` (a string in `.rdata`)
  - It's base64 for a DER `INTEGER`: tag `0x02`, length `0x28`, then a
    40-byte big-endian modulus *n* of 319 bits. The exponent is `0x10001`.
    `[community]`
  - Split the key source into two 40-byte blocks. Read each as a
    little-endian number *c*, compute *c*^e mod *n*, and write the result as
    39 little-endian bytes. (`(bits(n) − 1) / 8 = 39`.) `[community]`
  - The 78 resulting bytes, truncated to the first 56, are the **Blowfish
    key**. `[community]`
- **Blowfish** is the standard cipher: big-endian 32-bit halves, ECB, no IV.
  `[observed]` The byte-swapped (little-endian) variant gives garbage. Some
  descriptions read as if Westwood's were byte-swapped, so this is worth
  recording.
- The first decrypted 8-byte block starts with the `u16` count and `u32`
  body size.
- The index is `6 + 12 × count` bytes, rounded up to a multiple of 8 and
  decrypted as a whole. The body starts after the padding:
  `4 + 80 + padded`.

### Checksum

- With flag `0x00010000`, the 20 bytes after the body are the SHA-1 of the
  body. `[community]` `[observed]` (it matched on all 103 archives that carry
  one).
- The game's own use of the checksum (whether it verifies it, and when) is
  unknown. See the open questions.

### Nesting

- An archive can be a file inside another archive, and is read in place: its
  offsets are relative to its own start. `[observed]` On the Steam install,
  `ra2md.mix` holds `localmd.mix`, `cachemd.mix`, `conqmd.mix`, `loadmd.mix`,
  `ntrlmd.mix`, the `iso*md.mix` theater tile sets and others. `rulesmd.ini`
  is in `ra2md.mix` → `localmd.mix`.

### XCC's name database

- Archives built with XCC Mixer can hold `local mix database.dat`. It's a
  32-byte magic string (`XCC by Olaf van der Spek` + `1A 04 17 27 10 19 80
  00`), then `u32` size, type, version and count, then that many
  NUL-terminated names. `[community]`
- Westwood's own archives don't have one, so their names come from what the
  game asks for. For tools, `ra2-vfs::names` lists the archive names the
  executables reference (strings in `.rdata`, for example `RA2MD.MIX`,
  `EXPANDMD%02d.MIX`, `MOVMD%02d.MIX`) and the standard INI names.

## Results on a real install

`ra2tool mix check`, Steam *Ultimate Collection* in a CrossOver bottle:

| | |
|---|---|
| Archives opened (installed and nested) | 123 |
| Entries indexed | 14,516 |
| SHA-1 checksums verified | 103 of 103 |
| Failures | 0 |

That folder also has `.mmx` and `.yro` maps, which are MIX archives, and
community files (CnCNet's `cncnet.mix`, `ares.mix`). They open as well.

## Quirks we match

- None yet. Nothing here affects gameplay.

## Open questions

- Where `MixFileClass` parses the header and whether the game checks the
  SHA-1. `MixFileClass`'s vtables are known from RTTI, but its methods
  aren't named in the recomp yet. This matters only if the game behaves
  differently with a corrupt archive.
- Which TD/RA-layout archives, if any, the game still reads. None were found
  in the install.

## Parity

Format checks, not scenarios:

| Check | How | Tolerance |
|---|---|---|
| Every archive opens, entries in bounds | `ra2tool mix check` | exact |
| Checksums | SHA-1 of the body | exact |
| Real-install test | `crates/ra2-vfs/tests/real_install.rs` (skips without an install) | exact |
| Entry counts against the recomp's own reads | M2 observables hook | exact (not done yet) |
