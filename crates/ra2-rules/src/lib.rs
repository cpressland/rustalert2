//! Typed rules tables built from the game's rules INIs. See
//! `docs/spec/rules.md`.
//!
//! The INI is read as the game reads it (`ra2_formats::ini`), then turned
//! into plain tables the simulation indexes by number. Fractions and
//! distances are converted to fixed point here, so the simulation never
//! sees a float.
#![forbid(unsafe_code)]
// Rules entries are naturally lists of yes/no keys.
#![allow(clippy::struct_excessive_bools)]

pub mod production;
pub mod scenario;

use std::collections::BTreeMap;

pub use ra2_formats::ini::Ini;

use ra2_formats::ini::{Section, atoi, parse_bool, parse_list, parse_number};

/// A factor the game keeps as a double (`Verses=` percentages,
/// `PercentAtMax=`, `ProneDamage=`), held as the double's exact bits so the
/// simulation can apply it as the game does without float arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio(u64);

impl Default for Ratio {
    fn default() -> Ratio {
        Ratio::ONE
    }
}

impl Ratio {
    /// 1.0
    pub const ONE: Ratio = Ratio(0x3FF0_0000_0000_0000);

    /// The game's reading of a number: `50%` and `.5` both give a half.
    pub fn from_value(v: &str) -> Ratio {
        Ratio(parse_number(v).to_bits())
    }

    /// A number the game keeps as a single-precision float, widened.
    pub fn from_value_f32(v: &str) -> Ratio {
        #[allow(clippy::cast_possible_truncation)]
        let f = parse_number(v) as f32;
        Ratio(f64::from(f).to_bits())
    }

    /// The value as a double, for the rules' own arithmetic
    /// ([`production`]).
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// From a double's bits.
    pub fn from_bits(bits: u64) -> Ratio {
        Ratio(bits)
    }

    pub fn bits(self) -> u64 {
        self.0
    }

    /// `n × self` truncated toward zero, exactly as the game's double
    /// multiply and truncating conversion give it: the product rounded to
    /// 53 significant bits (ties to even), then truncated. Out-of-range
    /// results give `i32::MIN`, as the x86 conversion does.
    pub fn of(self, n: i32) -> i32 {
        let negative = (self.0 >> 63 == 1) != (n < 0);
        let exp = i32::try_from((self.0 >> 52) & 0x7FF).expect("11 bits");
        let frac = self.0 & ((1 << 52) - 1);
        if exp == 0x7FF {
            return i32::MIN; // infinity or NaN
        }
        let (mantissa, scale) = if exp == 0 {
            (frac, -1074) // subnormal
        } else {
            (frac | (1 << 52), exp - 1075)
        };
        let mut p = u128::from(n.unsigned_abs()) * u128::from(mantissa);
        if p == 0 {
            return 0;
        }
        let mut scale = scale;
        // Round to 53 significant bits, ties to even.
        let width = 128 - p.leading_zeros();
        if width > 53 {
            let shift = width - 53;
            let rest = p & ((1u128 << shift) - 1);
            let half = 1u128 << (shift - 1);
            p >>= shift;
            if rest > half || (rest == half && p & 1 == 1) {
                p += 1;
            }
            scale += i32::try_from(shift).expect("small");
        }
        // Truncate toward zero.
        let magnitude = if scale >= 0 {
            if scale > 40 {
                return i32::MIN;
            }
            p << scale
        } else if scale <= -128 {
            0
        } else {
            p >> (-scale)
        };
        match i32::try_from(magnitude) {
            Ok(m) if negative => -m,
            Ok(m) => m,
            Err(_) if negative && magnitude == 1 << 31 => i32::MIN,
            Err(_) => i32::MIN,
        }
    }
}

/// A distance in cells, as `Range=` and `CellSpread=` give it, in leptons
/// (256 to the cell), truncated. `[community]`
#[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
fn leptons(v: &str) -> i32 {
    (parse_number(v) * 256.0) as i32
}

/// The game's eleven armor types, in `Verses=` order. `[community]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Armor {
    #[default]
    None,
    Flak,
    Plate,
    Light,
    Medium,
    Heavy,
    Wood,
    Steel,
    Concrete,
    Special1,
    Special2,
}

