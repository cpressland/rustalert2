//! MIX archives: the containers almost all of the game's data lives in.
//!
//! See `docs/spec/mix.md` for the format. This module is pure: it parses an
//! archive's index from bytes and says where each file lies. Reading those
//! bytes from disk, and nesting archives, is `ra2-vfs`'s job.

use std::fmt;

use blowfish::Blowfish;
use blowfish::cipher::{BlockCipherDecrypt, KeyInit};
use num_bigint::BigUint;

/// Flag: a SHA-1 of the body follows it at the end of the file.
pub const FLAG_CHECKSUM: u32 = 0x0001_0000;
/// Flag: the index is encrypted with Blowfish, its key wrapped with RSA.
pub const FLAG_ENCRYPTED: u32 = 0x0002_0000;

/// The modulus of Westwood's public RSA key, big-endian. The game holds it as
/// base64 (`1=AihRvNo…`, `[recomp 0x007E1A8C]`), which decodes to a DER
/// `INTEGER` (tag `0x02`, length `0x28`) followed by these 40 bytes.
const PUBLIC_MODULUS: [u8; 40] = [
    0x51, 0xbc, 0xda, 0x08, 0x6d, 0x39, 0xfc, 0xe4, 0x56, 0x51, 0x60, 0xd6, 0x51, 0x71, 0x3f, 0xa2,
    0xe8, 0xaa, 0x54, 0xfa, 0x66, 0x82, 0xb0, 0x4a, 0xab, 0xdd, 0x0e, 0x6a, 0xf8, 0xb0, 0xc1, 0xe6,
    0xd1, 0xfb, 0x4f, 0x3d, 0xaa, 0x43, 0x7f, 0x15,
];
const PUBLIC_EXPONENT: u32 = 0x1_0001;
/// The encrypted key source: two RSA blocks of 40 bytes.
const KEY_SOURCE_LEN: usize = 80;
/// Each RSA block decrypts to this many bytes (the modulus is 319 bits).
const RSA_PLAIN_LEN: usize = 39;
/// Of the 78 decrypted bytes, the Blowfish key is the first 56.
const BLOWFISH_KEY_LEN: usize = 56;
const ENTRY_LEN: usize = 12;
/// More files than any real archive has; guards against garbage headers.
const MAX_ENTRIES: usize = 0x0010_0000;

/// The ID the game files an entry under, for a file name: Tiberian Sun and
/// Red Alert 2's scheme, a CRC-32 of the upper-cased, padded name.
///
/// Names whose length is not a multiple of 4 are padded: one byte holding
/// `len % 4`, then copies of the first byte of the last partial group, to
/// a multiple of 4.
pub fn mix_id(name: &str) -> u32 {
    let mut bytes: Vec<u8> = name.bytes().map(|b| b.to_ascii_uppercase()).collect();
    let len = bytes.len();
    let rem = len % 4;
    if rem != 0 {
        let fill = bytes[len - rem];
        // rem is 1..=3, so the cast is lossless.
        #[allow(clippy::cast_possible_truncation)]
        bytes.push(rem as u8);
        while !bytes.len().is_multiple_of(4) {
            bytes.push(fill);
        }
    }
    crc32fast::hash(&bytes)
}

/// The ID scheme of the older *Tiberian Dawn* / *Red Alert* MIXes: a
/// rotate-and-add over little-endian 32-bit groups of the upper-cased name.
pub fn td_id(name: &str) -> u32 {
    let bytes: Vec<u8> = name.bytes().map(|b| b.to_ascii_uppercase()).collect();
    let mut id: u32 = 0;
    for chunk in bytes.chunks(4) {
        let mut group = [0u8; 4];
        group[..chunk.len()].copy_from_slice(chunk);
        id = id.rotate_left(1).wrapping_add(u32::from_le_bytes(group));
    }
    id
}

/// One file in an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    /// From the start of the body.
    pub offset: u32,
    pub size: u32,
}

/// The two header layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// *Tiberian Dawn* / *Red Alert*: no flags word.
    Classic,
    /// *Tiberian Sun* onwards: a flags word, maybe encrypted, maybe checksummed.
    Extended { flags: u32 },
}

