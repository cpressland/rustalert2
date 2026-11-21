//! The objects on a map, drawn over its terrain: overlays (walls, ore,
//! bridges), terrain objects (trees), smudges, buildings and infantry.
//! Vehicles are voxels (see [`crate::voxel`]). See `docs/spec/terrain.md`.

use std::collections::BTreeMap;

use ra2_formats::ini::{Ini, atoi};
use ra2_formats::map::Map;
use ra2_formats::pal::{Hsv, Palette, Rgb, remap};
use ra2_formats::shp::Shp;
use ra2_sim::coords::Cell;
use ra2_vfs::mount::Vfs;
use ra2_vfs::theater::Theater;

use crate::canvas::Canvas;

/// What the scene draws, for reports.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneStats {
    pub overlays: usize,
    pub terrain_objects: usize,
    pub smudges: usize,
    pub buildings: usize,
    pub infantry: usize,
    pub vehicles: usize,
    /// Objects whose art couldn't be found, by art name.
    pub missing: BTreeMap<String, usize>,
}

/// Which palette a piece uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PaletteKind {
    /// `ISO{ext}.PAL`: terrain, smudges, terrain objects.
    Iso,
    /// `{root}.PAL`, the theater palette: ore and other overlays.
    Theater,
    /// `UNIT{ext}.PAL` with a house's colours in the remap slots.
    Unit(usize),
}

/// One sprite to draw.
#[derive(Debug, Clone)]
pub struct Piece {
    /// Draw order: screen y of the object's base, then x, then layer.
    pub order: (i32, i32, u8),
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub pixels: Vec<u8>,
    pub palette: PaletteKind,
    /// Index 1 is a shadow that darkens what's below.
    pub shadow: bool,
    /// The height level the object stands at, for lighting.
    pub level: i32,
}

/// Rules and art the scene needs.
#[derive(Debug)]
pub struct Art {
    pub rules: Ini,
    pub art: Ini,
    pub theater: Theater,
    pub iso: [Rgb; 256],
    pub theater_pal: [Rgb; 256],
    pub unit: [Rgb; 256],
    /// House name → colour (from the map, else the rules' `[Colors]`).
    house_colors: BTreeMap<String, Hsv>,
    /// Remapped unit palettes, by house name, in first-use order.
    pub house_palettes: Vec<(String, [Rgb; 256])>,
    shp_cache: BTreeMap<String, Option<Vec<u8>>>,
    shades: Option<crate::voxel::ShadeTable>,
    /// The voxel normals, from the player's `gamemd.exe`.
    normals: Option<crate::voxel::Normals>,
    voxel_cache: BTreeMap<(String, u8), Option<crate::voxel::Sprite>>,
}

fn palette(vfs: &Vfs, name: &str) -> [Rgb; 256] {
    vfs.read(name)
        .ok()
        .flatten()
        .and_then(|d| Palette::parse(&d))
        .map_or([[0; 3]; 256], |p| p.to_rgb8())
}

impl Art {
    pub fn load(vfs: &Vfs, theater: Theater, rules: Ini, art: Ini, map_ini: &Ini) -> Art {
        let names = theater.palettes();
        let mut house_colors = BTreeMap::new();
        if let Some(houses) = map_ini.section("Houses") {
            for (_, name) in houses.entries() {
                let color = map_ini
                    .get(name, "Color")
                    .and_then(|c| rules.get("Colors", c))
                    .map(Hsv::parse);
                if let Some(c) = color {
                    house_colors.insert(name.to_string(), c);
                }
            }
        }
        Art {
            rules,
            art,
            theater,
            iso: palette(vfs, &names[2]),
            theater_pal: palette(vfs, &names[0]),
            unit: palette(vfs, &names[1]),
            house_colors,
            house_palettes: Vec::new(),
            shp_cache: BTreeMap::new(),
            shades: vfs
                .read("voxels.vpl")
                .ok()
                .flatten()
                .and_then(|d| crate::voxel::ShadeTable::parse(&d)),
            normals: vfs
                .read("gamemd.exe")
                .ok()
                .flatten()
                .and_then(|d| crate::voxel::Normals::from_gamemd(&d)),
            voxel_cache: BTreeMap::new(),
        }
    }

    /// The palette for an owner, remapped to its colour.
    fn house_palette(&mut self, owner: &str) -> PaletteKind {
        if let Some(i) = self.house_palettes.iter().position(|(n, _)| n == owner) {
            return PaletteKind::Unit(i);
        }
        let hsv = self
            .house_colors
            .get(owner)
            .copied()
            .unwrap_or(Hsv { h: 0, s: 0, v: 150 });
        self.house_palettes
            .push((owner.to_string(), remap(&self.unit, hsv)));
        PaletteKind::Unit(self.house_palettes.len() - 1)
    }

