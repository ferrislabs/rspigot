use crate::climate::SmearedStack;
use crate::legacy::JavaRandom;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlendParams {
    pub(crate) xz_scale: f64,
    pub(crate) y_scale: f64,
    pub(crate) xz_factor: f64,
    pub(crate) y_factor: f64,
    pub(crate) smear: f64,
}

pub(crate) const NETHER_BLEND: BlendParams = BlendParams {
    xz_scale: 0.25,
    y_scale: 0.375,
    xz_factor: 80.0,
    y_factor: 60.0,
    smear: 8.0,
};

pub(crate) const END_BLEND: BlendParams = BlendParams {
    xz_scale: 0.25,
    y_scale: 0.25,
    xz_factor: 80.0,
    y_factor: 160.0,
    smear: 4.0,
};

const LIMIT_FACTOR: f64 = 0.999_984_741_210_937_5;
const MAIN_FACTOR: f64 = 12.75;
const MULTIPLIER: f64 = 684.412;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Lattice {
    pub(crate) xz: usize,
    pub(crate) y: usize,
}

pub(crate) const NETHER_LATTICE: Lattice = Lattice { xz: 4, y: 8 };
pub(crate) const END_LATTICE: Lattice = Lattice { xz: 8, y: 4 };
const FINAL_SCALE: f32 = 0.64;

#[derive(Debug, Clone)]
pub(crate) struct BlendedNoise {
    min: SmearedStack,
    max: SmearedStack,
    main: SmearedStack,
    xz: f64,
    y: f64,
    xz_main: f64,
    y_main: f64,
}

impl BlendedNoise {
    pub(crate) fn new(seed: i64, params: BlendParams) -> Self {
        let mut random = JavaRandom::new(seed);
        let (xz, y) = (params.xz_scale * MULTIPLIER, params.y_scale * MULTIPLIER);
        let smear = y * params.smear;
        Self {
            min: SmearedStack::create(&mut random, -15, smear, LIMIT_FACTOR),
            max: SmearedStack::create(&mut random, -15, smear, LIMIT_FACTOR),
            main: SmearedStack::create(&mut random, -7, smear / params.y_factor, MAIN_FACTOR),
            xz,
            y,
            xz_main: xz / params.xz_factor,
            y_main: y / params.y_factor,
        }
    }

    pub(crate) fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let alpha = (self
            .main
            .get(x * self.xz_main, y * self.y_main, z * self.xz_main)
            + 0.5)
            .clamp(0.0, 1.0);
        let low = self.min.get(x * self.xz, y * self.y, z * self.xz);
        let high = self.max.get(x * self.xz, y * self.y, z * self.xz);
        low + alpha * (high - low)
    }
}

pub(crate) fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

pub(crate) fn clamped_map(value: f32, from: f32, to: f32, low: f32, high: f32) -> f32 {
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
            noise: BlendedNoise::new(seed, NETHER_BLEND),
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
    lattice: Lattice,
    levels: usize,
    values: Vec<f32>,
}

impl Corners {
    pub(crate) fn build(
        sample: impl Fn(i32, i32, i32) -> f32,
        lattice: Lattice,
        x0: i32,
        z0: i32,
        min_y: i32,
        height: i32,
    ) -> Self {
        let levels = height as usize / lattice.y + 1;
        let stride = 16 / lattice.xz + 1;
        let mut values = Vec::with_capacity(levels * stride * stride);
        for j in 0..levels {
            for k in 0..stride {
                for i in 0..stride {
                    values.push(sample(
                        x0 + (i * lattice.xz) as i32,
                        min_y + (j * lattice.y) as i32,
                        z0 + (k * lattice.xz) as i32,
                    ));
                }
            }
        }
        Self {
            lattice,
            levels,
            values,
        }
    }

    fn stride(&self) -> usize {
        16 / self.lattice.xz + 1
    }

    fn at(&self, i: usize, j: usize, k: usize) -> f32 {
        let stride = self.stride();
        self.values[(j * stride + k) * stride + i]
    }

    pub(crate) fn fill(&self, mut visit: impl FnMut(usize, usize, usize, f32)) {
        let Lattice { xz, y: cell_y } = self.lattice;
        let cells = self.stride() - 1;
        for j in 0..self.levels - 1 {
            for k in 0..cells {
                for i in 0..cells {
                    let corner = |dx: usize, dy: usize, dz: usize| self.at(i + dx, j + dy, k + dz);
                    for yy in 0..cell_y {
                        let ty = yy as f32 / cell_y as f32;
                        let plane =
                            |dx: usize, dz: usize| lerp(ty, corner(dx, 0, dz), corner(dx, 1, dz));
                        let (p00, p10) = (plane(0, 0), plane(1, 0));
                        let (p01, p11) = (plane(0, 1), plane(1, 1));
                        for xx in 0..xz {
                            let tx = xx as f32 / xz as f32;
                            let (r0, r1) = (lerp(tx, p00, p10), lerp(tx, p01, p11));
                            for zz in 0..xz {
                                let tz = zz as f32 / xz as f32;
                                visit(
                                    i * xz + xx,
                                    j * cell_y + yy,
                                    k * xz + zz,
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