/// Why an index could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Parsing needs the first `n` bytes of the file; call again with them.
    NeedMore(usize),
    /// The header makes no sense: the file is not a MIX, or is damaged.
    Invalid(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NeedMore(n) => write!(f, "need the first {n} bytes of the archive"),
            Error::Invalid(why) => write!(f, "not a valid MIX archive: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// A parsed archive index.
#[derive(Debug, Clone)]
pub struct Index {
    pub layout: Layout,
    /// Entries in the order the index lists them.
    pub entries: Vec<Entry>,
    /// Where the body starts in the file. Entry offsets are relative to it.
    pub body_offset: u64,
    /// The size of the body, as the header states it.
    pub body_size: u32,
    /// Entry positions sorted by ID, for lookup.
    by_id: Vec<(u32, usize)>,
}

impl Index {
    /// Parse the index from the start of a file.
    ///
    /// `prefix` may be shorter than the whole file. If it is too short, the
    /// result is [`Error::NeedMore`] with the length needed. Read at least
    /// [`Index::MIN_PREFIX`] bytes the first time to avoid a second round.
    pub fn parse(prefix: &[u8]) -> Result<Index, Error> {
        let first = u16_at(prefix, 0).ok_or(Error::NeedMore(6))?;
        if first != 0 {
            return parse_plain(prefix, 0, Layout::Classic);
        }
        let flags = u32_at(prefix, 0).ok_or(Error::NeedMore(4))?;
        if flags & !(FLAG_CHECKSUM | FLAG_ENCRYPTED) != 0 {
            return Err(Error::Invalid("unknown flags"));
        }
        let layout = Layout::Extended { flags };
        if flags & FLAG_ENCRYPTED == 0 {
            parse_plain(prefix, 4, layout)
        } else {
            parse_encrypted(prefix, layout)
        }
    }

    /// Enough bytes for the fixed part of any header.
    pub const MIN_PREFIX: usize = 4 + KEY_SOURCE_LEN + 8;

    /// Find an entry by ID.
    pub fn get(&self, id: u32) -> Option<Entry> {
        let pos = self.by_id.binary_search_by_key(&id, |&(i, _)| i).ok()?;
        Some(self.entries[self.by_id[pos].1])
    }

    /// Find an entry by file name.
    pub fn find(&self, name: &str) -> Option<Entry> {
        self.get(mix_id(name)).or_else(|| match self.layout {
            Layout::Classic => self.get(td_id(name)),
            Layout::Extended { .. } => None,
        })
    }

    /// Where an entry's bytes are in the file: `(start, len)`.
    pub fn span(&self, entry: Entry) -> (u64, u64) {
        (
            self.body_offset + u64::from(entry.offset),
            u64::from(entry.size),
        )
    }

    pub fn is_encrypted(&self) -> bool {
        matches!(self.layout, Layout::Extended { flags } if flags & FLAG_ENCRYPTED != 0)
    }

    pub fn has_checksum(&self) -> bool {
        matches!(self.layout, Layout::Extended { flags } if flags & FLAG_CHECKSUM != 0)
    }

    /// Check the index against the file it came from: every entry inside the
    /// body, and the body inside the file. Returns the first problem.
    pub fn check_bounds(&self, file_len: u64) -> Result<(), &'static str> {
        let body_end = self.body_offset + u64::from(self.body_size);
        let tail = if self.has_checksum() { 20 } else { 0 };
        if body_end + tail > file_len {
            return Err("body runs past the end of the file");
        }
        if self
            .entries
            .iter()
            .any(|e| u64::from(e.offset) + u64::from(e.size) > u64::from(self.body_size))
        {
            return Err("an entry runs past the end of the body");
        }
        Ok(())
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The six-byte count-and-size header, then the entries, all in the clear.
fn parse_plain(prefix: &[u8], at: usize, layout: Layout) -> Result<Index, Error> {
    let count = usize::from(u16_at(prefix, at).ok_or(Error::NeedMore(at + 6))?);
    let body_size = u32_at(prefix, at + 2).ok_or(Error::NeedMore(at + 6))?;
    let index_end = at + 6 + count * ENTRY_LEN;
    let table = prefix
        .get(at + 6..index_end)
        .ok_or(Error::NeedMore(index_end))?;
    build(layout, table, count, body_size, index_end as u64)
}

/// The key source, then a Blowfish-encrypted index padded to 8 bytes.
fn parse_encrypted(prefix: &[u8], layout: Layout) -> Result<Index, Error> {
    let source = prefix
        .get(4..4 + KEY_SOURCE_LEN)
        .ok_or(Error::NeedMore(Index::MIN_PREFIX))?;
    let cipher =
        <Blowfish>::new_from_slice(&blowfish_key(source)).map_err(|_| Error::Invalid("bad key"))?;
    let start = 4 + KEY_SOURCE_LEN;

    let decrypt = |range: std::ops::Range<usize>| -> Result<Vec<u8>, Error> {
        let mut out = prefix
            .get(range.clone())
            .ok_or(Error::NeedMore(range.end))?
            .to_vec();
        for block in out.as_chunks_mut::<8>().0 {
            cipher.decrypt_block(block.into());
        }
        Ok(out)
    };

    // The first block holds the count and body size.
    let head = decrypt(start..start + 8)?;
    let count = usize::from(u16_at(&head, 0).expect("8 bytes"));
    let body_size = u32_at(&head, 2).expect("8 bytes");
    if count > MAX_ENTRIES {
        return Err(Error::Invalid("too many entries"));
    }
    let padded = (6 + count * ENTRY_LEN).div_ceil(8) * 8;
    let plain = decrypt(start..start + padded)?;
    let table = &plain[6..6 + count * ENTRY_LEN];
    build(layout, table, count, body_size, (start + padded) as u64)
}

fn build(
    layout: Layout,
    table: &[u8],
    count: usize,
    body_size: u32,
    body_offset: u64,
) -> Result<Index, Error> {
    let entries: Vec<Entry> = table
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .take(count)
        .map(|e| Entry {
            id: u32_at(e, 0).expect("12 bytes"),
            offset: u32_at(e, 4).expect("12 bytes"),
            size: u32_at(e, 8).expect("12 bytes"),
        })
        .collect();
    let mut by_id: Vec<(u32, usize)> = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    by_id.sort_unstable();
    if by_id.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(Error::Invalid("duplicate entry IDs"));
    }
    Ok(Index {
        layout,
        entries,
        body_offset,
        body_size,
        by_id,
    })
}

/// Unwrap the Blowfish key from the 80-byte key source: each 40-byte block,
/// read as a little-endian number, raised to the public exponent modulo the
/// public modulus, gives 39 little-endian bytes. The key is the first 56 of
/// the 78.
fn blowfish_key(source: &[u8]) -> [u8; BLOWFISH_KEY_LEN] {
    let n = BigUint::from_bytes_be(&PUBLIC_MODULUS);
    let e = BigUint::from(PUBLIC_EXPONENT);
    let mut plain = Vec::with_capacity(2 * RSA_PLAIN_LEN);
    for block in source.as_chunks::<{ KEY_SOURCE_LEN / 2 }>().0 {
        let m = BigUint::from_bytes_le(block).modpow(&e, &n);
        let mut bytes = m.to_bytes_le();
        bytes.resize(RSA_PLAIN_LEN, 0);
        plain.extend_from_slice(&bytes[..RSA_PLAIN_LEN]);
    }
    plain[..BLOWFISH_KEY_LEN].try_into().expect("78 bytes")
}

/// Names in an XCC "local mix database", which archives made with XCC Mixer
/// carry as `local mix database.dat`. Westwood's own archives don't have one.
pub fn local_database_names(data: &[u8]) -> Option<Vec<String>> {
    const MAGIC: &[u8] = b"XCC by Olaf van der Spek\x1a\x04\x17\x27\x10\x19\x80\x00";
    let rest = data.strip_prefix(MAGIC)?;
    // size, type, version, count; then NUL-terminated names.
    let count = usize::try_from(u32_at(rest, 12)?).ok()?;
    let names = rest.get(16..)?;
    Some(
        names
            .split(|&b| b == 0)
            .take(count)
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect(),
    )
}

/// The name of the XCC database entry.
pub const LOCAL_DATABASE: &str = "local mix database.dat";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        // Well-known: XCC's database entry.
        assert_eq!(mix_id(LOCAL_DATABASE), 0x366E_051F);
        // Case-insensitive, and padding for each remainder.
        assert_eq!(mix_id("rulesmd.ini"), mix_id("RULESMD.INI"));
        for name in ["a", "ab", "abc", "abcd", "abcde"] {
            let _ = mix_id(name);
        }
        assert_ne!(mix_id("ab"), mix_id("abc"));
        assert_eq!(mix_id(""), crc32fast::hash(b""));
    }

    #[test]
    fn modulus_matches_the_games_key() {
        // The base64 key text, decoded by hand: 0x02 0x28 then the modulus.
        let text = "AihRvNoIbTn85FZRYNZRcT+i6KpU+maCsEqr3Q5q+LDB5tH7Tz2qQ38V";
        let decoded = base64_decode(text);
        assert_eq!(&decoded[..2], &[0x02, 0x28]);
        assert_eq!(&decoded[2..], &PUBLIC_MODULUS);
        assert_eq!(BigUint::from_bytes_be(&PUBLIC_MODULUS).bits(), 319);
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut n = 0;
        let mut out = Vec::new();
        for c in s.bytes().filter(|&c| c != b'=') {
            let v = u32::try_from(alphabet.iter().position(|&a| a == c).unwrap()).unwrap();
            bits = (bits << 6) | v;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push(u8::try_from((bits >> n) & 0xff).unwrap());
            }
        }
        out
    }

    /// Build a plain extended archive from (name, data) pairs.
    fn build_plain(files: &[(&str, &[u8])], flags: u32) -> Vec<u8> {
        let mut body = Vec::new();
        let mut entries = Vec::new();
        for (name, data) in files {
            entries.push((
                mix_id(name),
                u32::try_from(body.len()).unwrap(),
                u32::try_from(data.len()).unwrap(),
            ));
            body.extend_from_slice(data);
        }
        entries.sort_by_key(|e| e.0);
        let mut out = flags.to_le_bytes().to_vec();
        out.extend_from_slice(&u16::try_from(files.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        for (id, off, size) in entries {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&off.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn plain_archive_round_trip() {
        let file = build_plain(&[("rulesmd.ini", b"[General]\n"), ("a.shp", b"xyz")], 0);
        // Too short: asks for more, then succeeds.
        assert_eq!(Index::parse(&file[..3]).unwrap_err(), Error::NeedMore(4));
        let need = match Index::parse(&file[..10]) {
            Err(Error::NeedMore(n)) => n,
            other => panic!("{other:?}"),
        };
        let idx = Index::parse(&file[..need]).unwrap();
        assert_eq!(idx.entries.len(), 2);
        idx.check_bounds(file.len() as u64).unwrap();
        let (start, len) = idx.span(idx.find("RULESMD.INI").unwrap());
        let start = usize::try_from(start).unwrap();
        assert_eq!(
            &file[start..start + usize::try_from(len).unwrap()],
            b"[General]\n"
        );
        assert!(idx.find("missing.ini").is_none());
    }

    #[test]
    fn classic_layout() {
        // Count 1, body 3, one entry, body "abc".
        let mut file = vec![1, 0, 3, 0, 0, 0];
        file.extend_from_slice(&td_id("x.txt").to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&3u32.to_le_bytes());
        file.extend_from_slice(b"abc");
        let idx = Index::parse(&file).unwrap();
        assert_eq!(idx.layout, Layout::Classic);
        assert_eq!(idx.find("X.TXT").unwrap().size, 3);
    }

    #[test]
    fn garbage_never_panics() {
        // A small LCG for reproducible noise, and every truncation of a real
        // header with bytes flipped.
        let mut seed = 0x1234_5678_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        };
        for len in 0..400 {
            let mut buf: Vec<u8> = (0..len).map(|_| next()).collect();
            let _ = Index::parse(&buf);
            if len >= 4 {
                buf[..4].copy_from_slice(&FLAG_ENCRYPTED.to_le_bytes());
                let _ = Index::parse(&buf);
                buf[..2].copy_from_slice(&[0, 0]);
                buf[2..4].copy_from_slice(&0u16.to_le_bytes());
                let _ = Index::parse(&buf);
            }
        }
        let file = build_plain(&[("a", b"1"), ("b", b"22")], FLAG_CHECKSUM);
        for i in 0..file.len() {
            let mut f = file.clone();
            f[i] ^= 0xff;
            if let Ok(idx) = Index::parse(&f) {
                let _ = idx.check_bounds(f.len() as u64);
            }
        }
    }

    #[test]
    fn local_database() {
        let mut data = b"XCC by Olaf van der Spek\x1a\x04\x17\x27\x10\x19\x80\x00".to_vec();
        for v in [0u32, 0, 0, 2] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(b"a.shp\0b.ini\0");
        assert_eq!(local_database_names(&data).unwrap(), vec!["a.shp", "b.ini"]);
        assert!(local_database_names(b"nope").is_none());
    }
}
