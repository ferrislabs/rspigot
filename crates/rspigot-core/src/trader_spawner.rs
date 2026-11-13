use rand::Rng;
use rspigot_ai::AiWorld;
use rspigot_domain::entity::spawn::finalize_spawn;
use rspigot_domain::entity::species::Species;
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::spawn::placement::is_spawn_position_ok;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind;

use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

const OVERWORLD: &str = "minecraft:overworld";
const TICK_DELAY: i32 = 1_200;
const SPAWN_DELAY: i32 = 24_000;
const MIN_CHANCE: i32 = 25;
const MAX_CHANCE: i32 = 75;
const CHANCE_STEP: i32 = 25;
const CHANCE_ROLL: i32 = 100;
const SPAWN_ODDS: i32 = 10;
const SPAWN_RADIUS: i32 = 48;
const SPAWN_TRIES: i32 = 10;
const HOME_RADIUS: i32 = 16;
const NO_TRADERS: &str = "minecraft:without_wandering_trader_spawns";
const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TraderSpawner {
    pub(crate) tick_delay: i32,
    pub(crate) spawn_delay: i32,
    pub(crate) spawn_chance: i32,
}

impl Default for TraderSpawner {
    fn default() -> Self {
        Self {
            tick_delay: TICK_DELAY,
            spawn_delay: SPAWN_DELAY,
            spawn_chance: MIN_CHANCE,
        }
    }
}

pub(crate) enum Attempt {
    Wait,
    Roll { chance: i32 },
}

impl TraderSpawner {
    pub(crate) fn tick(&mut self) -> Attempt {
        self.tick_delay -= 1;
        if self.tick_delay >= 1 {
            return Attempt::Wait;
        }
        self.tick_delay = TICK_DELAY;
        self.spawn_delay -= TICK_DELAY;
        if self.spawn_delay >= 1 {
            return Attempt::Wait;
        }
        self.spawn_delay = SPAWN_DELAY;
        let chance = self.spawn_chance;
        self.spawn_chance = (chance + CHANCE_STEP).clamp(MIN_CHANCE, MAX_CHANCE);
        Attempt::Roll { chance }
    }

    pub(crate) fn spawned(&mut self) {
        self.spawn_chance = MIN_CHANCE;
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_trader_spawner(&mut self) {
        if !self.config.rules.spawn_wandering_traders
            || self.world.dimension.key.as_str() != OVERWORLD
        {
            return;
        }
        let Attempt::Roll { chance } = self.spawning.trader.tick() else {
            return;
        };
        let mut random = ServerRandom(&mut self.rng);
        if random.next_int(CHANCE_ROLL) <= chance && self.spawn_wandering_trader() {
            self.spawning.trader.spawned();
        }
    }

    pub fn spawn_wandering_trader(&mut self) -> bool {
        let mut players: Vec<_> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .map(|player| (player.entity_id, player.position))
            .collect();
        if players.is_empty() {
            return true;
        }
        players.sort_by_key(|(id, _)| id.0);
        let mut random = ServerRandom(&mut self.rng);
        let (_, position) = players[random.next_int(players.len() as i32) as usize];
        if random.next_int(SPAWN_ODDS) != 0 {
            return false;
        }
        let player_pos = position.block();
        let meeting = self.world.chunks.poi().find(
            |kind| kind == PoiType::Meeting,
            |_| true,
            player_pos,
            SPAWN_RADIUS,
            Occupancy::Any,
        );
        let target = meeting.unwrap_or(player_pos);
        let Some(spawn) = self.find_spawn_position_near(target, SPAWN_RADIUS) else {
            return false;
        };
        if !self.has_enough_space(spawn) {
            return false;
        }
        let without = rspigot_data::tags::tag(BIOME_REGISTRY, NO_TRADERS)
            .is_some_and(|ids| ids.contains(&u32::from(self.biome_at(spawn).0)));
        if without {
            return false;
        }
        let mut entity = new_mob(self.new_entity_base(EntityKind::WanderingTrader));
        let mut own = LegacyRandom::new(self.rng.random());
        let center = Vec3::new(
            f64::from(spawn.x) + 0.5,
            f64::from(spawn.y),
            f64::from(spawn.z) + 0.5,
        );
        entity.base.snap_to(center, 0.0, 0.0);
        let environment = self.spawn_environment(center);
        finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Event,
            None,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        if let Some(mob) = entity.living_mut().and_then(|living| living.mob.as_mut()) {
            if let Species::WanderingTrader(state) = &mut mob.species {
                state.wander_target = Some(target);
            }
            mob.home_pos = target;
            mob.home_radius = HOME_RADIUS;
        }
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Natural).with_random(own),
        );
        true
    }

    fn find_spawn_position_near(&mut self, reference: BlockPos, radius: i32) -> Option<BlockPos> {
        for _ in 0..SPAWN_TRIES {
            let mut random = ServerRandom(&mut self.rng);
            let x = reference.x + random.next_int(radius * 2) - radius;
            let z = reference.z + random.next_int(radius * 2) - radius;
            let surface = self
                .mob_world(sky_darken(&self.world.level))
                .motion_blocking_no_leaves_pos(BlockPos::new(x, 0, z));
            let pos = BlockPos::new(x, surface.y, z);
            if is_spawn_position_ok(&*self, EntityKind::WanderingTrader, pos) {
                return Some(pos);
            }
        }
        None
    }

    fn has_enough_space(&self, pos: BlockPos) -> bool {
        (pos.x..=pos.x + 1).all(|x| {
            (pos.y..=pos.y + 2).all(|y| {
                (pos.z..=pos.z + 1).all(|z| {
                    self.world
                        .chunks
                        .block(BlockPos::new(x, y, z))
                        .is_none_or(|state| state.collision_shape().is_empty())
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spawner_waits_twelve_hundred_ticks_then_a_full_day_before_rolling() {
        let mut spawner = TraderSpawner::default();
        let mut rolls = Vec::new();
        for tick in 1..=(SPAWN_DELAY / TICK_DELAY * TICK_DELAY + TICK_DELAY) {
            if let Attempt::Roll { chance } = spawner.tick() {
                rolls.push((tick, chance));
            }
        }
        assert_eq!(rolls, vec![(24_000, 25)]);
        assert_eq!(spawner.spawn_chance, 50);
    }

    #[test]
    fn each_failed_roll_raises_the_chance_up_to_seventy_five_and_a_spawn_resets_it() {
        let mut spawner = TraderSpawner::default();
        let mut chances = Vec::new();
        while chances.len() < 4 {
            if let Attempt::Roll { chance } = spawner.tick() {
                chances.push(chance);
            }
        }
        assert_eq!(chances, vec![25, 50, 75, 75]);
        spawner.spawned();
        assert_eq!(spawner.spawn_chance, 25);
    }
}
