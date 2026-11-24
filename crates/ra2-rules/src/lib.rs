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

/// The powerups a crate can hold, in the game's order (a crate's overlay
/// data names one; 19 or more means a pick by `[Powerups]` weight).
/// `[recomp 0x00481ACE]` (the order is Tiberian Sun's; Money first and Unit
/// second are `[observed]`)
pub const POWERUPS: [&str; 19] = [
    "Money",
    "Unit",
    "HealBase",
    "Cloak",
    "Explosion",
    "Napalm",
    "Squad",
    "Darkness",
    "Reveal",
    "Armor",
    "Speed",
    "Firepower",
    "ICBM",
    "Invulnerability",
    "Veteran",
    "IonStorm",
    "Gas",
    "Tiberium",
    "Pod",
];

/// A powerup's index by name, or 255 (a random pick) for `Random`.
pub fn crate_kind(name: &str) -> Option<u8> {
    if name.eq_ignore_ascii_case("random") {
        return Some(255);
    }
    POWERUPS
        .iter()
        .position(|p| p.eq_ignore_ascii_case(name))
        .and_then(|i| u8::try_from(i).ok())
}
pub mod scenario;
pub mod veterancy;
pub mod x87;

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

    /// The game's reading of a number (`INIClass::Get_Double`): `sscanf`
    /// into a single-precision float, widened to a double, then × 0.01 if
    /// it has a `%`. So `.3` is 0.300000012, and `50%` and `.5` both give
    /// a half. `[recomp 0x00528558]`
    pub fn from_value(v: &str) -> Ratio {
        #[allow(clippy::cast_possible_truncation)]
        let f = f64::from(ra2_formats::ini::atof(v) as f32);
        let f = if v.contains('%') { f * 0.01 } else { f };
        Ratio(f.to_bits())
    }

    /// A number the game keeps as a single-precision float: read as
    /// [`Ratio::from_value`] reads it, then rounded to a float.
    pub fn from_value_f32(v: &str) -> Ratio {
        #[allow(clippy::cast_possible_truncation)]
        let f = f64::from_bits(Ratio::from_value(v).0) as f32;
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
    /// Crushed by a `Crusher=` vehicle driving over it (`Crushable=`; yes
    /// for infantry unless set, no for the rest). `[hypothesis]` (the
    /// default)
    pub crushable: bool,
    /// Crushes vehicles too, and drives onto one it's ordered to attack
    /// (`OmniCrusher=`, the Battle Fortress), unless the vehicle is
    /// `OmniCrushResistant=`.
    pub omni_crusher: bool,
    pub omni_crush_resistant: bool,
    /// The tech a spy must have stolen for its house to field it, by side:
    /// Allied, Soviet, Third (`RequiresStolenAlliedTech=`,
    /// `RequiresStolenSovietTech=`, `RequiresStolenThirdTech=`).
    pub requires_stolen: [bool; 3],
    /// Flies as a jumpjet (`JumpJet=`, the Siege Chopper), climbing
    /// `JumpjetClimb=` a frame to `JumpjetHeight=` (by default
    /// `[JumpjetControls]`'s `Climb=` and `CruiseHeight=`).
    pub jumpjet_unit: bool,
    pub jumpjet_climb: i32,
    pub jumpjet_height: i32,
    /// Deploys where it is, without becoming something else
    /// (`IsSimpleDeployer=`).
    pub simple_deployer: bool,
    /// The planes its airstrike flare calls in, how many, and the frames
    /// after they've all gone before it can call again (`AirstrikeTeam=`,
    /// `AirstrikeTeamType=`, `AirstrikeRechargeTime=`, and the `Elite…`
    /// ones; Boris).
    pub airstrike_team: i32,
    pub elite_airstrike_team: i32,
    pub airstrike_team_type: Option<String>,
    pub elite_airstrike_team_type: Option<String>,
    pub airstrike_recharge: i32,
    pub elite_airstrike_recharge: i32,
    /// Buildings: whose tech a spy steals in it, if it's one of
    /// `[AI] BuildTech=` (`AIBasePlanningSide=`: 0 Allied, 1 Soviet, else
    /// Third).
    pub ai_base_planning_side: i32,
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
    /// Heals itself (`SelfHealing=`).
    pub self_healing: bool,
    /// Gains experience from its kills (`Trainable=`; buildings no by
    /// default, others yes).
    pub trainable: bool,
    /// Buildings: repairs units that dock on it (`UnitRepair=`, the
    /// Service Depot).
    pub unit_repair: bool,
    /// `Weapon1=`… and `EliteWeapon1=`… (`WeaponCount=` of them): a
    /// gunner's weapons, by its passenger's `IFVMode=` (the first when
    /// empty). `Gunner=`, `Passengers=`, and an infantry's `IFVMode=`.
    pub weapons: Vec<Option<WeaponId>>,
    pub elite_weapons: Vec<Option<WeaponId>>,
    pub gunner: bool,
    /// A Gattling (`IsGattling=`): its rate climbs `RateUp=` a frame while
    /// it fires and falls `RateDown=` while it doesn't; past each of
    /// `Stage1=`… (`WeaponStages=` of them) it fires the next pair of its
    /// `Weapon<n>`.
    pub gattling: bool,
    pub gattling_stages: Vec<i32>,
    pub rate_up: i32,
    pub rate_down: i32,
    /// Plants C4 on buildings it attacks (`C4=`, Tanya).
    pub c4: bool,
    /// Buildings: copies each infantryman its house trains (`Cloning=`,
    /// the Cloning Vats).
    pub cloning: bool,
    /// Infantry that can garrison a building (`Occupier=`), and how many a
    /// garrisonable building holds (`CanBeOccupied=` and
    /// `MaxNumberOccupants=`; 0 if it can't be).
    pub occupier: bool,
    pub max_occupants: i32,
    /// Infantry that infiltrates enemy buildings (`Infiltrate=`, the spy).
    pub infiltrate: bool,
    /// Infantry that deploys in place and then fires its `Secondary=`
    /// (`Deployer=`, the GI), and deploys by itself to fire when not under
    /// a player's control (`DeployFire=`).
    pub deployer: bool,
    pub deploy_fire: bool,
    /// Moves by warping (`Teleporter=`, the Chrono Legionnaire).
    pub teleporter: bool,
    /// Can show as another unit (`CanDisguise=`, the spy), and sees
    /// through that (`DetectDisguise=`, dogs).
    pub can_disguise: bool,
    pub detect_disguise: bool,
    /// Buildings: a refinery (`Refinery=`).
    pub refinery: bool,
    pub passengers: i32,
    pub ifv_mode: i32,
    /// What goes off when it's destroyed (`DeathWeapon=`), and a factor on
    /// that weapon's damage (`DeathWeaponDamageModifier=`, a float).
    pub death_weapon: Option<WeaponId>,
    /// What it fires from inside a garrisoned building (`OccupyWeapon=`).
    pub occupy_weapon: Option<WeaponId>,
    /// A transport its passengers fire out of (`OpenTopped=`).
    pub open_topped: bool,
    /// Which weapon it fires from an open-topped transport
    /// (`OpenTransportWeapon=`): 0 its primary, 1 its secondary, otherwise
    /// as it would anyway.
    pub open_transport_weapon: i32,
    pub death_weapon_damage_modifier: Ratio,
    /// Crew get out when it's destroyed (`Crewed=`).
    pub crewed: bool,
    /// What being a veteran and elite give it (`VeteranAbilities=`,
    /// `EliteAbilities=`; an elite has both).
    pub veteran_abilities: Vec<String>,
    pub elite_abilities: Vec<String>,
    /// What selling it pays, if set (`Soylent=`) instead of a share of
    /// its cost.
    pub soylent: i32,
    /// Buildings: how far from the house's other buildings it may be
    /// placed (`Adjacent=`, default 3), and whether others may be placed
    /// near it (`BaseNormal=`, default yes). `[recomp 0x0045DEAC]`
    pub adjacent: i32,
    pub base_normal: bool,
    /// Buildings: a unit that comes with it (`FreeUnit=`).
    pub free_unit: Option<String>,
    /// Buildings: where aircraft land on it, from its centre in leptons
    /// (the art's `DockingOffset0=` and on).
    pub docking_offsets: Vec<(i32, i32)>,
    /// Buildings: a defence (`BuildCat=Combat`), built at the country's
    /// defence rate.
    pub combat: bool,
    /// Buildings: the superweapons it gives its house (`SuperWeapon=`,
    /// `SuperWeapon2=`).
    pub super_weapons: Vec<String>,
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
    /// Fired at the ground under its firer (`AreaFire=`, Yuri Prime's
    /// psychic wave).
    pub area_fire: bool,
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
    /// Takes control of what it hits instead of hurting it
    /// (`MindControl=`).
    pub mind_control: bool,
    /// Latches onto what it hits (`Parasite=`): the dog's bite, which kills
    /// infantry outright.
    pub parasite: bool,
    /// Freezes what it hits and erases it from time (`Temporal=`, the
    /// `ChronoBeam`).
    pub temporal: bool,
    /// Calls in its firer's airstrike on what it hits (`Airstrike=`, Boris's
    /// flare).
    pub airstrike: bool,
    /// Takes over how what it hits moves instead of hurting it
    /// (`IsLocomotor=`, the Magnetron's beam).
    pub is_locomotor: bool,
    /// Hurts its firer's own house too (`AffectsAllies=`, by default yes).
    pub affects_allies: bool,
}

