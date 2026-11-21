# Shroud and fog

**Status:** Implemented (start-of-game shroud with soft edges; fog to come)
**Crate:** `ra2-render` (`compose`)
**Issue:** #34

## Scope

What a player sees of the map at the start, and how unseen cells are
drawn. Revealing as units move belongs to the simulation (M4).

## Sources

- `shroud.shp`: 96 frames of 60×30, in two halves; the first 48 are the
  edge shapes. Pixels are brightness, not colours: 2 is dark, 127 clear,
  0xFE transparent. Frame 0 is all clear and frame 15 all dark. `[observed]`
- `gamemd.exe` 1.001: the neighbour-mask table, 256 signed bytes at
  0x7F4194, read from the player's own copy at run time. `[recomp]`
- `Sight=` in the rules.

## Behaviour

- The player house is `[Basic] Player=`. At the start it sees every cell
  within each of its objects' `Sight=` (in cells, default 4) of that
  object's cell. `[hypothesis]` (circle test on cell offsets)
- Terrain and objects on unseen cells aren't drawn. `[observed]` (the
  game shows black there, with no cliff faces poking out)
- Each cell picks a frame (recomp 0x006D8700 in YR, 0x006A8D40 in RA2):
  - unseen: −2, drawn as frame 15 (solid);
  - seen: a mask of its unseen neighbours, one bit each: (−1,−1) 0x40,
    (0,−1) 0x80, (+1,−1) 0x01, (−1,0) 0x20, (+1,0) 0x02, (−1,+1) 0x10,
    (0,+1) 0x08, (+1,+1) 0x04, where the first offset is the cell's x. The
    table maps the mask to a frame, or −1 (drawn as frame 0, clear) or −2
    (solid). `[recomp]`
- The frame's 60×30 box goes at the cell's centre on screen, at its
  height, less (30, 15). Its bytes are copied into a brightness buffer,
  skipping 0xFE; later cells overwrite earlier ones. `[recomp]` (0x0047EFE0)
- The buffer then darkens the frame: each channel × brightness / 127.
  `[hypothesis]` (the final blend isn't traced; 127 reads as "clear")
- Without `gamemd.exe` or `shroud.shp`, unseen cells are black and seen
  cells next to unseen ones are darkened to 55%.

## Open questions

- The second half of `shroud.shp` (frames 48–95), and when it's used.
- The exact blend of the brightness buffer onto the frame.
- Cell draw order for overlapping shapes (row order assumed).
- Fog of war (shroud that returns), gap generators and the reveal-all
  debug toggle (planned for the M4 game loop).

## Parity

| Check | How | Tolerance |
|---|---|---|
| Start-of-game shroud against the recomp | M3 exit comparison | visual |
