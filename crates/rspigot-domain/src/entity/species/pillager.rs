use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use crate::crossbow;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingState, slot_index};
use crate::entity::spawn::{mob_finalize, set_item_slot};
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_IS_CHARGING_CROSSBOW: u8 = 17;
pub const KEYS: &[&str] = raider::KEYS;
pub const AMBIENT_SOUND: &str = "minecraft:entity.pillager.ambient";
pub const CELEBRATE_SOUND: &str = "minecraft:entity.pillager.celebrate";
pub const CROSSBOW_ENCHANT_ODDS: i32 = 300;
pub const SPAWN_CROSSBOW_ENCHANTMENT: &str = crossbow::PIERCING;
pub const SPAWN_CROSSBOW_LEVEL: u8 = 1;
pub const INVENTORY_SIZE: usize = 5;
const ARMOR_SLOTS: [EquipmentSlot; 4] = [
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PillagerState {
    pub raider: RaiderState,
}

pub fn define(base: &mut EntityBase) -> PillagerState {
    raider::define(base);
    base.data
        .define(DATA_IS_CHARGING_CROSSBOW, SyncedValue::Boolean(false));
    PillagerState::default()
}

pub fn is_charging_crossbow(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_CHARGING_CROSSBOW)
}

pub fn set_charging_crossbow(base: &mut EntityBase, charging: bool) {
    base.data
        .set(DATA_IS_CHARGING_CROSSBOW, SyncedValue::Boolean(charging));
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut PillagerState) {
    raider::read(nbt, &mut state.raider);
}

pub fn write(_base: &EntityBase, state: &PillagerState, out: &mut Compound) {
    raider::write(&state.raider, out);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    set_item_slot(
        living,
        EquipmentSlot::Mainhand,
        Some(crossbow::single(ItemKind::Crossbow)),
    );
    enchant_spawned_equipment(living, level);
    let leader = match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(crate::entity::species::Species::Pillager(state)) => {
            state.raider.finalize(reason, true, true, level);
            state.raider.patrol_leader
        }
        _ => false,
    };
    if leader {
        set_item_slot(living, EquipmentSlot::Head, Some(raider::ominous_banner()));
        if let Some(mob) = &mut living.mob {
            mob.drop_chances[slot_index(EquipmentSlot::Head)] = raider::OMINOUS_BANNER_DROP_CHANCE;
        }
    }
    mob_finalize(base, living, group, level)
}

pub const QUICK_CHARGE: &str = "minecraft:quick_charge";

pub fn apply_raid_buffs(
    living: &mut LivingState,
    wave: i32,
    enchant_odds: f32,
    random: &mut impl RandomSource,
) {
    if random.next_float() > enchant_odds {
        return;
    }
    let Some(level) = crate::raid::pillager_quick_charge(wave) else {
        return;
    };
    let mut crossbow = crossbow::single(ItemKind::Crossbow);
    crossbow.set_enchantments(&[(QUICK_CHARGE.to_owned(), level)]);
    set_item_slot(living, EquipmentSlot::Mainhand, Some(crossbow));
}