/// `[JumpjetControls]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jumpjet {
    pub turn_rate: i32,
    /// Leptons a frame.
    pub speed: i32,
    pub climb: i32,
    pub cruise_height: i32,
    pub acceleration: i32,
    pub wobbles_per_second: Ratio,
    pub wobble_deviation: i32,
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
    /// Build-time multipliers (floats) for infantry, units, aircraft,
    /// buildings and defences (`BuildTimeInfantryMult=` and so on).
    /// `[recomp 0x0050C0A0]`
    pub build_time_mults: [Ratio; 5],
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
    /// Aircraft's height in flight, in leptons (`FlightLevel=`).
    pub flight_level: i32,
    /// Repair: minutes between steps (`RepairRate=`), hit points a step
    /// (`RepairStep=`), and the share of a building's cost a full repair
    /// costs (`RepairPercent=`).
    pub repair_rate: Ratio,
    pub repair_step: i32,
    pub repair_percent: Ratio,
    /// Minutes between a Service Depot's repair steps (`URepairRate=`).
    pub unit_repair_rate: Ratio,
    /// Frames a spy in a power plant cuts its owner's power for
    /// (`SpyPowerBlackout=`), and the share of a house's money a spy in
    /// its refinery takes (`SpyMoneyStealPercent=`).
    pub spy_power_blackout: i32,
    /// Frames a computer's deployer (the GI) stands on Guard before it
    /// deploys by itself, by difficulty: hard, medium, easy
    /// (`AIAutoDeployFrameDelay=`).
    pub ai_auto_deploy_delay: Vec<i32>,
    /// The way a simple deployer (the Siege Chopper) faces to deploy, in
    /// eighths of a turn from north (`DeployDir=`).
    pub deploy_dir: i32,
    /// The battle labs, whose tech a spy steals (`[AI] BuildTech=`).
    pub build_tech: Vec<String>,
    /// A teleporter's wait after a warp: the distance in leptons over
    /// `ChronoDistanceFactor=`, at least `ChronoMinimumDelay=` frames.
    pub chrono_distance_factor: i32,
    pub chrono_minimum_delay: i32,
    pub spy_money_steal: Ratio,
    /// What a spy shows as before it takes a disguise, by side
    /// (`AlliedDisguise=`, `SovietDisguise=`, `ThirdDisguise=`).
    pub disguises: [String; 3],
    /// Veterancy: kills worth this many times a unit's cost make it a
    /// veteran, and twice that elite (`VeteranRatio=`); experience stops at
    /// `VeteranCap=`; and what being a veteran does to damage dealt
    /// (`VeteranCombat=`), reload (`VeteranROF=`), damage taken
    /// (`VeteranArmor=`) and speed (`VeteranSpeed=`).
    pub veteran_ratio: Ratio,
    pub veteran_cap: Ratio,
    pub veteran_combat: Ratio,
    pub veteran_rof: Ratio,
    pub veteran_armor: Ratio,
    pub veteran_speed: Ratio,
    /// The Nuke's blast (`[NukePayload]`).
    pub nuke_payload: Option<WeaponId>,
    /// What goes off when a unit without its own death weapon or a weapon
    /// of its own is destroyed (`[CombatDamage] DeathWeapon=`,
    /// `DefaultDeathWeapon`).
    pub default_death_weapon: Option<WeaponId>,
    /// Crew types by side (`AlliedCrew=`, `SovietCrew=`, `ThirdCrew=`), a
    /// destroyed vehicle's chance of letting its crew out (`CrewEscape=`),
    /// and the share of a destroyed building's refund each survivor stands
    /// for (`AlliedSurvivorDivisor=` and the rest).
    pub crews: [String; 3],
    pub crew_escape: Ratio,
    pub survivor_divisors: [i32; 3],
    /// The Genetic Mutator's warhead (`[SpecialWeapons]
    /// MutateExplosionWarhead=`), and the
    /// infantry it turns its victims into (the death animation's
    /// `AnimToInfantry=`, `BRUTE`).
    pub mutate_warhead: Option<WarheadId>,
    /// The Psychic Dominator: its warhead and damage (`DominatorWarhead=`,
    /// `DominatorDamage=`), and how near its target units are captured
    /// (`DominatorCaptureRange=`, cells).
    pub dominator_warhead: Option<WarheadId>,
    pub dominator_damage: i32,
    pub dominator_capture_range: i32,
    /// The Weather Storm: its warhead and damage (`LightningWarhead=`,
    /// `LightningDamage=`), how long before it starts and how long it lasts
    /// (`LightningDeferment=`, `LightningStormDuration=`), and how often it
    /// strikes its target (`LightningHitDelay=`), in frames.
    pub lightning_warhead: Option<WarheadId>,
    pub lightning_damage: i32,
    pub lightning_deferment: i32,
    pub lightning_duration: i32,
    pub lightning_hit_delay: i32,
    /// Paratroopers: who each side's planes drop, and how many of each
    /// (`AmerParaDropInf=`/`Num=`, `AllyParaDrop…`, `SovParaDrop…`,
    /// `YuriParaDrop…`), and how near the target the plane starts dropping
    /// (`ParadropRadius=`, leptons).
    pub para_drops: Vec<(String, Vec<(String, i32)>)>,
    pub paradrop_radius: i32,
    /// How long the Iron Curtain makes things invulnerable, in frames
    /// (`[CombatDamage] IronCurtainDuration=`).
    pub iron_curtain_duration: i32,
    /// The Force Shield: how far round its target buildings are shielded
    /// (cells), for how long, and how long its house's power is out
    /// (frames). `ForceShieldRadius=`, `ForceShieldDuration=`,
    /// `ForceShieldBlackoutDuration=`.
    pub force_shield_radius: i32,
    pub force_shield_duration: i32,
    pub force_shield_blackout: i32,
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
    /// Minutes from planting C4 to the building's end (`C4Delay=`).
    pub c4_delay: Ratio,
    /// A garrison's damage and rate of fire (`OccupyDamageMultiplier=`,
    /// `OccupyROFMultiplier=`).
    pub occupy_damage: Ratio,
    pub occupy_rof: Ratio,
    /// A garrison's range in cells, before its footprint's bonus
    /// (`OccupyWeaponRange=`).
    pub occupy_weapon_range: i32,
    /// Passengers firing out of an open-topped transport: their damage
    /// and extra range in cells (`OpenToppedDamageMultiplier=`,
    /// `OpenToppedRangeBonus=`).
    pub open_topped_damage: Ratio,
    pub open_topped_range_bonus: i32,
    pub harvester_immune: bool,
    pub destroyable_bridges: bool,
}