    pub fn palette(&self, kind: PaletteKind) -> &[Rgb; 256] {
        match kind {
            PaletteKind::Iso => &self.iso,
            PaletteKind::Theater => &self.theater_pal,
            PaletteKind::Unit(i) => &self.house_palettes[i].1,
        }
    }

    /// The art section name for a rules type: its `Image=`, else itself.
    fn image(&self, type_id: &str) -> String {
        self.rules
            .get(type_id, "Image")
            .unwrap_or(type_id)
            .to_string()
    }

    /// The SHP file for an art name, following `Theater=` and `NewTheater=`.
    fn shp_name(&self, image: &str) -> Vec<String> {
        let yes = |k| {
            self.art
                .get(image, k)
                .and_then(ra2_formats::ini::parse_bool)
                .unwrap_or(false)
        };
        let lower = image.to_lowercase();
        if yes("Theater") {
            return vec![format!("{lower}.{}", self.theater.ext())];
        }
        if yes("NewTheater") && lower.len() >= 2 {
            let letter = self.theater.info().letter.to_ascii_lowercase();
            let mut themed: Vec<char> = lower.chars().collect();
            themed[1] = letter;
            let mut generic = themed.clone();
            generic[1] = 'g';
            return vec![
                themed.iter().collect::<String>() + ".shp",
                generic.iter().collect::<String>() + ".shp",
                format!("{lower}.shp"),
            ];
        }
        vec![format!("{lower}.shp")]
    }

    /// A vehicle's hull, turret and barrel, rendered at a facing.
    fn vehicle(&mut self, vfs: &Vfs, image: &str, facing: u8) -> Option<crate::voxel::Sprite> {
        use ra2_formats::vxl::{Hva, Vxl};
        let key = (image.to_string(), facing);
        if !self.voxel_cache.contains_key(&key) {
            let lower = image.to_lowercase();
            let load = |part: &str| -> Option<(Vxl, Option<Hva>)> {
                let v =
                    Vxl::parse(&vfs.read(&format!("{lower}{part}.vxl")).ok().flatten()?).ok()?;
                let h = vfs
                    .read(&format!("{lower}{part}.hva"))
                    .ok()
                    .flatten()
                    .and_then(|d| Hva::parse(&d).ok());
                Some((v, h))
            };
            let parts: Vec<(Vxl, Option<Hva>)> =
                ["", "tur", "barl"].iter().filter_map(|p| load(p)).collect();
            let sprite = if parts.is_empty() {
                None
            } else {
                let refs: Vec<(&Vxl, Option<&Hva>)> =
                    parts.iter().map(|(v, h)| (v, h.as_ref())).collect();
                Some(crate::voxel::render(
                    &refs,
                    facing,
                    self.shades.as_ref(),
                    self.normals.as_ref(),
                ))
            };
            self.voxel_cache.insert(key.clone(), sprite);
        }
        self.voxel_cache.get(&key).cloned().flatten()
    }

    fn shp(&mut self, vfs: &Vfs, image: &str) -> Option<Vec<u8>> {
        if !self.shp_cache.contains_key(image) {
            let data = self
                .shp_name(image)
                .into_iter()
                .find_map(|n| vfs.read(&n).ok().flatten());
            self.shp_cache.insert(image.to_string(), data);
        }
        self.shp_cache.get(image).cloned().flatten()
    }
}

/// Add an SHP frame, and its shadow (the frame half the count later), with
/// the frame's canvas centred on `center`.
#[allow(clippy::too_many_arguments)]
fn push_frame(
    out: &mut Vec<Piece>,
    data: &[u8],
    frame: usize,
    center: (i32, i32),
    order: (i32, i32, u8),
    palette: PaletteKind,
    with_shadow: bool,
    level: i32,
) -> bool {
    let Ok(shp) = Shp::parse(data) else {
        return false;
    };
    let n = shp.frames.len();
    if n == 0 {
        return false;
    }
    let (cw, ch) = (i32::from(shp.width), i32::from(shp.height));
    let mut add = |index: usize, layer: u8, shadow: bool| {
        if let Ok(f) = shp.frame(index) {
            out.push(Piece {
                order: (order.0, order.1, layer),
                x: center.0 - cw / 2 + i32::from(f.x),
                y: center.1 - ch / 2 + i32::from(f.y),
                width: usize::from(f.width),
                pixels: f.pixels,
                palette,
                shadow,
                level,
            });
        }
    };
    if with_shadow && n >= 2 {
        add(frame % (n / 2) + n / 2, order.2, true);
        add(frame % (n / 2), order.2 + 1, false);
    } else {
        add(frame % n, order.2, false);
    }
    true
}

