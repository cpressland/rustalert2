//! Houses and objects: what the simulation holds. See
//! `docs/spec/objects-model.md`.

use ra2_rules::scenario::Placement;
use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL, LEPTONS_PER_LEVEL};
use crate::tick::StateHasher;

/// A player, AI, or neutral house.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct House {
    /// Its place in the house list.
    pub index: u32,
    /// The house's name: a country for players, else the map's house
    /// (`Neutral`, `Special`).
    pub country: String,
    pub credits: i32,
    /// Power produced and drawn by its buildings.
    pub power_output: i32,
    pub power_drain: i32,
    /// A blackout (the Force Shield's): no power made from the first frame
    /// to before the second.
    pub blackout: Option<(u32, u32)>,
    /// What spies brought back: the kinds of unit it now builds as
    /// veterans (a barracks' infiltrated: infantry).
    pub veteran_kinds: Vec<ra2_rules::Kind>,
}

/// What an object is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mission {
    #[default]
    Guard,
    Sleep,
    Hunt,
    AreaGuard,
    Harvest,
    Other,
}

impl Mission {
    pub fn parse(v: &str) -> Mission {
        match v.trim().to_ascii_lowercase().as_str() {
            "sleep" => Mission::Sleep,
            "hunt" => Mission::Hunt,
            "area guard" => Mission::AreaGuard,
            "harvest" => Mission::Harvest,
            "guard" | "none" | "" => Mission::Guard,
            _ => Mission::Other,
        }
    }
}

/// A building, vehicle, infantryman or aircraft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// Stable for the object's life; allocated in creation order.
    pub id: u32,
    pub kind: Kind,
    /// Its type: an index into `Rules::technos`.
    pub techno: usize,
    /// The owning house's index.
    pub owner: u32,
    /// Its centre, in leptons.
    pub coord: Coord,
    pub health: i32,
    /// 0–255, as maps give it.
    pub facing: u8,
    /// The body's facing as the game keeps it: 16 bits, 0 north (−y),
    /// clockwise.
    pub dir: u16,
    pub mission: Mission,
    /// Where it's going and its weapon's reload.
    pub motion: crate::hunt::Motion,
    /// Bails of ore or gems it carries, and what they're worth.
    pub cargo: i32,
    pub cargo_value: i32,
    /// Where a harvester is in its round.
    pub harvest: crate::harvest::Harvest,
    /// Not on the map: being built, or finished and waiting to be placed.
    pub limbo: bool,
    /// The frame a building was placed on (power output counts from the
    /// next).
    pub placed: u32,
    /// The frame it was sold on, while it's being sold.
    pub sold_at: Option<u32>,
    /// Shielded (the Force Shield) or iron-curtained until this frame: it
    /// takes no damage.
    pub shielded_until: u32,
    /// Killed, and taken away on this frame (infantry linger as they die).
    pub dies_at: Option<u32>,
    /// A building its owner has ordered repaired.
    pub repairing: bool,
    /// Experience, the bits of the float the game keeps (`TechnoClass
    /// +0x150`): 1 makes a veteran, 2 elite. See `Object::experience`.
    pub experience_bits: u32,
    /// A player's order it's carrying out.
    pub command: Option<crate::command::Command>,
    /// Its stay on a Service Depot.
    pub depot: Option<crate::command::DepotVisit>,
    /// What crates gave it: × its speed, ÷ damage it takes, × damage it
    /// deals.
    pub speed_bonus: ra2_rules::Ratio,
    pub armor_bonus: ra2_rules::Ratio,
    pub firepower_bonus: ra2_rules::Ratio,
    /// Its turret's facing, 16 bits; `None` until it first turns (then
    /// it starts from the body's).
    pub turret: Option<u16>,
    /// The objects inside it (in limbo), first in first.
    pub passengers: Vec<u32>,
    /// A garrison's last occupant to fire, by index into `passengers`.
    pub firing_occupant: usize,
    /// A building with C4 on it: the frame it's destroyed.
    pub c4_at: Option<u32>,
    /// Under mind control: by whom, and the house it belonged to.
    pub controlled_by: Option<(u32, u32)>,
    /// A Gattling's rate (see `TechnoType::gattling`).
    pub gattling_rate: i32,
    /// What it shows as to other houses, and as whose: a spy's disguise.
    pub disguise: Option<(usize, u32)>,
    /// Deployed infantry (a GI behind sandbags), and the shots it has fired
    /// on Guard (a computer's GI deploys after its third).
    pub deployed: bool,
    pub guard_shots: u32,
    /// Being erased by a Chrono beam: by whom, and what's left of it (the
    /// target's strength × 10 to begin with).
    pub erasing: Option<(u32, i32)>,
}

