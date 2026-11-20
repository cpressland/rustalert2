//! Sound: `.aud` files, the `audio.idx` / `audio.bag` sound bank, and WAV
//! files with PCM or IMA ADPCM data. See `docs/spec/audio.md`.

use std::fmt;

/// Decoded sound: interleaved signed 16-bit samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl Sound {
    /// Length in sample frames (one sample per channel).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

/// Why a sound could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotRecognised(&'static str),
    Truncated,
    /// A codec this decoder doesn't handle (no shipped file uses it).
    Unsupported(String),
    /// An IMA block header is out of range: the data is damaged or misread.
    BadBlock(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotRecognised(what) => write!(f, "not a valid {what}"),
            Error::Truncated => f.write_str("sound data is truncated"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::BadBlock(i) => write!(f, "IMA block {i} has a bad header"),
        }
    }
}

impl std::error::Error for Error {}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// One channel's IMA ADPCM state.
#[derive(Debug, Clone, Copy, Default)]
struct Ima {
    predictor: i32,
    index: i32,
}

impl Ima {
    fn decode(&mut self, nibble: u8) -> i16 {
        let step = IMA_STEPS[usize::try_from(self.index).expect("0..=88")];
        let n = i32::from(nibble);
        let mut diff = step >> 3;
        if n & 4 != 0 {
            diff += step;
        }
        if n & 2 != 0 {
            diff += step >> 1;
        }
        if n & 1 != 0 {
            diff += step >> 2;
        }
        self.predictor = if n & 8 != 0 {
            self.predictor - diff
        } else {
            self.predictor + diff
        }
        .clamp(-32768, 32767);
        self.index = (self.index + IMA_INDEX[usize::from(nibble & 7)]).clamp(0, 88);
        i16::try_from(self.predictor).expect("clamped")
    }
}

/// Decode Microsoft-style IMA ADPCM blocks: per channel a 4-byte header
/// (`i16` first sample, `u8` step index, `u8` 0), then 4-byte groups of
/// nibbles per channel in turn, low nibble first. `frames` limits the output
/// when the last block is partial.
pub fn ms_ima(
    data: &[u8],
    channels: u16,
    block_align: usize,
    frames: Option<usize>,
) -> Result<Vec<i16>, Error> {
    let ch = usize::from(channels);
    if ch == 0 || block_align < 4 * ch {
        return Err(Error::NotRecognised("IMA ADPCM block layout"));
    }
    let mut out = Vec::new();
    for (b, block) in data.chunks(block_align).enumerate() {
        if block.len() < 4 * ch {
            break;
        }
        let mut state = vec![Ima::default(); ch];
        for (c, s) in state.iter_mut().enumerate() {
            let h = &block[4 * c..4 * c + 4];
            s.predictor = i32::from(i16::from_le_bytes([h[0], h[1]]));
            s.index = i32::from(h[2]);
            if s.index > 88 || h[3] != 0 {
                return Err(Error::BadBlock(b));
            }
        }
        let first = out.len();
        for s in &state {
            out.push(i16::try_from(s.predictor).expect("from i16"));
        }
        // Each group is 4 bytes per channel = 8 samples per channel.
        let body = &block[4 * ch..];
        let groups = body.len() / (4 * ch);
        let mut frame_block = vec![[0i16; 8]; ch];
        for g in 0..groups {
            for (c, s) in state.iter_mut().enumerate() {
                let bytes = &body[(g * ch + c) * 4..(g * ch + c) * 4 + 4];
                for (k, byte) in bytes.iter().enumerate() {
                    frame_block[c][2 * k] = s.decode(byte & 0x0f);
                    frame_block[c][2 * k + 1] = s.decode(byte >> 4);
                }
            }
            for k in 0..8 {
                for samples in &frame_block {
                    out.push(samples[k]);
                }
            }
        }
        debug_assert!(out.len() >= first);
    }
    if let Some(f) = frames {
        out.truncate(f * ch);
    }
    Ok(out)
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("2 bytes"),
    ))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("4 bytes"),
    ))
}

