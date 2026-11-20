//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(f) = ra2_formats::fnt::Font::parse(data) {
        for c in ['A', ' ', '\u{6f22}'] {
            let _ = f.index(c).and_then(|i| f.glyph(i));
        }
    }
    let _ = ra2_formats::pcx::decode(data);
    if let Some(p) = ra2_formats::pal::Palette::parse(data) {
        let _ = ra2_formats::pal::remap(&p.to_rgb8(), ra2_formats::pal::Hsv { h: data[0], s: data[1], v: data[2] });
    }
});