/// One `[Powerups]` entry: `weight,anim,water,data` (data the powerup's
/// number: money, a multiplier, veterancy levels…). `[community]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Powerup {
    pub weight: i32,
    pub data: Ratio,
}

/// `[Powerups]`, in [`POWERUPS`] order, and `[CrateRules]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crates {
    pub powerups: Vec<Powerup>,
    /// Leptons from the crate in which area powerups (veterancy, speed,
    /// armor, firepower) reach (`CrateRadius=`, cells).
    pub radius: i32,
    /// A unit crate's unit (`UnitCrateType=`; `none`, a random one).
    pub unit_type: Option<String>,
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
    /// `[JumpjetControls]`: jumpjets' defaults, and how the Magnetron's
    /// beam carries what it lifts.
    pub jumpjet: Jumpjet,
    pub audio_visual: AudioVisual,
    pub crates: Crates,
    by_id: BTreeMap<String, usize>,
}

/// `[Powerups]` and `[CrateRules]`.
fn crates(rules: &Ini) -> Crates {
    let p = rules.section("Powerups");
    let powerups = POWERUPS
        .iter()
        .map(|name| {
            let fields: Vec<String> = p
                .and_then(|s| s.get(name))
                .map(|v| v.split(',').map(|f| f.trim().to_string()).collect())
                .unwrap_or_default();
            Powerup {
                weight: fields.first().map_or(0, |w| atoi(w)),
                data: fields
                    .get(3)
                    .filter(|d| !d.is_empty())
                    .map_or(Ratio::ONE, |d| Ratio::from_value(d)),
            }
        })
        .collect();
    let c = rules.section("CrateRules");
    Crates {
        powerups,
        radius: ratio(c, "CrateRadius", Ratio::from_value("3.0")).of(256),
        unit_type: text(c, "UnitCrateType").filter(|u| !u.eq_ignore_ascii_case("none")),
    }
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
                    death_weapon: weapon("DeathWeapon"),
                    occupy_weapon: weapon("OccupyWeapon"),
                    open_topped: flag(s, "OpenTopped", false),
                    open_transport_weapon: int(s, "OpenTransportWeapon", -1),
                    weapons: (1..=int(s, "WeaponCount", 0).clamp(0, 32))
                        .map(|n| weapon(&format!("Weapon{n}")))
                        .collect(),
                    elite_weapons: (1..=int(s, "WeaponCount", 0).clamp(0, 32))
                        .map(|n| weapon(&format!("EliteWeapon{n}")))
                        .collect(),
                    gunner: flag(s, "Gunner", false),
                    gattling: flag(s, "IsGattling", false),
                    gattling_stages: (1..=int(s, "WeaponStages", 0).clamp(0, 8))
                        .map(|n| int(s, &format!("Stage{n}"), 0))
                        .collect(),
                    rate_up: int(s, "RateUp", 0),
                    rate_down: int(s, "RateDown", 0),
                    c4: flag(s, "C4", false),
                    cloning: flag(s, "Cloning", false),
                    occupier: flag(s, "Occupier", false),
                    infiltrate: flag(s, "Infiltrate", false),
                    deployer: flag(s, "Deployer", false),
                    teleporter: flag(s, "Teleporter", false),
                    deploy_fire: flag(s, "DeployFire", false),
                    can_disguise: flag(s, "CanDisguise", false),
                    detect_disguise: flag(s, "DetectDisguise", false),
                    refinery: flag(s, "Refinery", false),
                    max_occupants: if flag(s, "CanBeOccupied", false) {
                        int(s, "MaxNumberOccupants", 0)
                    } else {
                        0
                    },
                    passengers: int(s, "Passengers", 0),
                    ifv_mode: int(s, "IFVMode", 0),
                    death_weapon_damage_modifier: s
                        .and_then(|s| s.get("DeathWeaponDamageModifier"))
                        .map_or(Ratio::ONE, Ratio::from_value_f32),
                    crewed: flag(s, "Crewed", false),
                    elite_secondary: weapon("EliteSecondary"),
                    turret: flag(s, "Turret", false),
                    crusher: flag(s, "Crusher", false),
                    crushable: flag(s, "Crushable", kind == Kind::Infantry),
                    omni_crusher: flag(s, "OmniCrusher", false),
                    omni_crush_resistant: flag(s, "OmniCrushResistant", false),
                    requires_stolen: [
                        flag(s, "RequiresStolenAlliedTech", false),
                        flag(s, "RequiresStolenSovietTech", false),
                        flag(s, "RequiresStolenThirdTech", false),
                    ],
                    ai_base_planning_side: int(s, "AIBasePlanningSide", 0),
                    jumpjet_unit: flag(s, "JumpJet", false),
                    jumpjet_climb: int(
                        s,
                        "JumpjetClimb",
                        int(rules.section("JumpjetControls"), "Climb", 5),
                    ),
                    jumpjet_height: int(
                        s,
                        "JumpjetHeight",
                        int(rules.section("JumpjetControls"), "CruiseHeight", 500),
                    ),
                    simple_deployer: flag(s, "IsSimpleDeployer", false),
                    airstrike_team: int(s, "AirstrikeTeam", 0),
                    elite_airstrike_team: int(s, "EliteAirstrikeTeam", 0),
                    airstrike_team_type: text(s, "AirstrikeTeamType"),
                    elite_airstrike_team_type: text(s, "EliteAirstrikeTeamType"),
                    airstrike_recharge: int(s, "AirstrikeRechargeTime", 0),
                    elite_airstrike_recharge: int(s, "EliteAirstrikeRechargeTime", 0),
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
                    self_healing: flag(s, "SelfHealing", false),
                    trainable: flag(s, "Trainable", kind != Kind::Building),
                    unit_repair: flag(s, "UnitRepair", false),
                    veteran_abilities: list(s, "VeteranAbilities"),
                    elite_abilities: list(s, "EliteAbilities"),
                    soylent: int(s, "Soylent", 0),
                    adjacent: int(s, "Adjacent", 3),
                    base_normal: flag(s, "BaseNormal", true),
                    free_unit: text(s, "FreeUnit"),
                    docking_offsets: (0..)
                        .map_while(|n| text(a, &format!("DockingOffset{n}")))
                        .map(|v| {
                            let p = parse_list(&v);
                            let at = |i: usize| p.get(i).map_or(0, |s| atoi(s));
                            (at(0), at(1))
                        })
                        .collect(),
                    super_weapons: ["SuperWeapon", "SuperWeapon2"]
                        .iter()
                        .filter_map(|k| text(s, k))
                        .filter(|v| !v.eq_ignore_ascii_case("none"))
                        .collect(),
                    combat: text(s, "BuildCat")
                        .is_some_and(|c| c.trim().eq_ignore_ascii_case("Combat")),
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
        // The nuke's blast, which the game finds by name.
        // `[recomp]` (the string `NukePayload` in `gamemd.exe`)
        let nuke_payload = WeaponId(weapon_names.id("NukePayload"));
        let default_death_weapon = WeaponId(
            weapon_names.id(&text(rules.section("CombatDamage"), "DeathWeapon")
                .unwrap_or_else(|| "DefaultDeathWeapon".into())),
        );
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
                area_fire: flag(s, "AreaFire", false),
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
        // Superweapon warheads the game finds through `[General]`.
        let mutate_warhead = text(rules.section("SpecialWeapons"), "MutateExplosionWarhead")
            .map(|w| WarheadId(warhead_names.id(&w)));
        let dominator_warhead = text(rules.section("General"), "DominatorWarhead")
            .map(|w| WarheadId(warhead_names.id(&w)));
        let lightning_warhead = text(rules.section("General"), "LightningWarhead")
            .map(|w| WarheadId(warhead_names.id(&w)));
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
                    mind_control: flag(s, "MindControl", false),
                    parasite: flag(s, "Parasite", false),
                    temporal: flag(s, "Temporal", false),
                    airstrike: flag(s, "Airstrike", false),
                    is_locomotor: flag(s, "IsLocomotor", false),
                    affects_allies: flag(s, "AffectsAllies", true),
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
                    build_time_mults: [
                        "BuildTimeInfantryMult",
                        "BuildTimeUnitsMult",
                        "BuildTimeAircraftMult",
                        "BuildTimeBuildingsMult",
                        "BuildTimeDefensesMult",
                    ]
                    .map(|k| f32_ratio(s, k, "1")),
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
                flight_level: int(rules.section("General"), "FlightLevel", 1500),
                repair_rate: ratio(
                    rules.section("General"),
                    "RepairRate",
                    Ratio::from_value(".016"),
                ),
                repair_step: int(rules.section("General"), "RepairStep", 8),
                repair_percent: ratio(
                    rules.section("General"),
                    "RepairPercent",
                    Ratio::from_value("15%"),
                ),
                spy_power_blackout: int(rules.section("General"), "SpyPowerBlackout", 1000),
                ai_auto_deploy_delay: list(rules.section("General"), "AIAutoDeployFrameDelay")
                    .iter()
                    .filter_map(|v| v.trim().parse().ok())
                    .collect(),
                build_tech: list(rules.section("AI"), "BuildTech"),
                deploy_dir: int(rules.section("General"), "DeployDir", 2),
                chrono_distance_factor: int(rules.section("General"), "ChronoDistanceFactor", 32),
                chrono_minimum_delay: int(rules.section("General"), "ChronoMinimumDelay", 0),
                spy_money_steal: ratio(
                    rules.section("General"),
                    "SpyMoneyStealPercent",
                    Ratio::from_value(".5"),
                ),
                disguises: [
                    text(rules.section("General"), "AlliedDisguise").unwrap_or_else(|| "E1".into()),
                    text(rules.section("General"), "SovietDisguise").unwrap_or_else(|| "E2".into()),
                    text(rules.section("General"), "ThirdDisguise")
                        .unwrap_or_else(|| "INIT".into()),
                ],
                unit_repair_rate: ratio(
                    rules.section("General"),
                    "URepairRate",
                    Ratio::from_value(".016"),
                ),
                veteran_ratio: ratio(
                    rules.section("General"),
                    "VeteranRatio",
                    Ratio::from_value("3.0"),
                ),
                veteran_cap: ratio(
                    rules.section("General"),
                    "VeteranCap",
                    Ratio::from_value("2"),
                ),
                veteran_combat: ratio(
                    rules.section("General"),
                    "VeteranCombat",
                    Ratio::from_value("1.1"),
                ),
                veteran_rof: ratio(
                    rules.section("General"),
                    "VeteranROF",
                    Ratio::from_value("0.6"),
                ),
                veteran_armor: ratio(
                    rules.section("General"),
                    "VeteranArmor",
                    Ratio::from_value("1.5"),
                ),
                veteran_speed: ratio(
                    rules.section("General"),
                    "VeteranSpeed",
                    Ratio::from_value("1.2"),
                ),
                nuke_payload: Some(nuke_payload),
                default_death_weapon: Some(default_death_weapon),
                crews: [
                    text(rules.section("General"), "AlliedCrew").unwrap_or_else(|| "E1".into()),
                    text(rules.section("General"), "SovietCrew").unwrap_or_else(|| "E2".into()),
                    text(rules.section("General"), "ThirdCrew").unwrap_or_else(|| "INIT".into()),
                ],
                crew_escape: ratio(
                    rules.section("General"),
                    "CrewEscape",
                    Ratio::from_value("50%"),
                ),
                survivor_divisors: [
                    int(rules.section("General"), "AlliedSurvivorDivisor", 500),
                    int(rules.section("General"), "SovietSurvivorDivisor", 250),
                    int(rules.section("General"), "ThirdSurvivorDivisor", 750),
                ],
                mutate_warhead,
                dominator_warhead,
                lightning_warhead,
                lightning_damage: int(rules.section("General"), "LightningDamage", 250),
                lightning_deferment: int(rules.section("General"), "LightningDeferment", 250),
                lightning_duration: int(rules.section("General"), "LightningStormDuration", 180),
                lightning_hit_delay: int(rules.section("General"), "LightningHitDelay", 10),
                para_drops: ["Amer", "Ally", "Sov", "Yuri"]
                    .iter()
                    .map(|p| {
                        let g = rules.section("General");
                        let inf = list(g, &format!("{p}ParaDropInf"));
                        let num = list(g, &format!("{p}ParaDropNum"));
                        let each = inf
                            .into_iter()
                            .zip(num.iter().map(|n| atoi(n)).chain(std::iter::repeat(1)))
                            .collect();
                        ((*p).to_string(), each)
                    })
                    .collect(),
                paradrop_radius: int(rules.section("General"), "ParadropRadius", 1024),
                dominator_damage: int(rules.section("General"), "DominatorDamage", 1000),
                dominator_capture_range: int(rules.section("General"), "DominatorCaptureRange", 1),
                iron_curtain_duration: int(c, "IronCurtainDuration", 750),
                force_shield_radius: int(rules.section("General"), "ForceShieldRadius", 4),
                force_shield_duration: int(rules.section("General"), "ForceShieldDuration", 500),
                force_shield_blackout: int(
                    rules.section("General"),
                    "ForceShieldBlackoutDuration",
                    1000,
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
            jumpjet: {
                let j = rules.section("JumpjetControls");
                Jumpjet {
                    turn_rate: int(j, "TurnRate", 4),
                    speed: int(j, "Speed", 14),
                    climb: int(j, "Climb", 5),
                    cruise_height: int(j, "CruiseHeight", 500),
                    acceleration: int(j, "Acceleration", 2),
                    wobbles_per_second: ratio(j, "WobblesPerSecond", Ratio::from_value(".15")),
                    wobble_deviation: int(j, "WobbleDeviation", 40),
                }
            },
            combat_damage: CombatDamage {
                ion_cannon_damage: int(c, "IonCannonDamage", 0),
                harvester_immune: flag(c, "HarvesterImmune", false),
                c4_delay: ratio(c, "C4Delay", Ratio::from_value(".03")),
                occupy_damage: ratio(c, "OccupyDamageMultiplier", Ratio::from_value("1.2")),
                occupy_rof: ratio(c, "OccupyROFMultiplier", Ratio::from_value("1.2")),
                occupy_weapon_range: int(c, "OccupyWeaponRange", 5),
                open_topped_damage: ratio(
                    c,
                    "OpenToppedDamageMultiplier",
                    Ratio::from_value("1.2"),
                ),
                open_topped_range_bonus: int(c, "OpenToppedRangeBonus", 0),
                destroyable_bridges: flag(c, "DestroyableBridges", true),
            },
            audio_visual: AudioVisual {
                condition_yellow: ratio(av, "ConditionYellow", Ratio(32_768)),
                condition_red: ratio(av, "ConditionRed", Ratio(16_384)),
            },
            crates: crates(rules),
            by_id,
        }
    }

    /// A techno type by its ID, ignoring case.
    /// A techno type's index, by its ID.
    pub fn techno_index(&self, id: &str) -> Option<usize> {
        self.by_id.get(&id.to_ascii_uppercase()).copied()
    }

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
