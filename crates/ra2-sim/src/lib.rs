//! The game simulation.
//!
//! The simulation must be deterministic: the same rules, map, seed and orders
//! give the same state on every machine, every time. That is what our own
//! lockstep multiplayer, replays and parity scenarios depend on. So this crate
//! has rules of its own, which CI enforces:
//!
//! - no floating point in simulation state (`clippy::float_arithmetic`);
//! - no wall clocks, threads, or `HashMap`/`HashSet` iteration
//!   (`clippy.toml` in this crate);
//! - only the dependencies listed in `allowed-deps.txt`.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod coords;
pub mod harvest;
pub mod hunt;
pub mod pathfind;
pub mod rng;
pub mod tick;
pub mod world;