/// The ground level of each cell, from the map's terrain.
fn levels(map: &Map) -> BTreeMap<(i16, i16), i32> {
    map.tiles
        .iter()
        .map(|t| ((t.x, t.y), i32::from(t.level)))
        .collect()
}

/// Infantry facing (0–255) to a sequence frame offset: 8 facings,
/// numbered anticlockwise from north. `[community]`
fn infantry_facing(facing: i32) -> usize {
    let dir = usize::try_from(((facing + 16) / 32).rem_euclid(8)).expect("0..8");
    (8 - dir) % 8
}

/// Collect every object of a map as pieces, leaving out those standing on
/// cells `hidden` says are under shroud.
#[allow(clippy::too_many_lines)]
pub fn collect(
    vfs: &Vfs,
    art: &mut Art,
    map: &Map,
    map_ini: &Ini,
    stats: &mut SceneStats,
    hidden: &dyn Fn(i32, i32) -> bool,
) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let level = levels(map);
    let height = |x: i32, y: i32| -> i32 {
        level
            .get(&(i16::try_from(x).unwrap_or(0), i16::try_from(y).unwrap_or(0)))
            .copied()
            .unwrap_or(0)
    };
    let ground = |x: i32, y: i32| -> (i32, i32) {
        let (cx, cy) = (i16::try_from(x).unwrap_or(0), i16::try_from(y).unwrap_or(0));
        let p = Cell::new(cx, cy).screen(height(x, y));
        (p.x, p.y + 15)
    };
    let miss = |stats: &mut SceneStats, name: &str| {
        *stats.missing.entry(name.to_string()).or_default() += 1;
    };

    // Overlays: [OverlayTypes] in list order; the frame is the data byte.
    let overlay_types: Vec<String> = art
        .rules
        .section("OverlayTypes")
        .map(|s| s.entries().map(|(_, v)| v.to_string()).collect())
        .unwrap_or_default();
    if let (Some(ov), Some(data)) = (&map.overlay, &map.overlay_data) {
        for (i, &t) in ov.iter().enumerate() {
            if t == 0xff {
                continue;
            }
            let (x, y) = (
                i32::try_from(i % 512).expect("small"),
                i32::try_from(i / 512).expect("small"),
            );
            let Some(id) = overlay_types.get(usize::from(t)) else {
                continue;
            };
            let image = art.image(id);
            let wall = art
                .rules
                .get(id, "Wall")
                .and_then(ra2_formats::ini::parse_bool)
                .unwrap_or(false);
            let pal = if wall {
                art.house_palette("Neutral")
            } else {
                PaletteKind::Theater
            };
            if hidden(x, y) {
                continue;
            }
            let c = ground(x, y);
            match art.shp(vfs, &image) {
                Some(d)
                    if push_frame(
                        &mut pieces,
                        &d,
                        usize::from(data[i]),
                        c,
                        (c.1, c.0, 0),
                        pal,
                        false,
                        height(x, y),
                    ) =>
                {
                    stats.overlays += 1;
                }
                _ => miss(stats, &image),
            }
        }
    }

    // Smudges: n=TYPE,x,y,0 — drawn first in their cell.
    if let Some(s) = map_ini.section("Smudge") {
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            if p.len() < 3 {
                continue;
            }
            if hidden(atoi(p[1]), atoi(p[2])) {
                continue;
            }
            let c = ground(atoi(p[1]), atoi(p[2]));
            let image = art.image(p[0]);
            match art.shp(vfs, &image) {
                Some(d)
                    if push_frame(
                        &mut pieces,
                        &d,
                        0,
                        c,
                        (c.1 - 30, c.0, 0),
                        PaletteKind::Iso,
                        false,
                        height(atoi(p[1]), atoi(p[2])),
                    ) =>
                {
                    stats.smudges += 1;
                }
                _ => miss(stats, &image),
            }
        }
    }

    // Terrain objects: yyyxxx=TYPE.
    if let Some(s) = map_ini.section("Terrain") {
        for (k, v) in s.entries() {
            let n = atoi(k);
            if hidden(n % 1000, n / 1000) {
                continue;
            }
            let c = ground(n % 1000, n / 1000);
            let image = art.image(v);
            match art.shp(vfs, &image) {
                Some(d)
                    if push_frame(
                        &mut pieces,
                        &d,
                        0,
                        c,
                        (c.1, c.0, 0),
                        PaletteKind::Iso,
                        true,
                        height(n % 1000, n / 1000),
                    ) =>
                {
                    stats.terrain_objects += 1;
                }
                _ => miss(stats, &image),
            }
        }
    }

    // Buildings: owner,type,health,x,y,... The SHP is centred on the
    // foundation's middle.
    if let Some(s) = map_ini.section("Structures") {
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            if p.len() < 5 {
                continue;
            }
            let (owner, id, x, y) = (p[0], p[1], atoi(p[3]), atoi(p[4]));
            if hidden(x, y) {
                continue;
            }
            let image = art.image(id);
            let (fw, fh) = art
                .art
                .get(&image, "Foundation")
                .and_then(|f| {
                    f.to_lowercase()
                        .split_once('x')
                        .map(|(a, b)| (atoi(a), atoi(b)))
                })
                .unwrap_or((1, 1));
            let top = ground(x, y);
            // The middle of a fw x fh foundation, from its top cell's centre.
            let c = (top.0 + (fw - fh) * 15, top.1 + (fw + fh - 2) * 15 / 2);
            let pal = art.house_palette(owner);
            let order = (top.1 + (fw + fh - 1) * 15, top.0, 0);
            match art.shp(vfs, &image) {
                Some(d) if push_frame(&mut pieces, &d, 0, c, order, pal, true, height(x, y)) => {
                    stats.buildings += 1;
                }
                _ => miss(stats, &image),
            }
        }
    }

    // Infantry: owner,type,health,x,y,subcell,mission,facing,...
    if let Some(s) = map_ini.section("Infantry") {
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            if p.len() < 8 {
                continue;
            }
            let (owner, id, x, y, sub, facing) =
                (p[0], p[1], atoi(p[3]), atoi(p[4]), atoi(p[5]), atoi(p[7]));
            if hidden(x, y) {
                continue;
            }
            let image = art.image(id);
            let seq = art
                .art
                .get(&image, "Sequence")
                .unwrap_or_default()
                .to_string();
            let ready = art.art.get(&seq, "Ready").map_or((0, 1), |r| {
                let f: Vec<i32> = r.split(',').map(atoi).collect();
                (
                    f.first().copied().unwrap_or(0),
                    f.get(2).copied().unwrap_or(1),
                )
            });
            let frame = usize::try_from(ready.0).unwrap_or(0)
                + infantry_facing(facing) * usize::try_from(ready.1.max(1)).unwrap_or(1);
            // Sub-cells 2, 3, 4: right, left, bottom of the cell; 0 and 1 the middle. `[community]`
            let off = match sub {
                2 => (15, -7),
                3 => (-15, -7),
                4 => (0, 7),
                _ => (0, 0),
            };
            let g = ground(x, y);
            let c = (g.0 + off.0, g.1 + off.1);
            let pal = art.house_palette(owner);
            match art.shp(vfs, &image) {
                Some(d)
                    if push_frame(
                        &mut pieces,
                        &d,
                        frame,
                        c,
                        (c.1, c.0, 0),
                        pal,
                        true,
                        height(x, y),
                    ) =>
                {
                    stats.infantry += 1;
                }
                _ => miss(stats, &image),
            }
        }
    }
    // Vehicles: owner,type,health,x,y,facing,mission,... Voxel models.
    if let Some(s) = map_ini.section("Units") {
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            if p.len() < 6 {
                continue;
            }
            let (owner, id, x, y, facing) = (p[0], p[1], atoi(p[3]), atoi(p[4]), atoi(p[5]));
            if hidden(x, y) {
                continue;
            }
            let image = art.image(id);
            let c = ground(x, y);
            let pal = art.house_palette(owner);
            if let Some(sprite) = art.vehicle(
                vfs,
                &image,
                u8::try_from(facing.rem_euclid(256)).expect("0..256"),
            ) {
                pieces.push(Piece {
                    order: (c.1, c.0, 0),
                    x: c.0 - sprite.anchor.0,
                    y: c.1 - sprite.anchor.1,
                    width: sprite.width,
                    pixels: sprite.pixels,
                    palette: pal,
                    shadow: false,
                    level: height(x, y),
                });
                stats.vehicles += 1;
            } else if let Some(d) = art.shp(vfs, &image) {
                if push_frame(
                    &mut pieces,
                    &d,
                    0,
                    c,
                    (c.1, c.0, 0),
                    pal,
                    true,
                    height(x, y),
                ) {
                    stats.vehicles += 1;
                }
            } else {
                miss(stats, &image);
            }
        }
    }
    pieces
}

/// Draw pieces in order onto a canvas, lit by the map's lighting.
pub fn draw(
    canvas: &mut Canvas,
    art: &Art,
    light: &crate::lighting::Lighting,
    mut pieces: Vec<Piece>,
) {
    pieces.sort_by_key(|p| p.order);
    let mut lit: BTreeMap<(PaletteKind, i32), [Rgb; 256]> = BTreeMap::new();
    for p in pieces {
        let palette = lit
            .entry((p.palette, p.level))
            .or_insert_with(|| light.apply(art.palette(p.palette), p.level));
        canvas.blit(p.x, p.y, p.width, &p.pixels, palette, p.shadow);
    }
}
