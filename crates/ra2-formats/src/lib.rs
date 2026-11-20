//! Readers for the file formats of *Red Alert 2* and *Yuri's Revenge*.
//!
//! Every reader is pure: it takes bytes and returns typed data, with no file
//! system or global state. Behaviour is specified in `docs/spec/` first.

pub mod codec;
pub mod csf;
pub mod ini;
pub mod map;
pub mod mix;
pub mod pal;
pub mod shp;
pub mod tmp;
pub mod vxl;
