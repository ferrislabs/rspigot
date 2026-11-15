use rspigot_domain::BiomeId;

use crate::climate::NoiseStack;
use crate::datapack::noise_parameters;
use crate::resources::biome_id;

const TEMPERATURE: &str = "minecraft:nether/temperature";
const VEGETATION: &str = "minecraft:nether/vegetation";
const XZ_SCALE: f64 = 0.25;

pub(crate) const NETHER_BIOMES: [&str; 5] = [
    "minecraft:nether_wastes",
    "minecraft:soul_sand_valley",
    "minecraft:crimson_forest",
    "minecraft:warped_forest",
    "minecraft:basalt_deltas",
];

#[derive(Debug, Clone, Copy)]
struct Point {
    temperature: i64,
    humidity: i64,
    offset: i64,
}

const POINTS: [Point; 5] = [
    Point {
        temperature: 0,
        humidity: 0,
        offset: 0,
    },
    Point {
        temperature: 0,
        humidity: -5000,
        offset: 0,
    },
    Point {
        temperature: 4000,
        humidity: 0,
        offset: 0,
    },
    Point {
        temperature: 0,
        humidity: 5000,
        offset: 3750,
    },
    Point {
        temperature: -5000,
        humidity: 0,
        offset: 1750,
    },
];

fn quantize(value: f32) -> i64 {
    (value * 10_000.0_f32) as i64
}

#[derive(Debug, Clone)]
pub(crate) struct NetherBiomes {
    temperature: NoiseStack,
    humidity: NoiseStack,
    ids: [BiomeId; 5],
}

impl NetherBiomes {
    pub(crate) fn new(seed: i64) -> Self {
        let build = |name: &str, offset: i64| {
            let parameters = noise_parameters(name)
                .unwrap_or_else(|| panic!("noise {name} missing from the worldgen data"));
            NoiseStack::create_legacy_nether(&parameters, seed.wrapping_add(offset))
        };
        Self {
            temperature: build(TEMPERATURE, 0),
            humidity: build(VEGETATION, 1),
            ids: NETHER_BIOMES.map(biome_id),
        }
    }

    pub(crate) fn target(&self, qx: i32, qz: i32) -> (i64, i64) {
        let (x, z) = (f64::from(qx << 2) * XZ_SCALE, f64::from(qz << 2) * XZ_SCALE);
        (
            quantize(self.temperature.get(x, 0.0, z)),
            quantize(self.humidity.get(x, 0.0, z)),
        )
    }

    pub(crate) fn index(&self, qx: i32, qz: i32) -> usize {
        let (temperature, humidity) = self.target(qx, qz);
        let fitness = |point: &Point| {
            let dt = temperature - point.temperature;
            let dh = humidity - point.humidity;
            dt * dt + dh * dh + point.offset * point.offset
        };
        let mut best = 0;
        for (index, point) in POINTS.iter().enumerate() {
            if fitness(point) < fitness(&POINTS[best]) {
                best = index;
            }
        }
        best
    }

    pub(crate) fn ids(&self) -> &[BiomeId; 5] {
        &self.ids
    }
}
