//! The encodings map files use: base64 text, LZO1X and Westwood's LCW
//! ("Format80") compression, and the block framing around them.
//! Written from the formats' descriptions; see `docs/spec/map.md`.

use std::fmt;

/// Why data could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Base64,
    /// The compressed stream ran out, or referred outside its output.
    Corrupt(&'static str),
    /// A block didn't decompress to the size its header gave.
    BlockSize {
        expected: usize,
        got: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Base64 => f.write_str("invalid base64"),
            Error::Corrupt(why) => write!(f, "corrupt compressed data: {why}"),
            Error::BlockSize { expected, got } => {
                write!(f, "block decompressed to {got} bytes, not {expected}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Decode standard base64, ignoring whitespace; padding is optional.
pub fn base64(text: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut n) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return Err(Error::Base64),
        };
        bits = (bits << 6) | u32::from(v);
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push(((bits >> n) & 0xff).to_le_bytes()[0]);
        }
    }
    Ok(out)
}

/// The two compressions used in map packs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Lzo,
    Lcw,
}

/// Decompress a pack: a run of blocks, each a `u16` compressed size, a
/// `u16` decompressed size, then the compressed bytes.
pub fn unpack_blocks(data: &[u8], compression: Compression) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let head = data
            .get(at..at + 4)
            .ok_or(Error::Corrupt("block header is truncated"))?;
        let packed = usize::from(u16::from_le_bytes([head[0], head[1]]));
        let unpacked = usize::from(u16::from_le_bytes([head[2], head[3]]));
        let body = data
            .get(at + 4..at + 4 + packed)
            .ok_or(Error::Corrupt("block is truncated"))?;
        let block = match compression {
            Compression::Lzo => lzo1x(body, unpacked)?,
            Compression::Lcw => lcw(body, unpacked)?,
        };
        if block.len() != unpacked {
            return Err(Error::BlockSize {
                expected: unpacked,
                got: block.len(),
            });
        }
        out.extend_from_slice(&block);
        at += 4 + packed;
    }
    Ok(out)
}

struct Input<'a> {
    data: &'a [u8],
    at: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .data
            .get(self.at)
            .ok_or(Error::Corrupt("input ran out"))?;
        self.at += 1;
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from(self.byte()?) | (u16::from(self.byte()?) << 8))
    }

    /// A length extended by zero bytes: each 0 adds 255, then the next byte.
    fn extended(&mut self, base: usize) -> Result<usize, Error> {
        let mut n = 0usize;
        loop {
            let b = self.byte()?;
            if b != 0 {
                return Ok(n + base + usize::from(b));
            }
            n += 255;
            if n > 1 << 24 {
                return Err(Error::Corrupt("length too long"));
            }
        }
    }

    fn literals(&mut self, out: &mut Vec<u8>, n: usize) -> Result<(), Error> {
        let src = self
            .data
            .get(self.at..self.at + n)
            .ok_or(Error::Corrupt("literals run past the input"))?;
        out.extend_from_slice(src);
        self.at += n;
        Ok(())
    }
}

/// Copy `n` bytes from `distance` back in the output, byte by byte so a
/// short distance repeats.
fn copy_back(out: &mut Vec<u8>, distance: usize, n: usize, limit: usize) -> Result<(), Error> {
    if distance == 0 || distance > out.len() {
        return Err(Error::Corrupt("match before the start of the output"));
    }
    if out.len() + n > limit {
        return Err(Error::Corrupt("output larger than expected"));
    }
    let start = out.len() - distance;
    for k in 0..n {
        out.push(out[start + k]);
    }
    Ok(())
}