impl Armor {
    pub const ALL: [Armor; 11] = [
        Armor::None,
        Armor::Flak,
        Armor::Plate,
        Armor::Light,
        Armor::Medium,
        Armor::Heavy,
        Armor::Wood,
        Armor::Steel,
        Armor::Concrete,
        Armor::Special1,
        Armor::Special2,
    ];

    /// The `Armor=` name. Unknown names read as `none`. `[hypothesis]`
    pub fn parse(v: &str) -> Armor {
        match v.trim().to_ascii_lowercase().as_str() {
            "flak" => Armor::Flak,
            "plate" => Armor::Plate,
            "light" => Armor::Light,
            "medium" => Armor::Medium,
            "heavy" => Armor::Heavy,
            "wood" => Armor::Wood,
            "steel" => Armor::Steel,
            "concrete" => Armor::Concrete,
            "special_1" => Armor::Special1,
            "special_2" => Armor::Special2,
            _ => Armor::None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// What kind of techno a type is: the list it's declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Building,
    Vehicle,
    Infantry,
    Aircraft,
}

/// Indices into the rules tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WeaponId(pub u16);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WarheadId(pub u16);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectileId(pub u16);

/// A building, vehicle, infantry or aircraft type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnoType {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// The art section (`Image=`, else the ID).
    pub image: String,
    pub strength: i32,
    pub armor: Armor,
    pub tech_level: i32,
    pub cost: i32,
    pub points: i32,
    /// In cells.
    pub sight: i32,
    /// `Speed=` as written.
    pub speed: i32,
    /// The speed the game keeps: `Speed=` clamped to 0–100, × 256 / 100,
    /// capped at 255. `[recomp 0x0071465A]`
    pub speed_fraction: i32,
    pub rot: i32,
    /// Drivers: whether it speeds up from a crawl rather than setting off
    /// at full speed (`Accelerates=`, default yes), by how much of full
    /// speed a frame (`AccelerationFactor=`, default 0.03), and how it
    /// slows within `SlowdownDistance=` leptons (default 500) of where it's
    /// going: by `DeaccelerationFactor=` (default 0.002) × `speed_fraction`
    /// a frame. `[recomp 0x00710BB2, 0x00711651]`
    pub accelerates: bool,
    pub acceleration_factor: Ratio,
    pub deacceleration_factor: Ratio,
    pub slowdown_distance: i32,
    pub owner: Vec<String>,
    pub prerequisite: Vec<String>,
    pub primary: Option<WeaponId>,
    pub secondary: Option<WeaponId>,
    pub elite_primary: Option<WeaponId>,
    pub elite_secondary: Option<WeaponId>,
    pub turret: bool,
    pub crusher: bool,
    pub locomotor: Option<String>,
    /// Buildings: power produced (positive) or drawn (negative).
    pub power: i32,
    /// Buildings: the art's `Foundation=`, in cells.
    pub foundation: (i32, i32),
    /// Gathers ore (`Harvester=`).
    pub harvester: bool,
    /// Units: the building it unpacks into (`DeploysInto=`).
    pub deploys_into: Option<String>,
    /// Buildings: what kind of thing it builds (`Factory=`).
    pub factory: Option<Kind>,
    /// `BuildTimeMultiplier=`, a float in the game. `[recomp 0x0071437C]`
    pub build_time_multiplier: Ratio,
    /// Buildings: a wall (`Wall=`), built slower.
    pub wall: bool,
    /// What selling it pays, if set (`Soylent=`) instead of a share of
    /// its cost.
    pub soylent: i32,
    /// Bails a harvester holds, or a refinery stores (`Storage=`).
    pub storage: i32,
    /// The buildings it docks with (`Dock=`).
    pub dock: Vec<String>,
    /// Refineries: where a harvester waits to dock, from the foundation's
    /// top-left (the art's `QueueingCell=`).
    pub queueing_cell: Option<(i32, i32)>,
}

/// A `[WeaponType]`: what a techno fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponType {
    pub id: String,
    pub damage: i32,
    /// Frames between shots.
    pub rof: i32,
    /// In leptons.
    pub range: i32,
    pub minimum_range: i32,
    pub speed: i32,
    pub burst: i32,
    pub projectile: Option<ProjectileId>,
    pub warhead: Option<WarheadId>,
}

