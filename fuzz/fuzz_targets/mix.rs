//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(index) = ra2_formats::mix::Index::parse(data) {
        let _ = index.check_bounds(data.len() as u64);
        let _ = index.find("rulesmd.ini");
    }
    let _ = ra2_formats::mix::local_database_names(data);
});
