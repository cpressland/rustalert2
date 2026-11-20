//! The six theaters, and the archives and files each one brings in.
//! From the game's theater table (`[recomp 0x007E1B78]`, 0x70 bytes each)
//! and theater init (`[recomp 0x005349C0]`). See `docs/spec/tmp.md`.

use crate::mount::{Mounted, Step, Vfs};

/// A theater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Theater {
    Temperate,
    Snow,
    Urban,
    Desert,
    NewUrban,
    Lunar,
}

/// One row of the theater table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TheaterInfo {
    /// As maps write it in `[Map] Theater=`.
    pub name: &'static str,
    /// Base of the theater's INI, palette and art archive: `TEMPERAT`.
    pub root: &'static str,
    /// The tile-set archive: `ISOTEMP`.
    pub iso: &'static str,
    /// Base of the Yuri's Revenge tile-set archive: `ISOTEM` (+ `MD`).
    pub iso_md: &'static str,
    /// The tiles' file extension and the unit palette's suffix: `TEM`.
    pub ext: &'static str,
    /// The letter swapped into theater-specific art names (`GAPOWR` →
    /// `GTPOWR`), at offset `0x56` of the table row.
    pub letter: char,
}

impl Theater {
    pub const ALL: [Theater; 6] = [
        Theater::Temperate,
        Theater::Snow,
        Theater::Urban,
        Theater::Desert,
        Theater::NewUrban,
        Theater::Lunar,
    ];

    pub fn info(self) -> TheaterInfo {
        let row = |name, root, iso, iso_md, ext, letter| TheaterInfo {
            name,
            root,
            iso,
            iso_md,
            ext,
            letter,
        };
        match self {
            Theater::Temperate => row("TEMPERATE", "TEMPERAT", "ISOTEMP", "ISOTEM", "TEM", 'T'),
            Theater::Snow => row("SNOW", "SNOW", "ISOSNOW", "ISOSNO", "SNO", 'A'),
            Theater::Urban => row("URBAN", "URBAN", "ISOURB", "ISOURB", "URB", 'U'),
            Theater::Desert => row("DESERT", "DESERT", "ISODES", "ISODES", "DES", 'D'),
            Theater::NewUrban => row("NEWURBAN", "URBANN", "ISOUBN", "ISOUBN", "UBN", 'N'),
            Theater::Lunar => row("LUNAR", "LUNAR", "ISOLUN", "ISOLUN", "LUN", 'L'),
        }
    }

    /// The theater a map names, ignoring case.
    pub fn from_name(name: &str) -> Option<Theater> {
        Theater::ALL
            .into_iter()
            .find(|t| t.info().name.eq_ignore_ascii_case(name))
    }

    /// The archive names the game mounts for this theater, in order
    /// (`[recomp 0x00534AC0]`–`0x00534BD0`): `TEMPERATMD.MIX`,
    /// `TEMPERAT.MIX`, `TEM.MIX`, `ISOTEMMD.MIX`, `ISOTEMP.MIX`.
    pub fn archives(self) -> [String; 5] {
        let i = self.info();
        [
            format!("{}md.mix", i.root),
            format!("{}.mix", i.root),
            format!("{}.mix", i.ext),
            format!("{}md.mix", i.iso_md),
            format!("{}.mix", i.iso),
        ]
        .map(|s| s.to_lowercase())
    }

    /// The theater's INI (`%sMD.INI`, `[recomp 0x005454FE]`).
    pub fn ini(self) -> String {
        format!("{}md.ini", self.info().root).to_lowercase()
    }

    /// The tile extension, lower case.
    pub fn ext(self) -> String {
        self.info().ext.to_lowercase()
    }

    /// The theater's palettes: the general one (`%s.PAL`), units
    /// (`UNIT%s.PAL`) and tiles (`ISO%s.PAL`).
    pub fn palettes(self) -> [String; 3] {
        let i = self.info();
        [
            format!("{}.pal", i.root),
            format!("unit{}.pal", i.ext),
            format!("iso{}.pal", i.ext),
        ]
        .map(|s| s.to_lowercase())
    }
}

impl Vfs {
    /// Mount a theater's archives, as the game does when a scenario loads.
    pub fn mount_theater(&mut self, theater: Theater) -> Vec<Mounted> {
        let names = theater.archives();
        let steps: Vec<Step> = names.iter().map(|n| Step::Owned(n.clone())).collect();
        self.mount_sequence(&steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            Theater::Temperate.archives(),
            [
                "temperatmd.mix",
                "temperat.mix",
                "tem.mix",
                "isotemmd.mix",
                "isotemp.mix"
            ]
        );
        assert_eq!(Theater::NewUrban.ini(), "urbannmd.ini");
        assert_eq!(
            Theater::Snow.palettes(),
            ["snow.pal", "unitsno.pal", "isosno.pal"]
        );
        assert_eq!(Theater::from_name("newurban"), Some(Theater::NewUrban));
    }
}
