//! File names we know, so archive listings can show names instead of IDs.
//!
//! A MIX index holds only a CRC of each name. The game knows its names
//! because it asks for them; tools have to guess. These are the archive
//! names the executables ask for (`[recomp]` strings in `.rdata`) and the
//! standard INI and string files. Names found in rules INIs get added as the
//! parsers that read them land.

use std::collections::BTreeMap;

use ra2_formats::mix::mix_id;

/// Archive names, with `##` standing for two digits (`MAPS%02d.MIX`).
const ARCHIVES: &[&str] = &[
    "ra2.mix",
    "ra2md.mix",
    "language.mix",
    "langmd.mix",
    "multi.mix",
    "multimd.mix",
    "theme.mix",
    "thememd.mix",
    "themes.mix",
    "themesmd.mix",
    "audio.mix",
    "audiomd.mix",
    "cache.mix",
    "cachemd.mix",
    "local.mix",
    "localmd.mix",
    "conquer.mix",
    "conqmd.mix",
    "generic.mix",
    "genermd.mix",
    "isogen.mix",
    "isogenmd.mix",
    "neutral.mix",
    "ntrlmd.mix",
    "cameo.mix",
    "cameomd.mix",
    "load.mix",
    "loadmd.mix",
    "main.mix",
    "wdt.mix",
    "wdtvox.mix",
    // Theaters: the tile set and the theater's own art, each base game then YR.
    "temperat.mix",
    "isotemp.mix",
    "tem.mix",
    "isotem.mix",
    "tempermd.mix",
    "isotemmd.mix",
    "snow.mix",
    "isosnow.mix",
    "sno.mix",
    "isosno.mix",
    "snowmd.mix",
    "isosnomd.mix",
    "urban.mix",
    "isourb.mix",
    "urb.mix",
    "urbanmd.mix",
    "isourbmd.mix",
    "desert.mix",
    "isodes.mix",
    "des.mix",
    "desertmd.mix",
    "isodesmd.mix",
    "lunar.mix",
    "isolun.mix",
    "lun.mix",
    "lunarmd.mix",
    "isolunmd.mix",
    "urbann.mix",
    "isoubn.mix",
    "ubn.mix",
    "urbannmd.mix",
    "isoubnmd.mix",
    "sounds.mix",
    "soundsmd.mix",
    "speech.mix",
    // Numbered.
    "expand##.mix",
    "expandmd##.mix",
    "maps##.mix",
    "mapsmd##.mix",
    "movies##.mix",
    "movmd##.mix",
    "sidec##.mix",
    "sidec##md.mix",
    "sidenc##.mix",
    "speech##.mix",
    "sounds##.mix",
];

/// Data files the engine reads by name.
const FILES: &[&str] = &[
    "rules.ini",
    "rulesmd.ini",
    "art.ini",
    "artmd.ini",
    "ai.ini",
    "aimd.ini",
    "sound.ini",
    "soundmd.ini",
    "eva.ini",
    "evamd.ini",
    "theme.ini",
    "thememd.ini",
    "battle.ini",
    "battlemd.ini",
    "mapsel.ini",
    "mapselmd.ini",
    "missions.ini",
    "missionmd.ini",
    "mpmodes.ini",
    "mpmodesmd.ini",
    "coop.ini",
    "coopmd.ini",
    "ui.ini",
    "uimd.ini",
    "temperat.ini",
    "temperatmd.ini",
    "snow.ini",
    "snowmd.ini",
    "urban.ini",
    "urbanmd.ini",
    "desert.ini",
    "desertmd.ini",
    "lunar.ini",
    "lunarmd.ini",
    "urbann.ini",
    "urbannmd.ini",
    "keyboard.ini",
    "keyboardmd.ini",
    "ra2.csf",
    "ra2md.csf",
    "audio.bag",
    "audio.idx",
    "audiomd.bag",
    "audiomd.idx",
    "game.fnt",
    "mouse.sha",
    "local mix database.dat",
];

/// Every known name, by ID.
pub fn known() -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    let mut add = |name: String| {
        out.entry(mix_id(&name)).or_insert(name);
    };
    for &name in ARCHIVES.iter().chain(FILES) {
        if name.contains("##") {
            for n in 0..100 {
                add(name.replace("##", &format!("{n:02}")));
            }
        } else {
            add(name.to_string());
        }
    }
    out
}
