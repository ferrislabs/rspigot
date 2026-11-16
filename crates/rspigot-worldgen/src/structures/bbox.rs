pub(crate) type Pos = (i32, i32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bbox {
    pub(crate) min: Pos,
    pub(crate) max: Pos,
}

impl Bbox {
    pub(crate) fn from_corners(a: Pos, b: Pos) -> Self {
        Self {
            min: (a.0.min(b.0), a.1.min(b.1), a.2.min(b.2)),
            max: (a.0.max(b.0), a.1.max(b.1), a.2.max(b.2)),
        }
    }

    pub(crate) fn contains(&self, (x, y, z): Pos) -> bool {
        x >= self.min.0
            && x <= self.max.0
            && y >= self.min.1
            && y <= self.max.1
            && z >= self.min.2
            && z <= self.max.2
    }

    pub(crate) fn moved(&self, dx: i32, dy: i32, dz: i32) -> Self {
        Self {
            min: (self.min.0 + dx, self.min.1 + dy, self.min.2 + dz),
            max: (self.max.0 + dx, self.max.1 + dy, self.max.2 + dz),
        }
    }

    pub(crate) fn intersects(&self, other: &Self) -> bool {
        self.max.0 >= other.min.0
            && self.min.0 <= other.max.0
            && self.max.1 >= other.min.1
            && self.min.1 <= other.max.1
            && self.max.2 >= other.min.2
            && self.min.2 <= other.max.2
    }

    pub(crate) fn encapsulate_point(&mut self, (x, y, z): Pos) {
        self.min = (self.min.0.min(x), self.min.1.min(y), self.min.2.min(z));
        self.max = (self.max.0.max(x), self.max.1.max(y), self.max.2.max(z));
    }

    pub(crate) fn encapsulate(&mut self, other: &Self) {
        self.encapsulate_point(other.min);
        self.encapsulate_point(other.max);
    }

    pub(crate) fn y_span(&self) -> i32 {
        self.max.1 - self.min.1 + 1
    }
}
