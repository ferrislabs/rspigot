use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use crate::entity::base::EntityBase;
use crate::entity::living::{LivingState, slot_index};
use crate::entity::spawn::{mob_finalize, set_item_slot};
use crate::item::ItemStack;
use crate::raid;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};
use crate::text::Component;

pub const JOHNNY_KEY: &str = "Johnny";
pub const JOHNNY_NAME: &str = "Johnny";
pub const KEYS: &[&str] = &[
    "Wave",
    "CanJoinRaid",
    "patrol_target",
    "PatrolLeader",
    "Patrolling",
    "RaidId",
    JOHNNY_KEY,
];
pub const AMBIENT_SOUND: &str = "minecraft:entity.vindicator.ambient";
pub const HURT_SOUND: &str = "minecraft:entity.vindicator.hurt";
pub const DEATH_SOUND: &str = "minecraft:entity.vindicator.death";
pub const CELEBRATE_SOUND: &str = "minecraft:entity.vindicator.celebrate";
pub const SHARPNESS: &str = "minecraft:sharpness";
pub const BREAK_DOOR_DISTANCE: i32 = 6;
pub const BREAK_DOOR_TICK_DELAY: i32 = 10;
const ARMOR_SLOTS: [EquipmentSlot; 4] = [
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VindicatorState {
    pub raider: RaiderState,
    pub johnny: bool,
}

impl VindicatorState {
    pub fn name_makes_johnny(&mut self, name: Option<&Component>) {
        if !self.johnny
            && let Some(name) = name
            && name.plain_text() == JOHNNY_NAME
        {
            self.johnny = true;
        }
    }
}

pub fn define(base: &mut EntityBase) -> VindicatorState {
    raider::define(base);
    VindicatorState::default()
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut VindicatorState) {
    raider::read(nbt, &mut state.raider);
    state.johnny = nbt.get_bool(JOHNNY_KEY).unwrap_or(false);
    let name = base.custom_name().cloned();
    state.name_makes_johnny(name.as_ref());
}

pub fn write(_base: &EntityBase, state: &VindicatorState, out: &mut Compound) {
    raider::write(&state.raider, out);
    if state.johnny {
        out.insert(JOHNNY_KEY, true);
    }
}

fn iron_axe() -> ItemStack {
    ItemStack::new(ItemKind::IronAxe.id(), std::num::NonZeroU8::MIN)
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let (leader, in_raid) = match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(crate::entity::species::Species::Vindicator(state)) => {
            state.raider.finalize(reason, true, true, level);
            (state.raider.patrol_leader, state.raider.raid_id.is_some())
        }
        _ => (false, false),
    };
    if leader {
        set_item_slot(living, EquipmentSlot::Head, Some(raider::ominous_banner()));
        if let Some(mob) = &mut living.mob {
            mob.drop_chances[slot_index(EquipmentSlot::Head)] = raider::OMINOUS_BANNER_DROP_CHANCE;
        }
    }
    let group = mob_finalize(base, living, group, level);
    if !in_raid {
        set_item_slot(living, EquipmentSlot::Mainhand, Some(iron_axe()));
    }
    if living.equipment(EquipmentSlot::Mainhand).is_some() {
        let _ = level.next_float();
    }
    for slot in ARMOR_SLOTS {
        if living.equipment(slot).is_some() {
            let _ = level.next_float();
        }
    }
    group
}

