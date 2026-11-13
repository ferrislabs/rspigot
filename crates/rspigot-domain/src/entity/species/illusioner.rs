use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use super::spellcaster::{self, SpellcasterState};
use crate::entity::base::EntityBase;
use crate::entity::living::{LivingState, slot_index};
use crate::entity::spawn::{mob_finalize, set_item_slot};
use crate::item::ItemStack;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const KEYS: &[&str] = spellcaster::KEYS;
pub const AMBIENT_SOUND: &str = "minecraft:entity.illusioner.ambient";
pub const HURT_SOUND: &str = "minecraft:entity.illusioner.hurt";
pub const DEATH_SOUND: &str = "minecraft:entity.illusioner.death";
pub const CAST_SOUND: &str = "minecraft:entity.illusioner.cast_spell";
pub const PREPARE_BLINDNESS_SOUND: &str = "minecraft:entity.illusioner.prepare_blindness";
pub const PREPARE_MIRROR_SOUND: &str = "minecraft:entity.illusioner.prepare_mirror";
pub const MIRROR_MOVE_SOUND: &str = "minecraft:entity.illusioner.mirror_move";
pub const BLINDNESS_TICKS: i32 = 400;
pub const INVISIBILITY_TICKS: i32 = 1200;
pub const MIRROR_CASTING_TICKS: i32 = 20;
pub const MIRROR_INTERVAL: i32 = 340;
pub const BLINDNESS_CASTING_TICKS: i32 = 20;
pub const BLINDNESS_INTERVAL: i32 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IllusionerState {
    pub raider: RaiderState,
    pub spell: SpellcasterState,
}

pub fn define(base: &mut EntityBase) -> IllusionerState {
    raider::define(base);
    spellcaster::define(base);
    IllusionerState::default()
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut IllusionerState) {
    raider::read(nbt, &mut state.raider);
    spellcaster::read(nbt, &mut state.spell);
}

pub fn write(_base: &EntityBase, state: &IllusionerState, out: &mut Compound) {
    raider::write(&state.raider, out);
    spellcaster::write(&state.spell, out);
}

fn bow() -> ItemStack {
    ItemStack::new(ItemKind::Bow.id(), std::num::NonZeroU8::MIN)
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    set_item_slot(living, EquipmentSlot::Mainhand, Some(bow()));
    let leader = match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(crate::entity::species::Species::Illusioner(state)) => {
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

    fn illusioner() -> Entity {
        load_entity(
            &Compound::new().with("id", "minecraft:illusioner"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
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

    fn state_of(entity: &Entity) -> IllusionerState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Illusioner(state)) => state,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_illusioner_spawns_with_a_bow() {
        let mut entity = illusioner();
        let mut level = LegacyRandom::new(4);
        let mut own = LegacyRandom::new(3);
        finalize_spawn(
            &mut entity,
            &environment(),
            SpawnReason::SpawnItemUse,
            None,
            &mut level,
            &mut own,
        );
        let bow = entity
            .living()
            .unwrap()
            .equipment(EquipmentSlot::Mainhand)
            .unwrap();
        assert_eq!(bow.item, ItemKind::Bow.id());
        assert!(matches!(entity.state, EntityState::Living(_)));
        assert_eq!(entity.base.kind, EntityKind::Illusioner);
    }

    #[test]
    fn the_saved_layout_is_the_raider_fields_and_the_spell_ticks() {
        let mut entity = illusioner();
        if let Some(Species::Illusioner(state)) = entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            state.spell.casting_ticks = 12;
            state.raider.wave = 2;
        }
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("SpellTicks"), Some(12));
        assert_eq!(saved.get_i32("Wave"), Some(2));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(state_of(&loaded).spell.casting_ticks, 12);
        assert_eq!(save_entity(&loaded, None), Some(saved));
    }
}