/// LZO1X decompression.
pub fn lzo1x(data: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(expected);
    let mut r = Input { data, at: 0 };
    let limit = expected;

    // The state between instructions: how many literals the last match
    // carries (0 to 3), and whether we're right after a literal run.
    let mut t = usize::from(*data.first().ok_or(Error::Corrupt("empty"))?);
    let mut after_literals;
    if t > 17 {
        r.at = 1;
        t -= 17;
        r.literals(&mut out, t)?;
        after_literals = t >= 4;
        t = usize::from(r.byte()?);
        if !after_literals {
            // Fewer than 4 literals: the next byte is a match instruction.
            after_literals = false;
        }
    } else {
        t = usize::from(r.byte()?);
        after_literals = false;
        if t < 16 {
            // A literal run.
            let n = if t == 0 { r.extended(15)? } else { t };
            r.literals(&mut out, n + 3)?;
            after_literals = true;
            t = usize::from(r.byte()?);
        }
    }

    loop {
        // `t` is an instruction byte.
        let trailing = if t >= 64 {
            let distance = 1 + ((t >> 2) & 7) + (usize::from(r.byte()?) << 3);
            copy_back(&mut out, distance, (t >> 5) + 1, limit)?;
            t & 3
        } else if t >= 32 {
            let n = if t & 31 == 0 { r.extended(31)? } else { t & 31 };
            let d = usize::from(r.u16()?);
            copy_back(&mut out, 1 + (d >> 2), n + 2, limit)?;
            d & 3
        } else if t >= 16 {
            let high = (t & 8) << 11;
            let n = if t & 7 == 0 { r.extended(7)? } else { t & 7 };
            let d = usize::from(r.u16()?);
            let distance = high + (d >> 2);
            if distance == 0 {
                // End of stream.
                if r.at != data.len() || out.len() != expected {
                    return Err(Error::Corrupt("stream ended early or late"));
                }
                return Ok(out);
            }
            copy_back(&mut out, distance + 0x4000, n + 2, limit)?;
            d & 3
        } else if after_literals {
            // Right after a literal run of 4 or more: a 3-byte match far back.
            let distance = 1 + 0x0800 + (t >> 2) + (usize::from(r.byte()?) << 2);
            copy_back(&mut out, distance, 3, limit)?;
            t & 3
        } else {
            let distance = 1 + (t >> 2) + (usize::from(r.byte()?) << 2);
            copy_back(&mut out, distance, 2, limit)?;
            t & 3
        };

        if trailing > 0 {
            // The match carries 1 to 3 literals; then another instruction
            // that is read as a match.
            r.literals(&mut out, trailing)?;
            after_literals = false;
            t = usize::from(r.byte()?);
            continue;
        }
        // No trailing literals: the next byte may start a literal run.
        t = usize::from(r.byte()?);
        if t < 16 {
            let n = if t == 0 { r.extended(15)? } else { t };
            r.literals(&mut out, n + 3)?;
            after_literals = true;
            t = usize::from(r.byte()?);
        } else {
            after_literals = false;
        }
        if out.len() > limit {
            return Err(Error::Corrupt("output larger than expected"));
        }
    }
}

/// Westwood LCW ("Format80") decompression, with absolute positions for the
/// long forms, as TS/RA2 use it.
pub fn lcw(data: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    let mut r = Input { data, at: 0 };
    while let Ok(cmd) = r.byte() {
        if cmd & 0x80 == 0 {
            // 0cccpppp pppppppp: copy c+3 bytes from p back.
            let n = usize::from((cmd & 0x70) >> 4) + 3;
            let distance = (usize::from(cmd & 0x0f) << 8) | usize::from(r.byte()?);
            copy_back(&mut out, distance, n, expected)?;
        } else if cmd & 0x40 == 0 {
            // 10cccccc: c literals; 0x80 alone ends the stream.
            let n = usize::from(cmd & 0x3f);
            if n == 0 {
                break;
            }
            if out.len() + n > expected {
                return Err(Error::Corrupt("output larger than expected"));
            }
            r.literals(&mut out, n)?;
        } else if cmd == 0xfe {
            let n = usize::from(r.u16()?);
            let v = r.byte()?;
            if out.len() + n > expected {
                return Err(Error::Corrupt("output larger than expected"));
            }
            out.resize(out.len() + n, v);
        } else {
            let n = if cmd == 0xff {
                usize::from(r.u16()?)
            } else {
                usize::from(cmd & 0x3f) + 3
            };
            let pos = usize::from(r.u16()?);
            if pos >= out.len() && n > 0 {
                return Err(Error::Corrupt("copy from beyond the output"));
            }
            if out.len() + n > expected {
                return Err(Error::Corrupt("output larger than expected"));
            }
            for k in 0..n {
                out.push(out[pos + k]);
            }
        }
        if out.len() == expected && r.at >= data.len() {
            break;
        }
    }
    Ok(out)
}

