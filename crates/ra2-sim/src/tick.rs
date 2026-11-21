//! Game frames and speed, the state hash, and the simulation's outer API.
//! See `docs/spec/tick.md`.

use crate::rng::Randomizer;
use crate::world::World;

/// The game speed setting, 0 (fastest) to 6 (slowest), as the options
/// slider and `GameSpeed=` in spawn files give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct GameSpeed(u8);

impl GameSpeed {
    /// The slowest setting.
    pub const SLOWEST: u8 = 6;

    /// A speed setting, clamped to 0–6.
    pub fn new(setting: u8) -> GameSpeed {
        GameSpeed(setting.min(Self::SLOWEST))
    }

    pub fn setting(self) -> u8 {
        self.0
    }

    /// Logic frames per second at this speed. `[community]` (60, 45, 30,
    /// 20, 15, 12 and 10 from fastest to slowest)
    pub fn frames_per_second(self) -> u32 {
        [60, 45, 30, 20, 15, 12, 10][usize::from(self.0)]
    }
}

/// A 64-bit FNV-1a hash over the simulation state, written field by field
/// in a fixed order, so two machines agree exactly. For our own desync
/// checks, not the game's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> StateHasher {
        StateHasher(0xCBF2_9CE4_8422_2325)
    }
}

impl StateHasher {
    pub fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

/// Something a player asks for in one frame. Orders arrive in frame order
/// from every player and are applied in that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Draw from the game's random numbers: a stand-in that exercises the
    /// lockstep path until real orders (move, attack, build) land.
    Roll { min: i32, max: i32 },
}

/// Something that happened in a frame, for the presentation and the logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Rolled { value: i32 },
}

/// The simulation: everything that must agree between machines.
#[derive(Debug, Clone)]
pub struct Sim {
    frame: u32,
    rng: Randomizer,
    world: World,
}

impl Sim {
    pub fn new(seed: u32) -> Sim {
        Sim::with_world(seed, World::default())
    }

    /// A simulation starting from `world` (houses and objects set up from
    /// a map and the game's options).
    pub fn with_world(seed: u32, world: World) -> Sim {
        Sim {
            frame: 0,
            rng: Randomizer::new(seed),
            world,
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    /// The number of frames stepped.
    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn rng(&mut self) -> &mut Randomizer {
        &mut self.rng
    }

    /// Advance one logic frame, applying `orders` in order.
    pub fn step(&mut self, orders: &[Order]) -> Vec<Event> {
        let mut events = Vec::new();
        for order in orders {
            match *order {
                Order::Roll { min, max } => events.push(Event::Rolled {
                    value: self.rng.ranged(min, max),
                }),
            }
        }
        self.frame += 1;
        events
    }

    /// The state hash after the last step.
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::default();
        h.u32(self.frame);
        let (i1, i2, table) = self.rng.state();
        h.u64(i1 as u64);
        h.u64(i2 as u64);
        for &t in table {
            h.u32(t);
        }
        self.world.feed(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u32) -> Vec<u64> {
        let mut sim = Sim::new(seed);
        (0..300)
            .map(|f| {
                let orders: Vec<Order> = (0..f % 4)
                    .map(|k| Order::Roll {
                        min: -k,
                        max: 100 * k,
                    })
                    .collect();
                sim.step(&orders);
                sim.hash()
            })
            .collect()
    }

    #[test]
    fn same_seed_and_orders_same_hashes() {
        assert_eq!(run(7), run(7));
        assert_ne!(run(7).last(), run(8).last());
    }

    /// Pinned so any change to the generator, the seeding or the hash shows
    /// up, and so CI on every platform checks the same value.
    #[test]
    fn hashes_are_pinned_across_platforms() {
        let hashes = run(2026);
        assert_eq!(hashes.len(), 300);
        assert_eq!(hashes[299], PINNED);
    }

    const PINNED: u64 = 10_693_829_126_038_112_946;

    #[test]
    fn speeds_run_from_sixty_to_ten_frames_a_second() {
        assert_eq!(GameSpeed::new(0).frames_per_second(), 60);
        assert_eq!(GameSpeed::new(6).frames_per_second(), 10);
        assert_eq!(GameSpeed::new(9).setting(), 6);
    }
}
