//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use ra2_formats::audio::{aud, bag_index, bag_sound, ms_ima, wav};
    let _ = wav(data);
    let _ = aud(data);
    let _ = ms_ima(data, 1, 512, None);
    let _ = ms_ima(data, 2, 1024, Some(100));
    if let Ok(entries) = bag_index(data) {
        for e in entries.iter().take(16) {
            let _ = bag_sound(data, e);
        }
    }
});
