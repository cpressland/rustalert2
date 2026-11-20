//! CSF string tables: the game's translated text (`ra2md.csf`).
//! See `docs/spec/csf.md`.

use std::collections::BTreeMap;
use std::fmt;

const MAGIC_FILE: u32 = u32::from_le_bytes(*b" FSC");
const MAGIC_LABEL: u32 = u32::from_le_bytes(*b" LBL");
const MAGIC_STRING: u32 = u32::from_le_bytes(*b" RTS");
const MAGIC_WIDE_EXTRA: u32 = u32::from_le_bytes(*b"WRTS");

/// The language a table declares, from the game's own table
/// (`[recomp 0x00845678]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Us,
    Uk,
    German,
    French,
    Spanish,
    Italian,
    Japanese,
    Jabberwockie,
    Korean,
    Chinese,
    Unknown(u32),
}

impl Language {
    pub fn from_id(id: u32) -> Language {
        use Language::{
            Chinese, French, German, Italian, Jabberwockie, Japanese, Korean, Spanish, Uk, Unknown,
            Us,
        };
        match id {
            0 => Us,
            1 => Uk,
            2 => German,
            3 => French,
            4 => Spanish,
            5 => Italian,
            6 => Japanese,
            7 => Jabberwockie,
            8 => Korean,
            9 => Chinese,
            n => Unknown(n),
        }
    }
}

/// One label's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The label as written in the file.
    pub label: String,
    /// The text, after the game's whitespace clean-up.
    pub text: String,
    /// The ASCII "extra" value of a `WRTS` string (a sound name, for EVA-style
    /// lines), if it has one.
    pub extra: Option<String>,
    /// Further strings under the same label. The game stores them but its
    /// lookup returns only the first.
    pub alternates: Vec<String>,
}

/// Why a table could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadMagic,
    Truncated,
    /// A label or string block didn't start with its magic.
    BadBlock {
        offset: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic => f.write_str("not a CSF file"),
            Error::Truncated => f.write_str("CSF file is truncated"),
            Error::BadBlock { offset } => write!(f, "CSF block at {offset} has no valid magic"),
        }
    }
}

impl std::error::Error for Error {}

/// A string table.
#[derive(Debug, Clone)]
pub struct StringTable {
    pub version: u32,
    pub language: Language,
    entries: Vec<Entry>,
    /// Lower-cased label to entry. The game compares with `_stricmp`.
    index: BTreeMap<String, usize>,
    /// Labels that appear more than once; the first is kept.
    pub duplicates: Vec<String>,
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).ok_or(Error::Truncated)?;
        let out = self.data.get(self.at..end).ok_or(Error::Truncated)?;
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn len(&mut self, unit: usize) -> Result<usize, Error> {
        let n = usize::try_from(self.u32()?).map_err(|_| Error::Truncated)?;
        // Cheap sanity check before allocating: it must fit in what is left.
        if n.saturating_mul(unit) > self.data.len() - self.at {
            return Err(Error::Truncated);
        }
        Ok(n)
    }
}

impl StringTable {
    pub fn parse(data: &[u8]) -> Result<StringTable, Error> {
        let mut r = Reader { data, at: 0 };
        if r.u32().map_err(|_| Error::BadMagic)? != MAGIC_FILE {
            return Err(Error::BadMagic);
        }
        let version = r.u32()?;
        let label_count = r.u32()?;
        let _string_count = r.u32()?;
        let _reserved = r.u32()?;
        let language_id = r.u32()?;
        // Versions before 2 carry no language. [recomp 0x007347B1]
        let language = Language::from_id(if version < 2 { 0 } else { language_id });

        let mut entries = Vec::new();
        let mut index = BTreeMap::new();
        let mut duplicates = Vec::new();
        for _ in 0..label_count {
            let offset = r.at;
            if r.u32()? != MAGIC_LABEL {
                return Err(Error::BadBlock { offset });
            }
            let pairs = r.u32()?;
            let n = r.len(1)?;
            let label = String::from_utf8_lossy(r.bytes(n)?).into_owned();
            let mut strings = Vec::new();
            let mut extra = None;
            for i in 0..pairs {
                let offset = r.at;
                let magic = r.u32()?;
                if magic != MAGIC_STRING && magic != MAGIC_WIDE_EXTRA {
                    return Err(Error::BadBlock { offset });
                }
                let n = r.len(2)?;
                strings.push(decode(r.bytes(n * 2)?));
                if magic == MAGIC_WIDE_EXTRA {
                    let n = r.len(1)?;
                    let value = String::from_utf8_lossy(r.bytes(n)?).into_owned();
                    if i == 0 && !value.is_empty() {
                        extra = Some(value);
                    }
                }
            }
            let mut strings = strings.into_iter();
            let entry = Entry {
                label: label.clone(),
                text: strings.next().unwrap_or_default(),
                extra,
                alternates: strings.collect(),
            };
            match index.entry(label.to_ascii_lowercase()) {
                std::collections::btree_map::Entry::Occupied(_) => duplicates.push(label),
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert(entries.len());
                }
            }
            entries.push(entry);
        }
        Ok(StringTable {
            version,
            language,
            entries,
            index,
            duplicates,
        })
    }

    /// Entries in file order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Look a label up, ignoring ASCII case.
    pub fn entry(&self, label: &str) -> Option<&Entry> {
        self.index
            .get(&label.to_ascii_lowercase())
            .map(|&i| &self.entries[i])
    }

    /// A label's text, or what the game shows when it has none:
    /// `MISSING:'label'` (`[recomp 0x00845820]`).
    pub fn text(&self, label: &str) -> String {
        self.entry(label)
            .map_or_else(|| format!("MISSING:'{label}'"), |e| e.text.clone())
    }
}

