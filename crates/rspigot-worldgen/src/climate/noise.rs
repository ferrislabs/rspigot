use rspigot_data::multi_noise::NoiseParameters;

use super::random::{Positional, Xoroshiro};
use crate::legacy::JavaRandom;

pub(crate) trait NoiseDraw {
    fn draw_double(&mut self) -> f64;
    fn draw_bounded(&mut self, bound: u32) -> u32;
}

impl NoiseDraw for Xoroshiro {
    fn draw_double(&mut self) -> f64 {
        self.next_double()
    }

    fn draw_bounded(&mut self, bound: u32) -> u32 {
        self.next_int(bound)
    }
}

impl NoiseDraw for JavaRandom {
    fn draw_double(&mut self) -> f64 {
        self.next_double()
    }

    fn draw_bounded(&mut self, bound: u32) -> u32 {
        self.next_int(bound as i32) as u32
    }
}

fn java_hash(name: &str) -> i32 {
    name.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

const GRADIENTS: [[i8; 3]; 16] = [
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
    [1, 1, 0],
    [0, -1, 1],
    [-1, 1, 0],
    [0, -1, -1],
];
const SECOND_LAYER_FREQUENCY: f64 = 1.018_126_888_217_522_7;
const DEVIATION_FACTOR: f64 = 0.270_224_783_124_521_1;
const WRAP_PERIOD: f64 = 33_554_432.0;

fn wrap(x: f64) -> f64 {
    let half = f64::from_bits(16_777_216.0_f64.to_bits() - 1);
    if (-half..half).contains(&x) {
        x
    } else {
        x - (x / WRAP_PERIOD + 0.5).floor() * WRAP_PERIOD
    }
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

fn lerp2(tx: f32, ty: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    lerp(ty, lerp(tx, a, b), lerp(tx, c, d))
}

fn smoothstep(x: f32) -> f32 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

fn dot(hash: usize, x: f32, y: f32, z: f32) -> f32 {
    let g = GRADIENTS[hash & 15];
    f32::from(g[0]) * x + f32::from(g[1]) * y + f32::from(g[2]) * z
}

#[derive(Debug, Clone)]
pub(crate) struct Perlin {
    perms: [u8; 256],
    offset: [f64; 3],
}

impl Perlin {
    fn new(rng: &mut impl NoiseDraw) -> Self {
        let offset = [
            rng.draw_double() * 256.0,
            rng.draw_double() * 256.0,
            rng.draw_double() * 256.0,
        ];
        let mut perms: [u8; 256] = std::array::from_fn(|i| i as u8);
        for i in 0..256usize {
            let j = rng.draw_bounded(256 - i as u32) as usize;
            perms.swap(i, i + j);
        }
        Self { perms, offset }
    }

    fn permute(&self, x: i32) -> i32 {
        i32::from(self.perms[(x & 255) as usize])
    }

    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let x = wrap(x) + self.offset[0];
        let y = wrap(y) + self.offset[1];
        let z = wrap(z) + self.offset[2];
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let zi = z.floor() as i32;
        let rx = (x - f64::from(xi)) as f32;
        let ry = (y - f64::from(yi)) as f32;
        let rz = (z - f64::from(zi)) as f32;
        let a = self.permute(xi);
        let b = self.permute(xi + 1);
        let aa = self.permute(a + yi);
        let ab = self.permute(a + yi + 1);
        let ba = self.permute(b + yi);
        let bb = self.permute(b + yi + 1);
        let g = |hash: i32, dx: f32, dy: f32, dz: f32| dot(hash as usize, dx, dy, dz);
        let c000 = g(self.permute(aa + zi), rx, ry, rz);
        let c100 = g(self.permute(ba + zi), rx - 1.0, ry, rz);
        let c010 = g(self.permute(ab + zi), rx, ry - 1.0, rz);
        let c110 = g(self.permute(bb + zi), rx - 1.0, ry - 1.0, rz);
        let c001 = g(self.permute(aa + zi + 1), rx, ry, rz - 1.0);
        let c101 = g(self.permute(ba + zi + 1), rx - 1.0, ry, rz - 1.0);
        let c011 = g(self.permute(ab + zi + 1), rx, ry - 1.0, rz - 1.0);
        let c111 = g(self.permute(bb + zi + 1), rx - 1.0, ry - 1.0, rz - 1.0);
        let (sx, sy, sz) = (smoothstep(rx), smoothstep(ry), smoothstep(rz));
        lerp(
            sz,
            lerp2(sx, sy, c000, c100, c010, c110),
            lerp2(sx, sy, c001, c101, c011, c111),
        )
    }
}

#[derive(Debug, Clone)]
struct Layer {
    perlin: Perlin,
    frequency: f64,
    amplitude: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct NoiseStack {
    layers: Vec<Layer>,
}

struct Octave {
    index: i32,
    frequency: f64,
    amplitude: f64,
}

fn compensated_sum(values: &[f64]) -> f64 {
    let (mut sum, mut compensation) = (0.0_f64, 0.0_f64);
    for value in values {
        let adjusted = value - compensation;
        let next = sum + adjusted;
        compensation = (next - sum) - adjusted;
        sum = next;
    }
    sum - compensation
}

fn octaves(parameters: &NoiseParameters) -> Vec<Octave> {
    let count = parameters.octave_count as i32;
    let mut frequency = 2.0_f64.powi(parameters.base_octave);
    let mut amplitude =
        parameters.base_amplitude * (0.5_f64.powi(-(count - 1)) / (0.5_f64.powi(-count) - 1.0));
    let mut result = Vec::new();
    for i in 0..count {
        let modifier = parameters
            .amplitude_modifiers
            .get(i as usize)
            .copied()
            .unwrap_or(1.0);
        if modifier != 0.0 {
            result.push(Octave {
                index: parameters.base_octave + i,
                frequency,
                amplitude: amplitude * modifier,
            });
        }
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    result
}

fn normalization(octaves: &[Octave]) -> f64 {
    let amplitudes: Vec<f64> = octaves.iter().map(|o| o.amplitude).collect();
    let target = compensated_sum(&amplitudes);
    let deviation = octaves
        .iter()
        .map(|o| {
            let v = o.amplitude.abs() * DEVIATION_FACTOR;
            v * v
        })
        .sum::<f64>()
        .sqrt();
    if deviation == 0.0 {
        0.0
    } else {
        (target * 0.333_333_333_333_333_3) / (deviation * 2.0_f64.sqrt())
    }
}

impl NoiseStack {
    pub(crate) fn create(parameters: &NoiseParameters, rng: &mut Xoroshiro) -> Self {
        let first: Positional = rng.fork_positional();
        let second: Positional = rng.fork_positional();
        let octaves = octaves(parameters);
        let factor = normalization(&octaves);
        let mut layers = Vec::with_capacity(octaves.len() * 2);
        for octave in &octaves {
            let seed = format!("octave_{}", octave.index);
            let amplitude = (factor * octave.amplitude) as f32;
            layers.push(Layer {
                perlin: Perlin::new(&mut first.named(&seed)),
                frequency: octave.frequency,
                amplitude,
            });
            layers.push(Layer {
                perlin: Perlin::new(&mut second.named(&seed)),
                frequency: octave.frequency * SECOND_LAYER_FREQUENCY,
                amplitude,
            });
        }
        Self { layers }
    }

    pub(crate) fn create_legacy(parameters: &NoiseParameters, seed: i64) -> Self {
        let mut rng = JavaRandom::new(seed);
        let first = rng.next_long();
        let second = rng.next_long();
        let octaves = octaves(parameters);
        let factor = normalization(&octaves);
        let mut layers = Vec::with_capacity(octaves.len() * 2);
        for octave in &octaves {
            let hash = i64::from(java_hash(&format!("octave_{}", octave.index)));
            let amplitude = (factor * octave.amplitude) as f32;
            layers.push(Layer {
                perlin: Perlin::new(&mut JavaRandom::new(hash ^ first)),
                frequency: octave.frequency,
                amplitude,
            });
            layers.push(Layer {
                perlin: Perlin::new(&mut JavaRandom::new(hash ^ second)),
                frequency: octave.frequency * SECOND_LAYER_FREQUENCY,
                amplitude,
            });
        }
        Self { layers }
    }

    pub(crate) fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let mut total = 0.0_f32;
        for layer in &self.layers {
            let f = layer.frequency;
            total += layer.amplitude * layer.perlin.get(x * f, y * f, z * f);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_small_values_and_folds_large_ones() {
        assert_eq!(wrap(12.5), 12.5);
        let folded = wrap(33_554_432.0 + 7.0);
        assert!((folded - 7.0).abs() < 1e-9);
    }

    #[test]
    fn compensated_sum_matches_plain_sum_for_exact_values() {
        assert_eq!(compensated_sum(&[1.0, 0.5, 0.25]), 1.75);
    }
}
