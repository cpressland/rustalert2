//! Superweapons: which a house has, and their charge. See
//! `docs/spec/superweapons.md`.

use ra2_rules::production::recharge_frames;
use ra2_rules::{Kind, Rules};

use crate::world::World;

/// One of a house's superweapons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Super {
    pub house: u32,
    /// Its index in `[SuperWeaponTypes]`.
    pub kind: usize,
    /// The charge timer: the frame it started (`None` while held) and its
    /// length; while held, the length is what was left.
    pub start: Option<u32>,
    pub frames: i32,
    pub charged: bool,
    pub on_hold: bool,
}

impl Super {
    /// Frames left to charge at `frame`, as the game reports them.
    pub fn left(&self, frame: u32) -> i32 {
        match self.start {
            None => self.frames,
            Some(s) => {
                (self.frames - i32::try_from(frame.saturating_sub(s)).unwrap_or(i32::MAX)).max(0)
            }
        }
    }
}

/// One frame of every house's superweapons, after power is worked out.
///
/// - A building's `SuperWeapon=` comes to its house the frame after the
///   building is placed, its charge timed from the frame it was placed.
///   `[observed]`
/// - A house with none of a superweapon's buildings left loses it.
///   `[hypothesis]`
/// - A powered superweapon (`IsPowered=`) holds while its house is short of
///   power, keeping what's left as of the frame before; when the power is
///   back it carries on, timed from the frame before. `[observed]`
///   (`allied-force-shield`; the hold is `SuperClass +0x70`)
/// - It's charged the frame after its time runs out. `[observed]`
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    // What each house's buildings give it, and since when.
    let mut given: Vec<(u32, usize, u32)> = Vec::new();
    for o in &world.objects {
        if o.limbo || o.kind != Kind::Building || frame <= o.placed {
            continue;
        }
        for name in &rules.technos[o.techno].super_weapons {
            if let Some(k) = rules
                .super_weapons
                .iter()
                .position(|s| s.id.eq_ignore_ascii_case(name))
            {
                given.push((o.owner, k, o.placed));
            }
        }
    }
    world
        .supers
        .retain(|s| given.iter().any(|&(h, k, _)| h == s.house && k == s.kind));
    for &(house, kind, placed) in &given {
        if !world
            .supers
            .iter()
            .any(|s| s.house == house && s.kind == kind)
        {
            world.supers.push(Super {
                house,
                kind,
                start: Some(placed),
                frames: recharge_frames(&rules.super_weapons[kind]),
                charged: false,
                on_hold: false,
            });
        }
    }
    for s in &mut world.supers {
        let h = &world.houses[usize::try_from(s.house).unwrap_or(0)];
        let short = h.power_output < h.power_drain;
        if s.charged {
            continue;
        }
        if rules.super_weapons[s.kind].is_powered {
            if short && !s.on_hold {
                s.frames = s.left(frame - 1);
                s.start = None;
                s.on_hold = true;
            } else if !short && s.on_hold {
                s.start = Some(frame - 1);
                s.on_hold = false;
            }
        }
        if let Some(start) = s.start
            && frame > start + u32::try_from(s.frames).unwrap_or(0)
        {
            s.charged = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Coord;
    use ra2_rules::Ini;

    #[test]
    fn a_powered_superweapon_holds_on_low_power_and_carries_on() {
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[SuperWeaponTypes]\n1=ForceShieldSpecial\n[ForceShieldSpecial]\nRechargeTime=.5\nIsPowered=yes\n\
                  [BuildingTypes]\n1=GATECH\n[GATECH]\nStrength=500\nSuperWeapon=ForceShieldSpecial\n",
            ),
            &Ini::parse(b""),
        );
        let mut world = World::default();
        world.add_house("British", 0);
        let lab = world.spawn(&rules, 0, 0, Coord::default());
        world.objects[0].placed = 901;
        assert_eq!(lab, 0);
        update(&mut world, &rules, 901);
        assert!(world.supers.is_empty(), "not until the frame after");
        update(&mut world, &rules, 902);
        assert_eq!(world.supers[0].left(902), 449);
        world.houses[0].power_drain = 100;
        update(&mut world, &rules, 1051);
        assert!(world.supers[0].on_hold);
        assert_eq!(world.supers[0].left(1500), 301);
        world.houses[0].power_drain = 0;
        update(&mut world, &rules, 2042);
        assert_eq!(world.supers[0].left(2042), 300);
        update(&mut world, &rules, 2342);
        assert!(!world.supers[0].charged);
        update(&mut world, &rules, 2343);
        assert!(world.supers[0].charged);
    }
}
