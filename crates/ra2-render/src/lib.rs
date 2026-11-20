//! The renderer. A scene is composed from the map and its objects into a
//! list of palette-indexed pieces, which [`canvas`] composites on the CPU
//! (the reference, and `ra2tool`'s images). [`viewer`] shows a composed map
//! in a wgpu window; drawing the pieces on the GPU directly comes with the
//! game loop (M4).

pub mod canvas;
pub mod compose;
pub mod lighting;
pub mod scene;
pub mod terrain;
pub mod viewer;
pub mod voxel;
