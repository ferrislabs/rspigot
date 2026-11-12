use std::borrow::Cow;
use std::num::NonZeroU8;

use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind};

use super::ageable::{self, BABY_START_AGE};
use super::base::EntityBase;
use super::living::{self, FOLLOW_RANGE, LivingState, MAX_HEALTH, RANDOM_SPAWN_BONUS, slot_index};
use super::species::{self, Species};
use super::{Entity, EntityState};
use crate::BiomeId;
use crate::difficulty::DifficultyInstance;
use crate::effect::{
    AttributeModifier, AttributeOperation, EffectDuration, EffectId, EffectInstance,
};
use crate::item::ItemStack;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};
use crate::world::Difficulty;

const SPAWN_BONUS_SPREAD: f64 = 0.114_850_000_000_000_01;
const LEFT_HANDED_CHANCE: f32 = 0.05;
const PICK_UP_LOOT_CHANCE: f32 = 0.55;
const ARMOR_CHANCE: f32 = 0.15;
const ARMOR_UPGRADE_CHANCE: f32 = 0.1087;
const ARMOR_UPGRADES: i32 = 3;
const HARD_ARMOR_STOP: f32 = 0.1;
const ARMOR_STOP: f32 = 0.25;
const WEAPON_ENCHANT_CHANCE: f32 = 0.25;
const ARMOR_ENCHANT_CHANCE: f32 = 0.5;
const HALLOWEEN_CHANCE: f32 = 0.25;
const JACK_O_LANTERN_CHANCE: f32 = 0.1;
const ZOMBIE_BABY_CHANCE: f32 = 0.05;
const JOCKEY_CHANCE: f64 = 0.05;
const BREAK_DOORS_FACTOR: f32 = 0.1;
const HARD_WEAPON_CHANCE: f32 = 0.05;
const WEAPON_CHANCE: f32 = 0.01;
const KNOCKBACK_BONUS: f64 = 0.050_000_000_745_058_06;
const FOLLOW_BONUS_FACTOR: f64 = 1.5;
const LEADER_CHANCE: f32 = 0.05;
const REINFORCEMENT_SPREAD: f64 = 0.100_000_001_490_116_12;
const SKELETON_JOCKEY_ROLL: i32 = 100;
const SPIDER_EFFECT_CHANCE: f32 = 0.1;
pub const SPAWN_REINFORCEMENTS: &str = "minecraft:spawn_reinforcements";
const KNOCKBACK_RESISTANCE: &str = "minecraft:knockback_resistance";
const ZOMBIE_SPAWN_BONUS: &str = "minecraft:zombie_random_spawn_bonus";
const LEADER_BONUS: &str = "minecraft:leader_zombie_bonus";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnEnvironment {
    pub biome: BiomeId,
    pub difficulty: DifficultyInstance,
    pub halloween: bool,
}

fn stack(kind: ItemKind) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::MIN)
}

pub fn set_item_slot(living: &mut LivingState, slot: EquipmentSlot, item: Option<ItemStack>) {
    living.equipment[slot_index(slot)] = item;
}

pub(crate) fn mob_finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if !living
        .attributes
        .has_modifier(FOLLOW_RANGE, RANDOM_SPAWN_BONUS)
    {
        let bonus = level.triangle(0.0, SPAWN_BONUS_SPREAD);
        living.attributes.add_modifier(
            FOLLOW_RANGE,
            AttributeModifier {
                id: Cow::Borrowed(RANDOM_SPAWN_BONUS),
                amount: bonus,
                operation: AttributeOperation::AddMultipliedBase,
            },
        );
    }
    living::set_left_handed(base, level.next_float() < LEFT_HANDED_CHANCE);
    group
}

pub(crate) fn ageable_finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut group = group.unwrap_or(SpawnGroupData::ageable(true));
    if let SpawnGroupData::Ageable {
        group_size,
        should_spawn_baby,
        baby_chance,
    } = &mut group
    {
        if *should_spawn_baby
            && *group_size > 0
            && level.next_float() <= *baby_chance
            && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::set_age(base, ageable, BABY_START_AGE);
        }
        *group_size += 1;
    }
    mob_finalize(base, living, Some(group), level)
}

