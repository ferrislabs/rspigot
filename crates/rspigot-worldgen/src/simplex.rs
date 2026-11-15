use crate::legacy::JavaRandom;

const GRADIENT: [[i32; 3]; 12] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
];
const F2: f64 = 0.366_025_403_784_438_6;
const G2: f64 = 0.211_324_865_405_187_13;

#[derive(Debug, Clone)]
struct Simplex {
    p: [u8; 256],
    xo: f64,
    yo: f64,
}

impl Simplex {
    fn new(random: &mut JavaRandom) -> Self {
        let xo = random.next_double() * 256.0;
        let yo = random.next_double() * 256.0;
        random.next_double();
        let mut p: [u8; 256] = std::array::from_fn(|i| i as u8);
        for i in 0..256usize {
            let j = random.next_int(256 - i as i32) as usize;
            p.swap(i, i + j);
        }
        Self { p, xo, yo }
    }

    fn perm(&self, i: i32) -> i32 {
        i32::from(self.p[(i & 0xFF) as usize])
    }

    fn corner(index: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut t = 0.5 - x * x - y * y - z * z;
        if t < 0.0 {
            0.0
        } else {
            t *= t;
            let g = GRADIENT[index as usize];
            t * t * (f64::from(g[0]) * x + f64::from(g[1]) * y + f64::from(g[2]) * z)
        }
    }

    fn value(&self, xin: f64, yin: f64) -> f64 {
        let s = (xin + yin) * F2;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let t = f64::from(i + j) * G2;
        let xd = xin - (f64::from(i) - t);
        let yd = yin - (f64::from(j) - t);
        let (i1, j1) = if xd > yd { (1, 0) } else { (0, 1) };
        let x1 = xd - f64::from(i1) + G2;
        let y1 = yd - f64::from(j1) + G2;
        let x2 = xd - 1.0 + 2.0 * G2;
        let y2 = yd - 1.0 + 2.0 * G2;
        let ii = i & 0xFF;
        let jj = j & 0xFF;
        let g0 = self.perm(ii + self.perm(jj)) % 12;
        let g1 = self.perm(ii + i1 + self.perm(jj + j1)) % 12;
        let g2 = self.perm(ii + 1 + self.perm(jj + 1)) % 12;
        70.0 * (Self::corner(g0, xd, yd, 0.0)
            + Self::corner(g1, x1, y1, 0.0)
            + Self::corner(g2, x2, y2, 0.0))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PerlinSimplex {
    levels: Vec<Option<Simplex>>,
    input_factor: f64,
    value_factor: f64,
}

impl PerlinSimplex {
    pub(crate) fn new(random: &mut JavaRandom, octaves: &[i32]) -> Self {
        debug_assert!(octaves.last().is_some_and(|last| *last == 0));
        let first = -octaves[0];
        let last = octaves[octaves.len() - 1];
        let total = first + last + 1;
        let base = Simplex::new(random);
        let mut levels: Vec<Option<Simplex>> = vec![None; total as usize];
        let has = |octave: i32| octaves.contains(&octave);
        if last >= 0 && last < total && has(0) {
            levels[last as usize] = Some(base.clone());
        }
        for index in (last + 1)..total {
            if index >= 0 && has(last - index) {
                levels[index as usize] = Some(Simplex::new(random));
            } else {
                random.skip(262);
            }
        }
        Self {
            levels,
            input_factor: 2.0_f64.powi(last),
            value_factor: 1.0 / (2.0_f64.powi(total) - 1.0),
        }
    }

    pub(crate) fn value(&self, x: f64, y: f64, use_offsets: bool) -> f64 {
        let mut sum = 0.0;
        let mut input = self.input_factor;
        let mut factor = self.value_factor;
        for level in &self.levels {
            if let Some(level) = level {
                let (ox, oy) = if use_offsets {
                    (level.xo, level.yo)
                } else {
                    (0.0, 0.0)
                };
                sum += level.value(x * input + ox, y * input + oy) * factor;
            }
            input /= 2.0;
            factor *= 2.0;
        }
        sum
    }
}
