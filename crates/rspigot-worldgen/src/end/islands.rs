use crate::legacy::JavaRandom;
use crate::simplex::Simplex;

const SKIPPED_DRAWS: u32 = 17_292;
const CENTRAL_RADIUS: f32 = 100.0;

#[derive(Debug, Clone)]
pub(crate) struct EndIslands {
    noise: Simplex,
}

fn island_height(noise: &Simplex, section_x: i32, section_z: i32) -> f32 {
    let (base_x, base_z) = (section_x / 2, section_z / 2);
    let (rest_x, rest_z) = (section_x % 2, section_z % 2);
    let mut best = -100.0_f32;
    for dx in -12..=12 {
        for dz in -12..=12 {
            let (x, z) = (i64::from(base_x + dx), i64::from(base_z + dz));
            if x * x + z * z <= 4096 {
                continue;
            }
            if (noise.value(x as f64, z as f64) as f32) < -0.9 {
                let shape = (x as f32).abs() * 3439.0 + (z as f32).abs() * 147.0;
                let size = shape % 13.0 + 9.0;
                let deltax = (rest_x - dx * 2) as f32;
                let deltaz = (rest_z - dz * 2) as f32;
                let distance = (deltax * deltax + deltaz * deltaz).sqrt();
                let value = (100.0 - distance * size).clamp(-100.0, 80.0);
                best = best.max(value);
            }
        }
    }
    best
}

impl EndIslands {
    pub(crate) fn new(seed: i64) -> Self {
        let mut random = JavaRandom::new(seed);
        random.skip(SKIPPED_DRAWS);
        Self {
            noise: Simplex::new(&mut random),
        }
    }

    pub(crate) fn outer(&self, x: i32, z: i32) -> f32 {
        (island_height(&self.noise, x / 8, z / 8) - 8.0) / 128.0
    }

    pub(crate) fn erosion(&self, x: i32, z: i32) -> f32 {
        let distance = ((x as f32) * (x as f32) + (z as f32) * (z as f32)).sqrt();
        let central = (CENTRAL_RADIUS - distance).clamp(-100.0, 80.0);
        let main = (central - 8.0) * 0.007_812_5;
        main.max(self.outer(x, z))
    }
}
