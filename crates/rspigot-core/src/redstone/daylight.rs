use rspigot_blocks::BlockKind;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};

use super::{RedstoneChange, RedstoneStep};
use crate::lighting::light_level;
use crate::mobs::sky_darken;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const PERIOD: i64 = 20;
const DAY_LENGTH: i64 = 24_000;
const NOON: i64 = 6_000;
const EASE: [f64; 4] = [0.362, 0.241, 0.638, 0.759];
const SECTION: i32 = 16;

fn bezier(control_a: f64, control_b: f64, along: f64) -> f64 {
    let inverse = 1.0 - along;
    3.0 * inverse * inverse * along * control_a
        + 3.0 * inverse * along * along * control_b
        + along * along * along
}

fn ease(progress: f64) -> f64 {
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if bezier(EASE[0], EASE[2], middle) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }
    bezier(EASE[1], EASE[3], (low + high) / 2.0)
}

pub(crate) fn sun_angle(day_time: i64) -> f32 {
    let since_noon = (day_time - NOON).rem_euclid(DAY_LENGTH);
    (ease(since_noon as f64 / DAY_LENGTH as f64) * 360.0) as f32
}

pub(crate) fn daylight_power(brightness: i32, angle_degrees: f32, inverted: bool) -> i32 {
    let mut power = brightness;
    if inverted {
        power = 15 - power;
    } else if power > 0 {
        let radians = angle_degrees * 0.017_453_292;
        let target = if radians < std::f32::consts::PI {
            0.0
        } else {
            std::f32::consts::TAU
        };
        let bent = radians + (target - radians) * 0.2;
        power = (power as f32 * bent.cos()).round() as i32;
    }
    power.clamp(0, 15)
}

impl<L: ClientLink> Server<L> {
    fn effective_sky_brightness(&self, pos: BlockPos) -> i32 {
        let sky = if self.world.dimension.has_skylight {
            self.world
                .chunks
                .get(pos.chunk())
                .map_or(0, |chunk| light_level(chunk, pos, true))
        } else {
            0
        };
        (sky - sky_darken(&self.world.level)).max(0)
    }

    fn wanted_daylight(&self, pos: BlockPos, state: BlockStateId) -> i32 {
        daylight_power(
            self.effective_sky_brightness(pos),
            sun_angle(self.world.level.day_time),
            flag(state, "inverted"),
        )
    }

    fn update_daylight(&mut self, pos: BlockPos, state: BlockStateId) {
        let wanted = self.wanted_daylight(pos, state);
        let current = level(state, "power");
        if wanted != current {
            self.ask_redstone(RedstoneStep {
                pos,
                old: current,
                new: wanted,
                change: RedstoneChange::Daylight,
            });
        }
    }

    pub(crate) fn finish_daylight(&mut self, pos: BlockPos, new: i32) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::DaylightDetector) {
            return;
        }
        if let Some(next) = state.with_property("power", &new.clamp(0, 15).to_string()) {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn use_daylight_detector(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        let builds = self.players.get(&id).is_some_and(|player| {
            !matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
        });
        if !builds {
            return false;
        }
        let inverted = if flag(state, "inverted") {
            "false"
        } else {
            "true"
        };
        let Some(next) = state.with_property("inverted", inverted) else {
            return false;
        };
        self.set_block_quiet(pos, next);
        self.update_daylight(pos, next);
        true
    }

    pub(crate) fn tick_daylight_detectors(&mut self) {
        if !self.world.dimension.has_skylight || self.world.level.game_time % PERIOD != 0 {
            return;
        }
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.ticking_chunks_near_players() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            let min_y = chunk.height().min_section() * SECTION;
            for (index, section) in chunk.sections().iter().enumerate() {
                let holds = section
                    .blocks()
                    .palette()
                    .iter()
                    .any(|state| state.kind() == Some(BlockKind::DaylightDetector));
                if holds {
                    found.extend(detectors_in(
                        chunk_pos,
                        min_y + index as i32 * SECTION,
                        section,
                    ));
                }
            }
        }
        for pos in found {
            let state = self.block_or_air(pos);
            if state.kind() == Some(BlockKind::DaylightDetector) {
                self.update_daylight(pos, state);
            }
        }
    }
}

fn detectors_in(
    chunk: ChunkPos,
    base_y: i32,
    section: &rspigot_domain::chunk::ChunkSection,
) -> Vec<BlockPos> {
    let mut found = Vec::new();
    for x in 0..SECTION as usize {
        for y in 0..SECTION as usize {
            for z in 0..SECTION as usize {
                if section.block(x, y, z).kind() == Some(BlockKind::DaylightDetector) {
                    found.push(BlockPos::new(
                        chunk.min_block_x() + x as i32,
                        base_y + y as i32,
                        chunk.min_block_z() + z as i32,
                    ));
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_is_overhead_at_noon_and_back_there_a_day_later() {
        assert!(sun_angle(6_000).abs() < 0.01);
        assert!(sun_angle(30_000).abs() < 0.01);
    }

    #[test]
    fn the_sun_is_half_way_round_at_midnight() {
        assert!((sun_angle(18_000) - 180.0).abs() < 0.5);
    }

    #[test]
    fn the_angle_grows_through_the_day() {
        assert!(sun_angle(9_000) > sun_angle(7_000));
        assert!(sun_angle(7_000) > 0.0);
    }

    #[test]
    fn full_daylight_at_noon_gives_fifteen() {
        assert_eq!(daylight_power(15, 0.0, false), 15);
    }

    #[test]
    fn dimmer_light_and_a_lower_sun_give_less() {
        assert!(daylight_power(15, 60.0, false) < 15);
        assert_eq!(daylight_power(0, 0.0, false), 0);
    }

    #[test]
    fn an_inverted_detector_gives_the_complement_of_the_brightness() {
        assert_eq!(daylight_power(15, 0.0, true), 0);
        assert_eq!(daylight_power(0, 0.0, true), 15);
        assert_eq!(daylight_power(6, 0.0, true), 9);
    }
}
