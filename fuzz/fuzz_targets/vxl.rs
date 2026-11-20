//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ra2_formats::vxl::Vxl::parse(data);
    let _ = ra2_formats::vxl::Hva::parse(data);
});
