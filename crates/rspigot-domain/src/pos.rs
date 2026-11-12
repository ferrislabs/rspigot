use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x >> 4, self.z >> 4)
    }

    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    pub fn center(self) -> Vec3 {
        Vec3::new(
            f64::from(self.x) + 0.5,
            f64::from(self.y),
            f64::from(self.z) + 0.5,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SectionPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectionPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn of(pos: BlockPos) -> Self {
        Self::new(pos.x >> 4, pos.y >> 4, pos.z >> 4)
    }

    pub const fn from_chunk(chunk: ChunkPos, y: i32) -> Self {
        Self::new(chunk.x, y, chunk.z)
    }

    pub const fn chunk(self) -> ChunkPos {
        ChunkPos::new(self.x, self.z)
    }

    pub const fn origin(self) -> BlockPos {
        BlockPos::new(self.x << 4, self.y << 4, self.z << 4)
    }

    pub const fn center(self) -> BlockPos {
        self.origin().offset(8, 8, 8)
    }

    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    pub const fn chebyshev_distance(self, other: Self) -> i32 {
        let dx = (self.x - other.x).abs();
        let dy = (self.y - other.y).abs();
        let dz = (self.z - other.z).abs();
        let horizontal = if dx > dz { dx } else { dz };
        if horizontal > dy { horizontal } else { dy }
    }

    pub fn cube(self, radius: i32) -> impl Iterator<Item = Self> {
        let (x, y, z) = (self.x, self.y, self.z);
        (z - radius..=z + radius).flat_map(move |sz| {
            (y - radius..=y + radius)
                .flat_map(move |sy| (x - radius..=x + radius).map(move |sx| Self::new(sx, sy, sz)))
        })
    }

    pub const fn relative(pos: BlockPos) -> u16 {
        (((pos.x & 15) << 8) | ((pos.y & 15) << 4) | (pos.z & 15)) as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn from_block(x: f64, z: f64) -> Self {
        Self::new((x.floor() as i32) >> 4, (z.floor() as i32) >> 4)
    }

    pub const fn chebyshev_distance(self, other: Self) -> i32 {
        let dx = (self.x - other.x).abs();
        let dz = (self.z - other.z).abs();
        if dx > dz { dx } else { dz }
    }

    pub const fn squared_distance(self, other: Self) -> i64 {
        let dx = (self.x - other.x) as i64;
        let dz = (self.z - other.z) as i64;
        dx * dx + dz * dz
    }

    pub const fn region(self) -> (i32, i32) {
        (self.x >> 5, self.z >> 5)
    }

    pub const fn min_block_x(self) -> i32 {
        self.x << 4
    }

    pub const fn min_block_z(self) -> i32 {
        self.z << 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn block(self) -> BlockPos {
        BlockPos::new(
            self.x.floor() as i32,
            self.y.floor() as i32,
            self.z.floor() as i32,
        )
    }

    pub fn chunk(self) -> ChunkPos {
        ChunkPos::from_block(self.x, self.z)
    }

    pub fn distance_squared(self, other: Self) -> f64 {
        let d = self - other;
        d.x * d.x + d.y * d.y + d.z * d.z
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }

    pub fn horizontal_distance_sqr(self) -> f64 {
        self.x * self.x + self.z * self.z
    }

    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    pub fn multiply(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x * x, self.y * y, self.z * z)
    }

    pub fn normalize(self) -> Self {
        let length = self.length();
        if length < 9.999_999_747_378_752e-6 {
            Self::ZERO
        } else {
            Self::new(self.x / length, self.y / length, self.z / length)
        }
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub const fn new(yaw: f32, pitch: f32) -> Self {
        Self { yaw, pitch }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_block_maps_to_negative_chunk() {
        assert_eq!(BlockPos::new(-1, 0, -17).chunk(), ChunkPos::new(-1, -2));
        assert_eq!(Vec3::new(-0.5, 64.0, 15.9).chunk(), ChunkPos::new(-1, 0));
    }

    #[test]
    fn region_of_negative_chunk() {
        assert_eq!(ChunkPos::new(-1, 33).region(), (-1, 1));
    }
}
