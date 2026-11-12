use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind;

use crate::animal::block_tag;
use crate::behavior::{Memories, MemoryModuleType, MemoryValue, Sensor};
use crate::brain::AiContext;
use crate::goals::TemptItems;
use crate::scan::manhattan_ordered;
use crate::targeting::mob_can_attack;
use crate::world::{AiWorld, Living};

const REPELLENT_REACH_XZ: i32 = 8;
const REPELLENT_REACH_Y: i32 = 4;
const HOGLIN_REPELLENTS: &str = "minecraft:hoglin_repellents";
const PIGLIN_REPELLENTS: &str = "minecraft:piglin_repellents";
const PIGLIN_LOVED: TemptItems = TemptItems::Tag("minecraft:piglin_loved");

fn visible_entities<W: AiWorld>(
    cx: &mut AiContext<'_, W>,
    memories: &Memories,
    filter: impl Fn(&Living) -> bool,
) -> Vec<Living> {
    memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .map(|visible| visible.find_all(cx, memories, filter))
        .unwrap_or_default()
}

fn entity_value(living: Option<&Living>) -> Option<MemoryValue> {
    living.map(|living| MemoryValue::Entity(living.id))
}

fn entities_value(list: &[Living]) -> Option<MemoryValue> {
    Some(MemoryValue::Entities(
        list.iter().map(|living| living.id).collect(),
    ))
}

fn find_nearest_repellent<W: AiWorld>(
    cx: &AiContext<'_, W>,
    tag: &str,
    lit_soul_campfire: bool,
) -> Option<BlockPos> {
    let origin = cx.mob.block_pos();
    manhattan_ordered(
        origin,
        REPELLENT_REACH_XZ,
        REPELLENT_REACH_Y,
        REPELLENT_REACH_XZ,
    )
    .find(|pos| {
        cx.world.state(*pos).is_some_and(|state| {
            block_tag(Some(state), tag) && (!lit_soul_campfire || valid_repellent(state))
        })
    })
}

fn valid_repellent(state: StateId) -> bool {
    state.kind() != Some(BlockKind::SoulCampfire) || state.property("lit") == Some("true")
}

pub fn is_zombified(kind: EntityKind) -> bool {
    kind == EntityKind::ZombifiedPiglin || kind == EntityKind::Zoglin
}

pub fn is_player_holding_loved_item(entity: &Living) -> bool {
    entity.player
        && (PIGLIN_LOVED.matches(entity.main_hand) || PIGLIN_LOVED.matches(entity.off_hand))
}