/// Undo the encoding (each UTF-16 unit is bitwise NOT-ed), stop at a NUL,
/// then apply the game's whitespace clean-up.
fn decode(raw: &[u8]) -> String {
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| !u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    tidy(&String::from_utf16_lossy(&units))
}

/// The clean-up the loader applies to every string (`[recomp 0x00734B4B]`):
/// spaces at the start, or after a newline or tab, are dropped; runs of
/// spaces become one; a space before a newline, a tab or the end is dropped.
/// Newlines and tabs are kept.
pub fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at_line_start = true;
    let mut prev: Option<char> = None;
    for c in s.chars() {
        match c {
            ' ' => {
                if prev == Some(' ') || at_line_start {
                    continue;
                }
                out.push(c);
            }
            '\n' | '\t' => {
                if prev == Some(' ') {
                    out.pop();
                }
                out.push(c);
                at_line_start = true;
            }
            _ => {
                out.push(c);
                at_line_start = false;
            }
        }
        prev = Some(c);
    }
    if prev == Some(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Our own encoder, for fixtures.
    fn encode(version: u32, lang: u32, labels: &[(&str, &[&str], Option<&str>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let count = u32::try_from(labels.len()).unwrap();
        for v in [MAGIC_FILE, version, count, count, 0, lang] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (label, strings, extra) in labels {
            out.extend_from_slice(&MAGIC_LABEL.to_le_bytes());
            out.extend_from_slice(&u32::try_from(strings.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&u32::try_from(label.len()).unwrap().to_le_bytes());
            out.extend_from_slice(label.as_bytes());
            for s in *strings {
                let units: Vec<u16> = s.encode_utf16().collect();
                let magic = if extra.is_some() {
                    MAGIC_WIDE_EXTRA
                } else {
                    MAGIC_STRING
                };
                out.extend_from_slice(&magic.to_le_bytes());
                out.extend_from_slice(&u32::try_from(units.len()).unwrap().to_le_bytes());
                for u in units {
                    out.extend_from_slice(&(!u).to_le_bytes());
                }
                if let Some(x) = extra {
                    out.extend_from_slice(&u32::try_from(x.len()).unwrap().to_le_bytes());
                    out.extend_from_slice(x.as_bytes());
                }
            }
        }
        out
    }

    #[test]
    fn round_trip() {
        let data = encode(
            3,
            0,
            &[
                ("Name:E1", &["GI"], None),
                ("TXT_HELLO", &["Hello,   commander. "], Some("ceva001")),
                ("Two", &["first", "second"], None),
                ("Name:e1", &["dupe"], None),
            ],
        );
        let t = StringTable::parse(&data).unwrap();
        assert_eq!(t.version, 3);
        assert_eq!(t.language, Language::Us);
        assert_eq!(t.text("NAME:E1"), "GI", "labels ignore case");
        let hello = t.entry("txt_hello").unwrap();
        assert_eq!(hello.text, "Hello, commander.");
        assert_eq!(hello.extra.as_deref(), Some("ceva001"));
        assert_eq!(t.entry("Two").unwrap().alternates, ["second"]);
        assert_eq!(t.duplicates, ["Name:e1"]);
        assert_eq!(t.text("Nope"), "MISSING:'Nope'");
    }

    #[test]
    fn old_versions_have_no_language() {
        let t = StringTable::parse(&encode(1, 2, &[])).unwrap();
        assert_eq!(t.language, Language::Us);
        assert_eq!(
            StringTable::parse(&encode(3, 2, &[])).unwrap().language,
            Language::German
        );
    }

    #[test]
    fn whitespace_clean_up() {
        assert_eq!(tidy("  a  b  "), "a b");
        assert_eq!(tidy("line one \n  line two"), "line one\nline two");
        assert_eq!(tidy("col1 \t  col2"), "col1\tcol2");
        assert_eq!(tidy("a\n\nb"), "a\n\nb");
        assert_eq!(tidy(""), "");
    }

    #[test]
    fn bad_input() {
        assert_eq!(StringTable::parse(b"nope").unwrap_err(), Error::BadMagic);
        let mut data = encode(3, 0, &[("A", &["text"], None)]);
        data.truncate(data.len() - 3);
        assert_eq!(StringTable::parse(&data).unwrap_err(), Error::Truncated);
        let mut data = encode(3, 0, &[("A", &["x"], None)]);
        data[24] = b'X';
        assert!(matches!(
            StringTable::parse(&data).unwrap_err(),
            Error::BadBlock { offset: 24 }
        ));
        // A length that claims more than the file holds.
        let mut data = encode(3, 0, &[("A", &["x"], None)]);
        data[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        data[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(StringTable::parse(&data).unwrap_err(), Error::Truncated);
    }
}