pub fn equipment_for_slot(slot: EquipmentSlot, tier: i32) -> Option<ItemKind> {
    let row = match slot {
        EquipmentSlot::Head => [
            ItemKind::LeatherHelmet,
            ItemKind::CopperHelmet,
            ItemKind::GoldenHelmet,
            ItemKind::ChainmailHelmet,
            ItemKind::IronHelmet,
            ItemKind::DiamondHelmet,
        ],
        EquipmentSlot::Chest => [
            ItemKind::LeatherChestplate,
            ItemKind::CopperChestplate,
            ItemKind::GoldenChestplate,
            ItemKind::ChainmailChestplate,
            ItemKind::IronChestplate,
            ItemKind::DiamondChestplate,
        ],
        EquipmentSlot::Legs => [
            ItemKind::LeatherLeggings,
            ItemKind::CopperLeggings,
            ItemKind::GoldenLeggings,
            ItemKind::ChainmailLeggings,
            ItemKind::IronLeggings,
            ItemKind::DiamondLeggings,
        ],
        EquipmentSlot::Feet => [
            ItemKind::LeatherBoots,
            ItemKind::CopperBoots,
            ItemKind::GoldenBoots,
            ItemKind::ChainmailBoots,
            ItemKind::IronBoots,
            ItemKind::DiamondBoots,
        ],
        _ => return None,
    };
    usize::try_from(tier)
        .ok()
        .and_then(|tier| row.get(tier).copied())
}

pub fn populate_armor(
    living: &mut LivingState,
    difficulty: &DifficultyInstance,
    random: &mut impl RandomSource,
) {
    if random.next_float() >= ARMOR_CHANCE * difficulty.special_multiplier() {
        return;
    }
    let mut tier = random.next_int(3);
    for _ in 0..ARMOR_UPGRADES {
        if random.next_float() < ARMOR_UPGRADE_CHANCE {
            tier += 1;
        }
    }
    let stop = if difficulty.base == Difficulty::Hard {
        HARD_ARMOR_STOP
    } else {
        ARMOR_STOP
    };
    let mut first = true;
    for slot in [
        EquipmentSlot::Head,
        EquipmentSlot::Chest,
        EquipmentSlot::Legs,
        EquipmentSlot::Feet,
    ] {
        if !first && random.next_float() < stop {
            break;
        }
        first = false;
        if living.equipment(slot).is_none()
            && let Some(item) = equipment_for_slot(slot, tier)
        {
            set_item_slot(living, slot, Some(stack(item)));
        }
    }
}

