//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut ini = ra2_formats::ini::Ini::parse(data);
    ini.load(data);
    for s in ini.sections() {
        for (_, v) in s.entries() {
            let _ = ra2_formats::ini::parse_int(v);
            let _ = ra2_formats::ini::parse_number(v);
            let _ = ra2_formats::ini::parse_bool(v);
            let _ = ra2_formats::ini::parse_list(v);
        }
    }
});