pub fn apply_raid_buffs(
    living: &mut LivingState,
    wave: i32,
    enchant_odds: f32,
    random: &mut impl RandomSource,
) {
    let mut axe = iron_axe();
    if random.next_float() <= enchant_odds {
        axe.set_enchantments(&[(SHARPNESS.to_owned(), raid::vindicator_sharpness(wave))]);
    }
    set_item_slot(living, EquipmentSlot::Mainhand, Some(axe));
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

    fn vindicator() -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Vindicator);
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

    fn state_of(entity: &Entity) -> VindicatorState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Vindicator(state)) => state,
            _ => panic!("a vindicator has the vindicator species"),
        }
    }

    fn state_mut(entity: &mut Entity) -> &mut VindicatorState {
        match entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            Some(Species::Vindicator(state)) => state,
            _ => panic!("a vindicator has the vindicator species"),
        }
    }

    fn spawn(entity: &mut Entity, reason: SpawnReason, seed: i64) {
        let mut level = LegacyRandom::new(seed);
        let mut own = LegacyRandom::new(3);
        finalize_spawn(entity, &environment(), reason, None, &mut level, &mut own);
    }

    #[test]
    fn a_vindicator_has_the_vindicator_species_and_the_celebration_flag() {
        let entity = vindicator();
        assert!(!raider::is_celebrating(&entity.base));
        assert!(!state_of(&entity).johnny);
        assert!(!state_of(&entity).raider.can_join_raid);
    }

    #[test]
    fn a_natural_vindicator_carries_an_iron_axe_and_joins_raids() {
        let mut entity = vindicator();
        spawn(&mut entity, SpawnReason::Structure, 4);
        let living = entity.living().unwrap();
        let axe = living.equipment(EquipmentSlot::Mainhand).unwrap();
        assert_eq!(axe.item, ItemKind::IronAxe.id());
        assert!(state_of(&entity).raider.can_join_raid);
    }

    #[test]
    fn a_vindicator_already_in_a_raid_gets_no_default_axe() {
        let mut entity = vindicator();
        state_mut(&mut entity).raider.raid_id = Some(2);
        spawn(&mut entity, SpawnReason::Event, 4);
        assert!(
            entity
                .living()
                .unwrap()
                .equipment(EquipmentSlot::Mainhand)
                .is_none()
        );
    }

    #[test]
    fn raid_buffs_replace_the_axe_and_enchant_it_by_the_odds_and_wave() {
        let mut entity = vindicator();
        let mut sure = LegacyRandom::new(1);
        apply_raid_buffs(entity.living_mut().unwrap(), 2, 1.0, &mut sure);
        let axe = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap()
            .clone();
        assert_eq!(axe.enchantments(), vec![(SHARPNESS.to_owned(), 1)]);
        apply_raid_buffs(entity.living_mut().unwrap(), 6, 1.0, &mut sure);
        let axe = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap();
        assert_eq!(axe.enchantments(), vec![(SHARPNESS.to_owned(), 2)]);
        apply_raid_buffs(entity.living_mut().unwrap(), 2, 0.0, &mut sure);
        let axe = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap();
        assert!(axe.enchantments().is_empty());
        assert_eq!(axe.item, ItemKind::IronAxe.id());
    }

    #[test]
    fn a_leader_wears_the_ominous_banner() {
        let seed = (0..100_000)
            .find(|seed| LegacyRandom::new(*seed).next_float() < raider::PATROL_LEADER_ODDS)
            .unwrap();
        let mut entity = vindicator();
        spawn(&mut entity, SpawnReason::Natural, seed);
        assert!(state_of(&entity).raider.patrol_leader);
        let living = entity.living().unwrap();
        assert!(living.equipment(EquipmentSlot::Head).is_some());
    }

    #[test]
    fn johnny_is_remembered_once_the_name_matches() {
        let mut state = VindicatorState::default();
        state.name_makes_johnny(Some(&Component::text("Bob")));
        assert!(!state.johnny);
        state.name_makes_johnny(None);
        assert!(!state.johnny);
        state.name_makes_johnny(Some(&Component::text("Johnny")));
        assert!(state.johnny);
        state.name_makes_johnny(Some(&Component::text("Bob")));
        assert!(state.johnny);
    }

    #[test]
    fn the_saved_layout_round_trips_johnny_and_the_raider_fields() {
        let mut entity = vindicator();
        {
            let state = state_mut(&mut entity);
            state.johnny = true;
            state.raider.wave = 3;
            state.raider.can_join_raid = true;
            state.raider.raid_id = Some(7);
        }
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_bool("Johnny"), Some(true));
        assert_eq!(saved.get_i32("Wave"), Some(3));
        assert_eq!(saved.get_i32("RaidId"), Some(7));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        let state = state_of(&loaded);
        assert!(state.johnny);
        assert_eq!(state.raider.wave, 3);
        assert_eq!(state.raider.raid_id, Some(7));
    }

    #[test]
    fn johnny_is_not_written_unless_set_and_a_johnny_name_sets_it_on_load() {
        let mut entity = vindicator();
        let saved = save_entity(&entity, None).unwrap();
        assert!(!saved.contains_key("Johnny"));
        entity.base.set_custom_name(Some(Component::text("Johnny")));
        let saved = save_entity(&entity, None).unwrap();
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(state_of(&loaded).johnny);
    }
}
