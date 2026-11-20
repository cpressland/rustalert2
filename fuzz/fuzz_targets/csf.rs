//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(t) = ra2_formats::csf::StringTable::parse(data) {
        let _ = t.text("Name:E1");
    }
});