/// A `[Warheads]` entry: how damage lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warhead {
    pub id: String,
    /// Damage against each armor type, in `Armor` order.
    pub verses: [Ratio; 11],
    /// In leptons.
    pub cell_spread: i32,
    pub percent_at_max: Ratio,
    pub wall: bool,
    pub wood: bool,
    pub conventional: bool,
    pub inf_death: i32,
    pub prone_damage: Ratio,
}

/// A projectile type (named by weapons' `Projectile=`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projectile {
    pub id: String,
    pub arcing: bool,
    pub inviso: bool,
    pub anti_air: bool,
    pub anti_ground: bool,
    pub rot: i32,
    pub subject_to_cliffs: bool,
    pub subject_to_walls: bool,
}

/// A `[Countries]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    pub id: String,
    pub name: String,
    /// The `[Sides]` key it belongs to (`GDI` for the Allies, `Nod` for the
    /// Soviets, `ThirdSide` for Yuri). `Side=` names it too.
    pub side: String,
    pub prefix: String,
    pub suffix: String,
    pub multiplay: bool,
}

/// An animation (`[Animations]`; its keys live in the art file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimType {
    pub id: String,
    /// `Rate=` as written; its timing belongs to the animation system.
    pub rate: i32,
    pub loop_start: i32,
    pub loop_end: i32,
    /// −1 loops for ever.
    pub loop_count: i32,
    /// The animation that follows this one.
    pub next: Option<String>,
    /// Damage dealt each loop, through `warhead`.
    pub damage: Ratio,
    pub warhead: Option<WarheadId>,
    pub translucent: bool,
    pub shadow: bool,
}

/// A `[SuperWeaponTypes]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperWeaponType {
    pub id: String,
    pub name: String,
    /// `Type=`: what it does (`MultiMissile`, `IronCurtain`, …).
    pub kind: String,
    /// `RechargeTime=`, in minutes, as the game's double.
    pub recharge_minutes: Ratio,
    pub is_powered: bool,
    pub weapon: Option<WeaponId>,
    /// `Range=` in cells, as the game's double.
    pub range: Ratio,
}

/// A `[ParticleSystems]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticleSystemType {
    pub id: String,
    /// The `[Particles]` type it spawns.
    pub holds_what: Option<String>,
    /// `BehavesLike=`: `Smoke`, `Gas`, `Fire`, `Spark`, `Railgun`.
    pub behaves_like: String,
    pub particle_cap: i32,
    pub spawn_frames: i32,
}

/// General values the simulation uses so far. More of `[General]` arrives
/// with the systems that use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct General {
    /// Credits per bail of ore (`[Riparius] Value=`) and gems
    /// (`[Cruentus] Value=`).
    pub ore_value: i32,
    pub gem_value: i32,
    /// How far a harvester looks for ore, in cells: near where it is
    /// working (`TiberiumShortScan=`), and when it starts out
    /// (`TiberiumLongScan=`).
    pub tiberium_short_scan: i32,
    pub tiberium_long_scan: i32,
    /// `[General] BaseUnit=`: the unit each side starts a skirmish with,
    /// by side (Allied, Soviet, Yuri).
    pub base_units: Vec<String>,
    /// Minutes to build 1,000 credits' worth (`BuildSpeed=`).
    pub build_speed: Ratio,
    /// The time multiplier per extra factory of a kind
    /// (`MultipleFactory=`, a float).
    pub multiple_factory: Ratio,
    /// Low power: the slowest production goes (`MinLowPowerProductionSpeed=`),
    /// the fastest while short at all (`MaxLowPowerProductionSpeed=`), and
    /// how hard a shortfall bites (`LowPowerPenaltyModifier=`); floats.
    pub min_low_power_speed: Ratio,
    pub max_low_power_speed: Ratio,
    pub low_power_penalty: Ratio,
    /// How much slower walls build (`WallBuildSpeedCoefficient=`).
    pub wall_build_speed: Ratio,
    /// The share of cost a sale pays back (`RefundPercent=`, a float when
    /// applied).
    pub refund_percent: Ratio,
    /// The buildings that satisfy the generic prerequisites `POWER`,
    /// `FACTORY`, `BARRACKS`, `RADAR`, `TECH` and `PROC`
    /// (`Prerequisite<Name>=`).
    pub prerequisite_groups: Vec<(String, Vec<String>)>,
    /// The soldier that comes out of a sold or destroyed building, by side
    /// (`AlliedCrew=`, `SovietCrew=`).
    pub allied_crew: Option<String>,
    pub soviet_crew: Option<String>,
}