impl World {
    /// A house's side: 0 Allied, 1 Soviet, 2 the third (by its country's
    /// `Side=`: `Nod` is the Soviets).
    pub fn side_of(&self, rules: &Rules, house: u32) -> usize {
        let Some(h) = self
            .houses
            .get(usize::try_from(house).unwrap_or(usize::MAX))
        else {
            return 0;
        };
        rules
            .countries
            .iter()
            .find(|c| c.id.eq_ignore_ascii_case(&h.country))
            .map_or(0, |c| match c.side.to_ascii_lowercase().as_str() {
                "nod" => 1,
                "thirdside" => 2,
                _ => 0,
            })
    }

    /// A spy's look before it takes a disguise: its side's default
    /// infantry (`AlliedDisguise=`…), as its own house. `[observed]` (a
    /// spy logged as E1 of its own house from the start)
    pub(crate) fn default_disguise(
        &self,
        rules: &Rules,
        techno: usize,
        owner: u32,
    ) -> Option<(usize, u32)> {
        if !rules.technos[techno].can_disguise {
            return None;
        }
        let name = &rules.general.disguises[self.side_of(rules, owner)];
        rules
            .technos
            .iter()
            .position(|t| t.id.eq_ignore_ascii_case(name))
            .map(|k| (k, owner))
    }
}

impl Object {
    /// 0 a rookie, 1 a veteran, 2 elite: experience of 1 or more, and 2
    /// or more. Positive floats order as their bits do. `[recomp
    /// 0x0074FF90]`
    pub fn rank(&self) -> u8 {
        const ONE: u32 = 0x3F80_0000;
        const TWO: u32 = 0x4000_0000;
        match self.experience_bits {
            b if b >> 31 == 1 => 0,
            b if b >= TWO => 2,
            b if b >= ONE => 1,
            _ => 0,
        }
    }
}

/// Something on its way down: a superweapon's missile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strike {
    pub lands: u32,
    pub cell: (i16, i16),
    /// Its damage and warhead.
    pub damage: i32,
    pub warhead: ra2_rules::WarheadId,
}

/// Ore or gems on a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ore {
    /// The overlay's index in `[OverlayTypes]`.
    pub overlay: u8,
    /// Its data: bails left, less one.
    pub data: u8,
}

impl Ore {
    /// Whether an overlay is ore or gems: the ore and gem overlays of
    /// `[OverlayTypes]` (`GEM01`–`GEM12`, `TIB01`–`TIB20`, `TIB2_01`–`TIB3_20`).
    pub fn is_ore(overlay: u8) -> bool {
        matches!(overlay, 27..=38 | 102..=121 | 127..=166)
    }

    /// Credits a bail of it is worth.
    pub fn bail_value(self, rules: &Rules) -> i32 {
        if (27..=38).contains(&self.overlay) {
            rules.general.gem_value
        } else {
            rules.general.ore_value
        }
    }

    /// What a harvester looking for ore weighs it at: a bail's value times
    /// its data plus one. `[recomp 0x00485020]`
    pub fn worth(self, rules: &Rules) -> i32 {
        self.bail_value(rules) * (i32::from(self.data) + 1)
    }
}

/// A shell or bullet on its way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    /// Who fired it, to credit the kill.
    pub shooter: u32,
    /// Its damage, after the shooter's veterancy.
    pub damage: i32,
    pub target: u32,
    pub weapon: ra2_rules::WeaponId,
    /// The frame it hits on.
    pub lands: u32,
}

/// Where infantry stand in a cell, by sub-cell: the centre, then the four
/// quarters. `[hypothesis]` (sub-cell 0 at the centre is `[observed]`; the
/// quarters follow the published C&C source's stopping spots)
const SUB_CELLS: [(i32, i32); 5] = [(128, 128), (64, 64), (192, 64), (64, 192), (192, 192)];

