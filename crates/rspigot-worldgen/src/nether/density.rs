use crate::climate::SmearedStack;
use crate::legacy::JavaRandom;

const XZ_SCALE: f64 = 0.25 * 684.412;
const Y_SCALE: f64 = 0.375 * 684.412;
const XZ_FACTOR: f64 = 80.0;
const Y_FACTOR: f64 = 60.0;
const SMEAR: f64 = 8.0;
const LIMIT_FACTOR: f64 = 0.999_984_741_210_937_5;
const MAIN_FACTOR: f64 = 12.75;

pub(crate) const CELL_XZ: usize = 4;
pub(crate) const CELL_Y: usize = 8;
const FINAL_SCALE: f32 = 0.64;

#[derive(Debug, Clone)]
pub(crate) struct BlendedNoise {
    min: SmearedStack,
    max: SmearedStack,
    main: SmearedStack,
}

impl BlendedNoise {
    pub(crate) fn new(seed: i64) -> Self {
        let mut random = JavaRandom::new(seed);
        let smear = Y_SCALE * SMEAR;
        Self {
            min: SmearedStack::create(&mut random, -15, smear, LIMIT_FACTOR),
            max: SmearedStack::create(&mut random, -15, smear, LIMIT_FACTOR),
            main: SmearedStack::create(&mut random, -7, smear / Y_FACTOR, MAIN_FACTOR),
        }
    }

    pub(crate) fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let alpha = (self.main.get(
            x * (XZ_SCALE / XZ_FACTOR),
            y * (Y_SCALE / Y_FACTOR),
            z * (XZ_SCALE / XZ_FACTOR),
        ) + 0.5)
            .clamp(0.0, 1.0);
        let low = self.min.get(x * XZ_SCALE, y * Y_SCALE, z * XZ_SCALE);
        let high = self.max.get(x * XZ_SCALE, y * Y_SCALE, z * XZ_SCALE);
        low + alpha * (high - low)
    }
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

fn clamped_map(value: f32, from: f32, to: f32, low: f32, high: f32) -> f32 {
    lerp(((value - from) / (to - from)).clamp(0.0, 1.0), low, high)
}

pub(crate) fn squeeze(value: f32) -> f32 {
    let clamped = value.clamp(-1.0, 1.0);
    clamped / 2.0 - clamped * clamped * clamped / 24.0
}

#[derive(Debug, Clone)]
pub(crate) struct NetherDensity {
    noise: BlendedNoise,
}

impl NetherDensity {
    pub(crate) fn new(seed: i64) -> Self {
        Self {
            noise: BlendedNoise::new(seed),
        }
    }

    pub(crate) fn pre_interpolation(&self, x: i32, y: i32, z: i32) -> f32 {
        let yf = y as f32;
        let top = lerp(
            clamped_map(yf, 104.0, 128.0, 1.0, 0.0),
            0.9375,
            self.noise.get(f64::from(x), f64::from(y), f64::from(z)),
        );
        lerp(clamped_map(yf, -8.0, 24.0, 0.0, 1.0), 2.5, top) * FINAL_SCALE
    }
}

#[derive(Debug)]
pub(crate) struct Corners {
    levels: usize,
    values: Vec<f32>,
}

const STRIDE: usize = 16 / CELL_XZ + 1;

impl Corners {
    pub(crate) fn build(
        density: &NetherDensity,
        x0: i32,
        z0: i32,
        min_y: i32,
        height: i32,
    ) -> Self {
        let levels = height as usize / CELL_Y + 1;
        let mut values = Vec::with_capacity(levels * STRIDE * STRIDE);
        for j in 0..levels {
            for k in 0..STRIDE {
                for i in 0..STRIDE {
                    values.push(density.pre_interpolation(
                        x0 + (i * CELL_XZ) as i32,
                        min_y + (j * CELL_Y) as i32,
                        z0 + (k * CELL_XZ) as i32,
                    ));
                }
            }
        }
        Self { levels, values }
    }

    fn at(&self, i: usize, j: usize, k: usize) -> f32 {
        self.values[(j * STRIDE + k) * STRIDE + i]
    }

    pub(crate) fn fill(&self, mut visit: impl FnMut(usize, usize, usize, f32)) {
        for j in 0..self.levels - 1 {
            for k in 0..STRIDE - 1 {
                for i in 0..STRIDE - 1 {
                    let corner = |dx: usize, dy: usize, dz: usize| self.at(i + dx, j + dy, k + dz);
                    for yy in 0..CELL_Y {
                        let ty = yy as f32 / CELL_Y as f32;
                        let plane =
                            |dx: usize, dz: usize| lerp(ty, corner(dx, 0, dz), corner(dx, 1, dz));
                        let (p00, p10) = (plane(0, 0), plane(1, 0));
                        let (p01, p11) = (plane(0, 1), plane(1, 1));
                        for xx in 0..CELL_XZ {
                            let tx = xx as f32 / CELL_XZ as f32;
                            let (r0, r1) = (lerp(tx, p00, p10), lerp(tx, p01, p11));
                            for zz in 0..CELL_XZ {
                                let tz = zz as f32 / CELL_XZ as f32;
                                visit(
                                    i * CELL_XZ + xx,
                                    j * CELL_Y + yy,
                                    k * CELL_XZ + zz,
                                    squeeze(lerp(tz, r0, r1)),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
