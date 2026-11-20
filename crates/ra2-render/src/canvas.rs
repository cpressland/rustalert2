//! A CPU canvas: the reference compositor the GPU renderer must match, and
//! what `ra2tool` uses to draw maps to images.

use ra2_formats::pal::Rgb;

/// An RGBA image covering a rectangle of world pixels.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// The world pixel at the canvas's top-left.
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    /// A canvas over world pixels `x0..x1`, `y0..y1`, filled with `background`.
    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32, background: [u8; 4]) -> Canvas {
        let width = u32::try_from((x1 - x0).max(1)).expect("positive");
        let height = u32::try_from((y1 - y0).max(1)).expect("positive");
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            *px = background;
        }
        Canvas {
            origin_x: x0,
            origin_y: y0,
            width,
            height,
            rgba,
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = (x - self.origin_x, y - self.origin_y);
        let (w, h) = (
            i32::try_from(self.width).ok()?,
            i32::try_from(self.height).ok()?,
        );
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            return None;
        }
        Some((usize::try_from(cy).ok()? * self.width as usize + usize::try_from(cx).ok()?) * 4)
    }

    /// Set one world pixel.
    pub fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if let Some(i) = self.index(x, y) {
            self.rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }

    /// Darken one world pixel, as shadows do.
    pub fn shade(&mut self, x: i32, y: i32, keep_256ths: u16) {
        if let Some(i) = self.index(x, y) {
            for c in &mut self.rgba[i..i + 3] {
                *c = u8::try_from(u16::from(*c) * keep_256ths / 256).expect("smaller");
            }
        }
    }

    /// Draw palette-indexed pixels at a world position. Index 0 is
    /// transparent; with `shadow`, index 1 darkens what is underneath.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        pixels: &[u8],
        palette: &[Rgb; 256],
        shadow: bool,
    ) {
        if width == 0 {
            return;
        }
        for (k, &p) in pixels.iter().enumerate() {
            if p == 0 {
                continue;
            }
            let (px, py) = (
                x + i32::try_from(k % width).expect("small"),
                y + i32::try_from(k / width).expect("small"),
            );
            if shadow && p == 1 {
                self.shade(px, py, 128);
            } else {
                self.put(px, py, palette[usize::from(p)]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blits_in_world_space() {
        let mut pal = [[0u8; 3]; 256];
        pal[5] = [10, 20, 30];
        let mut c = Canvas::new(-10, -10, 10, 10, [0, 0, 0, 255]);
        c.blit(-1, -1, 2, &[5, 0, 0, 5], &pal, false);
        let at = |c: &Canvas, x: i32, y: i32| {
            c.index(x, y)
                .map(|i| [c.rgba[i], c.rgba[i + 1], c.rgba[i + 2]])
        };
        assert_eq!(at(&c, -1, -1), Some([10, 20, 30]));
        assert_eq!(at(&c, 0, -1), Some([0, 0, 0]));
        assert_eq!(at(&c, 0, 0), Some([10, 20, 30]));
        assert_eq!(at(&c, 10, 0), None);
        c.put(100, 100, [1, 1, 1]);
    }
}