pub fn enchantment_rolls(
    living: &LivingState,
    difficulty: &DifficultyInstance,
    random: &mut impl RandomSource,
) -> Vec<EquipmentSlot> {
    let special = difficulty.special_multiplier();
    [
        (EquipmentSlot::Mainhand, WEAPON_ENCHANT_CHANCE),
        (EquipmentSlot::Feet, ARMOR_ENCHANT_CHANCE),
        (EquipmentSlot::Legs, ARMOR_ENCHANT_CHANCE),
        (EquipmentSlot::Chest, ARMOR_ENCHANT_CHANCE),
        (EquipmentSlot::Head, ARMOR_ENCHANT_CHANCE),
    ]
    .into_iter()
    .filter(|(slot, chance)| {
        living.equipment(*slot).is_some() && random.next_float() < chance * special
    })
    .map(|(slot, _)| slot)
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JockeyRoll {
    Existing,
    New,
}

pub fn chicken_jockey_roll(random: &mut impl RandomSource) -> Option<JockeyRoll> {
    if f64::from(random.next_float()) < JOCKEY_CHANCE {
        Some(JockeyRoll::Existing)
    } else if f64::from(random.next_float()) < JOCKEY_CHANCE {
        Some(JockeyRoll::New)
    } else {
        None
    }
}

pub fn skeleton_jockey_roll(random: &mut impl RandomSource) -> bool {
    random.next_int(SKELETON_JOCKEY_ROLL) == 0
}

fn halloween_head(
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    random: &mut impl RandomSource,
) {
    if living.equipment(EquipmentSlot::Head).is_some()
        || !environment.halloween
        || random.next_float() >= HALLOWEEN_CHANCE
    {
        return;
    }
    let head = if random.next_float() < JACK_O_LANTERN_CHANCE {
        ItemKind::JackOLantern
    } else {
        ItemKind::CarvedPumpkin
    };
    set_item_slot(living, EquipmentSlot::Head, Some(stack(head)));
    if let Some(mob) = &mut living.mob {
        mob.drop_chances[slot_index(EquipmentSlot::Head)] = 0.0;
    }
}

fn add_or_replace(
    living: &mut LivingState,
    attribute: &str,
    id: &'static str,
    amount: f64,
    operation: AttributeOperation,
) {
    living.attributes.add_or_replace_modifier(
        attribute,
        AttributeModifier {
            id: Cow::Borrowed(id),
            amount,
            operation,
        },
    );
}

pub fn zombie_handle_attributes(
    base: &mut EntityBase,
    living: &mut LivingState,
    special: f32,
    reason: SpawnReason,
    own: &mut impl RandomSource,
) {
    living.attributes.set_base(
        SPAWN_REINFORCEMENTS,
        own.next_double() * REINFORCEMENT_SPREAD,
    );
    let knockback = own.next_double() * KNOCKBACK_BONUS;
    add_or_replace(
        living,
        KNOCKBACK_RESISTANCE,
        RANDOM_SPAWN_BONUS,
        knockback,
        AttributeOperation::AddValue,
    );
    let follow = own.next_double() * FOLLOW_BONUS_FACTOR * f64::from(special);
    if follow > 1.0 {
        add_or_replace(
            living,
            FOLLOW_RANGE,
            ZOMBIE_SPAWN_BONUS,
            follow,
            AttributeOperation::AddMultipliedTotal,
        );
    }
    if own.next_float() < special * LEADER_CHANCE {
        let reinforcements = own.next_double() * 0.25 + 0.5;
        add_or_replace(
            living,
            SPAWN_REINFORCEMENTS,
            LEADER_BONUS,
            reinforcements,
            AttributeOperation::AddValue,
        );
        let health = own.next_double() * 3.0 + 1.0;
        add_or_replace(
            living,
            MAX_HEALTH,
            LEADER_BONUS,
            health,
            AttributeOperation::AddMultipliedTotal,
        );
        if !matches!(
            reason,
            SpawnReason::Conversion | SpawnReason::Load | SpawnReason::DimensionTravel
        ) {
            let max = living.max_health() as f32;
            living::set_health(base, living, max);
        }
        if let Some(Species::Zombie(zombie)) = living.mob.as_mut().map(|mob| &mut mob.species) {
            zombie.can_break_doors = true;
        }
    }
}

fn set_can_pick_up_loot(living: &mut LivingState, value: bool) {
    if let Some(mob) = &mut living.mob {
        mob.can_pick_up_loot = value;
    }
}

fn zombie_finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
    own: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let group = mob_finalize(base, living, group, level);
    let difficulty = environment.difficulty;
    let special = difficulty.special_multiplier();
    if reason != SpawnReason::Conversion {
        set_can_pick_up_loot(living, level.next_float() < PICK_UP_LOOT_CHANCE * special);
    }
    let group = group.unwrap_or(SpawnGroupData::Zombie {
        baby: level.next_float() < ZOMBIE_BABY_CHANCE,
        can_spawn_jockey: true,
    });
    if let SpawnGroupData::Zombie {
        baby,
        can_spawn_jockey,
    } = group
    {
        if baby {
            species::set_zombie_baby(base, &mut living.attributes, true);
            if can_spawn_jockey {
                chicken_jockey_roll(level);
            }
        }
        let doors = level.next_float() < special * BREAK_DOORS_FACTOR;
        if let Some(Species::Zombie(zombie)) = living.mob.as_mut().map(|mob| &mut mob.species) {
            zombie.can_break_doors = doors;
        }
        if reason != SpawnReason::Conversion {
            populate_armor(living, &difficulty, level);
            let chance = if difficulty.base == Difficulty::Hard {
                HARD_WEAPON_CHANCE
            } else {
                WEAPON_CHANCE
            };
            if level.next_float() < chance {
                let weapon = match level.next_int(6) {
                    0 => ItemKind::IronSword,
                    1 => ItemKind::IronSpear,
                    _ => ItemKind::IronShovel,
                };
                set_item_slot(living, EquipmentSlot::Mainhand, Some(stack(weapon)));
            }
            enchantment_rolls(living, &difficulty, level);
        }
    }
    halloween_head(living, environment, level);
    zombie_handle_attributes(base, living, special, reason, own);
    Some(group)
}

