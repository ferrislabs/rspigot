use rand::Rng;
use rspigot_ai::AiWorld;
use rspigot_domain::entity::spawn::finalize_spawn;
use rspigot_domain::player::GameMode;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::placement::is_spawn_position_ok;
use rspigot_domain::spawn::rules::check_spawn_rules;
use rspigot_domain::spawn::{SpawnLevel, SpawnReason};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::{EntityKind, SpawnHeightmap};

use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

const DAY_LENGTH: i64 = 24_000;
const MIDNIGHT: i64 = 18_000;
const SIEGE_ODDS: i32 = 10;
const SIEGE_ZOMBIES: i32 = 20;
const SPAWN_INTERVAL: i32 = 2;
const SETUP_ATTEMPTS: i32 = 10;
const POSITION_ATTEMPTS: i32 = 10;
const RING_RADIUS: f32 = 32.0;
const SPREAD: i32 = 16;
const NO_SIEGE_BIOMES: &str = "minecraft:without_zombie_sieges";
const BIOME_TAGS: &str = "minecraft:worldgen/biome";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Done,
    Tonight,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Siege {
    phase: Phase,
    set_up: bool,
    next_spawn: i32,
    zombies: i32,
    spawn_at: BlockPos,
}

impl Siege {
    fn reset(&mut self) {
        self.phase = Phase::Done;
        self.set_up = false;
    }

    fn decide(&mut self, roll: i32) {
        self.phase = if roll == 0 {
            Phase::Tonight
        } else {
            Phase::Done
        };
    }

    fn pending(&self) -> bool {
        self.phase == Phase::Tonight
    }

    fn countdown(&mut self) -> bool {
        if self.next_spawn > 0 {
            self.next_spawn -= 1;
            return false;
        }
        self.next_spawn = SPAWN_INTERVAL;
        if self.zombies > 0 {
            self.zombies -= 1;
            true
        } else {
            self.phase = Phase::Done;
            false
        }
    }
}

fn is_midnight(day_time: i64) -> bool {
    day_time.rem_euclid(DAY_LENGTH) == MIDNIGHT
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_village_siege(&mut self) {
        if !self.world.level.spawn_mobs {
            return;
        }
        let enemies =
            self.world.level.spawn_monsters && self.world.difficulty != Difficulty::Peaceful;
        let bright = self
            .mob_world(sky_darken(&self.world.level))
            .bright_outside();
        if bright || !enemies {
            self.spawning.siege.reset();
            return;
        }
        if is_midnight(self.world.level.day_time) {
            let mut peek = self.rng.clone();
            let roll = ServerRandom(&mut peek).next_int(SIEGE_ODDS);
            self.spawning.siege.decide(roll);
        }
        if !self.spawning.siege.pending() {
            return;
        }
        if !self.spawning.siege.set_up {
            if !self.set_up_siege() {
                return;
            }
            self.spawning.siege.set_up = true;
        }
        if self.spawning.siege.countdown() {
            self.spawn_siege_zombie();
        }
    }

    fn set_up_siege(&mut self) -> bool {
        let mut players: Vec<_> = self
            .players
            .values()
            .filter(|player| player.game_mode != GameMode::Spectator)
            .map(|player| (player.entity_id, player.position.block()))
            .collect();
        players.sort_by_key(|(id, _)| id.0);
        for (_, here) in players {
            let world = self.mob_world(sky_darken(&self.world.level));
            if !world.is_village(here) {
                continue;
            }
            let barred = rspigot_data::tags::tag(BIOME_TAGS, NO_SIEGE_BIOMES)
                .is_some_and(|ids| ids.binary_search(&u32::from(self.biome_at(here).0)).is_ok());
            if barred {
                continue;
            }
            if self.raid_at(here).is_some() {
                return false;
            }
            for _ in 0..SETUP_ATTEMPTS {
                let angle = ServerRandom(&mut self.rng).next_float() * std::f32::consts::TAU;
                let ring = BlockPos::new(
                    here.x + (angle.cos() * RING_RADIUS).floor() as i32,
                    here.y,
                    here.z + (angle.sin() * RING_RADIUS).floor() as i32,
                );
                if self.find_siege_position(ring).is_some() {
                    let siege = &mut self.spawning.siege;
                    siege.next_spawn = 0;
                    siege.zombies = SIEGE_ZOMBIES;
                    siege.spawn_at = ring;
                    break;
                }
            }
            return true;
        }
        false
    }

    fn find_siege_position(&mut self, around: BlockPos) -> Option<Vec3> {
        for _ in 0..POSITION_ATTEMPTS {
            let x = around.x + ServerRandom(&mut self.rng).next_int(SPREAD) - SPREAD / 2;
            let z = around.z + ServerRandom(&mut self.rng).next_int(SPREAD) - SPREAD / 2;
            let y = self.top_height(SpawnHeightmap::WorldSurface, x, z);
            let pos = BlockPos::new(x, y, z);
            if is_spawn_position_ok(&*self, EntityKind::Zombie, pos)
                && check_spawn_rules(self, EntityKind::Zombie, SpawnReason::Event, pos)
            {
                return Some(Vec3::new(
                    f64::from(x) + 0.5,
                    f64::from(y),
                    f64::from(z) + 0.5,
                ));
            }
        }
        None
    }

    fn spawn_siege_zombie(&mut self) {
        let around = self.spawning.siege.spawn_at;
        let Some(position) = self.find_siege_position(around) else {
            return;
        };
        let mut entity = new_mob(self.new_entity_base(EntityKind::Zombie));
        entity.base.snap_to(position, 0.0, 0.0);
        let mut own = LegacyRandom::new(self.rng.random());
        let environment = self.spawn_environment(position);
        finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Event,
            None,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        let yaw = ServerRandom(&mut self.rng).next_float() * 360.0;
        entity.base.snap_to(position, yaw, 0.0);
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::VillageInvasion).with_random(own),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tonight() -> Siege {
        let mut siege = Siege::default();
        siege.decide(0);
        siege.zombies = SIEGE_ZOMBIES;
        siege
    }

    #[test]
    fn a_siege_is_decided_by_a_one_in_ten_roll() {
        let mut siege = Siege::default();
        assert!(!siege.pending());
        siege.decide(3);
        assert!(!siege.pending());
        siege.decide(0);
        assert!(siege.pending());
    }

    #[test]
    fn only_midnight_decides() {
        assert!(is_midnight(18_000));
        assert!(is_midnight(18_000 + 24_000 * 5));
        assert!(!is_midnight(12_000));
        assert!(!is_midnight(17_999));
    }

    #[test]
    fn twenty_zombies_come_one_every_three_ticks_then_the_siege_is_done() {
        let mut siege = tonight();
        let mut spawned = Vec::new();
        for tick in 0..80 {
            if siege.countdown() {
                spawned.push(tick);
            }
        }
        assert_eq!(spawned.len(), 20);
        assert_eq!(spawned[0], 0);
        assert!(spawned.windows(2).all(|pair| pair[1] - pair[0] == 3));
        assert!(!siege.pending());
    }

    #[test]
    fn reset_clears_the_night() {
        let mut siege = tonight();
        siege.set_up = true;
        siege.reset();
        assert!(!siege.pending() && !siege.set_up);
    }
}