fn enchant_spawned_equipment(living: &mut LivingState, level: &mut impl RandomSource) {
    if living.equipment(EquipmentSlot::Mainhand).is_some() {
        let _ = level.next_float();
    }
    if level.next_int(CROSSBOW_ENCHANT_ODDS) == 0
        && let Some(weapon) = living.equipment[slot_index(EquipmentSlot::Mainhand)].as_mut()
        && weapon.item == ItemKind::Crossbow.id()
    {
        let mut levels = weapon.enchantments();
        levels.retain(|(name, _)| name != SPAWN_CROSSBOW_ENCHANTMENT);
        levels.push((SPAWN_CROSSBOW_ENCHANTMENT.to_owned(), SPAWN_CROSSBOW_LEVEL));
        weapon.set_enchantments(&levels);
    }
    for slot in ARMOR_SLOTS {
        if living.equipment(slot).is_some() {
            let _ = level.next_float();
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};
    use crate::random::LegacyRandom;

    fn pillager() -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Pillager);
        let living = LivingState::new(&mut base, 0.0);
        Entity::new(base, EntityState::Living(Box::new(living)))
    }

    fn environment() -> SpawnEnvironment {
        SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        }
    }

    fn state_of(entity: &Entity) -> PillagerState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Pillager(state)) => state,
            _ => panic!("a pillager has the pillager species"),
        }
    }

    fn spawned(reason: SpawnReason, seed: i64) -> Entity {
        let mut entity = pillager();
        let mut level = LegacyRandom::new(seed);
        let mut own = LegacyRandom::new(3);
        finalize_spawn(
            &mut entity,
            &environment(),
            reason,
            None,
            &mut level,
            &mut own,
        );
        entity
    }

    fn seed_where(check: impl Fn(&mut LegacyRandom) -> bool) -> i64 {
        (0..100_000)
            .find(|seed| check(&mut LegacyRandom::new(*seed)))
            .expect("a matching seed")
    }

    #[test]
    fn raid_buffs_give_a_quick_charge_crossbow_only_after_wave_three_and_by_the_odds() {
        let mut entity = pillager();
        let mut random = LegacyRandom::new(1);
        apply_raid_buffs(entity.living_mut().unwrap(), 3, 1.0, &mut random);
        assert!(
            entity
                .living()
                .unwrap()
                .equipment(EquipmentSlot::Mainhand)
                .is_none()
        );
        apply_raid_buffs(entity.living_mut().unwrap(), 4, 0.0, &mut random);
        assert!(
            entity
                .living()
                .unwrap()
                .equipment(EquipmentSlot::Mainhand)
                .is_none()
        );
        apply_raid_buffs(entity.living_mut().unwrap(), 4, 1.0, &mut random);
        let crossbow = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap();
        assert_eq!(crossbow.enchantments(), vec![(QUICK_CHARGE.to_owned(), 1)]);
        apply_raid_buffs(entity.living_mut().unwrap(), 6, 1.0, &mut random);
        let crossbow = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap();
        assert_eq!(crossbow.enchantments(), vec![(QUICK_CHARGE.to_owned(), 2)]);
    }

    #[test]
    fn a_pillager_defines_the_charging_flag_after_the_celebration_flag() {
        let entity = pillager();
        assert!(!raider::is_celebrating(&entity.base));
        assert!(!is_charging_crossbow(&entity.base));
        let mut base = entity.base;
        set_charging_crossbow(&mut base, true);
        assert!(is_charging_crossbow(&base));
        assert_eq!(DATA_IS_CHARGING_CROSSBOW, raider::DATA_IS_CELEBRATING + 1);
    }

    #[test]
    fn a_spawned_pillager_holds_a_crossbow_in_its_main_hand_and_nothing_else() {
        let entity = spawned(SpawnReason::Natural, 5);
        let living = entity.living().unwrap();
        let weapon = living.equipment(EquipmentSlot::Mainhand).unwrap();
        assert_eq!(weapon.item, ItemKind::Crossbow.id());
        assert_eq!(weapon.count.get(), 1);
        for slot in [
            EquipmentSlot::Offhand,
            EquipmentSlot::Feet,
            EquipmentSlot::Legs,
            EquipmentSlot::Chest,
        ] {
            assert!(living.equipment(slot).is_none(), "{slot:?}");
        }
    }

    #[test]
    fn the_spawn_draws_follow_the_vanilla_order() {
        let seed = seed_where(|random| {
            random.next_float();
            random.next_int(300) != 0
        });
        let mut mirror = LegacyRandom::new(seed);
        mirror.next_float();
        mirror.next_int(300);
        let patrol_roll = mirror.next_float();
        let mut level = LegacyRandom::new(seed);
        let mut entity = pillager();
        let mut own = LegacyRandom::new(3);
        finalize_spawn(
            &mut entity,
            &environment(),
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        assert_eq!(
            state_of(&entity).raider.patrol_leader,
            patrol_roll < raider::PATROL_LEADER_ODDS
        );
        let living = entity.living().unwrap();
        assert!(living.equipment(EquipmentSlot::Mainhand).is_some());
    }

    #[test]
    fn one_spawn_in_three_hundred_gets_a_piercing_crossbow() {
        let lucky = seed_where(|random| {
            random.next_float();
            random.next_int(300) == 0
        });
        let entity = spawned(SpawnReason::Natural, lucky);
        let weapon = entity
            .living()
            .and_then(|living| living.equipment(EquipmentSlot::Mainhand).cloned())
            .unwrap();
        assert_eq!(
            weapon.enchantments(),
            vec![("minecraft:piercing".to_owned(), 1)]
        );
        let plain = seed_where(|random| {
            random.next_float();
            random.next_int(300) != 0
        });
        let entity = spawned(SpawnReason::Natural, plain);
        let weapon = entity
            .living()
            .and_then(|living| living.equipment(EquipmentSlot::Mainhand).cloned())
            .unwrap();
        assert!(weapon.enchantments().is_empty());
    }

    #[test]
    fn a_pillager_can_join_raids_whatever_the_spawn_reason() {
        for reason in [
            SpawnReason::Natural,
            SpawnReason::Spawner,
            SpawnReason::Command,
            SpawnReason::Patrol,
        ] {
            assert!(
                state_of(&spawned(reason, 7)).raider.can_join_raid,
                "{reason:?}"
            );
        }
    }

    #[test]
    fn a_patrol_spawn_patrols_and_never_rolls_to_lead() {
        for seed in 0..50 {
            let entity = spawned(SpawnReason::Patrol, seed);
            let state = state_of(&entity);
            assert!(state.raider.patrolling);
            assert!(!state.raider.patrol_leader);
            assert!(
                entity
                    .living()
                    .unwrap()
                    .equipment(EquipmentSlot::Head)
                    .is_none()
            );
        }
    }

    #[test]
    fn a_natural_leader_wears_an_ominous_banner_that_always_drops() {
        let seed = seed_where(|random| {
            random.next_float();
            random.next_int(300);
            random.next_float() < raider::PATROL_LEADER_ODDS
        });
        let entity = spawned(SpawnReason::Natural, seed);
        assert!(state_of(&entity).raider.patrol_leader);
        let living = entity.living().unwrap();
        let banner = living.equipment(EquipmentSlot::Head).unwrap();
        assert_eq!(*banner, raider::ominous_banner());
        let mob = living.mob.as_ref().unwrap();
        assert_eq!(mob.drop_chances[slot_index(EquipmentSlot::Head)], 2.0);
        assert_eq!(
            mob.drop_chances[slot_index(EquipmentSlot::Mainhand)],
            crate::entity::living::DEFAULT_DROP_CHANCE
        );
    }

    #[test]
    fn the_ominous_banner_has_the_eight_vanilla_layers_a_hidden_tooltip_and_a_name() {
        let banner = raider::ominous_banner();
        assert_eq!(banner.item, ItemKind::WhiteBanner.id());
        let layers = banner
            .components
            .get_list("minecraft:banner_patterns")
            .unwrap();
        assert_eq!(layers.len(), 8);
        assert_eq!(
            banner.components.get_str("minecraft:rarity"),
            Some("uncommon")
        );
        assert!(banner.components.get("minecraft:item_name").is_some());
        let hidden = banner
            .components
            .get_compound("minecraft:tooltip_display")
            .and_then(|display| display.get_list("hidden_components"))
            .unwrap();
        assert_eq!(hidden.len(), 1);
    }

    #[test]
    fn a_pillager_round_trips_its_raider_state_and_keeps_its_inventory() {
        let nbt = Compound::new()
            .with("id", "minecraft:pillager")
            .with("Wave", 3)
            .with("CanJoinRaid", true)
            .with("Patrolling", true)
            .with("PatrolLeader", true)
            .with(
                "Inventory",
                rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:arrow")
                        .with("count", 4),
                )]),
            );
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let state = state_of(&entity);
        assert_eq!(state.raider.wave, 3);
        assert!(state.raider.patrol_leader && state.raider.patrolling);
        assert!(
            entity
                .living()
                .and_then(|living| living.mob.as_ref())
                .is_some_and(|mob| mob.can_pick_up_loot)
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Wave"), Some(3));
        assert_eq!(saved.get_list("Inventory").map(<[_]>::len), Some(1));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }
}
