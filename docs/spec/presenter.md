# Presenter: window and scaling

**Status:** Implemented (map viewer; the game's own frame comes with M4)
**Crate:** `ra2-render` (`viewer`)
**Issue:** #28

## Scope

How a finished frame reaches the screen: the window, fullscreen, and how
texels are scaled to screen pixels. Drawing the frame itself is in the
other M3 notes. This isn't game behaviour (the original ran at a fixed
resolution), so the reference is the recomp's presenter rather than
`gamemd.exe`.

## Sources

- The recomp's presenter (`docs/presenter.md` there): its scaling modes
  and their order.

## Behaviour

- One window, resizable, HiDPI-aware; fullscreen is borderless on the
  current monitor (`F` or `F11` in the viewer).
- Scaling modes (`M` cycles):
  - **nearest**: each screen pixel shows the texel under its centre, at
    any zoom;
  - **integer**: nearest, with the zoom held to whole numbers (or their
    reciprocals below 1), so every texel is the same size;
  - **sharp bilinear**: texels are flat squares and only the last screen
    pixel at each texel edge blends with the neighbour, so any zoom looks
    sharp without uneven texel sizes.
- At zoom 1 every mode reproduces the frame exactly. (A headless test
  renders through the GPU path and compares bytes.)

## Running a game

- The simulation steps at a fixed rate, the game speed's frames per
  second; wall time is carried over between calls, and a stall runs at
  most five frames at once rather than speeding up. (`ra2-game::FixedStep`)
- Drawing blends each object between its position before the last step
  and its current one by how far wall time is into the next step, so
  movement is smooth at any display rate.
- A frame is the map without its own units (composed once per layer
  change) with the live objects drawn over it each frame
  (`MapView::draw_live`).
- `rustalert2 <scenario.toml>` plays a parity scenario this way in the
  viewer; `--snapshot out.png --frames N` writes frame N headless.

## Open questions

- The recomp makes sharp bilinear its default; we keep nearest as the
  viewer's default until the game's own frame (M4) settles which reads
  best for comparisons.
- The palette-indexed GPU path (8-bit sprites with the palette applied in
  the shader): frames are still composed on the CPU and uploaded whole,
  fine for small maps but not for full-size ones (#95).
