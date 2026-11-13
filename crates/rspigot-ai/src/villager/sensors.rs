use std::collections::HashMap;

use rspigot_domain::poi::Occupancy;
use rspigot_domain::poi::PoiType;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;

use crate::behavior::{
    GlobalPos, Memories, MemoryModuleType, MemoryValue, Sensor, nearest_visible_entity,
};
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::world::AiWorld;

const NEAREST_BED_SCAN_RATE: i32 = 20;
const NEAREST_BED_RADIUS: i32 = 48;
const NEAREST_BED_MAX_TRIES: i32 = 5;
const NEAREST_BED_RETRY_DELAY: i64 = 40;
const SECONDARY_POI_SCAN_RATE: i32 = 40;
const GOLEM_SCAN_RATE: i32 = 200;
const GOLEM_MEMORY_TICKS: i64 = 599;

pub const HOSTILE_DISTANCES: [(EntityKind, f32); 11] = [
    (EntityKind::Drowned, 8.0),
    (EntityKind::Evoker, 12.0),
    (EntityKind::Husk, 8.0),
    (EntityKind::Illusioner, 12.0),
    (EntityKind::Pillager, 15.0),
    (EntityKind::Ravager, 12.0),
    (EntityKind::Vex, 8.0),
    (EntityKind::Vindicator, 10.0),
    (EntityKind::Zoglin, 10.0),
    (EntityKind::Zombie, 8.0),
    (EntityKind::ZombieVillager, 8.0),
];

pub fn hostile_distance(kind: EntityKind) -> Option<f32> {
    HOSTILE_DISTANCES
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, distance)| *distance)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NearestBedSensor {
    tried_count: i32,
    last_update: i64,
    batch_cache: HashMap<BlockPos, i64>,
}

impl Sensor for NearestBedSensor {
    fn scan_rate(&self) -> i32 {
        NEAREST_BED_SCAN_RATE
    }

    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::NearestBed]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        if !cx.mob.baby {
            return;
        }
        self.tried_count = 0;
        let now = cx.world.game_time();
        self.last_update = now + i64::from(cx.brain.random.next_int(20));
        let candidates: Vec<BlockPos> = cx
            .world
            .pois_in_range(cx.mob.block_pos(), NEAREST_BED_RADIUS, Occupancy::Any)
            .into_iter()
            .filter(|record| record.kind() == PoiType::Home)
            .map(|record| record.pos())
            .filter(|pos| {
                if self.batch_cache.contains_key(pos) {
                    return false;
                }
                self.tried_count += 1;
                if self.tried_count < NEAREST_BED_MAX_TRIES {
                    self.batch_cache
                        .insert(*pos, self.last_update + NEAREST_BED_RETRY_DELAY);
                }
                true
            })
            .collect();
        let path = cx.create_path_to_any(&candidates, PoiType::Home.valid_range());
        match path.filter(crate::Path::can_reach) {
            Some(path) => {
                let target = path.target();
                if cx.world.poi_type_at(target).is_some() {
                    memories.set(MemoryModuleType::NearestBed, MemoryValue::Block(target));
                }
            }
            None => {
                if self.tried_count < NEAREST_BED_MAX_TRIES {
                    let last_update = self.last_update;
                    self.batch_cache.retain(|_, time| *time >= last_update);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SecondaryPoiSensor;

impl Sensor for SecondaryPoiSensor {
    fn scan_rate(&self) -> i32 {
        SECONDARY_POI_SCAN_RATE
    }

    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::SecondaryJobSite]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let dimension = cx.world.dimension().to_owned();
        let origin = cx.mob.block_pos();
        let secondary = cx.mob.species.villager.profession.secondary_pois();
        let mut found = Vec::new();
        for x in -4..=4 {
            for y in -2..=2 {
                for z in -4..=4 {
                    let pos = origin.offset(x, y, z);
                    let is_secondary = cx
                        .world
                        .state(pos)
                        .and_then(|state| state.kind())
                        .is_some_and(|kind| secondary.contains(&kind));
                    if is_secondary {
                        found.push(GlobalPos {
                            dimension: dimension.clone(),
                            pos,
                        });
                    }
                }
            }
        }
        memories.set_option(
            MemoryModuleType::SecondaryJobSite,
            (!found.is_empty()).then_some(MemoryValue::GlobalPosList(found)),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerBabiesSensor;

impl Sensor for VillagerBabiesSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::VisibleVillagerBabies]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let babies: Vec<EntityId> = memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .map(|visible| {
                visible
                    .find_all(cx, memories, |entity| {
                        entity.kind == EntityKind::Villager && entity.baby
                    })
                    .into_iter()
                    .map(|entity| entity.id)
                    .collect()
            })
            .unwrap_or_default();
        memories.set(
            MemoryModuleType::VisibleVillagerBabies,
            MemoryValue::Entities(babies),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerHostilesSensor;

impl Sensor for VillagerHostilesSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestHostile,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let body = cx.mob.position;
        nearest_visible_entity(cx, memories, MemoryModuleType::NearestHostile, |entity| {
            hostile_distance(entity.kind).is_some_and(|distance| {
                entity.position.distance_squared(body) <= f64::from(distance * distance)
            })
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GolemSensor;

impl Sensor for GolemSensor {
    fn scan_rate(&self) -> i32 {
        GOLEM_SCAN_RATE
    }

    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestLivingEntities,
            MemoryModuleType::GolemDetectedRecently,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        check_for_nearby_golem(cx, memories);
    }
}

pub fn check_for_nearby_golem<W: AiWorld>(cx: &AiContext<'_, W>, memories: &mut Memories) {
    let Some(nearby) = memories
        .get(MemoryModuleType::NearestLivingEntities)
        .and_then(MemoryValue::as_entities)
    else {
        return;
    };
    let found = nearby.iter().any(|id| {
        cx.world
            .living(*id)
            .is_some_and(|entity| entity.kind == EntityKind::IronGolem)
    });
    if found {
        golem_detected(memories);
    }
}

pub fn golem_detected(memories: &mut Memories) {
    memories.set_with_expiry(
        MemoryModuleType::GolemDetectedRecently,
        MemoryValue::Bool(true),
        GOLEM_MEMORY_TICKS,
    );
}
