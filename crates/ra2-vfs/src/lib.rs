//! The game's file system.
//!
//! [`install`] finds the player's copy of the game, [`archive`] reads MIX
//! archives, and [`mount`] mounts them in the game's order and resolves names.

pub mod archive;
pub mod install;
pub mod mount;
pub mod names;
pub mod theater;