/// Encode standard base64 with padding.
pub fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= c.len() {
                out.push(char::from(
                    A[usize::try_from((n >> (18 - 6 * i)) & 63).expect("6 bits")],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Encode a pack as the game reads one, storing the data uncompressed: each
/// block a single literal run. Valid LZO1X and LCW streams, just not small.
/// Used to write scenario maps.
///
/// Blocks hold 8,000 bytes so that, stored, they still pack into fewer than
/// 8,192 bytes: the game's map readers expect no packed block larger, and a
/// map with 8,192-byte stored blocks crashed it reading `[OverlayPack]`.
pub fn pack_blocks_stored(data: &[u8], compression: Compression) -> Vec<u8> {
    const BLOCK: usize = 8000;
    let mut out = Vec::new();
    for block in data.chunks(BLOCK) {
        let packed = match compression {
            Compression::Lzo => {
                // A literal run of n bytes: 0, then n - 3 - 15 in extended
                // form; then the end-of-stream match (0x11 0 0).
                let mut p = Vec::new();
                if block.len() < 4 {
                    // Too short for the run form: first-byte literals.
                    p.push(u8::try_from(block.len() + 17).expect("small"));
                } else if block.len() <= 18 {
                    // A short run: its length less 3, in one byte.
                    p.push(u8::try_from(block.len() - 3).expect("small"));
                } else {
                    p.push(0);
                    let mut rest = block.len() - 3 - 15;
                    while rest > 255 {
                        p.push(0);
                        rest -= 255;
                    }
                    p.push(u8::try_from(rest).expect("<= 255"));
                }
                p.extend_from_slice(block);
                p.extend_from_slice(&[0x11, 0, 0]);
                p
            }
            Compression::Lcw => {
                let mut p = Vec::new();
                for run in block.chunks(63) {
                    p.push(0x80 | u8::try_from(run.len()).expect("< 64"));
                    p.extend_from_slice(run);
                }
                p.push(0x80);
                p
            }
        };
        out.extend_from_slice(
            &u16::try_from(packed.len())
                .expect("block fits")
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u16::try_from(block.len())
                .expect("block fits")
                .to_le_bytes(),
        );
        out.extend_from_slice(&packed);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("TWFu").unwrap(), b"Man");
        assert_eq!(base64("TWE=").unwrap(), b"Ma");
        assert_eq!(base64("TQ\n==").unwrap(), b"M");
        assert_eq!(base64("T!"), Err(Error::Base64));
    }

    #[test]
    fn lcw_forms() {
        // Literals "ab", relative copy 3 from 2 back ("aba"), fill 4 x 'z',
        // absolute copy 3 from 0 ("aba"), end.
        let data = [
            0x82, b'a', b'b', 0x00, 0x02, 0xfe, 4, 0, b'z', 0xc0, 0, 0, 0x80,
        ];
        assert_eq!(lcw(&data, 12).unwrap(), b"ababazzzzaba");
        assert!(lcw(&[0x00, 0x05], 3).is_err(), "copy before the start");
        assert!(lcw(&[0x85, 1, 2], 5).is_err(), "literals run out");
    }

    #[test]
    fn lzo_literal_and_match() {
        // First byte > 17: 4 literals "abcd" (21 - 17); then a short match
        // (t = 0x00 after a 4+ literal run means a 3-byte copy from
        // 1 + 0x800 back, which is invalid here), so instead use an M2 match:
        // t = 0x47: distance 1 + ((0x47 >> 2) & 7) + (0 << 3) = 2, length
        // (0x47 >> 5) + 1 = 3, trailing 3 literals "xyz"; then end marker
        // 0x11 0x00 0x00.
        let data = [
            21, b'a', b'b', b'c', b'd', 0x47, 0x00, b'x', b'y', b'z', 0x11, 0x00, 0x00,
        ];
        assert_eq!(lzo1x(&data, 10).unwrap(), b"abcdcdcxyz");
        assert!(lzo1x(&data, 9).is_err());
        assert!(lzo1x(&data[..12], 10).is_err());
    }

    #[test]
    fn blocks() {
        let block = [0x83, b'a', b'b', b'c', 0x80];
        let mut data = vec![5, 0, 3, 0];
        data.extend_from_slice(&block);
        data.extend_from_slice(&[5, 0, 3, 0]);
        data.extend_from_slice(&block);
        assert_eq!(unpack_blocks(&data, Compression::Lcw).unwrap(), b"abcabc");
        data[2] = 4;
        assert!(unpack_blocks(&data, Compression::Lcw).is_err());
    }

    #[test]
    fn stored_packs_round_trip() {
        for len in [0usize, 1, 3, 4, 18, 19, 300, 8000, 8192, 20_000] {
            let data: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect();
            for c in [Compression::Lzo, Compression::Lcw] {
                let packed = pack_blocks_stored(&data, c);
                assert_eq!(unpack_blocks(&packed, c).unwrap(), data, "{c:?} {len}");
            }
            assert_eq!(base64(&base64_encode(&data)).unwrap(), data);
        }
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        // Every packed block stays under the 8,192 bytes the game reads.
        let big = vec![7u8; 100_000];
        for c in [Compression::Lzo, Compression::Lcw] {
            let p = pack_blocks_stored(&big, c);
            let mut at = 0;
            while at < p.len() {
                let n = usize::from(u16::from_le_bytes([p[at], p[at + 1]]));
                assert!(n < 8192, "{c:?} block of {n}");
                at += 4 + n;
            }
        }
    }

    #[test]
    fn noise_never_panics() {
        let mut seed = 99u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            seed.to_le_bytes()[2]
        };
        for len in 0..300 {
            let d: Vec<u8> = (0..len).map(|_| next()).collect();
            let _ = lzo1x(&d, 512);
            let _ = lcw(&d, 512);
            let _ = unpack_blocks(&d, Compression::Lzo);
        }
    }
}
