use rspigot_data::variants::VariantFamily;
use rspigot_domain::entity::species;
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::rules::no_collision;
use rspigot_domain::spawn::{SpawnReason, spawn_aabb};
use rspigot_domain::{BlockPos, SectionPos, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::mobs::ServerRandom;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::Birth;
use crate::ports::ClientLink;
use crate::server::Server;

const TICK_DELAY: i32 = 1200;
const VILLAGE_SECTIONS: i32 = 2;
const VILLAGE_RANGE: i32 = 48;
const VILLAGE_HEIGHT: f64 = 8.0;
const HOMES_NEEDED: usize = 4;
const VILLAGE_CAT_LIMIT: usize = 5;
const HUT_RANGE: f64 = 16.0;
const HUT_HEIGHT: f64 = 8.0;
const OFFSET_BASE: i32 = 8;
const OFFSET_SPREAD: i32 = 24;
const LOAD_MARGIN: i32 = 10;
const SWAMP_HUT: &str = "minecraft:swamp_hut";
const ALL_BLACK: &str = "minecraft:all_black";

#[derive(Debug, Clone, Default)]
pub(crate) struct CatSpawner {
    pub(crate) next_tick: i32,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_cat_spawner(&mut self) {
        if !self.world.level.spawn_mobs {
            return;
        }
        self.spawning.cat.next_tick -= 1;
        if self.spawning.cat.next_tick > 0 {
            return;
        }
        self.spawning.cat.next_tick = TICK_DELAY;
        let mut ids: Vec<_> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .map(|player| (player.id, player.position.block()))
            .collect();
        if ids.is_empty() {
            return;
        }
        ids.sort_by_key(|(id, _)| id.0);
        let mut random = ServerRandom(&mut self.rng);
        let (_, origin) = ids[random.next_int(ids.len() as i32) as usize];
        let dx = (OFFSET_BASE + random.next_int(OFFSET_SPREAD))
            * if random.next_int(2) == 0 { -1 } else { 1 };
        let dz = (OFFSET_BASE + random.next_int(OFFSET_SPREAD))
            * if random.next_int(2) == 0 { -1 } else { 1 };
        let pos = origin.offset(dx, 0, dz);
        let loaded = [(-1, -1), (1, 1)].into_iter().all(|(sx, sz)| {
            self.world
                .chunks
                .is_loaded(pos.offset(sx * LOAD_MARGIN, 0, sz * LOAD_MARGIN).chunk())
        });
        if !loaded || !self.cat_position_ok(pos) {
            return;
        }
        let village = self
            .world
            .chunks
            .poi()
            .sections_to_village(SectionPos::of(pos))
            <= VILLAGE_SECTIONS;
        if village {
            self.spawn_cat_in_village(pos);
        } else if self
            .structures_at(pos)
            .iter()
            .any(|structure| structure == SWAMP_HUT)
        {
            self.spawn_cat_in_hut(pos);
        }
    }

    fn cat_position_ok(&self, pos: BlockPos) -> bool {
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        let below = Vec3::new(at.x, at.y - 1.0, at.z);
        no_collision(self, &spawn_aabb(Kind::Cat, at.x, at.y, at.z))
            && !no_collision(self, &spawn_aabb(Kind::Cat, below.x, below.y, below.z))
    }

    fn cats_around(&self, pos: BlockPos, range: f64, height: f64) -> usize {
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::Cat && !entity.base.is_removed())
            .filter(|entity| {
                let at = entity.base.position;
                (at.x - center.x).abs() <= range + 0.5
                    && (at.z - center.z).abs() <= range + 0.5
                    && (at.y - center.y).abs() <= height + 0.5
            })
            .count()
    }

    fn spawn_cat_in_village(&mut self, pos: BlockPos) {
        let homes = self.world.chunks.poi().count_in_range(
            |kind| kind == PoiType::Home,
            pos,
            VILLAGE_RANGE,
            Occupancy::IsOccupied,
        );
        if homes <= HOMES_NEEDED {
            return;
        }
        if self.cats_around(pos, f64::from(VILLAGE_RANGE), VILLAGE_HEIGHT) < VILLAGE_CAT_LIMIT {
            self.spawn_cat(pos, false);
        }
    }

    fn spawn_cat_in_hut(&mut self, pos: BlockPos) {
        if self.cats_around(pos, HUT_RANGE, HUT_HEIGHT) == 0 {
            self.spawn_cat(pos, true);
        }
    }

    fn spawn_cat(&mut self, pos: BlockPos, persistent: bool) {
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        let Ok((mut entity, _)) = self.summon_entity(Kind::Cat, at, None, SpawnReason::Natural)
        else {
            return;
        };
        if persistent {
            if let Some(black) = VariantFamily::Cat.variant_index(ALL_BLACK) {
                species::set_variant(&mut entity.base, VariantFamily::Cat, black);
            }
            if let Some(mob) = entity.living_mut().and_then(|living| living.mob.as_mut()) {
                mob.persistence_required = true;
            }
        }
        entity.base.snap_to(at, 0.0, 0.0);
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Natural));
    }
}
