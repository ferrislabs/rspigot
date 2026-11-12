use rspigot_domain::EntityId;

use crate::behavior::memory::{
    Memories, MemoryModuleType, MemoryValue, NearestVisibleLivingEntities,
};
use crate::behavior::sensor::{Sensor, is_entity_attackable, is_entity_targetable};
use crate::brain::AiContext;
use crate::sensing::has_line_of_sight_between;
use crate::world::{AiWorld, DroppedItem, Living, TargetKind, inflate};

const ITEM_XZ_RANGE: f64 = 32.0;
const ITEM_Y_RANGE: f64 = 16.0;
const ITEM_MAX_DISTANCE: f64 = 32.0;
const ITEM_EYE_HEIGHT_FACTOR: f64 = 0.85;

fn by_distance(cx: &AiContext<'_, impl AiWorld>, a: &Living, b: &Living) -> std::cmp::Ordering {
    cx.mob
        .distance_to_sqr(a.position)
        .total_cmp(&cx.mob.distance_to_sqr(b.position))
}

fn ids(entities: &[Living]) -> Vec<EntityId> {
    entities.iter().map(|living| living.id).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DummySensor;

impl Sensor for DummySensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[]
    }

    fn do_tick<W: AiWorld>(&mut self, _cx: &mut AiContext<'_, W>, _memories: &mut Memories) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IsInWaterSensor;

impl Sensor for IsInWaterSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::IsInWater]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        if cx.mob.fluid.in_water {
            memories.set(MemoryModuleType::IsInWater, MemoryValue::Unit);
        } else {
            memories.erase(MemoryModuleType::IsInWater);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NearestLivingEntitySensor;

impl Sensor for NearestLivingEntitySensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestLivingEntities,
            MemoryModuleType::NearestVisibleLivingEntities,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let range = cx.mob.follow_range;
        let area = inflate(&cx.mob.bounds(), range, range, range);
        let mut found = cx.world.living_of(TargetKind::Player, &area);
        found.extend(cx.world.living_of(TargetKind::Class("LivingEntity"), &area));
        let id = cx.mob.id;
        found.retain(|living| living.id != id && living.alive);
        found.sort_by(|a, b| by_distance(cx, a, b));
        let nearby = ids(&found);
        memories.set(
            MemoryModuleType::NearestLivingEntities,
            MemoryValue::Entities(nearby.clone()),
        );
        memories.set(
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryValue::Visible(NearestVisibleLivingEntities::new(nearby)),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerSensor;

impl Sensor for PlayerSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestPlayers,
            MemoryModuleType::NearestVisiblePlayer,
            MemoryModuleType::NearestVisibleAttackablePlayer,
            MemoryModuleType::NearestVisibleAttackablePlayers,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let follow = cx.mob.follow_range;
        let mut players: Vec<Living> = cx
            .world
            .players()
            .iter()
            .filter(|player| !player.spectator)
            .filter(|player| cx.mob.distance_to_sqr(player.position) < follow * follow)
            .copied()
            .collect();
        players.sort_by(|a, b| by_distance(cx, a, b));
        memories.set(
            MemoryModuleType::NearestPlayers,
            MemoryValue::Entities(ids(&players)),
        );
        let mut visible = Vec::new();
        for player in &players {
            if is_entity_targetable(cx, memories, player) {
                visible.push(*player);
            }
        }
        memories.set_option(
            MemoryModuleType::NearestVisiblePlayer,
            visible.first().map(|player| MemoryValue::Entity(player.id)),
        );
        let mut attackable = Vec::new();
        for player in &visible {
            if is_entity_attackable(cx, memories, player) {
                attackable.push(*player);
            }
        }
        memories.set(
            MemoryModuleType::NearestVisibleAttackablePlayers,
            MemoryValue::Entities(ids(&attackable)),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleAttackablePlayer,
            attackable
                .first()
                .map(|player| MemoryValue::Entity(player.id)),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HurtBySensor;

impl Sensor for HurtBySensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::HurtBy, MemoryModuleType::HurtByEntity]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        match cx.mob.last_damage {
            None => memories.erase(MemoryModuleType::HurtBy),
            Some(damage) => {
                memories.set(MemoryModuleType::HurtBy, MemoryValue::HurtBy(damage));
                if let Some(attacker) = damage.entity {
                    memories.set(
                        MemoryModuleType::HurtByEntity,
                        MemoryValue::Entity(attacker),
                    );
                }
            }
        }
        let attacker = memories
            .get(MemoryModuleType::HurtByEntity)
            .and_then(MemoryValue::as_entity);
        if let Some(attacker) = attacker
            && !cx.world.living(attacker).is_some_and(|living| living.alive)
        {
            memories.erase(MemoryModuleType::HurtByEntity);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemWish {
    #[default]
    Never,
    Any,
}

impl ItemWish {
    fn wants(self, _item: &DroppedItem) -> bool {
        match self {
            Self::Never => false,
            Self::Any => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NearestItemSensor {
    pub wish: ItemWish,
}

impl NearestItemSensor {
    pub fn new(wish: ItemWish) -> Self {
        Self { wish }
    }
}

impl Sensor for NearestItemSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::NearestVisibleWantedItem]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let area = inflate(&cx.mob.bounds(), ITEM_XZ_RANGE, ITEM_Y_RANGE, ITEM_XZ_RANGE);
        let mut items = cx.world.items_in(&area);
        items.retain(|item| item.alive);
        items.sort_by(|a, b| {
            cx.mob
                .distance_to_sqr(a.position)
                .total_cmp(&cx.mob.distance_to_sqr(b.position))
        });
        let eyes = cx.mob.eye_position();
        let limit = ITEM_MAX_DISTANCE * ITEM_MAX_DISTANCE;
        let found = items.iter().find(|item| {
            self.wish.wants(item) && cx.mob.distance_to_sqr(item.position) < limit && {
                let height = item.bounds.max_y - item.bounds.min_y;
                let to = rspigot_domain::Vec3::new(
                    item.position.x,
                    item.position.y + height * ITEM_EYE_HEIGHT_FACTOR,
                    item.position.z,
                );
                has_line_of_sight_between(cx.world, eyes, to)
            }
        });
        memories.set_option(
            MemoryModuleType::NearestVisibleWantedItem,
            found.map(|item| MemoryValue::Entity(item.id)),
        );
    }
}
