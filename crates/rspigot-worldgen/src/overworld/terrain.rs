use crate::climate::{BiomeSource, ClimateScratch};
use crate::perlin::Fractal;
use crate::rng::mix;

#[cfg(test)]
pub(crate) fn smooth(from: f64, to: f64, value: f64) -> f64 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) const SURFACE_SHIFT: i32 = 10;
const DETAIL_HEIGHT: f64 = 2.5;

#[derive(Debug, Clone)]
pub(crate) struct Heights {
    corners: [[f64; 5]; 5],
}

impl Heights {
    pub(crate) fn at(&self, dx: usize, dz: usize) -> f64 {
        let (cx, cz) = (dx >> 2, dz >> 2);
        let (fx, fz) = ((dx & 3) as f64 / 4.0, (dz & 3) as f64 / 4.0);
        let c = &self.corners;
        let near = c[cz][cx] + (c[cz][cx + 1] - c[cz][cx]) * fx;
        let far = c[cz + 1][cx] + (c[cz + 1][cx + 1] - c[cz + 1][cx]) * fx;
        near + (far - near) * fz
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Terrain {
    hills: Fractal,
}

impl Terrain {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            hills: Fractal::new(mix(seed ^ 3u64.wrapping_mul(0x9E37_79B9_7F4A_7C15)), 4),
        }
    }

    pub(crate) fn heights(
        &self,
        source: &BiomeSource,
        scratch: &mut ClimateScratch,
        x0: i32,
        z0: i32,
    ) -> Heights {
        let corners = std::array::from_fn(|cz| {
            std::array::from_fn(|cx| {
                let estimate =
                    source.surface_estimate(scratch, x0 + cx as i32 * 4, z0 + cz as i32 * 4);
                f64::from(estimate) - f64::from(SURFACE_SHIFT)
            })
        });
        Heights { corners }
    }

    pub(crate) fn detail(&self, x: i32, z: i32) -> f64 {
        let value = self.hills.at2(f64::from(x) / 96.0, f64::from(z) / 96.0) * 2.2;
        value.clamp(-1.0, 1.0) * DETAIL_HEIGHT
    }
}

#[cfg(test)]
mod tests {
    use rspigot_data::multi_noise::ClimateVariant;

    use super::*;

    fn terrain(seed: u64) -> Terrain {
        Terrain::new(seed)
    }

    #[test]
    fn smooth_handles_both_directions() {
        assert_eq!(smooth(0.0, 1.0, -1.0), 0.0);
        assert_eq!(smooth(0.0, 1.0, 2.0), 1.0);
        assert_eq!(smooth(1.0, 0.0, 2.0), 0.0);
        assert_eq!(smooth(1.0, 0.0, -1.0), 1.0);
        assert!((smooth(0.0, 1.0, 0.5) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn heights_are_deterministic_and_seed_dependent() {
        let (a, b) = (
            BiomeSource::new(1, ClimateVariant::Normal),
            BiomeSource::new(2, ClimateVariant::Normal),
        );
        let t = terrain(1);
        let (mut sa, mut sb) = (a.scratch(), b.scratch());
        let first = t.heights(&a, &mut sa, 96, -48);
        let again = t.heights(&a, &mut sa, 96, -48);
        let other = t.heights(&b, &mut sb, 96, -48);
        assert_eq!(first.at(5, 9), again.at(5, 9));
        assert_ne!(first.at(5, 9), other.at(5, 9));
    }

    #[test]
    fn heights_interpolate_between_corners() {
        let heights = Heights {
            corners: std::array::from_fn(|z| std::array::from_fn(|x| (x * 4 + z * 8) as f64)),
        };
        assert_eq!(heights.at(0, 0), 0.0);
        assert_eq!(heights.at(2, 0), 2.0);
        assert_eq!(heights.at(0, 4), 8.0);
        assert_eq!(heights.at(15, 15), 15.0 * 1.0 + 15.0 * 2.0);
    }

    #[test]
    fn terrain_spans_ocean_and_land() {
        let source = BiomeSource::new(4242, ClimateVariant::Normal);
        let t = terrain(4242);
        let mut scratch = source.scratch();
        let (mut below, mut above, mut high) = (0, 0, 0);
        for i in 0..40 {
            for j in 0..40 {
                let h = t
                    .heights(&source, &mut scratch, i * 128 - 2560, j * 128 - 2560)
                    .at(0, 0);
                if h < 60.0 {
                    below += 1;
                }
                if h > 66.0 {
                    above += 1;
                }
                if h > 130.0 {
                    high += 1;
                }
            }
        }
        assert!(below > 150, "{below}");
        assert!(above > 150, "{above}");
        assert!(high > 0, "{high}");
    }
}
