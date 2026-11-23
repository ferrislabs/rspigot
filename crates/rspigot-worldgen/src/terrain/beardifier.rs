use std::sync::OnceLock;

type Pos = (i32, i32, i32);

const KERNEL_SIZE: i32 = 24;
const KERNEL_RADIUS: i32 = 12;
const KERNEL_CELLS: usize = 24 * 24 * 24;
const INFLATE: i32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Adjustment {
    None,
    Bury,
    BeardThin,
    BeardBox,
    Encapsulate,
}

impl Adjustment {
    pub(crate) fn parse(name: &str) -> Self {
        match name.strip_prefix("minecraft:").unwrap_or(name) {
            "bury" => Self::Bury,
            "beard_thin" => Self::BeardThin,
            "beard_box" => Self::BeardBox,
            "encapsulate" => Self::Encapsulate,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone)]
struct Rigid {
    min: Pos,
    max: Pos,
    adjustment: Adjustment,
    ground_delta: i32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Beardifier {
    pieces: Vec<Rigid>,
    junctions: Vec<Pos>,
    affected: Option<(Pos, Pos)>,
}

fn kernel() -> &'static [f32] {
    static KERNEL: OnceLock<Vec<f32>> = OnceLock::new();
    KERNEL.get_or_init(|| {
        let mut table = vec![0.0f32; KERNEL_CELLS];
        for i in 0..KERNEL_SIZE {
            for j in 0..KERNEL_SIZE {
                for k in 0..KERNEL_SIZE {
                    let index = (i * 576 + j * 24 + k) as usize;
                    table[index] =
                        contribution(j - KERNEL_RADIUS, k - KERNEL_RADIUS, i - KERNEL_RADIUS)
                            as f32;
                }
            }
        }
        table
    })
}

fn contribution(dx: i32, dy: i32, dz: i32) -> f64 {
    let (x, y, z) = (f64::from(dx), f64::from(dy) + 0.5, f64::from(dz));
    let length_squared = x * x + y * y + z * z;
    std::f64::consts::E.powf(-length_squared / 16.0)
}

fn fast_inverse_square_root(value: f64) -> f64 {
    let half = 0.5 * value;
    let bits = value.to_bits() as i64;
    let bits = 6_910_469_410_427_058_090_i64.wrapping_sub(bits >> 1);
    let guess = f64::from_bits(bits as u64);
    guess * (1.5 - half * guess * guess)
}

fn in_kernel(index: i32) -> bool {
    (0..KERNEL_SIZE).contains(&index)
}

fn bury_contribution(dx: f32, dy: f32, dz: f32) -> f32 {
    let length_squared = dx * dx + dy * dy + dz * dz;
    if 36.0 <= length_squared {
        return 0.0;
    }
    1.0 - length_squared.sqrt() / 6.0
}

fn beard_contribution(dx: i32, dy: i32, dz: i32, y_to_ground: i32) -> f32 {
    let (kx, ky, kz) = (dx + KERNEL_RADIUS, dy + KERNEL_RADIUS, dz + KERNEL_RADIUS);
    if !(in_kernel(kx) && in_kernel(ky) && in_kernel(kz)) {
        return 0.0;
    }
    let ground = y_to_ground as f32 + 0.5;
    let (fx, fz) = (dx as f32, dz as f32);
    let length_squared = fx * fx + ground * ground + fz * fz;
    let inverse = fast_inverse_square_root(f64::from(length_squared / 2.0));
    ((-ground * inverse as f32) / 2.0) * kernel()[(kz * 576 + kx * 24 + ky) as usize]
}

impl Beardifier {
    fn include(&mut self, min: Pos, max: Pos) {
        self.affected = Some(match self.affected {
            None => (min, max),
            Some((low, high)) => (
                (low.0.min(min.0), low.1.min(min.1), low.2.min(min.2)),
                (high.0.max(max.0), high.1.max(max.1), high.2.max(max.2)),
            ),
        });
    }

    pub(crate) fn add_piece(
        &mut self,
        min: Pos,
        max: Pos,
        adjustment: Adjustment,
        ground_delta: i32,
    ) {
        self.pieces.push(Rigid {
            min,
            max,
            adjustment,
            ground_delta,
        });
        self.include(min, max);
    }

    pub(crate) fn add_junction(&mut self, source: Pos) {
        self.junctions.push(source);
        self.include(source, source);
    }