fn skeleton_finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let group = mob_finalize(base, living, group, level);
    let difficulty = environment.difficulty;
    populate_armor(living, &difficulty, level);
    set_item_slot(living, EquipmentSlot::Mainhand, Some(stack(ItemKind::Bow)));
    enchantment_rolls(living, &difficulty, level);
    set_can_pick_up_loot(
        living,
        level.next_float() < PICK_UP_LOOT_CHANCE * difficulty.special_multiplier(),
    );
    halloween_head(living, environment, level);
    group
}

fn spider_finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let group = mob_finalize(base, living, group, level);
    skeleton_jockey_roll(level);
    let difficulty = environment.difficulty;
    let group = group.unwrap_or_else(|| {
        let mut effect = None;
        if difficulty.base == Difficulty::Hard
            && level.next_float() < SPIDER_EFFECT_CHANCE * difficulty.special_multiplier()
        {
            let name = match level.next_int(5) {
                0 | 1 => "minecraft:speed",
                2 => "minecraft:strength",
                3 => "minecraft:regeneration",
                _ => "minecraft:invisibility",
            };
            effect = EffectId::from_name(name);
        }
        SpawnGroupData::SpiderEffects { effect }
    });
    if let SpawnGroupData::SpiderEffects {
        effect: Some(effect),
    } = group
    {
        let _ = living.effects.apply(EffectInstance::new(
            effect,
            EffectDuration::Infinite,
            0,
            true,
        ));
    }
    Some(group)
}

