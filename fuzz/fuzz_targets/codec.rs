//! Fuzz target: must never panic, whatever the input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use ra2_formats::codec::{Compression, lcw, lzo1x, unpack_blocks};
    let _ = lzo1x(data, 1 << 16);
    let _ = lcw(data, 1 << 16);
    let _ = unpack_blocks(data, Compression::Lzo);
    let _ = unpack_blocks(data, Compression::Lcw);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = ra2_formats::codec::base64(text);
        let _ = ra2_formats::map::Map::from_ini(&ra2_formats::ini::Ini::parse(data));
    }
});
