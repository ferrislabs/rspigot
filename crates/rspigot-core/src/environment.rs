use std::time::{SystemTime, UNIX_EPOCH};

use rspigot_domain::difficulty::{self, DifficultyInstance};
use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::spawn::SpawnEnvironment;
use rspigot_domain::spawn::SpawnLevel;
use rspigot_domain::{BiomeId, BlockPos, Vec3};

use crate::mobs::ChunkView;
use crate::ports::ClientLink;
use crate::server::Server;

const RAIN_TEMPERATURE: f32 = 0.15;
const SNOW_LINE_OFFSET: i32 = 17;
const LAPSE_RATE: f32 = 0.05 / 40.0;
const FROZEN_TEMPERATURE: f32 = 0.2;
const SECONDS_PER_DAY: i64 = 86_400;

pub(crate) fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub(crate) fn is_halloween(days_since_epoch: i64) -> bool {
    let (_, month, day) = civil_date(days_since_epoch);
    (month == 10 && day >= 20) || (month == 11 && day <= 3)
}

pub(crate) fn today() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64 / SECONDS_PER_DAY)
}

impl ChunkView<'_> {
    pub(crate) fn can_see_sky_at(&self, pos: BlockPos) -> bool {
        self.sky_light(pos) >= 15
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn biome_at(&self, pos: BlockPos) -> BiomeId {
        SpawnLevel::biome(self, pos)
    }

    pub fn inhabited_time(&self, chunk: rspigot_domain::ChunkPos) -> Option<i64> {
        self.world.chunks.inhabited_time(chunk)
    }

    pub(crate) fn difficulty_at(&self, pos: BlockPos) -> DifficultyInstance {
        let (inhabited, moon) = match self.world.chunks.inhabited_time(pos.chunk()) {
            Some(inhabited) => (
                inhabited,
                difficulty::moon_brightness(
                    self.world.level.day_time,
                    self.world.dimension.has_skylight,
                ),
            ),
            None => (0, 0.0),
        };
        DifficultyInstance::new(
            self.world.difficulty,
            self.world.level.day_time,
            inhabited,
            moon,
        )
    }

    pub(crate) fn spawn_environment(&self, position: Vec3) -> SpawnEnvironment {
        let pos = position.block();
        SpawnEnvironment {
            biome: self.biome_at(pos),
            difficulty: self.difficulty_at(pos),
            halloween: is_halloween(today()),
            moon_brightness: difficulty::moon_brightness(
                self.world.level.day_time,
                self.world.dimension.has_skylight,
            ),
        }
    }

    pub(crate) fn climate_at(&self, pos: BlockPos) -> Option<(bool, f32)> {
        let climate = rspigot_data::climate::climate(self.biome_at(pos).0)?;
        let mut temperature = if climate.frozen {
            FROZEN_TEMPERATURE.min(climate.temperature)
        } else {
            climate.temperature
        };
        let snow_line = self.world.dimension.sea_level + SNOW_LINE_OFFSET;
        if pos.y > snow_line {
            temperature -= (pos.y - snow_line) as f32 * LAPSE_RATE;
        }
        Some((climate.has_precipitation, temperature))
    }

    pub(crate) fn warm_enough_to_rain(&self, pos: BlockPos) -> bool {
        self.climate_at(pos)
            .is_some_and(|(_, temperature)| temperature >= RAIN_TEMPERATURE)
    }

    pub(crate) fn raining_at(&self, pos: BlockPos) -> bool {
        if !self.world.level.raining || !self.chunk_view().can_see_sky_at(pos) {
            return false;
        }
        self.climate_at(pos)
            .is_some_and(|(precipitation, temperature)| {
                precipitation && temperature >= RAIN_TEMPERATURE
            })
    }

    pub(crate) fn is_in_rain(&self, base: &EntityBase) -> bool {
        let pos = base.position.block();
        let top = BlockPos::new(pos.x, base.bounding_box().max_y.floor() as i32, pos.z);
        self.raining_at(pos) || self.raining_at(top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_follow_the_gregorian_calendar() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_756), (2026, 10, 30));
        assert!(is_halloween(20_756));
        assert!(!is_halloween(20_700));
    }
}
