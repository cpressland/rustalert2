//! The game's random number generator. See `docs/spec/tick.md`.
//!
//! A 250-entry XOR lagged-Fibonacci generator. Each entry is seeded by a
//! pseudo-DES hash of (seed, index), as in Numerical Recipes' `psdes`; draws
//! XOR two entries 103 apart. Ranged draws mask to the range's bit width and
//! reject values past the range, so outcomes distribute as the game's do.

/// Entries in the table.
const SIZE: usize = 250;
/// The second index starts this far ahead of the first.
const LAG: usize = 103;

/// The `psdes` round constants (Numerical Recipes, also in `gamemd.exe` at
/// `0x839644` and `0x839694`).
const C1: [u32; 4] = [0xBAA9_6887, 0x1E17_D32C, 0x03BC_DC3C, 0x0F33_D1B2];
const C2: [u32; 4] = [0x4B0F_3B58, 0xE874_F0C3, 0x6955_C5A6, 0x55A7_CA46];

/// The game's `Randomizer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Randomizer {
    index1: usize,
    index2: usize,
    table: [u32; SIZE],
}

/// The game's seeding hash of one table entry. Four Feistel rounds over
/// (`left`, `right`); unlike the published `psdes`, the halves are split
/// with an arithmetic shift, so the sign of each half spreads into the
/// other. `[recomp 0x0065C6D0]`
fn psdes(mut left: u32, mut right: u32) -> u32 {
    for (c1, c2) in C1.iter().zip(C2) {
        let swap = right;
        let a = right ^ c1;
        let lo = a & 0xFFFF;
        let hi = i32::from_ne_bytes(a.to_ne_bytes()) >> 16;
        let hi = u32::from_ne_bytes(hi.to_ne_bytes());
        let b = (!hi.wrapping_mul(hi)).wrapping_add(lo.wrapping_mul(lo));
        let b_hi = i32::from_ne_bytes(b.to_ne_bytes()) >> 16;
        let mixed = u32::from_ne_bytes(b_hi.to_ne_bytes()) | (b << 16);
        right = ((mixed ^ c2).wrapping_add(hi.wrapping_mul(lo))) ^ left;
        left = swap;
    }
    right
}

impl Randomizer {
    /// Seed the table: entry `n` is the hash of (seed, n).
    pub fn new(seed: u32) -> Randomizer {
        let mut table = [0u32; SIZE];
        for (n, t) in (0u32..).zip(table.iter_mut()) {
            *t = psdes(seed, n);
        }
        Randomizer {
            index1: 0,
            index2: LAG,
            table,
        }
    }

    /// The next raw value. `[recomp 0x0065C780]`
    pub fn next_u32(&mut self) -> u32 {
        self.table[self.index1] ^= self.table[self.index2];
        let r = self.table[self.index1];
        self.index1 = (self.index1 + 1) % SIZE;
        self.index2 = (self.index2 + 1) % SIZE;
        r
    }

    /// A value from `min` to `max` inclusive, in either order. Draws are
    /// masked to the width of the range and redrawn while past it.
    /// `[recomp 0x0065C7E0]`
    pub fn ranged(&mut self, min: i32, max: i32) -> i32 {
        if min == max {
            return min;
        }
        let (lo, hi) = (min.min(max), min.max(max));
        let range = hi.wrapping_sub(lo);
        if range < 0 {
            // A range wider than i32: the game masks every draw to 0 and
            // never leaves its loop. We return the bottom instead.
            return lo;
        }
        let range = range.cast_unsigned();
        let bits = range.bit_width();
        let mask = if bits >= 32 {
            u32::MAX
        } else {
            (1u32 << bits) - 1
        };
        loop {
            let r = self.next_u32() & mask;
            if r <= range {
                return lo.wrapping_add(r.cast_signed());
            }
        }
    }

    /// The table and indices, for state hashing.
    pub fn state(&self) -> (usize, usize, &[u32; SIZE]) {
        (self.index1, self.index2, &self.table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeding_is_the_hash_of_seed_and_index() {
        let r = Randomizer::new(12345);
        let (i1, i2, t) = r.state();
        assert_eq!((i1, i2), (0, 103));
        assert_eq!(t[7], psdes(12345, 7));
        assert_ne!(t[0], t[1]);
    }

    #[test]
    fn draws_xor_entries_103_apart_and_wrap() {
        let mut r = Randomizer::new(1);
        let t0 = *r.state().2;
        assert_eq!(r.next_u32(), t0[0] ^ t0[103]);
        for _ in 1..250 {
            r.next_u32();
        }
        assert_eq!(r.state().0, 0, "the first index wraps after 250 draws");
        assert_eq!(r.state().1, 103);
    }

    #[test]
    fn ranged_stays_in_range_both_ways_round() {
        let mut r = Randomizer::new(99);
        for _ in 0..10_000 {
            let v = r.ranged(-5, 12);
            assert!((-5..=12).contains(&v));
            let w = r.ranged(12, -5);
            assert!((-5..=12).contains(&w));
        }
        assert_eq!(r.ranged(7, 7), 7);
        // Every value turns up.
        let mut seen = [false; 18];
        for _ in 0..2_000 {
            seen[usize::try_from(r.ranged(-5, 12) + 5).unwrap()] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Randomizer::new(42), Randomizer::new(42));
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        assert_ne!(
            Randomizer::new(42).next_u32(),
            Randomizer::new(43).next_u32()
        );
    }
}
