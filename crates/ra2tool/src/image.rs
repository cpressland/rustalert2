//! Writing PNGs for inspection. Output goes wherever the user points it, never
//! into the repository: images of game data are game data.

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

/// Write an RGBA image.
pub(crate) fn write_rgba(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(io::Error::other)?;
    w.write_image_data(rgba).map_err(io::Error::other)?;
    Ok(())
}