/// Every house and object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct World {
    pub houses: Vec<House>,
    pub objects: Vec<Object>,
    pub shots: Vec<Shot>,
    /// The map's cells, for route planning. Empty means unknown: routes
    /// then keep to 32 cells round the unit.
    pub cells: std::collections::BTreeSet<(i16, i16)>,
    /// Ore and gems, by cell.
    pub ore: std::collections::BTreeMap<(i16, i16), Ore>,
    /// Wrecks whose death blast is still to go off.
    pub wrecks: Vec<crate::death::Wreck>,
    /// Crates by cell: the powerup each holds (see `crates`).
    pub crates: std::collections::BTreeMap<(i16, i16), u8>,
    /// What each house is building.
    pub factories: Vec<crate::production::Factory>,
    /// Units unpacking into buildings: the unit and the frame it's done.
    pub deploying: Vec<(u32, u32)>,
    /// Every house's superweapons.
    pub supers: Vec<crate::superweapon::Super>,
    /// Superweapon strikes on their way.
    pub strikes: Vec<Strike>,
    /// Units on their way through the Chronosphere: the unit, the frame it
    /// arrives, and where.
    pub warps: Vec<(u32, u32, Coord)>,
    /// Superweapon effects still to come.
    pub later: Vec<crate::superweapon::Later>,
    next_id: u32,
}

impl World {
    /// Add a house; returns its index.
    pub fn add_house(&mut self, country: &str, credits: i32) -> u32 {
        let index = u32::try_from(self.houses.len()).expect("few houses");
        self.houses.push(House {
            index,
            country: country.to_string(),
            credits,
            power_output: 0,
            power_drain: 0,
            blackout: None,
            veteran_kinds: Vec::new(),
        });
        index
    }

    /// A house by name, ignoring case.
    pub fn house(&self, name: &str) -> Option<u32> {
        self.houses
            .iter()
            .find(|h| h.country.eq_ignore_ascii_case(name))
            .map(|h| h.index)
    }

    /// Create an object from a map placement, standing on ground at
    /// `level`. `None` if its type or owner is unknown.
    ///
    /// - Health is `Strength × health / 256`, truncated, at least 1.
    ///   `[hypothesis]`
    /// - Units and aircraft stand at their cell's centre, infantry at their
    ///   sub-cell, buildings at the centre of their foundation. `[observed]`
    ///   (units and sub-cell 0 in parity logs)
    pub fn place(&mut self, rules: &Rules, p: &Placement, level: i32) -> Option<u32> {
        let techno = rules
            .technos
            .iter()
            .position(|t| t.id.eq_ignore_ascii_case(&p.type_id))?;
        let t = &rules.technos[techno];
        let owner = self.house(&p.owner)?;
        let (cx, cy) = (
            i32::from(p.cell.0) * LEPTONS_PER_CELL,
            i32::from(p.cell.1) * LEPTONS_PER_CELL,
        );
        let (dx, dy) = match t.kind {
            Kind::Infantry => SUB_CELLS[usize::from(p.sub_cell.min(4))],
            Kind::Building => (
                t.foundation.0 * LEPTONS_PER_CELL / 2,
                t.foundation.1 * LEPTONS_PER_CELL / 2,
            ),
            _ => (LEPTONS_PER_CELL / 2, LEPTONS_PER_CELL / 2),
        };
        let health = (i64::from(t.strength) * i64::from(p.health.clamp(0, 256)) / 256).max(1);
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(Object {
            id,
            kind: t.kind,
            techno,
            owner,
            coord: Coord {
                x: cx + dx,
                y: cy + dy,
                z: level * LEPTONS_PER_LEVEL,
            },
            health: i32::try_from(health).unwrap_or(i32::MAX),
            facing: p.facing,
            dir: u16::from(p.facing) << 8,
            mission: Mission::parse(&p.mission),
            motion: crate::hunt::Motion::default(),
            cargo: 0,
            cargo_value: 0,
            harvest: crate::harvest::Harvest::default(),
            limbo: false,
            placed: 0,
            sold_at: None,
            shielded_until: 0,
            dies_at: None,
            repairing: false,
            experience_bits: ra2_rules::veterancy::from_map(p.veterancy),
            command: None,
            depot: None,
            speed_bonus: ra2_rules::Ratio::ONE,
            armor_bonus: ra2_rules::Ratio::ONE,
            firepower_bonus: ra2_rules::Ratio::ONE,
            turret: None,
            passengers: Vec::new(),
            firing_occupant: 0,
            c4_at: None,
            controlled_by: None,
            gattling_rate: 0,
            disguise: None,
            deployed: false,
            guard_shots: 0,
            erasing: None,
        });
        let look = self.default_disguise(rules, techno, owner);
        if let Some(o) = self.objects.last_mut() {
            o.disguise = look;
        }
        Some(id)
    }

