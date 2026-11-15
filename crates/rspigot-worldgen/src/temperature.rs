use crate::legacy::JavaRandom;
use crate::simplex::PerlinSimplex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BiomeClimate {
    pub(crate) temperature: f32,
    pub(crate) frozen: bool,
    pub(crate) precipitation: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TemperatureNoise {
    height: PerlinSimplex,
    frozen: PerlinSimplex,
    info: PerlinSimplex,
}

impl TemperatureNoise {
    pub(crate) fn new() -> Self {
        Self {
            height: PerlinSimplex::new(&mut JavaRandom::new(1234), &[0]),
            frozen: PerlinSimplex::new(&mut JavaRandom::new(3456), &[-2, -1, 0]),
            info: PerlinSimplex::new(&mut JavaRandom::new(2345), &[0]),
        }
    }

    fn modified(&self, climate: BiomeClimate, x: i32, z: i32) -> f32 {
        if !climate.frozen {
            return climate.temperature;
        }
        let (x, z) = (f64::from(x), f64::from(z));
        let spread = self.frozen.value(x * 0.05, z * 0.05, false) * 7.0;
        let info = self.info.value(x * 0.2, z * 0.2, false);
        if spread + info < 0.3 && self.info.value(x * 0.09, z * 0.09, false) < 0.8 {
            return 0.2;
        }
        climate.temperature
    }

    pub(crate) fn at(&self, climate: BiomeClimate, x: i32, y: i32, z: i32, sea_level: i32) -> f32 {
        let base = self.modified(climate, x, z);
        let snow_line = sea_level + 17;
        if y > snow_line {
            let noise =
                (self
                    .height
                    .value(f64::from(x as f32 / 8.0), f64::from(z as f32 / 8.0), false)
                    * 8.0) as f32;
            base - (noise + y as f32 - snow_line as f32) * 0.05 / 40.0
        } else {
            base
        }
    }

    #[cfg(test)]
    pub(crate) fn cold_enough_to_snow(
        &self,
        climate: BiomeClimate,
        x: i32,
        y: i32,
        z: i32,
        sea_level: i32,
    ) -> bool {
        self.at(climate, x, y, z, sea_level) < 0.15
    }

    pub(crate) fn warm_enough_to_rain(
        &self,
        climate: BiomeClimate,
        x: i32,
        y: i32,
        z: i32,
        sea_level: i32,
    ) -> bool {
        self.at(climate, x, y, z, sea_level) >= 0.15
    }
}
