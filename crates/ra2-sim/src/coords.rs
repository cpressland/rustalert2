//! Positions on the map: cells, leptons and height levels, and how they
//! project to the screen. See `docs/spec/coords.md`.

/// Leptons per cell side.
pub const LEPTONS_PER_CELL: i32 = 256;
/// Leptons per height level.
pub const LEPTONS_PER_LEVEL: i32 = 104;
/// A cell's diamond on screen.
pub const CELL_WIDTH: i32 = 60;
pub const CELL_HEIGHT: i32 = 30;
/// Pixels a cell is raised per height level.
pub const PIXELS_PER_LEVEL: i32 = 15;

/// A cell on the map, in the map's iso coordinates (as map packs store them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cell {
    pub x: i16,
    pub y: i16,
}

/// A point in the world, in leptons. `z` is height above the ground plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// A point on screen, relative to the world's projection origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pixel {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    pub fn new(x: i16, y: i16) -> Cell {
        Cell { x, y }
    }

    /// The cell's centre on the ground at `level`.
    pub fn center(self, level: i32) -> Coord {
        Coord {
            x: i32::from(self.x) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            y: i32::from(self.y) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            z: level * LEPTONS_PER_LEVEL,
        }
    }

    /// The top corner of the cell's 60×30 diamond on screen at `level`; the
    /// diamond's box spans 30 pixels either side of it.
    pub fn screen(self, level: i32) -> Pixel {
        let (x, y) = (i32::from(self.x), i32::from(self.y));
        Pixel {
            x: (x - y) * CELL_WIDTH / 2,
            y: (x + y) * CELL_HEIGHT / 2 - level * PIXELS_PER_LEVEL,
        }
    }
}

impl Coord {
    /// The cell this point is in.
    pub fn cell(self) -> Cell {
        let c = |v: i32| i16::try_from(v.div_euclid(LEPTONS_PER_CELL)).unwrap_or(i16::MAX);
        Cell {
            x: c(self.x),
            y: c(self.y),
        }
    }

    /// The point on screen.
    pub fn screen(self) -> Pixel {
        Pixel {
            x: (self.x - self.y) * (CELL_WIDTH / 2) / LEPTONS_PER_CELL,
            y: (self.x + self.y) * (CELL_HEIGHT / 2) / LEPTONS_PER_CELL
                - self.z * PIXELS_PER_LEVEL / LEPTONS_PER_LEVEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_tessellate() {
        // Neighbours along x and y sit half a diamond apart each way.
        let o = Cell::new(10, 10).screen(0);
        assert_eq!(
            Cell::new(11, 10).screen(0),
            Pixel {
                x: o.x + 30,
                y: o.y + 15
            }
        );
        assert_eq!(
            Cell::new(10, 11).screen(0),
            Pixel {
                x: o.x - 30,
                y: o.y + 15
            }
        );
        assert_eq!(
            Cell::new(10, 10).screen(2),
            Pixel {
                x: o.x,
                y: o.y - 30
            }
        );
    }

    #[test]
    fn centres_project_to_the_diamond_middle() {
        let c = Cell::new(10, 10);
        let p = c.center(0).screen();
        let box_ = c.screen(0);
        // The centre lies 30 right and 15 down of the box's top-left corner
        // plus the half-cell offset.
        assert_eq!((p.x - box_.x, p.y - box_.y), (0, 15));
        assert_eq!(c.center(3).cell(), c);
        assert_eq!(Coord { x: -1, y: 0, z: 0 }.cell(), Cell::new(-1, 0));
        assert_eq!(c.center(1).screen().y, p.y - 15);
    }
}
