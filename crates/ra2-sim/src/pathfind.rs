//! The game's cell-level A* path search. See `docs/spec/pathfinding.md`.
//!
//! Costs and estimates are single-precision floats in the game. The
//! simulation has no floats, so they're held as their IEEE bit patterns
//! and every add, square root and rounding the game does is done here in
//! integers, giving the same bits. All values are positive, which keeps
//! the arithmetic small.

use std::collections::BTreeMap;

/// The eight directions in the game's order: north (−y) first, clockwise.
pub const DIRECTIONS: [(i16, i16); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// A small extra cost per direction (as `f32` bits: 0.001, 0.005, 0.002,
/// 0.006, 0.003, 0.007, 0.004, 0.008) that breaks ties between equally
/// long routes. `[recomp]` (`gamemd.exe` `0x81872C`)
const BIAS: [u32; 8] = [
    0x3A83_126F,
    0x3BA3_D70A,
    0x3B03_126F,
    0x3BC4_9BA6,
    0x3B44_9BA6,
    0x3BE5_6042,
    0x3B83_126F,
    0x3C03_126F,
];
/// 1.0 as `f32` bits: the cost of entering a clear cell. `[recomp]`
/// (`0x81870C`, the first of the per-move-result costs)
const CLEAR_COST: u32 = 0x3F80_0000;
/// 1.009 as a double: a cell already reached is revisited only if its
/// recorded cost is at least the current node's plus this. `[recomp]`
/// (`0x7E37C0`)
const REVISIT: u64 = 0x3FF0_24DD_2F1A_9FBE;
/// The search gives up after this many expansions. `[recomp]` (`0xFFF7`)
const MAX_EXPANSIONS: u32 = 0xFFF7;

/// An exact positive number: `m × 2^e`.
#[derive(Debug, Clone, Copy)]
struct Exact {
    m: u128,
    e: i32,
}

fn from_f32(bits: u32) -> Exact {
    let exp = i32::try_from((bits >> 23) & 0xFF).expect("8 bits");
    let frac = u128::from(bits & 0x7F_FFFF);
    if exp == 0 {
        Exact { m: frac, e: -149 }
    } else {
        Exact {
            m: frac | (1 << 23),
            e: exp - 150,
        }
    }
}

fn from_f64(bits: u64) -> Exact {
    let exp = i32::try_from((bits >> 52) & 0x7FF).expect("11 bits");
    let frac = u128::from(bits & ((1 << 52) - 1));
    if exp == 0 {
        Exact { m: frac, e: -1074 }
    } else {
        Exact {
            m: frac | (1 << 52),
            e: exp - 1075,
        }
    }
}

fn add(a: Exact, b: Exact) -> Exact {
    if a.m == 0 {
        return b;
    }
    if b.m == 0 {
        return a;
    }
    let e = a.e.min(b.e);
    let shift = |x: Exact| x.m << u32::try_from(x.e - e).expect("aligned");
    Exact {
        m: shift(a) + shift(b),
        e,
    }
}

/// Round to `bits` significant bits, ties to even; `sticky` says there are
/// nonzero bits below `m` already lost.
fn round(x: Exact, bits: u32, sticky: bool) -> Exact {
    if x.m == 0 {
        return x;
    }
    let width = 128 - x.m.leading_zeros();
    if width <= bits {
        return x;
    }
    let drop = width - bits;
    let rest = x.m & ((1u128 << drop) - 1);
    let half = 1u128 << (drop - 1);
    let mut m = x.m >> drop;
    let e = x.e + i32::try_from(drop).expect("small");
    if rest > half || (rest == half && (sticky || m & 1 == 1)) {
        m += 1;
    }
    Exact { m, e }
}

/// To `f32` bits, rounding to nearest. Values here are normal.
fn to_f32(x: Exact) -> u32 {
    let r = round(x, 24, false);
    if r.m == 0 {
        return 0;
    }
    let width = 128 - r.m.leading_zeros();
    let (m, e) = if width < 24 {
        (
            r.m << (24 - width),
            r.e - i32::try_from(24 - width).expect("small"),
        )
    } else if width == 25 {
        (r.m >> 1, r.e + 1)
    } else {
        (r.m, r.e)
    };
    let exp = u32::try_from(e + 150).expect("normal");
    (exp << 23) | (u32::try_from(m).expect("24 bits") & 0x7F_FFFF)
}

/// `f32(a + b)` for two `f32` values, as the game's x87 adds and stores.
fn add_f32(a: u32, b: u32) -> u32 {
    to_f32(add(from_f32(a), from_f32(b)))
}

/// The game's estimate for a node: `f32(sqrt(n) + g)`, with the square root
/// and the sum in double precision, then stored as a single.
fn estimate(n: u64, g: u32) -> u32 {
    let root = if n == 0 {
        Exact { m: 0, e: 0 }
    } else {
        // floor(sqrt(n) × 2^54), and whether anything was cut off.
        let scaled = u128::from(n) << 108;
        let s = scaled.isqrt();
        round(Exact { m: s, e: -54 }, 53, s * s != scaled)
    };
    to_f32(round(add(root, from_f32(g)), 53, false))
}

/// Whether `rec < g + 1.009`, exactly.
fn below_revisit(rec: u32, g: u32) -> bool {
    let lhs = from_f32(rec);
    let rhs = add(from_f32(g), from_f64(REVISIT));
    let e = lhs.e.min(rhs.e);
    let shift = |x: Exact| x.m << u32::try_from(x.e - e).expect("aligned");
    shift(lhs) < shift(rhs)
}

struct Node {
    cell: (i16, i16),
    parent: Option<usize>,
    /// Cost so far and estimate, as `f32` bits (positive, so their bits
    /// order like the numbers).
    g: u32,
    f: u32,
}

/// The open list: a 1-based binary heap on `f`, moving an entry up only
/// while its parent is strictly worse, as the game's does.
struct Heap(Vec<usize>);

impl Heap {
    fn push(&mut self, nodes: &[Node], n: usize) {
        self.0.push(n);
        let mut i = self.0.len() - 1;
        while i > 1 {
            let p = i / 2;
            if nodes[self.0[p]].f <= nodes[n].f {
                break;
            }
            self.0[i] = self.0[p];
            i = p;
        }
        self.0[i] = n;
    }

    /// Restore order from `i` down: a child replaces its parent only if it
    /// is strictly better, the left child first.
    fn sift(&mut self, nodes: &[Node], mut i: usize) {
        let size = self.0.len() - 1;
        loop {
            let mut m = i;
            for c in [2 * i, 2 * i + 1] {
                if c <= size && nodes[self.0[m]].f > nodes[self.0[c]].f {
                    m = c;
                }
            }
            if m == i {
                return;
            }
            self.0.swap(i, m);
            i = m;
        }
    }

    fn top(&self) -> Option<usize> {
        self.0.get(1).copied()
    }
}

/// A route from `start` to `goal` over cells `passable` lets in (and, for
/// the hierarchical corridor, would also check zones; see the spec), as
/// the cells after `start`. `None` if there's none within the game's limit.
///
/// The steps: expand the best node; for each of the eight neighbours in
/// order, skip it if blocked or already reached cheaply enough, else make
/// a node whose cost adds 1 plus the direction's bias and whose estimate
/// adds the straight-line distance to the goal. The best new neighbour is
/// kept aside to expand next unless the heap's top is at least as good;
/// the rest go on the heap. The search stops when it makes the goal's node.
/// `[recomp 0x00429A90]`
pub fn find_path(
    start: (i16, i16),
    goal: (i16, i16),
    passable: &dyn Fn((i16, i16)) -> bool,
) -> Option<Vec<(i16, i16)>> {
    let distance = |c: (i16, i16)| {
        let (dx, dy) = (i64::from(goal.0 - c.0), i64::from(goal.1 - c.1));
        u64::try_from(dx * dx + dy * dy).expect("positive")
    };
    let mut nodes = vec![Node {
        cell: start,
        parent: None,
        g: 0,
        f: estimate(distance(start), 0),
    }];
    let mut reached: BTreeMap<(i16, i16), u32> = BTreeMap::new();
    reached.insert(start, 0);
    let mut heap = Heap(vec![usize::MAX]);
    let mut current = Some(0);
    let mut expansions = 0;
    while let Some(cur) = current {
        expansions += 1;
        if expansions > MAX_EXPANSIONS {
            return None;
        }
        let mut aside: Option<usize> = None;
        for (d, (dx, dy)) in DIRECTIONS.iter().enumerate() {
            let cell = (nodes[cur].cell.0 + dx, nodes[cur].cell.1 + dy);
            if !passable(cell) {
                continue;
            }
            if reached
                .get(&cell)
                .is_some_and(|&rec| below_revisit(rec, nodes[cur].g))
            {
                continue;
            }
            let g = add_f32(add_f32(CLEAR_COST, BIAS[d]), nodes[cur].g);
            nodes.push(Node {
                cell,
                parent: Some(cur),
                g,
                f: estimate(distance(cell), g),
            });
            let n = nodes.len() - 1;
            match aside {
                None => aside = Some(n),
                Some(a) if nodes[n].f < nodes[a].f => {
                    heap.push(&nodes, a);
                    aside = Some(n);
                }
                Some(_) => heap.push(&nodes, n),
            }
            reached.insert(cell, g);
            if cell == goal {
                let mut path = Vec::new();
                let mut at = Some(n);
                while let Some(i) = at {
                    path.push(nodes[i].cell);
                    at = nodes[i].parent;
                }
                path.pop();
                path.reverse();
                return Some(path);
            }
        }
        current = match (aside, heap.top()) {
            (Some(a), Some(t)) if nodes[t].f <= nodes[a].f => {
                heap.0[1] = a;
                heap.sift(&nodes, 1);
                Some(t)
            }
            (Some(a), _) => Some(a),
            (None, Some(t)) => {
                let last = heap.0.pop().expect("non-empty");
                if heap.0.len() > 1 {
                    heap.0[1] = last;
                    heap.sift(&nodes, 1);
                }
                Some(t)
            }
            (None, None) => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_precision_steps_match_the_games_bits() {
        // 1 + 0.001 in single precision, and the sum of two such steps.
        assert_eq!(add_f32(CLEAR_COST, BIAS[0]), 0x3F80_20C5);
        assert_eq!(add_f32(0x3F80_20C5, 0x3F80_20C5), 0x4000_20C5);
        // sqrt(2) + 0 is 1.41421354 as a single.
        assert_eq!(estimate(2, 0), 0x3FB5_04F3);
        assert_eq!(estimate(0, 0x3F80_20C5), 0x3F80_20C5);
        assert!(below_revisit(0x3F80_0000, 0x3F80_0000));
        assert!(!below_revisit(0x4001_0000, 0x3F80_0000));
    }

    fn dirs(start: (i16, i16), path: &[(i16, i16)]) -> String {
        let names = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        std::iter::once(start)
            .chain(path.iter().copied())
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| {
                let d = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                names[DIRECTIONS.iter().position(|&x| x == d).unwrap()]
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn open_ground_routes_follow_the_straight_line_then_the_axis() {
        let open = |_: (i16, i16)| true;
        // As the Python model of the traced search gives.
        let p = find_path((47, 50), (35, 35), &open).unwrap();
        assert_eq!(p.len(), 15);
        assert_eq!(
            dirs((47, 50), &p),
            "NW,NW,NW,NW,NW,NW,NW,NW,NW,NW,NW,NW,N,N,N"
        );
        // The two legs the hunt scenarios' corridor makes of it.
        assert_eq!(
            dirs((47, 50), &find_path((47, 50), (46, 47), &open).unwrap()),
            "NW,N,N"
        );
        assert_eq!(
            dirs((46, 47), &find_path((46, 47), (35, 35), &open).unwrap()),
            "NW,NW,NW,NW,NW,NW,NW,NW,NW,NW,NW,N"
        );
    }

    #[test]
    fn routes_go_round_walls() {
        // A wall across x = 5 from y = 0 to 8, open below.
        let open = |c: (i16, i16)| !(c.0 == 5 && (0..=8).contains(&c.1));
        let p = find_path((2, 4), (8, 4), &open).unwrap();
        assert_eq!(*p.last().unwrap(), (8, 4));
        assert!(p.iter().all(|&c| open(c)));
        assert!(p.iter().any(|c| c.1 >= 9 || c.1 < 0), "round an end of the wall");
    }
}