    pub(crate) fn sample(&self, x: i32, y: i32, z: i32) -> f32 {
        let Some((low, high)) = self.affected else {
            return 0.0;
        };
        let inside =
            |value: i32, min: i32, max: i32| value >= min - INFLATE && value <= max + INFLATE;
        if !(inside(x, low.0, high.0) && inside(y, low.1, high.1) && inside(z, low.2, high.2)) {
            return 0.0;
        }
        let mut total = 0.0f32;
        for piece in &self.pieces {
            let dx = 0.max((piece.min.0 - x).max(x - piece.max.0));
            let dz = 0.max((piece.min.2 - z).max(z - piece.max.2));
            let base = piece.min.1 + piece.ground_delta;
            let y_to_ground = y - base;
            let dy = match piece.adjustment {
                Adjustment::None => 0,
                Adjustment::Bury | Adjustment::BeardThin => y_to_ground,
                Adjustment::BeardBox => 0.max((base - y).max(y - piece.max.1)),
                Adjustment::Encapsulate => 0.max((piece.min.1 - y).max(y - piece.max.1)),
            };
            total += match piece.adjustment {
                Adjustment::None => 0.0,
                Adjustment::Bury => bury_contribution(dx as f32, dy as f32 / 2.0, dz as f32),
                Adjustment::BeardThin | Adjustment::BeardBox => {
                    beard_contribution(dx, dy, dz, y_to_ground) * 0.8
                }
                Adjustment::Encapsulate => {
                    bury_contribution(dx as f32 / 2.0, dy as f32 / 2.0, dz as f32 / 2.0) * 0.8
                }
            };
        }
        for junction in &self.junctions {
            let dy = y - junction.1;
            total += beard_contribution(x - junction.0, dy, z - junction.2, dy) * 0.4;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(adjustment: Adjustment, ground_delta: i32) -> Beardifier {
        let mut beardifier = Beardifier::default();
        beardifier.add_piece((0, 64, 0), (10, 70, 10), adjustment, ground_delta);
        beardifier
    }

    #[test]
    fn the_kernel_is_the_gaussian_of_the_vanilla_table() {
        let table = kernel();
        let centre = (12 * 576 + 12 * 24 + 12) as usize;
        assert!((table[centre] - (-0.25f64 / 16.0).exp() as f32).abs() < 1e-6);
        let edge = (12 * 576 + 23 * 24 + 12) as usize;
        let expected = (-(121.0f64 + 0.25) / 16.0).exp() as f32;
        assert!((table[edge] - expected).abs() < 1e-6);
    }

    #[test]
    fn a_beard_piles_terrain_under_a_piece_and_carves_above_it() {
        let beardifier = one(Adjustment::BeardThin, 0);
        let under = beardifier.sample(5, 62, 5);
        let above = beardifier.sample(5, 66, 5);
        assert!(under > 0.0, "{under}");
        assert!(above < 0.0, "{above}");
        let far = beardifier.sample(5, 62, 5 + 40);
        assert_eq!(far, 0.0);
    }

    #[test]
    fn beard_thin_matches_the_vanilla_formula_at_a_known_point() {
        let beardifier = one(Adjustment::BeardThin, 0);
        let value = beardifier.sample(5, 63, 5);
        let ground = -1.0f32 + 0.5;
        let inverse = 1.0 / ((ground * ground / 2.0) as f64).sqrt();
        let gaussian = (-(0.25f64 + 0.0) / 16.0).exp() as f32;
        let expected = ((-ground * inverse as f32) / 2.0) * gaussian * 0.8;
        assert!((value - expected).abs() < 0.01, "{value} vs {expected}");
    }

    #[test]
    fn a_beard_box_ignores_the_height_inside_its_box_unlike_a_thin_beard() {
        let thin = one(Adjustment::BeardThin, 0).sample(5, 68, 5);
        let boxed = one(Adjustment::BeardBox, 0).sample(5, 68, 5);
        assert!(boxed < thin, "{boxed} should carve deeper than {thin}");
        let below = one(Adjustment::BeardBox, 0).sample(5, 60, 5);
        assert!(below > 0.0);
    }

    #[test]
    fn bury_fills_a_six_block_halo_and_nothing_beyond() {
        let beardifier = one(Adjustment::Bury, 0);
        assert!((beardifier.sample(5, 64, 5) - 1.0).abs() < 1e-6);
        assert!((beardifier.sample(10 + 3, 64, 5) - 0.5).abs() < 1e-6);
        assert_eq!(beardifier.sample(10 + 6, 64, 5), 0.0);
    }

    #[test]
    fn encapsulate_halves_the_distance_and_scales_by_point_eight() {
        let beardifier = one(Adjustment::Encapsulate, 0);
        assert!((beardifier.sample(5, 67, 5) - 0.8).abs() < 1e-6);
        let outside = beardifier.sample(10 + 6, 67, 5);
        assert!(
            (outside - 0.8 * (1.0 - 3.0 / 6.0)).abs() < 1e-6,
            "{outside}"
        );
    }

    #[test]
    fn junctions_pull_terrain_towards_their_source() {
        let mut beardifier = Beardifier::default();
        beardifier.add_junction((0, 64, 0));
        assert!(beardifier.sample(1, 60, 1) > 0.0);
        assert!(beardifier.sample(1, 70, 1) < 0.0);
        assert_eq!(beardifier.sample(1, 64 + 40, 1), 0.0);
    }

    #[test]
    fn an_empty_beardifier_adds_nothing() {
        let beardifier = Beardifier::default();
        assert_eq!(beardifier.sample(0, 0, 0), 0.0);
    }
}