fn nearby_adult_piglins<W: AiWorld>(cx: &AiContext<'_, W>, memories: &Memories) -> Vec<Living> {
    memories
        .get(MemoryModuleType::NearestLivingEntities)
        .and_then(MemoryValue::as_entities)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| cx.world.living(*id))
                .filter(|living| {
                    matches!(living.kind, EntityKind::Piglin | EntityKind::PiglinBrute)
                        && !living.baby
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoglinSensor;

impl Sensor for HoglinSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestRepellent,
            MemoryModuleType::NearestVisibleAdultPiglin,
            MemoryModuleType::NearestVisibleAdultHoglins,
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryModuleType::VisibleAdultHoglinCount,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let repellent = find_nearest_repellent(cx, HOGLIN_REPELLENTS, false);
        cx.brain.repellent = repellent;
        memories.set_option(
            MemoryModuleType::NearestRepellent,
            repellent.map(MemoryValue::Block),
        );
        let adults = visible_entities(cx, memories, |entity| {
            !entity.baby && matches!(entity.kind, EntityKind::Piglin | EntityKind::Hoglin)
        });
        let piglins: Vec<Living> = adults
            .iter()
            .filter(|entity| entity.kind == EntityKind::Piglin)
            .copied()
            .collect();
        let hoglins: Vec<Living> = adults
            .iter()
            .filter(|entity| entity.kind == EntityKind::Hoglin)
            .copied()
            .collect();
        memories.set_option(
            MemoryModuleType::NearestVisibleAdultPiglin,
            entity_value(piglins.first()),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleAdultHoglins,
            entities_value(&hoglins),
        );
        memories.set(
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryValue::Int(piglins.len() as i32),
        );
        memories.set(
            MemoryModuleType::VisibleAdultHoglinCount,
            MemoryValue::Int(hoglins.len() as i32),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PiglinSensor;

impl Sensor for PiglinSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestLivingEntities,
            MemoryModuleType::NearestVisibleNemesis,
            MemoryModuleType::NearestTargetablePlayerNotWearingGold,
            MemoryModuleType::NearestPlayerHoldingWantedItem,
            MemoryModuleType::NearestVisibleHuntableHoglin,
            MemoryModuleType::NearestVisibleBabyHoglin,
            MemoryModuleType::NearestVisibleAdultPiglins,
            MemoryModuleType::NearbyAdultPiglins,
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryModuleType::VisibleAdultHoglinCount,
            MemoryModuleType::NearestRepellent,
            MemoryModuleType::NearestVisibleZombified,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let repellent = find_nearest_repellent(cx, PIGLIN_REPELLENTS, true);
        memories.set_option(
            MemoryModuleType::NearestRepellent,
            repellent.map(MemoryValue::Block),
        );
        let mut nemesis: Option<Living> = None;
        let mut huntable: Option<Living> = None;
        let mut baby_hoglin: Option<Living> = None;
        let mut zombified: Option<Living> = None;
        let mut not_wearing_gold: Option<Living> = None;
        let mut holding_wanted: Option<Living> = None;
        let mut adult_hoglins = 0;
        let mut adult_piglins: Vec<Living> = Vec::new();
        let visible = visible_entities(cx, memories, |_| true);
        for entity in visible {
            match entity.kind {
                EntityKind::Hoglin => {
                    if entity.baby && baby_hoglin.is_none() {
                        baby_hoglin = Some(entity);
                    } else if !entity.baby {
                        adult_hoglins += 1;
                        if huntable.is_none() && !entity.facts.cannot_be_hunted {
                            huntable = Some(entity);
                        }
                    }
                }
                EntityKind::PiglinBrute => adult_piglins.push(entity),
                EntityKind::Piglin => {
                    if !entity.baby {
                        adult_piglins.push(entity);
                    }
                }
                EntityKind::Player => {
                    if not_wearing_gold.is_none()
                        && !entity.facts.piglin_safe_armor
                        && mob_can_attack(cx.mob, cx.world, &entity)
                    {
                        not_wearing_gold = Some(entity);
                    }
                    if holding_wanted.is_none()
                        && !entity.spectator
                        && is_player_holding_loved_item(&entity)
                    {
                        holding_wanted = Some(entity);
                    }
                }
                kind => {
                    if nemesis.is_none()
                        && matches!(kind, EntityKind::WitherSkeleton | EntityKind::Wither)
                    {
                        nemesis = Some(entity);
                    } else if zombified.is_none() && is_zombified(kind) {
                        zombified = Some(entity);
                    }
                }
            }
        }
        let nearby = nearby_adult_piglins(cx, memories);
        memories.set_option(
            MemoryModuleType::NearestVisibleNemesis,
            entity_value(nemesis.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleHuntableHoglin,
            entity_value(huntable.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleBabyHoglin,
            entity_value(baby_hoglin.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleZombified,
            entity_value(zombified.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearestTargetablePlayerNotWearingGold,
            entity_value(not_wearing_gold.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearestPlayerHoldingWantedItem,
            entity_value(holding_wanted.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearbyAdultPiglins,
            entities_value(&nearby),
        );
        memories.set_option(
            MemoryModuleType::NearestVisibleAdultPiglins,
            entities_value(&adult_piglins),
        );
        memories.set(
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryValue::Int(adult_piglins.len() as i32),
        );
        memories.set(
            MemoryModuleType::VisibleAdultHoglinCount,
            MemoryValue::Int(adult_hoglins),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PiglinBruteSensor;

impl Sensor for PiglinBruteSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestVisibleNemesis,
            MemoryModuleType::NearbyAdultPiglins,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let nemesis = memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .and_then(|visible| {
                visible.find_closest(cx, memories, |entity| {
                    matches!(entity.kind, EntityKind::WitherSkeleton | EntityKind::Wither)
                })
            });
        let nearby = nearby_adult_piglins(cx, memories);
        memories.set_option(
            MemoryModuleType::NearestVisibleNemesis,
            entity_value(nemesis.as_ref()),
        );
        memories.set_option(
            MemoryModuleType::NearbyAdultPiglins,
            entities_value(&nearby),
        );
    }
}
