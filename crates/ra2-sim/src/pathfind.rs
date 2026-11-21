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
/// expanded next; the rest go on the heap, which is used when an
/// expansion adds nothing. The search stops when it makes the goal's node.
/// `[recomp 0x00429A90]`
pub fn find_path(
    start: (i16, i16),
    goal: (i16, i16),
    passable: &dyn Fn((i16, i16)) -> bool,
) -> Option<Vec<(i16, i16)>> {
    search(start, goal, passable, true)
}

/// The search itself. With `heuristic` off the estimate is the cost alone,
/// as in the zone-level search. The start's neighbours that `passable`
/// turns away are still recorded as reached (so they're never revisited
/// cheaply) but not expanded. `[observed]` (search dumps)
fn search(
    start: (i16, i16),
    goal: (i16, i16),
    passable: &dyn Fn((i16, i16)) -> bool,
    heuristic: bool,
) -> Option<Vec<(i16, i16)>> {
    let distance = |c: (i16, i16)| {
        if !heuristic {
            return 0;
        }
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
                if nodes[cur].parent.is_none() {
                    reached
                        .entry(cell)
                        .or_insert_with(|| add_f32(add_f32(CLEAR_COST, BIAS[d]), 0));
                }
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
        // The best new neighbour is expanded next whenever there is one;
        // the heap is only drawn on when an expansion adds nothing. The
        // game's own dumps of a search show this (every reached cell and
        // cost match), though the code reads as if the heap's top could
        // win when better. `[observed]`
        current = match (aside, heap.top()) {
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

/// Zone sizes by level, from the top. On open ground the game's zones are
/// aligned square blocks of these sizes. `[observed]` (zone dumps; how
/// they split on real terrain is still to trace)
const ZONE_SIZES: [i16; 3] = [8, 4, 2];

/// The corridor: the level-0 zones (2×2 blocks, by block coordinate) a
/// route may use. At each level a search without an estimate runs over
/// that level's blocks, kept to the blocks inside the level above's route.
/// `[observed]` (fits every dumped corridor; the game's step cost also has
/// a threat term and a per-connection weight, both nil on open ground)
fn corridor(
    start: (i16, i16),
    goal: (i16, i16),
    valid: &dyn Fn((i16, i16)) -> bool,
) -> Option<std::collections::BTreeSet<(i16, i16)>> {
    let mut parent: Option<(i16, std::collections::BTreeSet<(i16, i16)>)> = None;
    for size in ZONE_SIZES {
        let block = |c: (i16, i16)| (c.0.div_euclid(size), c.1.div_euclid(size));
        let ok = |b: (i16, i16)| {
            let (x0, y0) = (
                i32::from(b.0) * i32::from(size),
                i32::from(b.1) * i32::from(size),
            );
            let (Ok(x0), Ok(y0)) = (i16::try_from(x0), i16::try_from(y0)) else {
                return false;
            };
            let inside_parent = parent
                .as_ref()
                .is_none_or(|(ps, set)| set.contains(&(x0.div_euclid(*ps), y0.div_euclid(*ps))));
            inside_parent && (0..size).any(|y| (0..size).any(|x| valid((x0 + x, y0 + y))))
        };
        let (from, to) = (block(start), block(goal));
        let mut route: std::collections::BTreeSet<(i16, i16)> =
            search(from, to, &ok, false)?.into_iter().collect();
        route.insert(from);
        parent = Some((size, route));
    }
    parent.map(|(_, set)| set)
}

/// Tidy a found route as the game does before storing it: two diagonal
/// steps a right angle apart become two straight steps; then everything
/// after the first step is re-laid as straight steps first, then
/// diagonals, covering the same ground in the fewest steps. `[observed]`
/// (every stored route recorded; the passes are `0x0042B210` and
/// `0x0042B7F0`)
pub fn tidy(route: &[u8]) -> Vec<u8> {
    let mut r = route.to_vec();
    for i in 0..r.len().saturating_sub(1) {
        let (a, b) = (r[i], r[i + 1]);
        if a % 2 == 1 && b % 2 == 1 && matches!((b + 8 - a) % 8, 2 | 6) {
            let (da, db) = (DIRECTIONS[usize::from(a)], DIRECTIONS[usize::from(b)]);
            let mid = (i16::midpoint(da.0, db.0), i16::midpoint(da.1, db.1));
            let s = u8::try_from(DIRECTIONS.iter().position(|&d| d == mid).expect("straight"))
                .expect("small");
            r[i] = s;
            r[i + 1] = s;
        }
    }
    if r.len() < 2 {
        return r;
    }
    let (dx, dy) = r[1..].iter().fold((0i16, 0i16), |(x, y), &d| {
        let (sx, sy) = DIRECTIONS[usize::from(d)];
        (x + sx, y + sy)
    });
    let mut out = vec![r[0]];
    out.extend(line(dx, dy));
    out
}

/// The fewest steps covering `(dx, dy)`: straight steps along the longer
/// axis first, then diagonals.
fn line(dx: i16, dy: i16) -> Vec<u8> {
    let (sx, sy) = (dx.signum(), dy.signum());
    let dir = |d: (i16, i16)| {
        u8::try_from(
            DIRECTIONS
                .iter()
                .position(|&x| x == d)
                .expect("a direction"),
        )
        .expect("small")
    };
    let diagonal = dx.abs().min(dy.abs());
    let straight = dx.abs().max(dy.abs()) - diagonal;
    let along = if dx.abs() > dy.abs() {
        (sx, 0)
    } else {
        (0, sy)
    };
    let mut out = Vec::new();
    for _ in 0..straight {
        out.push(dir(along));
    }
    for _ in 0..diagonal {
        out.push(dir((sx, sy)));
    }
    out
}

/// Plan a route from `start` to `goal` over `valid` cells: the corridor,
/// the cell search inside it, then the tidying. Directions are indices
/// into [`DIRECTIONS`].
pub fn plan(
    start: (i16, i16),
    goal: (i16, i16),
    valid: &dyn Fn((i16, i16)) -> bool,
) -> Option<Vec<u8>> {
    if start == goal {
        return Some(Vec::new());
    }
    let zones = corridor(start, goal, valid)?;
    let inside = |c: (i16, i16)| {
        c == start || (valid(c) && zones.contains(&(c.0.div_euclid(2), c.1.div_euclid(2))))
    };
    let cells = search(start, goal, &inside, true)?;
    let mut steps = Vec::with_capacity(cells.len());
    let mut at = start;
    for c in cells {
        let d = (c.0 - at.0, c.1 - at.1);
        steps.push(
            u8::try_from(DIRECTIONS.iter().position(|&x| x == d).expect("adjacent"))
                .expect("small"),
        );
        at = c;
    }
    Some(tidy(&steps))
}

/// Where a unit goes to attack: the target's cell, plus its weapon range
/// less half a cell, toward the attacker, each axis cut toward zero.
/// `[observed]` (fits the destinations of three weapons and seven starts)
pub fn approach(from: (i16, i16), target: (i16, i16), range_leptons: i32) -> (i16, i16) {
    let (dx, dy) = (i64::from(from.0 - target.0), i64::from(from.1 - target.1));
    let d2 = dx * dx + dy * dy;
    if d2 == 0 {
        return target;
    }
    let r = i64::from((range_leptons - 128).max(0));
    // |offset| = floor(|d| × r / (256 × sqrt(d2))), found exactly.
    let axis = |d: i64| -> i16 {
        let num = (d.abs() * r).pow(2);
        let mut k = 0i64;
        while (k + 1).pow(2) * 65_536 * d2 <= num {
            k += 1;
        }
        i16::try_from(k * d.signum()).unwrap_or(0)
    };
    (target.0 + axis(dx), target.1 + axis(dy))
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

    /// The corridor the game marked for grizzly-hunts-mcv's Grizzly (2×2
    /// level-0 zones, dumped from the original) and the route its search
    /// found inside it, every reached cell's cost matching bit for bit.
    #[test]
    fn inside_the_games_corridor_the_route_is_the_games() {
        let blocks = [
            (46, 50),
            (46, 48),
            (46, 46),
            (44, 44),
            (42, 42),
            (40, 40),
            (38, 38),
            (36, 38),
        ];
        let allowed = |c: (i16, i16)| {
            blocks
                .iter()
                .any(|&(bx, by)| (bx..bx + 2).contains(&c.0) && (by..by + 2).contains(&c.1))
        };
        let p = find_path((47, 50), (37, 38), &allowed).unwrap();
        assert_eq!(dirs((47, 50), &p), "NW,N,N,N,NW,NW,NW,NW,NW,NW,NW,NW,W");
    }

    /// Every route the original stored in the recorded starts (Grizzly from
    /// seven cells, Rhino from one) toward the MCV at (35,35), or the
    /// enemy MCV at (65,65) for the start nearer it.
    type Case = ((i16, i16), (i16, i16), i32, &'static str);

    #[test]
    fn planned_routes_are_the_games() {
        // The recordings' 50×50 map spans cells 7 to 92 on each axis.
        let open = |c: (i16, i16)| (7..=92).contains(&c.0) && (7..=92).contains(&c.1);
        let cases: [Case; 8] = [
            (
                (47, 50),
                (35, 35),
                1280,
                "NW,N,N,NW,NW,NW,NW,NW,NW,NW,NW,NW",
            ),
            ((40, 50), (35, 35), 1280, "N,N,N,N,N,N,N,NW,NW,NW,NW"),
            ((45, 50), (35, 35), 1280, "NW,N,N,N,N,NW,NW,NW,NW,NW,NW,NW"),
            ((47, 44), (35, 35), 1280, "W,W,NW,NW,NW,NW,NW,NW,NW"),
            ((50, 40), (35, 35), 1280, "W,W,W,W,W,W,W,NW,NW,NW,NW"),
            (
                (50, 47),
                (35, 35),
                1280,
                "NW,W,W,NW,NW,NW,NW,NW,NW,NW,NW,NW",
            ),
            ((52, 52), (65, 65), 1280, "SE,SE,SE,SE,SE,SE,SE,SE,SE,SE"),
            ((47, 50), (35, 35), 1472, "NW,N,N,NW,NW,NW,NW,NW,NW,NW,NW"),
        ];
        let names = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        for (start, target, range, want) in cases {
            let goal = approach(start, target, range);
            let route = plan(start, goal, &open).unwrap();
            let got: Vec<&str> = route.iter().map(|&d| names[usize::from(d)]).collect();
            assert_eq!(got.join(","), want, "from {start:?}");
        }
        assert_eq!(approach((47, 50), (35, 35), 1280), (37, 38));
        assert_eq!(approach((47, 50), (35, 35), 1472), (38, 39));
        assert_eq!(approach((47, 50), (35, 35), 1024), (37, 37));
    }

    #[test]
    fn routes_go_round_walls() {
        // A wall across x = 5 from y = 0 to 8, open below.
        let open = |c: (i16, i16)| !(c.0 == 5 && (0..=8).contains(&c.1));
        let p = find_path((2, 4), (8, 4), &open).unwrap();
        assert_eq!(*p.last().unwrap(), (8, 4));
        assert!(p.iter().all(|&c| open(c)));
        assert!(
            p.iter().any(|c| c.1 >= 9 || c.1 < 0),
            "round an end of the wall"
        );
    }
}
