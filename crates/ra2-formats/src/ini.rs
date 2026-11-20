//! INI files, read the way the game reads them. See `docs/spec/ini.md`.
//!
//! The game's reader (`INIClass`) differs from most INI libraries in ways
//! rules files depend on:
//!
//! - lines are cut at 511 bytes, and the rest of a long line is dropped;
//! - `;` starts a comment anywhere on a line;
//! - every byte up to `0x20` counts as whitespace when trimming;
//! - a key with an empty value isn't stored at all;
//! - section and key names are case-sensitive;
//! - loading a second file into the same object merges into it.
//!
//! Text is kept as Latin-1: one byte is one `char`, so nothing is lost.

use std::collections::BTreeMap;

/// The longest line the game reads; the line buffer is 512 bytes with its NUL.
pub const MAX_LINE: usize = 511;

/// One section: its entries in file order, plus an index by key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    entries: Vec<(String, String)>,
    index: BTreeMap<String, usize>,
}

impl Section {
    fn new(name: String) -> Section {
        Section {
            name,
            ..Section::default()
        }
    }

    /// Entries in file order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A key's value. With duplicate keys, the first one wins.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.index.get(key).map(|&i| self.entries[i].1.as_str())
    }

    /// Add an entry as a fresh load does: kept even if the key repeats.
    fn push(&mut self, key: String, value: String) {
        self.index.entry(key.clone()).or_insert(self.entries.len());
        self.entries.push((key, value));
    }

    /// Set an entry as a merge does: an existing key's value is replaced.
    pub fn put(&mut self, key: &str, value: &str) {
        match self.index.get(key) {
            Some(&i) => self.entries[i].1 = value.to_string(),
            None => self.push(key.to_string(), value.to_string()),
        }
    }
}

/// A parsed INI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ini {
    sections: Vec<Section>,
    index: BTreeMap<String, usize>,
    /// Lines in a fresh load that repeated a section or a key.
    pub duplicates: Vec<Duplicate>,
}

/// A repeated section header or key, which the original resolves through an
/// unstable sort; see the spec. We record them so tools can report them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Duplicate {
    Section(String),
    Key { section: String, key: String },
}

/// Bytes as Latin-1 text.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Strip bytes `<= 0x20` from both ends, like the game's trim.
fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b > 0x20).unwrap_or(s.len());
    let end = s.iter().rposition(|&b| b > 0x20).map_or(start, |e| e + 1);
    &s[start..end]
}

/// Split the input into lines as the game's line reader does: `\n` ends a
/// line, `\r` is dropped, and bytes past [`MAX_LINE`] are discarded.
fn lines(data: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    data.split(|&b| b == b'\n').map(|raw| {
        raw.iter()
            .copied()
            .filter(|&b| b != b'\r')
            .take(MAX_LINE)
            .collect()
    })
}

/// What one line is.
enum Line {
    Section(String),
    Entry(String, String),
    Other,
}

fn classify(raw: &[u8]) -> Line {
    // A section header: after leading whitespace, `[` and a `]` somewhere.
    // The name is what lies between, after trimming the line (so spaces
    // inside the brackets are kept).
    let lead = raw.iter().position(|&b| b > 0x20).unwrap_or(raw.len());
    let rest = &raw[lead..];
    if rest.first() == Some(&b'[')
        && let Some(close) = rest.iter().position(|&b| b == b']')
    {
        return Line::Section(latin1(&rest[1..close]));
    }
    // An entry: drop the comment, trim, skip lines that start with `=`.
    let line = match raw.iter().position(|&b| b == b';') {
        Some(c) => trim(&raw[..c]),
        None => raw,
    };
    if matches!(line.first(), None | Some(b'=')) {
        return Line::Other;
    }
    let Some(eq) = line.iter().position(|&b| b == b'=') else {
        return Line::Other;
    };
    let key = trim(&line[..eq]);
    let value = trim(&line[eq + 1..]);
    if key.is_empty() || value.is_empty() {
        return Line::Other;
    }
    Line::Entry(latin1(key), latin1(value))
}

