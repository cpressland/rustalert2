//! Jumpjet units (`JumpJet=`): the Siege Chopper taking off and coming
//! down to deploy. See `docs/spec/locomotors.md`, "Jumpjets".
//!
//! Recorded (`soviet-siege-chopper`): out of the war factory, it climbs
//! `JumpjetClimb=` a frame from 46 frames after it appears, and hovers at
//! `JumpjetHeight=` over the cell east of the door. Ordered to deploy (a
//! simple deployer), it comes down at the same rate from 3 frames after
//! the order, and on the ground it's deployed. `[observed]`
//!
//! Not modelled: it climbs higher over the factory's roof (to 1030) before
//! setting off, speeds up along the way, and comes down to its height over
//! the cell; ours drives out on the ground while it climbs.

use ra2_rules::{Kind, Rules};

use crate::world::World;

/// Frames from appearing out of the factory to the first climb.
/// `[observed]`
const CLIMB_DELAY: u32 = 46;
/// Frames from a deploy order to coming down. `[observed]`
pub(crate) const LANDING_DELAY: u32 = 3;

/// One frame of every jumpjet unit.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    for o in &mut world.objects {
        let t = &rules.technos[o.techno];
        if !t.jumpjet_unit || o.kind != Kind::Vehicle || o.limbo || o.deployed {
            continue;
        }
        if let Some(from) = o.landing {
            if frame >= from {
                o.coord.z = (o.coord.z - t.jumpjet_climb).max(0);
                if o.coord.z == 0 {
                    // Down: deployed, facing `DeployDir=`.
                    o.landing = None;
                    o.deployed = true;
                    o.dir =
                        u16::try_from(rules.general.deploy_dir.rem_euclid(8) * 8192).unwrap_or(0);
                }
            }
            continue;
        }
        if o.placed > 0 && frame >= o.placed + CLIMB_DELAY {
            o.coord.z = (o.coord.z + t.jumpjet_climb).min(t.jumpjet_height);
        }
    }
}