fn us(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

fn pcm16(data: &[u8]) -> Vec<i16> {
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect()
}

/// A RIFF WAVE file with PCM (format 1, 8- or 16-bit) or IMA ADPCM (0x11).
pub fn wav(data: &[u8]) -> Result<Sound, Error> {
    if data.get(..4) != Some(b"RIFF") || data.get(8..12) != Some(b"WAVE") {
        return Err(Error::NotRecognised("WAV"));
    }
    let (mut fmt, mut body, mut fact) = (None, None, None);
    let mut at = 12;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let len = us(u32_at(data, at + 4)?);
        let chunk = data.get(at + 8..at + 8 + len).ok_or(Error::Truncated)?;
        match id {
            b"fmt " => fmt = Some(chunk),
            b"data" => body = Some(chunk),
            b"fact" => fact = Some(us(u32_at(chunk, 0)?)),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    let fmt = fmt.ok_or(Error::NotRecognised("WAV (no fmt chunk)"))?;
    let body = body.ok_or(Error::NotRecognised("WAV (no data chunk)"))?;
    let (format, channels, rate) = (u16_at(fmt, 0)?, u16_at(fmt, 2)?, u32_at(fmt, 4)?);
    let (block_align, bits) = (usize::from(u16_at(fmt, 12)?), u16_at(fmt, 14)?);
    let samples = match (format, bits) {
        (1, 16) => pcm16(body),
        (1, 8) => body.iter().map(|&b| (i16::from(b) - 128) << 8).collect(),
        (0x11, 4) => ms_ima(body, channels, block_align, fact)?,
        _ => {
            return Err(Error::Unsupported(format!(
                "WAV format {format:#x}, {bits} bits"
            )));
        }
    };
    Ok(Sound {
        rate,
        channels,
        samples,
    })
}

/// A Westwood `.aud` file. Type 99 is IMA ADPCM with one state over the
/// whole file; type 1 (Westwood's own ADPCM) is not supported, and no
/// shipped file uses it.
pub fn aud(data: &[u8]) -> Result<Sound, Error> {
    let rate = u32::from(u16_at(data, 0)?);
    let size = us(u32_at(data, 2)?);
    let out_size = us(u32_at(data, 6)?);
    let (flags, kind) = (
        *data.get(10).ok_or(Error::Truncated)?,
        *data.get(11).ok_or(Error::Truncated)?,
    );
    if size + 12 > data.len() || !(1000..=96_000).contains(&rate) {
        return Err(Error::NotRecognised("AUD"));
    }
    let channels = if flags & 1 != 0 { 2 } else { 1 };
    if kind != 99 {
        return Err(Error::Unsupported(format!("AUD compression {kind}")));
    }
    if channels != 1 {
        return Err(Error::Unsupported("stereo AUD".into()));
    }
    let mut state = Ima::default();
    let mut samples = Vec::with_capacity(out_size / 2);
    let mut at = 12;
    while at < 12 + size {
        let packed = usize::from(u16_at(data, at)?);
        let _unpacked = u16_at(data, at + 2)?;
        if u32_at(data, at + 4)? != 0x0000_DEAF {
            return Err(Error::NotRecognised("AUD chunk (no DEAF marker)"));
        }
        let body = data.get(at + 8..at + 8 + packed).ok_or(Error::Truncated)?;
        for b in body {
            samples.push(state.decode(b & 0x0f));
            samples.push(state.decode(b >> 4));
        }
        at += 8 + packed;
    }
    samples.truncate(out_size / 2);
    Ok(Sound {
        rate,
        channels,
        samples,
    })
}

/// One sound in `audio.idx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagEntry {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub rate: u32,
    /// Bit 0 stereo, bit 1 PCM, bit 2 16-bit, bit 3 IMA ADPCM.
    pub flags: u32,
    /// The IMA block size (0 for PCM).
    pub chunk_size: u32,
}

/// Parse `audio.idx`: `"GABA"`, version 2, a count, then 36-byte entries.
pub fn bag_index(data: &[u8]) -> Result<Vec<BagEntry>, Error> {
    if data.get(..4) != Some(b"GABA") || u32_at(data, 4)? != 2 {
        return Err(Error::NotRecognised("audio.idx"));
    }
    let count = us(u32_at(data, 8)?);
    if data.len() != 12 + count * 36 {
        return Err(Error::NotRecognised(
            "audio.idx (size doesn't match its count)",
        ));
    }
    (0..count)
        .map(|i| {
            let e = &data[12 + i * 36..12 + i * 36 + 36];
            let end = e[..16].iter().position(|&c| c == 0).unwrap_or(16);
            Ok(BagEntry {
                name: String::from_utf8_lossy(&e[..end]).into_owned(),
                offset: u32_at(e, 16)?,
                size: u32_at(e, 20)?,
                rate: u32_at(e, 24)?,
                flags: u32_at(e, 28)?,
                chunk_size: u32_at(e, 32)?,
            })
        })
        .collect()
}

/// Decode one sound from `audio.bag`.
pub fn bag_sound(bag: &[u8], e: &BagEntry) -> Result<Sound, Error> {
    let data = bag
        .get(us(e.offset)..us(e.offset) + us(e.size))
        .ok_or(Error::Truncated)?;
    let channels = if e.flags & 1 != 0 { 2 } else { 1 };
    let samples = if e.flags & 8 != 0 {
        ms_ima(data, channels, us(e.chunk_size), None)?
    } else if e.flags & 2 != 0 && e.flags & 4 != 0 {
        pcm16(data)
    } else {
        return Err(Error::Unsupported(format!("bag flags {:#x}", e.flags)));
    };
    Ok(Sound {
        rate: e.rate,
        channels,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_table() {
        assert_eq!(IMA_STEPS.len(), 89);
        assert!(IMA_STEPS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn ima_moves_toward_the_signal() {
        let mut s = Ima::default();
        let up: Vec<i16> = (0..8).map(|_| s.decode(7)).collect();
        assert!(up.windows(2).all(|w| w[1] > w[0]));
        let down = s.decode(15);
        assert!(down < up[7]);
    }

    #[test]
    fn ms_ima_blocks() {
        // One mono block: header (sample 100, index 0), then two groups of
        // 4 bytes: 16 more samples.
        let block = [100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let out = ms_ima(&block, 1, 12, None).unwrap();
        assert_eq!(out.len(), 17);
        assert_eq!(out[0], 100);
        assert_eq!(ms_ima(&block, 1, 12, Some(5)).unwrap().len(), 5);
        let mut bad = block;
        bad[2] = 89;
        assert_eq!(ms_ima(&bad, 1, 12, None).unwrap_err(), Error::BadBlock(0));
        // Stereo: two headers, then one group per channel.
        let st = [1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let out = ms_ima(&st, 2, 16, None).unwrap();
        assert_eq!(out.len(), 18);
        assert_eq!(&out[..2], &[1, 2]);
    }

    fn wav_bytes(format: u16, channels: u16, bits: u16, block: u16, body: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&format.to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&22050u32.to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes());
        fmt.extend_from_slice(&block.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        let mut out = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        out.extend_from_slice(&u32::try_from(fmt.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn wavs() {
        let s = wav(&wav_bytes(1, 1, 16, 2, &[1, 0, 0xff, 0xff])).unwrap();
        assert_eq!(s.samples, [1, -1]);
        let s = wav(&wav_bytes(
            0x11,
            1,
            4,
            12,
            &[100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ))
        .unwrap();
        assert_eq!(s.frames(), 17);
        assert!(matches!(
            wav(&wav_bytes(2, 1, 4, 12, &[])),
            Err(Error::Unsupported(_))
        ));
        assert!(wav(b"nope").is_err());
    }

    #[test]
    fn aud_and_bag() {
        let mut a = Vec::new();
        a.extend_from_slice(&22050u16.to_le_bytes());
        a.extend_from_slice(&10u32.to_le_bytes());
        a.extend_from_slice(&8u32.to_le_bytes());
        a.extend_from_slice(&[0, 99]);
        a.extend_from_slice(&[2, 0, 8, 0, 0xaf, 0xde, 0, 0, 0x77, 0x77]);
        let s = aud(&a).unwrap();
        assert_eq!(s.samples.len(), 4);
        let mut idx = b"GABA".to_vec();
        idx.extend_from_slice(&2u32.to_le_bytes());
        idx.extend_from_slice(&1u32.to_le_bytes());
        let mut name = b"ceva001".to_vec();
        name.resize(16, 0);
        idx.extend_from_slice(&name);
        for v in [0u32, 4, 22050, 6, 0] {
            idx.extend_from_slice(&v.to_le_bytes());
        }
        let e = &bag_index(&idx).unwrap()[0];
        assert_eq!(e.name, "ceva001");
        assert_eq!(bag_sound(&[1, 0, 2, 0], e).unwrap().samples, [1, 2]);
    }
}