impl Ini {
    /// Parse a file into a new, empty INI.
    pub fn parse(data: &[u8]) -> Ini {
        let mut ini = Ini::default();
        ini.load(data);
        ini
    }

    /// Load a file into this INI. If it already has sections, the file is
    /// merged: an existing section gains the new keys, and an existing key's
    /// value is replaced. Otherwise every section and key is kept as read.
    ///
    /// Either way, a section that ends up with no entries is not kept, as in
    /// the original (`[recomp 0x00526202]`).
    pub fn load(&mut self, data: &[u8]) {
        let merging = !self.sections.is_empty();
        // The section entries go to: its name, and in a fresh load the index
        // of the section being filled (always the last one).
        let mut current: Option<String> = None;
        let mut filling: Option<usize> = None;
        for raw in lines(data) {
            match classify(&raw) {
                Line::Section(name) => {
                    if let Some(i) = filling.take() {
                        self.finish_section(i);
                    }
                    if !merging {
                        self.sections.push(Section::new(name.clone()));
                        filling = Some(self.sections.len() - 1);
                    }
                    current = Some(name);
                }
                Line::Entry(key, value) => {
                    // Entries before the first section are not stored.
                    let Some(name) = &current else { continue };
                    if let Some(i) = filling {
                        let section = &mut self.sections[i];
                        if section.index.contains_key(&key) {
                            self.duplicates.push(Duplicate::Key {
                                section: section.name.clone(),
                                key: key.clone(),
                            });
                        }
                        section.push(key, value);
                    } else {
                        let i = self.section_or_insert(name);
                        self.sections[i].put(&key, &value);
                    }
                }
                Line::Other => {}
            }
        }
        if let Some(i) = filling {
            self.finish_section(i);
        }
    }

    /// End a section of a fresh load: drop it if empty, else index it.
    fn finish_section(&mut self, i: usize) {
        if self.sections[i].is_empty() {
            self.sections.truncate(i);
            return;
        }
        let name = self.sections[i].name.clone();
        match self.index.entry(name) {
            std::collections::btree_map::Entry::Occupied(e) => {
                self.duplicates.push(Duplicate::Section(e.key().clone()));
            }
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(i);
            }
        }
    }

    fn push_section(&mut self, name: String) -> usize {
        let i = self.sections.len();
        self.index.entry(name.clone()).or_insert(i);
        self.sections.push(Section::new(name));
        i
    }

    fn section_or_insert(&mut self, name: &str) -> usize {
        match self.index.get(name) {
            Some(&i) => i,
            None => self.push_section(name.to_string()),
        }
    }

    /// Sections in file order, duplicates included.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// A section by name. With duplicate sections, the first one wins.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.index.get(name).map(|&i| &self.sections[i])
    }

    /// Set a value, creating the section if needed.
    pub fn put(&mut self, section: &str, key: &str, value: &str) {
        let s = self.section_or_insert(section);
        self.sections[s].put(key, value);
    }

    /// A raw value.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    /// An integer: `$1F` and `1Fh` are hex, anything else is read like C's
    /// `atoi`. `None` if the key is absent.
    pub fn get_int(&self, section: &str, key: &str) -> Option<i32> {
        self.get(section, key).map(parse_int)
    }

    /// A boolean: true if the value starts with `y`, `t` or `1`, false if it
    /// starts with `n`, `f` or `0` (either case). Anything else, or an absent
    /// key, gives `None`, and the caller keeps its default.
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key).and_then(parse_bool)
    }

    /// A number that may be a percentage: `50%` is 0.5, `0.5` is 0.5.
    /// Floating point is fine here: the sim converts rules values to fixed
    /// point when it builds its tables.
    #[allow(clippy::float_arithmetic)]
    pub fn get_f64(&self, section: &str, key: &str) -> Option<f64> {
        self.get(section, key).map(parse_number)
    }

    /// A comma-separated list as the game's `strtok` reads it: empty items
    /// are skipped, and items are **not** trimmed.
    pub fn get_list(&self, section: &str, key: &str) -> Option<Vec<&str>> {
        self.get(section, key).map(parse_list)
    }
}

