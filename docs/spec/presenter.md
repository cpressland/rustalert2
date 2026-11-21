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
- A frame is the map without its own units (composed on the CPU once per
  layer change and uploaded as the backdrop) with the live objects drawn
  over it on the GPU each frame.

### The GPU sprite path

- Each frame's sprites are packed, palette-indexed, into one 8-bit atlas
  (2,048 wide, shelf-packed in drawing order), with the palettes they use
  (lit by each sprite's cell light on the CPU) as 256-colour rows. The
  shader looks each index up in its row; index 0 is transparent.
  (`ra2-render::sprites`)
- The frame is built at world scale in an offscreen texture: the backdrop
  copied, then the sprites drawn over it in order. The camera pass then
  shows that texture, so every scaling mode works unchanged.
- Shadow sprites mark darkened pixels with index 1; the GPU darkens
  through the blend by 127/255, which rounds to the CPU's halving
  (`c × 128 / 256`) for every byte value.
- The CPU canvas stays the reference: a test draws overlapping sprites in
  two palettes under two cell lights with a shadow both ways and requires
  identical bytes, and a scenario frame drawn both ways matches exactly.
- `rustalert2 <scenario.toml>` plays a parity scenario this way in the
  viewer; `--snapshot out.png --frames N` writes frame N headless.

## Open questions

- The recomp makes sharp bilinear its default; we keep nearest as the
  viewer's default until the game's own frame (M4) settles which reads
  best for comparisons.
- The backdrop is still composed on the CPU (once per layer change); on
  big maps that's slow to change layers, and terrain could move to the GPU
  the same way.
- Sprites that need more atlas rows than the device's largest texture
  aren't drawn yet.