pub fn finalize_spawn(
    entity: &mut Entity,
    environment: &SpawnEnvironment,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
    own: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let EntityState::Living(living) = &mut entity.state else {
        return group;
    };
    let base = &mut entity.base;
    if living.mob.is_none() {
        return group;
    }
    let kind = base.kind;
    let species = living.mob.as_ref().map(|mob| mob.species);
    match species {
        Some(Species::Cow | Species::Pig | Species::Chicken(_)) => {
            if let Some(farm) = species.and_then(|s| s.farm()) {
                if let Some(id) = species::select_variant(farm, environment.biome, level) {
                    species::set_variant(base, farm, id);
                }
                let sound = species::pick_sound_variant(farm, level);
                species::set_sound_variant(base, farm, sound);
            }
            ageable_finalize(base, living, group, level)
        }
        Some(Species::Sheep(_)) => {
            let color = species::sheep_spawn_color(environment.biome, level);
            species::set_wool_color(base, color);
            ageable_finalize(base, living, group, level)
        }
        Some(Species::Zombie(_)) if kind == EntityKind::Zombie => {
            zombie_finalize(base, living, environment, reason, group, level, own)
        }
        Some(Species::Skeleton(_)) => skeleton_finalize(base, living, environment, group, level),
        Some(Species::Wolf(_)) => species::wolf::finalize(base, living, environment, group, level),
        Some(Species::Cat(_)) => species::cat::finalize(base, living, environment, group, level),
        Some(Species::Fox(_)) => species::fox::finalize(base, living, environment, group, level),
        Some(Species::Panda(_)) => {
            species::panda::finalize(base, living, environment, group, level)
        }
        Some(Species::PolarBear(_)) => {
            species::polar_bear::finalize(base, living, environment, group, level)
        }
        Some(Species::Enderman(_)) => {
            species::enderman::finalize(base, living, environment, group, level)
        }
        Some(Species::Endermite(_)) => {
            species::endermite::finalize(base, living, environment, group, level)
        }
        Some(Species::Silverfish(_)) => {
            species::silverfish::finalize(base, living, environment, group, level)
        }
        Some(Species::IronGolem(_)) => {
            species::iron_golem::finalize(base, living, environment, group, level)
        }
        Some(Species::Rabbit(_)) => {
            species::rabbit::finalize(base, living, environment, group, level)
        }
        Some(Species::Ocelot(_)) => {
            species::ocelot::finalize(base, living, environment, group, level)
        }
        _ if kind == EntityKind::Spider => spider_finalize(base, living, environment, group, level),
        _ if kind.extends("AgeableMob") => ageable_finalize(base, living, group, level),
        _ => mob_finalize(base, living, group, level),
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::EntityBase;
    use crate::entity::ageable::DEFAULT_BABY_CHANCE;
    use crate::random::LegacyRandom;

    fn mob(kind: EntityKind) -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let living = LivingState::new(&mut base, 0.0);
        Entity::new(base, EntityState::Living(Box::new(living)))
    }

    fn environment(biome: &str, difficulty: Difficulty) -> SpawnEnvironment {
        SpawnEnvironment {
            biome: BiomeId(rspigot_data::registries::biome_id(biome).unwrap()),
            difficulty: DifficultyInstance::new(difficulty, 0, 0, 1.0),
            halloween: false,
        }
    }

    #[test]
    fn cows_in_snow_get_the_cold_variant_and_the_first_never_rolls_baby() {
        let mut cow = mob(EntityKind::Cow);
        let env = environment("minecraft:snowy_plains", Difficulty::Normal);
        let mut level = LegacyRandom::new(1);
        let mut own = LegacyRandom::new(2);
        let group = finalize_spawn(
            &mut cow,
            &env,
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        assert_eq!(
            species::variant(&cow.base, rspigot_data::variants::Farm::Cow),
            0
        );
        assert_eq!(
            group,
            Some(SpawnGroupData::Ageable {
                group_size: 1,
                should_spawn_baby: true,
                baby_chance: DEFAULT_BABY_CHANCE
            })
        );
        assert!(!cow.is_baby());
        let living = cow.living().unwrap();
        assert!(
            living
                .attributes
                .has_modifier(FOLLOW_RANGE, RANDOM_SPAWN_BONUS)
        );
    }

    #[test]
    fn skeletons_always_hold_a_bow() {
        let mut skeleton = mob(EntityKind::Skeleton);
        let env = environment("minecraft:plains", Difficulty::Easy);
        let mut level = LegacyRandom::new(3);
        let mut own = LegacyRandom::new(4);
        finalize_spawn(
            &mut skeleton,
            &env,
            SpawnReason::Command,
            None,
            &mut level,
            &mut own,
        );
        let living = skeleton.living().unwrap();
        assert_eq!(
            living.equipment(EquipmentSlot::Mainhand).map(|s| s.item),
            Some(ItemKind::Bow.id())
        );
    }

    #[test]
    fn zombies_roll_reinforcements_and_knockback_bonus() {
        let mut zombie = mob(EntityKind::Zombie);
        let env = environment("minecraft:plains", Difficulty::Hard);
        let mut level = LegacyRandom::new(5);
        let mut own = LegacyRandom::new(6);
        let group = finalize_spawn(
            &mut zombie,
            &env,
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        assert!(matches!(group, Some(SpawnGroupData::Zombie { .. })));
        let living = zombie.living().unwrap();
        assert!(
            living
                .attributes
                .has_modifier(KNOCKBACK_RESISTANCE, RANDOM_SPAWN_BONUS)
        );
    }

    #[test]
    fn armor_tiers_map_to_vanilla_items() {
        assert_eq!(
            equipment_for_slot(EquipmentSlot::Head, 0),
            Some(ItemKind::LeatherHelmet)
        );
        assert_eq!(
            equipment_for_slot(EquipmentSlot::Feet, 5),
            Some(ItemKind::DiamondBoots)
        );
        assert_eq!(equipment_for_slot(EquipmentSlot::Legs, 6), None);
        assert_eq!(equipment_for_slot(EquipmentSlot::Mainhand, 1), None);
    }
}