/// See [`Ini::get_int`].
pub fn parse_int(v: &str) -> i32 {
    if let Some(hex) = v.strip_prefix('$') {
        return parse_hex(hex);
    }
    if v.ends_with(['h', 'H']) {
        return parse_hex(&v[..v.len() - 1]);
    }
    atoi(v)
}

/// `sscanf("%x")`: optional whitespace, sign and `0x`, then hex digits.
fn parse_hex(v: &str) -> i32 {
    let v = v.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (neg, v) = match v.as_bytes().first() {
        Some(b'-') => (true, &v[1..]),
        Some(b'+') => (false, &v[1..]),
        _ => (false, v),
    };
    let v = v
        .strip_prefix("0x")
        .or_else(|| v.strip_prefix("0X"))
        .unwrap_or(v);
    let mut n: u32 = 0;
    for c in v.chars() {
        let Some(d) = c.to_digit(16) else { break };
        n = n.wrapping_mul(16).wrapping_add(d);
    }
    // The bits are the value, as with the game's `%x` into an int.
    #[allow(clippy::cast_possible_wrap)]
    let n = n as i32;
    if neg { n.wrapping_neg() } else { n }
}

/// C's `atoi`: optional whitespace and sign, then decimal digits up to the
/// first non-digit. Overflow wraps, as on the original's 32-bit build.
pub fn atoi(v: &str) -> i32 {
    let v = v.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (neg, v) = match v.as_bytes().first() {
        Some(b'-') => (true, &v[1..]),
        Some(b'+') => (false, &v[1..]),
        _ => (false, v),
    };
    let mut n: i32 = 0;
    for c in v.chars() {
        let Some(d) = c.to_digit(10) else { break };
        #[allow(clippy::cast_possible_wrap)]
        let d = d as i32;
        n = n.wrapping_mul(10).wrapping_add(d);
    }
    if neg { n.wrapping_neg() } else { n }
}

/// See [`Ini::get_bool`].
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.chars().next()?.to_ascii_lowercase() {
        'y' | 't' | '1' => Some(true),
        'n' | 'f' | '0' => Some(false),
        _ => None,
    }
}

/// See [`Ini::get_f64`]: C's `atof`, divided by 100 if the value has a `%`.
#[allow(clippy::float_arithmetic)]
pub fn parse_number(v: &str) -> f64 {
    let n = atof(v);
    if v.contains('%') { n / 100.0 } else { n }
}

/// C's `atof`: the longest prefix that is a decimal number, else 0.
pub fn atof(v: &str) -> f64 {
    let v = v.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let b = v.as_bytes();
    let mut end = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let int = digits(end);
    end += int;
    let mut frac = 0;
    if b.get(end) == Some(&b'.') {
        frac = digits(end + 1);
        end += 1 + frac;
    }
    if int + frac == 0 {
        return 0.0;
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let mut e = end + 1;
        if matches!(b.get(e), Some(b'+' | b'-')) {
            e += 1;
        }
        let exp = digits(e);
        if exp > 0 {
            end = e + exp;
        }
    }
    v[..end]
        .trim_end_matches('.')
        .parse()
        .or_else(|_| v[..end].parse())
        .unwrap_or(0.0)
}