    /// A new object of type `techno` for house `owner`, at full health.
    pub fn spawn(&mut self, rules: &Rules, techno: usize, owner: u32, coord: Coord) -> u32 {
        let t = &rules.technos[techno];
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(Object {
            id,
            kind: t.kind,
            techno,
            owner,
            coord,
            health: t.strength.max(1),
            facing: 0,
            dir: 0,
            mission: Mission::Guard,
            motion: crate::hunt::Motion::default(),
            cargo: 0,
            cargo_value: 0,
            harvest: crate::harvest::Harvest::default(),
            limbo: false,
            placed: 0,
            sold_at: None,
            shielded_until: 0,
            dies_at: None,
            repairing: false,
            experience_bits: 0,
            command: None,
            depot: None,
            speed_bonus: ra2_rules::Ratio::ONE,
            armor_bonus: ra2_rules::Ratio::ONE,
            firepower_bonus: ra2_rules::Ratio::ONE,
            turret: None,
            passengers: Vec::new(),
            firing_occupant: 0,
            c4_at: None,
            controlled_by: None,
            gattling_rate: 0,
            disguise: None,
            deployed: false,
            guard_shots: 0,
            erasing: None,
        });
        let look = self.default_disguise(rules, techno, owner);
        if let Some(o) = self.objects.last_mut() {
            o.disguise = look;
        }
        id
    }

    /// Feed the world into a state hash, in a fixed order.
    pub fn feed(&self, h: &mut StateHasher) {
        h.u32(u32::try_from(self.houses.len()).unwrap_or(u32::MAX));
        for house in &self.houses {
            h.u32(house.index);
            h.bytes(house.country.as_bytes());
            h.i32(house.credits);
            h.i32(house.power_output);
            h.i32(house.power_drain);
        }
        h.u32(u32::try_from(self.objects.len()).unwrap_or(u32::MAX));
        for o in &self.objects {
            h.u32(o.id);
            h.u32(u32::try_from(o.techno).unwrap_or(u32::MAX));
            h.u32(o.owner);
            h.i32(o.coord.x);
            h.i32(o.coord.y);
            h.i32(o.coord.z);
            h.i32(o.health);
            h.u32(u32::from(o.facing));
            h.u32(u32::from(o.dir));
            h.i32(o.motion.reload);
            h.i32(o.motion.burst);
            h.i32(o.cargo);
        }
        h.u32(u32::try_from(self.ore.len()).unwrap_or(u32::MAX));
        for (&(x, y), ore) in &self.ore {
            h.i32(i32::from(x));
            h.i32(i32::from(y));
            h.u32(u32::from(ore.overlay));
            h.u32(u32::from(ore.data));
        }
        h.u32(u32::try_from(self.shots.len()).unwrap_or(u32::MAX));
        for s in &self.shots {
            h.u32(s.target);
            h.u32(s.lands);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_rules::Rules;
    use ra2_rules::scenario::placements;

    fn ini(t: &[u8]) -> ra2_rules::Ini {
        ra2_rules::Ini::parse(t)
    }

    #[test]
    fn placements_become_objects_where_the_original_puts_them() {
        let rules = Rules::from_ini(
            &ini(
                b"[VehicleTypes]\n1=MTNK\n[InfantryTypes]\n1=E1\n[BuildingTypes]\n1=GAPOWR\n\
                   [MTNK]\nStrength=300\n[E1]\nStrength=125\n[GAPOWR]\nStrength=750\n",
            ),
            &ini(b"[GAPOWR]\nFoundation=2x2\n"),
        );
        let map = ini(b"[Units]\n0=Neutral,MTNK,256,47,50,64,Hunt,None\n\
                        [Infantry]\n0=Neutral,E1,128,47,50,0,Guard,96,-1\n\
                        [Structures]\n0=Neutral,GAPOWR,256,10,12,0,None\n\
                        [Aircraft]\n0=Nobody,MTNK,256,1,1,0,Guard\n");
        let mut w = World::default();
        w.add_house("Neutral", 0);
        let ids: Vec<_> = placements(&map)
            .iter()
            .map(|p| w.place(&rules, p, 0))
            .collect();
        assert_eq!(
            ids,
            [Some(0), Some(1), None, Some(2)],
            "unknown owners are skipped"
        );
        let e1 = &w.objects[0];
        assert_eq!((e1.coord.x, e1.coord.y, e1.health), (12160, 12928, 62));
        let tank = &w.objects[1];
        assert_eq!(
            (tank.coord.x, tank.coord.y, tank.health),
            (12160, 12928, 300)
        );
        assert_eq!(tank.mission, Mission::Hunt);
        let plant = &w.objects[2];
        assert_eq!(
            (plant.coord.x, plant.coord.y),
            (10 * 256 + 256, 12 * 256 + 256)
        );
    }
}
