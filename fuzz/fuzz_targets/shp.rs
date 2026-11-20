//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(shp) = ra2_formats::shp::Shp::parse(data) {
        for i in 0..shp.frames.len().min(64) {
            let _ = shp.frame_on_canvas(i);
        }
    }
});