/// See [`Ini::get_list`].
pub fn parse_list(v: &str) -> Vec<&str> {
    v.split(',').filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn the_games_rules_of_reading() {
        let text = b"preamble=ignored\n\
            [General]\r\n\
            Name = Yuri ; the boss\n\
            ;Commented=1\n\
            Empty=\n\
            =NoKey\n\
            \tTabbed\t=\tvalue\t\n\
            NoEquals\n\
            Spaced Key = a b \n\
            [ Spaced ]trailing\n\
            k=v\n";
        let ini = Ini::parse(text);
        let g = ini.section("General").unwrap();
        assert_eq!(g.get("Name"), Some("Yuri"));
        assert_eq!(g.get("Commented"), None);
        assert_eq!(g.get("Empty"), None, "empty values are not stored");
        assert_eq!(g.get("Tabbed"), Some("value"));
        assert_eq!(g.get("Spaced Key"), Some("a b"));
        assert_eq!(g.len(), 3);
        assert_eq!(ini.get(" Spaced ", "k"), Some("v"));
        assert_eq!(ini.sections().len(), 2, "nothing before the first section");
        let ini = Ini::parse(
            b"[PARABOMB]\n\n[MEDUSA]\nRotates=yes\n[PARABOMB]\nRate=200\n[Empty]\n;nothing\n",
        );
        assert_eq!(
            ini.get("PARABOMB", "Rate"),
            Some("200"),
            "an empty section is dropped, not shadowing"
        );
        assert!(ini.section("Empty").is_none());
        assert_eq!(ini.duplicates, []);
        assert_eq!(ini.get("general", "Name"), None, "names are case-sensitive");
    }

    #[test]
    fn long_lines_are_cut() {
        let mut line = b"[S]\nK=".to_vec();
        line.extend(std::iter::repeat_n(b'x', 600));
        line.extend_from_slice(b"\nNext=1\n");
        let ini = Ini::parse(&line);
        assert_eq!(ini.get("S", "K").unwrap().len(), MAX_LINE - 2);
        assert_eq!(
            ini.get("S", "Next"),
            Some("1"),
            "the rest of the line is dropped, not wrapped"
        );
    }

    #[test]
    fn duplicates_first_wins_and_are_recorded() {
        let ini = Ini::parse(b"[A]\nk=1\nk=2\n[B]\nx=1\n[A]\nk=3\nj=4\n");
        assert_eq!(ini.get("A", "k"), Some("1"));
        assert_eq!(
            ini.get("A", "j"),
            None,
            "the second [A] is a separate section"
        );
        assert_eq!(ini.sections().len(), 3);
        assert_eq!(
            ini.duplicates,
            vec![
                Duplicate::Key {
                    section: "A".into(),
                    key: "k".into()
                },
                Duplicate::Section("A".into())
            ]
        );
    }

    #[test]
    fn merging_overrides() {
        let mut ini = Ini::parse(b"[Unit]\nStrength=100\nSpeed=4\n");
        ini.load(b"[Unit]\nStrength=200\n[New]\nx=1\n[Blank]\n");
        assert_eq!(ini.get_int("Unit", "Strength"), Some(200));
        assert_eq!(ini.get_int("Unit", "Speed"), Some(4));
        assert_eq!(ini.get("New", "x"), Some("1"));
        assert!(ini.section("Blank").is_none());
        assert_eq!(ini.duplicates, []);
    }

    #[test]
    fn values() {
        assert_eq!(parse_int("42"), 42);
        assert_eq!(parse_int("-7xyz"), -7);
        assert_eq!(parse_int("abc"), 0);
        assert_eq!(parse_int("$1F"), 31);
        assert_eq!(parse_int("1Fh"), 31);
        assert_eq!(parse_int("ffH"), 255);
        assert_eq!(parse_int("$FFFFFFFF"), -1);
        assert_eq!(parse_int("4294967297"), 1, "wraps like the 32-bit original");
        for (v, b) in [
            ("yes", Some(true)),
            ("True", Some(true)),
            ("1", Some(true)),
            ("no", Some(false)),
        ] {
            assert_eq!(parse_bool(v), b);
        }
        assert_eq!(parse_bool("F"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_number("50%"), 0.5);
        assert_eq!(parse_number("0.5"), 0.5);
        assert_eq!(parse_number(".25"), 0.25);
        assert_eq!(parse_number("3."), 3.0);
        assert_eq!(parse_number("1e2x"), 100.0);
        assert_eq!(parse_number("1e"), 1.0);
        assert_eq!(parse_number("abc"), 0.0);
        assert_eq!(parse_list("E1,E2,,E3"), vec!["E1", "E2", "E3"]);
        assert_eq!(
            parse_list("GI, E2"),
            vec!["GI", " E2"],
            "items are not trimmed"
        );
    }

    #[test]
    fn latin1_round_trips() {
        let ini = Ini::parse(b"[S]\nName=Caf\xe9\n");
        assert_eq!(ini.get("S", "Name"), Some("Caf\u{e9}"));
    }
}