/// `[CombatDamage]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatDamage {
    pub ion_cannon_damage: i32,
    pub harvester_immune: bool,
    pub destroyable_bridges: bool,
}

/// `[AudioVisual]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioVisual {
    /// Health fractions at which a techno shows yellow and red.
    pub condition_yellow: Ratio,
    pub condition_red: Ratio,
}

/// The whole rules database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub technos: Vec<TechnoType>,
    pub weapons: Vec<WeaponType>,
    pub warheads: Vec<Warhead>,
    pub projectiles: Vec<Projectile>,
    pub anims: Vec<AnimType>,
    pub super_weapons: Vec<SuperWeaponType>,
    pub particle_systems: Vec<ParticleSystemType>,
    pub countries: Vec<Country>,
    /// `[Sides]`: each side's name and the countries it lists.
    pub sides: Vec<(String, Vec<String>)>,
    pub general: General,
    pub combat_damage: CombatDamage,
    pub audio_visual: AudioVisual,
    by_id: BTreeMap<String, usize>,
}

/// A section's value as an int, else `default`.
fn int(s: Option<&Section>, key: &str, default: i32) -> i32 {
    s.and_then(|s| s.get(key)).map_or(default, atoi)
}

fn flag(s: Option<&Section>, key: &str, default: bool) -> bool {
    s.and_then(|s| s.get(key))
        .and_then(parse_bool)
        .unwrap_or(default)
}

fn text(s: Option<&Section>, key: &str) -> Option<String> {
    s.and_then(|s| s.get(key)).map(str::to_string)
}

