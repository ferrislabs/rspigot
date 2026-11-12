use crate::{BlockPos, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    pub fn step_order(movement: Vec3) -> [Self; 3] {
        if movement.x.abs() < movement.z.abs() {
            [Self::Y, Self::Z, Self::X]
        } else {
            [Self::Y, Self::X, Self::Z]
        }
    }

    pub const fn others(self) -> (Self, Self) {
        match self {
            Self::X => (Self::Y, Self::Z),
            Self::Y => (Self::Z, Self::X),
            Self::Z => (Self::X, Self::Y),
        }
    }

    pub const fn of(self, v: Vec3) -> f64 {
        match self {
            Self::X => v.x,
            Self::Y => v.y,
            Self::Z => v.z,
        }
    }

    pub const fn with(self, v: Vec3, value: f64) -> Vec3 {
        match self {
            Self::X => Vec3::new(value, v.y, v.z),
            Self::Y => Vec3::new(v.x, value, v.z),
            Self::Z => Vec3::new(v.x, v.y, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    pub fn new(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Self {
        Self {
            min_x: x1.min(x2),
            min_y: y1.min(y2),
            min_z: z1.min(z2),
            max_x: x1.max(x2),
            max_y: y1.max(y2),
            max_z: z1.max(z2),
        }
    }

    pub fn of_size(position: Vec3, width: f32, height: f32) -> Self {
        let half = f64::from(width / 2.0);
        let height = f64::from(height);
        Self::new(
            position.x - half,
            position.y,
            position.z - half,
            position.x + half,
            position.y + height,
            position.z + half,
        )
    }

    pub fn block(pos: BlockPos) -> Self {
        let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        Self::new(x, y, z, x + 1.0, y + 1.0, z + 1.0)
    }

    pub const fn min(&self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.min_x,
            Axis::Y => self.min_y,
            Axis::Z => self.min_z,
        }
    }

    pub const fn max(&self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.max_x,
            Axis::Y => self.max_y,
            Axis::Z => self.max_z,
        }
    }

    pub fn expand_towards(&self, v: Vec3) -> Self {
        let mut out = *self;
        if v.x < 0.0 {
            out.min_x += v.x;
        } else if v.x > 0.0 {
            out.max_x += v.x;
        }
        if v.y < 0.0 {
            out.min_y += v.y;
        } else if v.y > 0.0 {
            out.max_y += v.y;
        }
        if v.z < 0.0 {
            out.min_z += v.z;
        } else if v.z > 0.0 {
            out.max_z += v.z;
        }
        out
    }

    pub fn move_by(&self, v: Vec3) -> Self {
        Self {
            min_x: self.min_x + v.x,
            min_y: self.min_y + v.y,
            min_z: self.min_z + v.z,
            max_x: self.max_x + v.x,
            max_y: self.max_y + v.y,
            max_z: self.max_z + v.z,
        }
    }

    pub fn inflate(&self, amount: f64) -> Self {
        Self::new(
            self.min_x - amount,
            self.min_y - amount,
            self.min_z - amount,
            self.max_x + amount,
            self.max_y + amount,
            self.max_z + amount,
        )
    }

    pub fn deflate(&self, amount: f64) -> Self {
        self.inflate(-amount)
    }

    pub fn inflate_by(&self, x: f64, y: f64, z: f64) -> Self {
        Self::new(
            self.min_x - x,
            self.min_y - y,
            self.min_z - z,
            self.max_x + x,
            self.max_y + y,
            self.max_z + z,
        )
    }

    pub fn distance_to_sqr_box(&self, other: &Self) -> f64 {
        let dx = (other.min_x - self.max_x)
            .max(self.min_x - other.max_x)
            .max(0.0);
        let dy = (other.min_y - self.max_y)
            .max(self.min_y - other.max_y)
            .max(0.0);
        let dz = (other.min_z - self.max_z)
            .max(self.min_z - other.max_z)
            .max(0.0);
        dx * dx + dy * dy + dz * dz
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
            && self.min_z < other.max_z
            && self.max_z > other.min_z
    }

    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
            (self.min_z + self.max_z) / 2.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_box_uses_float_half_width() {
        let bb = Aabb::of_size(Vec3::new(0.5, 64.0, 0.5), 0.9, 1.4);
        assert_eq!(bb.min_x, 0.5 - f64::from(0.9f32 / 2.0));
        assert_eq!(bb.max_y, 64.0 + f64::from(1.4f32));
    }

    #[test]
    fn expand_towards_only_grows_the_moving_side() {
        let bb = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0).expand_towards(Vec3::new(-0.5, 2.0, 0.0));
        assert_eq!(bb, Aabb::new(-0.5, 0.0, 0.0, 1.0, 3.0, 1.0));
    }

    #[test]
    fn step_order_follows_the_dominant_horizontal_axis() {
        assert_eq!(
            Axis::step_order(Vec3::new(0.1, 0.0, 0.5)),
            [Axis::Y, Axis::Z, Axis::X]
        );
        assert_eq!(
            Axis::step_order(Vec3::new(0.5, 0.0, 0.5)),
            [Axis::Y, Axis::X, Axis::Z]
        );
    }

    #[test]
    fn the_distance_between_boxes_is_zero_when_they_touch_or_overlap() {
        let one = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        assert_eq!(
            one.distance_to_sqr_box(&Aabb::new(0.5, 0.5, 0.5, 2.0, 2.0, 2.0)),
            0.0
        );
        assert_eq!(
            one.distance_to_sqr_box(&Aabb::new(1.0, 0.0, 0.0, 2.0, 1.0, 1.0)),
            0.0
        );
    }

    #[test]
    fn the_distance_between_boxes_is_taken_between_their_nearest_faces() {
        let one = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let apart = Aabb::new(4.0, 0.0, 5.0, 5.0, 1.0, 6.0);
        assert_eq!(one.distance_to_sqr_box(&apart), 9.0 + 16.0);
        assert_eq!(apart.distance_to_sqr_box(&one), 25.0);
        let below = Aabb::new(0.0, -3.0, 0.0, 1.0, -2.0, 1.0);
        assert_eq!(one.distance_to_sqr_box(&below), 4.0);
    }

    #[test]
    fn inflating_by_axis_grows_each_side_by_its_own_amount() {
        let grown = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0).inflate_by(4.0, 2.0, 3.0);
        assert_eq!(grown, Aabb::new(-4.0, -2.0, -3.0, 5.0, 3.0, 4.0));
    }
}