fn list(s: Option<&Section>, key: &str) -> Vec<String> {
    s.and_then(|s| s.get(key))
        .map(|v| {
            parse_list(v)
                .into_iter()
                .map(|x| x.trim().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn f32_ratio(s: Option<&Section>, key: &str, default: &str) -> Ratio {
    Ratio::from_value_f32(s.and_then(|s| s.get(key)).unwrap_or(default))
}

fn ratio(s: Option<&Section>, key: &str, default: Ratio) -> Ratio {
    s.and_then(|s| s.get(key))
        .map_or(default, Ratio::from_value)
}

/// The values of a `[Kind]Types`-style list, in order, first time each.
fn type_list(ini: &Ini, section: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(s) = ini.section(section) {
        for (_, v) in s.entries() {
            let v = v.trim();
            if !v.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(v)) {
                out.push(v.to_string());
            }
        }
    }
    out
}

/// Names to table indices, allocated on first mention, as the game's
/// `FindOrAllocate` does.
#[derive(Default)]
struct Names(Vec<String>);

impl Names {
    fn id(&mut self, name: &str) -> u16 {
        let i = self
            .0
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                self.0.push(name.to_string());
                self.0.len() - 1
            });
        u16::try_from(i).expect("fewer than 65,536 types")
    }
}

impl Rules {
    /// Build the tables from `rules` (with any map overrides already loaded
    /// over it) and `art`.
    #[allow(clippy::too_many_lines)]
    pub fn from_ini(rules: &Ini, art: &Ini) -> Rules {
        let mut weapon_names = Names::default();
        let mut warhead_names = Names(type_list(rules, "Warheads"));
        let mut projectile_names = Names::default();
        let mut technos = Vec::new();
        for (list_name, kind) in [
            ("BuildingTypes", Kind::Building),
            ("VehicleTypes", Kind::Vehicle),
            ("InfantryTypes", Kind::Infantry),
            ("AircraftTypes", Kind::Aircraft),
        ] {
            for id in type_list(rules, list_name) {
                let s = rules.section(&id);
                let image = text(s, "Image").unwrap_or_else(|| id.clone());
                let a = art.section(&image);
                let mut weapon = |key: &str| text(s, key).map(|w| WeaponId(weapon_names.id(&w)));
                let foundation = text(a, "Foundation")
                    .and_then(|f| {
                        f.to_ascii_lowercase()
                            .split_once('x')
                            .map(|(w, h)| (atoi(w), atoi(h)))
                    })
                    .unwrap_or((1, 1));
                technos.push(TechnoType {
                    kind,
                    name: text(s, "Name").unwrap_or_default(),
                    strength: int(s, "Strength", 0),
                    armor: s
                        .and_then(|s| s.get("Armor"))
                        .map_or(Armor::None, Armor::parse),
                    tech_level: int(s, "TechLevel", -1),
                    cost: int(s, "Cost", 0),
                    points: int(s, "Points", 0),
                    sight: int(s, "Sight", 0),
                    speed: int(s, "Speed", 0),
                    speed_fraction: s
                        .and_then(|s| s.get("Speed"))
                        .map_or(0, |v| (atoi(v).clamp(0, 100) * 256 / 100).min(255)),
                    rot: int(s, "ROT", 0),
                    accelerates: flag(s, "Accelerates", true),
                    acceleration_factor: ratio(
                        s,
                        "AccelerationFactor",
                        Ratio::from_bits(0x3F9E_B851_EB85_1EB8),
                    ),
                    deacceleration_factor: ratio(
                        s,
                        "DeaccelerationFactor",
                        Ratio::from_bits(0x3F60_624D_D2F1_A9FC),
                    ),
                    slowdown_distance: int(s, "SlowdownDistance", 500),
                    owner: list(s, "Owner"),
                    prerequisite: list(s, "Prerequisite"),
                    primary: weapon("Primary"),
                    secondary: weapon("Secondary"),
                    elite_primary: weapon("ElitePrimary"),
                    elite_secondary: weapon("EliteSecondary"),
                    turret: flag(s, "Turret", false),
                    crusher: flag(s, "Crusher", false),
                    locomotor: text(s, "Locomotor"),
                    power: int(s, "Power", 0),
                    foundation,
                    harvester: flag(s, "Harvester", false),
                    deploys_into: text(s, "DeploysInto"),
                    factory: text(s, "Factory").and_then(|f| {
                        match f.trim().to_ascii_lowercase().as_str() {
                            "buildingtype" => Some(Kind::Building),
                            "unittype" => Some(Kind::Vehicle),
                            "infantrytype" => Some(Kind::Infantry),
                            "aircrafttype" => Some(Kind::Aircraft),
                            _ => None,
                        }
                    }),
                    build_time_multiplier: s
                        .and_then(|s| s.get("BuildTimeMultiplier"))
                        .map_or(Ratio::ONE, Ratio::from_value_f32),
                    wall: flag(s, "Wall", false),
                    soylent: int(s, "Soylent", 0),
                    storage: int(s, "Storage", 0),
                    dock: list(s, "Dock"),
                    queueing_cell: text(a, "QueueingCell")
                        .and_then(|q| q.split_once(',').map(|(x, y)| (atoi(x), atoi(y)))),
                    image,
                    id,
                });
            }
        }
        // Superweapons name weapons too, so they're read before the weapons.
        let super_weapons: Vec<SuperWeaponType> = type_list(rules, "SuperWeaponTypes")
            .into_iter()
            .map(|id| {
                let s = rules.section(&id);
                SuperWeaponType {
                    name: text(s, "Name").unwrap_or_default(),
                    kind: text(s, "Type").unwrap_or_default(),
                    recharge_minutes: ratio(s, "RechargeTime", Ratio::from_value("5")),
                    is_powered: flag(s, "IsPowered", true),
                    weapon: text(s, "WeaponType").map(|w| WeaponId(weapon_names.id(&w))),
                    range: ratio(s, "Range", Ratio::from_value("0")),
                    id,
                }
            })
            .collect();
        // Weapons, then the projectiles and warheads they name.
        let mut weapons = Vec::new();
        let mut i = 0;
        while i < weapon_names.0.len() {
            let id = weapon_names.0[i].clone();
            let s = rules.section(&id);
            weapons.push(WeaponType {
                damage: int(s, "Damage", 0),
                rof: int(s, "ROF", 0),
                range: s.and_then(|s| s.get("Range")).map_or(0, leptons),
                minimum_range: s.and_then(|s| s.get("MinimumRange")).map_or(0, leptons),
                speed: int(s, "Speed", 0),
                burst: int(s, "Burst", 1),
                projectile: text(s, "Projectile").map(|p| ProjectileId(projectile_names.id(&p))),
                warhead: text(s, "Warhead").map(|w| WarheadId(warhead_names.id(&w))),
                id,
            });
            i += 1;
        }
        // Animations: listed in the rules, defined in the art file; their
        // warheads join the warhead table.
        let anims: Vec<AnimType> = type_list(rules, "Animations")
            .into_iter()
            .map(|id| {
                let a = art.section(&id);
                AnimType {
                    rate: int(a, "Rate", 1),
                    loop_start: int(a, "LoopStart", 0),
                    loop_end: int(a, "LoopEnd", 1),
                    loop_count: int(a, "LoopCount", 1),
                    next: text(a, "Next"),
                    damage: ratio(a, "Damage", Ratio::from_value("0")),
                    warhead: text(a, "Warhead").map(|w| WarheadId(warhead_names.id(&w))),
                    translucent: flag(a, "Translucent", false),
                    shadow: flag(a, "Shadow", false),
                    id,
                }
            })
            .collect();
        let particle_systems = type_list(rules, "ParticleSystems")
            .into_iter()
            .map(|id| {
                let s = rules.section(&id);
                ParticleSystemType {
                    holds_what: text(s, "HoldsWhat"),
                    behaves_like: text(s, "BehavesLike").unwrap_or_default(),
                    particle_cap: int(s, "ParticleCap", 0),
                    spawn_frames: int(s, "SparkSpawnFrames", 0),
                    id,
                }
            })
            .collect();
        let warheads = warhead_names
            .0
            .iter()
            .map(|id| {
                let s = rules.section(id);
                let mut verses = [Ratio::ONE; 11];
                for (v, x) in verses.iter_mut().zip(list(s, "Verses")) {
                    *v = Ratio::from_value(&x);
                }
                Warhead {
                    id: id.clone(),
                    verses,
                    cell_spread: s.and_then(|s| s.get("CellSpread")).map_or(0, leptons),
                    percent_at_max: ratio(s, "PercentAtMax", Ratio::ONE),
                    wall: flag(s, "Wall", false),
                    wood: flag(s, "Wood", false),
                    conventional: flag(s, "Conventional", false),
                    inf_death: int(s, "InfDeath", 0),
                    prone_damage: ratio(s, "ProneDamage", Ratio::ONE),
                }
            })
            .collect();
        let projectiles = projectile_names
            .0
            .iter()
            .map(|id| {
                let s = rules.section(id);
                Projectile {
                    id: id.clone(),
                    arcing: flag(s, "Arcing", false),
                    inviso: flag(s, "Inviso", false),
                    anti_air: flag(s, "AA", false),
                    anti_ground: flag(s, "AG", true),
                    rot: int(s, "ROT", 0),
                    subject_to_cliffs: flag(s, "SubjectToCliffs", false),
                    subject_to_walls: flag(s, "SubjectToWalls", false),
                }
            })
            .collect();
        let countries = type_list(rules, "Countries")
            .into_iter()
            .map(|id| {
                let s = rules.section(&id);
                Country {
                    name: text(s, "Name").unwrap_or_default(),
                    side: text(s, "Side").unwrap_or_default(),
                    prefix: text(s, "Prefix").unwrap_or_default(),
                    suffix: text(s, "Suffix").unwrap_or_default(),
                    multiplay: flag(s, "Multiplay", false),
                    id,
                }
            })
            .collect();
        let sides = rules
            .section("Sides")
            .map(|s| {
                s.entries()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            parse_list(v)
                                .into_iter()
                                .map(|x| x.trim().to_string())
                                .collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let c = rules.section("CombatDamage");
        let av = rules.section("AudioVisual");
        let by_id = technos
            .iter()
            .enumerate()
            .map(|(i, t): (usize, &TechnoType)| (t.id.to_ascii_uppercase(), i))
            .collect();
        Rules {
            technos,
            weapons,
            warheads,
            projectiles,
            anims,
            super_weapons,
            particle_systems,
            countries,
            sides,
            general: General {
                ore_value: int(rules.section("Riparius"), "Value", 25),
                gem_value: int(rules.section("Cruentus"), "Value", 50),
                // The game's defaults are 6 and 32 cells. `[recomp 0x00667638]`
                tiberium_short_scan: int(rules.section("General"), "TiberiumShortScan", 6),
                tiberium_long_scan: int(rules.section("General"), "TiberiumLongScan", 32),
                base_units: list(rules.section("General"), "BaseUnit"),
                build_speed: ratio(
                    rules.section("General"),
                    "BuildSpeed",
                    Ratio::from_value("0.7"),
                ),
                multiple_factory: f32_ratio(rules.section("General"), "MultipleFactory", "1"),
                min_low_power_speed: f32_ratio(
                    rules.section("General"),
                    "MinLowPowerProductionSpeed",
                    "0.5",
                ),
                max_low_power_speed: f32_ratio(
                    rules.section("General"),
                    "MaxLowPowerProductionSpeed",
                    "0.9",
                ),
                low_power_penalty: f32_ratio(
                    rules.section("General"),
                    "LowPowerPenaltyModifier",
                    "1",
                ),
                refund_percent: f32_ratio(rules.section("General"), "RefundPercent", "50%"),
                prerequisite_groups: ["POWER", "FACTORY", "BARRACKS", "RADAR", "TECH", "PROC"]
                    .iter()
                    .map(|g| {
                        let key = format!("Prerequisite{}{}", &g[..1], g[1..].to_ascii_lowercase());
                        ((*g).to_string(), list(rules.section("General"), &key))
                    })
                    .collect(),
                allied_crew: text(rules.section("General"), "AlliedCrew"),
                soviet_crew: text(rules.section("General"), "SovietCrew"),
                wall_build_speed: ratio(
                    rules.section("General"),
                    "WallBuildSpeedCoefficient",
                    Ratio::from_value("1"),
                ),
            },
            combat_damage: CombatDamage {
                ion_cannon_damage: int(c, "IonCannonDamage", 0),
                harvester_immune: flag(c, "HarvesterImmune", false),
                destroyable_bridges: flag(c, "DestroyableBridges", true),
            },
            audio_visual: AudioVisual {
                condition_yellow: ratio(av, "ConditionYellow", Ratio(32_768)),
                condition_red: ratio(av, "ConditionRed", Ratio(16_384)),
            },
            by_id,
        }
    }

    /// A techno type by its ID, ignoring case.
    pub fn techno(&self, id: &str) -> Option<&TechnoType> {
        self.by_id
            .get(&id.to_ascii_uppercase())
            .map(|&i| &self.technos[i])
    }

    pub fn weapon(&self, id: WeaponId) -> &WeaponType {
        &self.weapons[usize::from(id.0)]
    }

    pub fn warhead(&self, id: WarheadId) -> &Warhead {
        &self.warheads[usize::from(id.0)]
    }

    pub fn projectile(&self, id: ProjectileId) -> &Projectile {
        &self.projectiles[usize::from(id.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: &[u8] = b"[VehicleTypes]\n1=MTNK\n2=mtnk\n[BuildingTypes]\n1=GAPOWR\n\
        [Warheads]\n1=AP\n\
        [MTNK]\nName=Grizzly\nImage=GTNK\nPrimary=105mm\nElitePrimary=105mmE\nStrength=300\nArmor=heavy\nSpeed=7\nOwner=British,French\n\
        [GAPOWR]\nStrength=750\nArmor=wood\nPower=200\n\
        [105mm]\nDamage=65\nROF=60\nRange=5.75\nProjectile=Cannon\nWarhead=AP\n\
        [105mmE]\nDamage=90\nWarhead=AP\n\
        [Cannon]\nArcing=true\n\
        [AP]\nCellSpread=.3\nPercentAtMax=.5\nVerses=25%,25%,15%,75%,100%,100%,65%,45%,60%,60%,100%\n\
        [AudioVisual]\nConditionYellow=0.5\nConditionRed=0.25\n";
    const ART: &[u8] = b"[GAPOWR]\nFoundation=2x2\n";

    #[test]
    fn types_weapons_and_warheads_link_up() {
        let r = Rules::from_ini(&Ini::parse(RULES), &Ini::parse(ART));
        assert_eq!(r.technos.len(), 2, "a repeated list entry counts once");
        let t = r.techno("mtnk").unwrap();
        assert_eq!(
            (t.kind, t.strength, t.armor, t.speed),
            (Kind::Vehicle, 300, Armor::Heavy, 7)
        );
        assert_eq!(t.image, "GTNK");
        assert_eq!(t.speed_fraction, 17, "7 x 256 / 100");
        assert_eq!(t.owner, ["British", "French"]);
        let w = r.weapon(t.primary.unwrap());
        assert_eq!((w.damage, w.rof, w.range, w.burst), (65, 60, 1472, 1));
        assert!(r.projectile(w.projectile.unwrap()).arcing);
        let wh = r.warhead(w.warhead.unwrap());
        assert_eq!(wh.id, "AP");
        assert_eq!(wh.verses[Armor::Heavy.index()], Ratio::ONE);
        assert_eq!(wh.verses[Armor::Plate.index()].of(100), 15);
        assert_eq!(
            wh.verses[Armor::Light.index()].of(65),
            48,
            "48.75 truncates"
        );
        assert_eq!(wh.cell_spread, 76);
        assert_eq!(wh.percent_at_max, Ratio::from_value("0.5"));
        assert_eq!(r.weapon(t.elite_primary.unwrap()).warhead, w.warhead);
        let p = r.techno("GAPOWR").unwrap();
        assert_eq!(
            (p.kind, p.power, p.foundation),
            (Kind::Building, 200, (2, 2))
        );
        assert_eq!(r.audio_visual.condition_red, Ratio::from_value(".25"));
    }

    /// The integer path agrees with the double multiply and truncation it
    /// stands for, across awkward factors.
    #[test]
    #[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
    fn ratio_matches_double_multiply_and_truncate() {
        let factors = [
            "0.29", "0.07", "15%", "33%", "1.5", "0.1", "2.3", "-0.5", "0.0001", "65%", "1e9",
        ];
        for f in factors {
            let r = Ratio::from_value(f);
            let d = f64::from_bits(r.bits());
            for n in [-1000, -7, -1, 0, 1, 3, 65, 100, 300, 1000, 12345, 1 << 20] {
                let want = d * f64::from(n);
                let want = if want.abs() < 2_147_483_648.0 {
                    want as i32
                } else {
                    i32::MIN
                };
                assert_eq!(r.of(n), want, "{n} x {f}");
            }
        }
        assert_eq!(
            Ratio::from_value("0.29").of(100),
            28,
            "the game's own quirk"
        );
    }

    #[test]
    fn missing_values_take_defaults() {
        let r = Rules::from_ini(
            &Ini::parse(b"[InfantryTypes]\n1=E1\n[E1]\nPrimary=M60\n"),
            &Ini::parse(b""),
        );
        let t = r.techno("E1").unwrap();
        assert_eq!(
            (t.tech_level, t.armor, t.foundation),
            (-1, Armor::None, (1, 1))
        );
        let w = r.weapon(t.primary.unwrap());
        assert_eq!((w.burst, w.warhead, w.projectile), (1, None, None));
    }
}
