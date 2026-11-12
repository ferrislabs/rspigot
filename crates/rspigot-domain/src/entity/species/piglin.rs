use std::borrow::Cow;
use std::num::NonZeroU8;

use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Compound;

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingAttributes, LivingState};
use crate::entity::spawn::{SpawnEnvironment, enchantment_rolls, mob_finalize, set_item_slot};
use crate::item::ItemStack;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_IMMUNE_TO_ZOMBIFICATION: u8 = 16;
pub const DATA_BABY: u8 = 17;
pub const DATA_CHARGING_CROSSBOW: u8 = 18;
pub const DATA_DANCING: u8 = 19;
pub const CONVERSION_TICKS: i32 = 300;
pub const BABY_SPEED_MODIFIER: &str = "minecraft:baby";
pub const BABY_SPEED_BONUS: f64 = 0.2;
pub const BABY_CHANCE: f32 = 0.2;
pub const CROSSBOW_CHANCE: f32 = 0.5;
pub const SPEAR_ODDS: i32 = 10;
pub const ARMOR_CHANCE: f32 = 0.1;
pub const HUNT_COOLDOWN_SECONDS: (i32, i32) = (30, 120);
pub const XP_REWARD: i32 = 5;
pub const BRUTE_XP_REWARD: i32 = 20;
pub const KEYS: &[&str] = &[
    "IsBaby",
    "CannotHunt",
    "IsImmuneToZombification",
    "TimeInOverworld",
];
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PiglinState {
    pub time_in_overworld: i32,
    pub cannot_hunt: bool,
    pub pending_hunt_cooldown: Option<i32>,
    pub needs_home: bool,
}

pub fn define(base: &mut EntityBase) -> PiglinState {
    base.data
        .define(DATA_IMMUNE_TO_ZOMBIFICATION, SyncedValue::Boolean(false));
    if base.kind == EntityKind::Piglin {
        base.data.define(DATA_BABY, SyncedValue::Boolean(false));
        base.data
            .define(DATA_CHARGING_CROSSBOW, SyncedValue::Boolean(false));
        base.data.define(DATA_DANCING, SyncedValue::Boolean(false));
    }
    PiglinState::default()
}

pub fn is_baby(base: &EntityBase) -> bool {
    base.kind == EntityKind::Piglin && base.data.boolean(DATA_BABY)
}

pub fn set_baby(base: &mut EntityBase, attributes: &mut LivingAttributes, baby: bool) {
    if base.kind != EntityKind::Piglin {
        return;
    }
    base.data.set(DATA_BABY, SyncedValue::Boolean(baby));
    crate::entity::ageable::refresh_dimensions(base, baby);
    attributes.remove_modifier(MOVEMENT_SPEED, BABY_SPEED_MODIFIER);
    if baby {
        attributes.add_modifier(
            MOVEMENT_SPEED,
            AttributeModifier {
                id: Cow::Borrowed(BABY_SPEED_MODIFIER),
                amount: BABY_SPEED_BONUS,
                operation: AttributeOperation::AddMultipliedBase,
            },
        );
    }
}

pub fn is_immune_to_zombification(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IMMUNE_TO_ZOMBIFICATION)
}

pub fn set_immune_to_zombification(base: &mut EntityBase, immune: bool) {
    base.data
        .set(DATA_IMMUNE_TO_ZOMBIFICATION, SyncedValue::Boolean(immune));
}

pub fn is_dancing(base: &EntityBase) -> bool {
    base.kind == EntityKind::Piglin && base.data.boolean(DATA_DANCING)
}

pub fn set_dancing(base: &mut EntityBase, dancing: bool) {
    if base.kind == EntityKind::Piglin {
        base.data.set(DATA_DANCING, SyncedValue::Boolean(dancing));
    }
}

pub fn is_charging_crossbow(base: &EntityBase) -> bool {
    base.kind == EntityKind::Piglin && base.data.boolean(DATA_CHARGING_CROSSBOW)
}

pub fn set_charging_crossbow(base: &mut EntityBase, charging: bool) {
    if base.kind == EntityKind::Piglin {
        base.data
            .set(DATA_CHARGING_CROSSBOW, SyncedValue::Boolean(charging));
    }
}

pub fn read(
    nbt: &Compound,
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    state: &mut PiglinState,
) {
    if base.kind == EntityKind::Piglin {
        set_baby(base, attributes, nbt.get_bool("IsBaby").unwrap_or(false));
        state.cannot_hunt = nbt.get_bool("CannotHunt").unwrap_or(false);
    }
    set_immune_to_zombification(
        base,
        nbt.get_bool("IsImmuneToZombification").unwrap_or(false),
    );
    state.time_in_overworld = nbt.get_i32("TimeInOverworld").unwrap_or(0);
}

pub fn write(base: &EntityBase, state: &PiglinState, out: &mut Compound) {
    if base.kind == EntityKind::Piglin {
        out.insert("IsBaby", i8::from(is_baby(base)));
        out.insert("CannotHunt", i8::from(state.cannot_hunt));
    }
    out.insert(
        "IsImmuneToZombification",
        i8::from(is_immune_to_zombification(base)),
    );
    out.insert("TimeInOverworld", state.time_in_overworld);
}

fn stack(kind: ItemKind) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::MIN)
}

fn create_spawn_weapon(own: &mut impl RandomSource) -> ItemStack {
    if own.next_float() < CROSSBOW_CHANCE {
        return stack(ItemKind::Crossbow);
    }
    if own.next_int(SPEAR_ODDS) == 0 {
        stack(ItemKind::GoldenSpear)
    } else {
        stack(ItemKind::GoldenSword)
    }
}

fn maybe_wear_armor(
    living: &mut LivingState,
    slot: EquipmentSlot,
    kind: ItemKind,
    level: &mut impl RandomSource,
) {
    if level.next_float() < ARMOR_CHANCE {
        set_item_slot(living, slot, Some(stack(kind)));
    }
}

pub fn finalize_piglin(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
    own: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if reason != SpawnReason::Structure {
        if level.next_float() < BABY_CHANCE {
            set_baby(base, &mut living.attributes, true);
        } else if !is_baby(base) {
            let weapon = create_spawn_weapon(own);
            set_item_slot(living, EquipmentSlot::Mainhand, Some(weapon));
        }
    }
    let cooldown =
        level.next_int_between(HUNT_COOLDOWN_SECONDS.0 * 20, HUNT_COOLDOWN_SECONDS.1 * 20);
    if let Some(crate::entity::species::Species::Piglin(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.pending_hunt_cooldown = Some(cooldown);
    }
    if !is_baby(base) {
        maybe_wear_armor(living, EquipmentSlot::Head, ItemKind::GoldenHelmet, level);
        maybe_wear_armor(
            living,
            EquipmentSlot::Chest,
            ItemKind::GoldenChestplate,
            level,
        );
        maybe_wear_armor(living, EquipmentSlot::Legs, ItemKind::GoldenLeggings, level);
        maybe_wear_armor(living, EquipmentSlot::Feet, ItemKind::GoldenBoots, level);
    }
    enchantment_rolls(living, &environment.difficulty, level);
    mob_finalize(base, living, group, level)
}

pub fn finalize_brute(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(crate::entity::species::Species::Piglin(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.needs_home = true;
    }
    set_item_slot(
        living,
        EquipmentSlot::Mainhand,
        Some(stack(ItemKind::GoldenAxe)),
    );
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::living::LivingState;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::spawn::finalize_spawn;
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};
    use crate::random::LegacyRandom;

    fn scatter(seed: i64) -> i64 {
        seed.wrapping_mul(0x5DEE_CE66_D1A4_F87D)
    }

    fn spawn(kind: EntityKind, seed: i64, reason: SpawnReason) -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let living = LivingState::new(&mut base, 0.0);
        let mut entity = Entity::new(base, EntityState::Living(Box::new(living)));
        let environment = SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(scatter(seed));
        let mut own = LegacyRandom::new(scatter(seed + 7));
        finalize_spawn(
            &mut entity,
            &environment,
            reason,
            None,
            &mut level,
            &mut own,
        );
        entity
    }

    fn state(entity: &Entity) -> PiglinState {
        match entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .map(|m| m.species)
        {
            Some(Species::Piglin(state)) => state,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn piglins_and_brutes_pick_up_loot_by_default() {
        for kind in [EntityKind::Piglin, EntityKind::PiglinBrute] {
            let entity = spawn(kind, 1, SpawnReason::Natural);
            assert!(
                entity
                    .living()
                    .unwrap()
                    .mob
                    .as_ref()
                    .unwrap()
                    .can_pick_up_loot
            );
        }
    }

    #[test]
    fn a_fifth_of_spawned_piglins_are_babies_and_the_rest_carry_a_weapon() {
        let mut babies = 0;
        let mut crossbows = 0_i32;
        let mut golden = 0_i32;
        for seed in 0..2000 {
            let piglin = spawn(EntityKind::Piglin, seed, SpawnReason::Natural);
            let living = piglin.living().unwrap();
            let weapon = living.equipment(EquipmentSlot::Mainhand).map(|s| s.item);
            if piglin.is_baby() {
                babies += 1;
                assert!(weapon.is_none());
                assert!(
                    living.equipment(EquipmentSlot::Head).is_none()
                        && living.equipment(EquipmentSlot::Feet).is_none()
                );
                assert!(piglin.base.dimensions.height < 1.0);
            } else if weapon == Some(ItemKind::Crossbow.id()) {
                crossbows += 1;
            } else {
                assert!(
                    weapon == Some(ItemKind::GoldenSword.id())
                        || weapon == Some(ItemKind::GoldenSpear.id())
                );
                golden += 1;
            }
        }
        assert!((330..=470).contains(&babies), "{babies}");
        assert!(crossbows.abs_diff(golden) < 160, "{crossbows} {golden}");
    }

    #[test]
    fn a_baby_piglin_runs_a_fifth_faster() {
        let mut found = false;
        for seed in 0..200 {
            let mut piglin = spawn(EntityKind::Piglin, seed, SpawnReason::Natural);
            if piglin.is_baby() {
                let living = piglin.living_mut().unwrap();
                let speed = living.attribute("minecraft:movement_speed");
                assert!((speed - 0.35 * 1.2).abs() < 1e-6, "{speed}");
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn structure_piglins_stay_adult_and_unarmed_until_equipped() {
        for seed in 0..100 {
            let piglin = spawn(EntityKind::Piglin, seed, SpawnReason::Structure);
            assert!(!piglin.is_baby());
            assert!(
                piglin
                    .living()
                    .unwrap()
                    .equipment(EquipmentSlot::Mainhand)
                    .is_none()
            );
        }
    }

    #[test]
    fn each_armor_piece_is_worn_one_time_in_ten() {
        let mut worn = [0; 4];
        let mut adults = 0;
        for seed in 0..4000 {
            let piglin = spawn(EntityKind::Piglin, seed, SpawnReason::Structure);
            adults += 1;
            let living = piglin.living().unwrap();
            for (index, slot) in [
                EquipmentSlot::Head,
                EquipmentSlot::Chest,
                EquipmentSlot::Legs,
                EquipmentSlot::Feet,
            ]
            .into_iter()
            .enumerate()
            {
                worn[index] += usize::from(living.equipment(slot).is_some());
            }
        }
        for count in worn {
            assert!((300..=500).contains(&count), "{count} of {adults}");
        }
    }

    #[test]
    fn a_new_piglin_waits_to_hunt_for_thirty_to_one_twenty_seconds() {
        for seed in 0..200 {
            let piglin = spawn(EntityKind::Piglin, seed, SpawnReason::Natural);
            let cooldown = state(&piglin).pending_hunt_cooldown.expect("cooldown");
            assert!((600..=2400).contains(&cooldown));
        }
    }

    #[test]
    fn a_brute_holds_a_golden_axe_and_wants_a_home() {
        let brute = spawn(EntityKind::PiglinBrute, 3, SpawnReason::Structure);
        let living = brute.living().unwrap();
        assert_eq!(
            living.equipment(EquipmentSlot::Mainhand).map(|s| s.item),
            Some(ItemKind::GoldenAxe.id())
        );
        assert!(state(&brute).needs_home);
        assert!(!brute.is_baby());
    }

    #[test]
    fn the_zombification_flags_and_timers_round_trip_through_nbt() {
        let mut piglin = spawn(EntityKind::Piglin, 5, SpawnReason::Natural);
        set_immune_to_zombification(&mut piglin.base, true);
        let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = &mut piglin
        else {
            panic!("living");
        };
        set_baby(base, &mut living.attributes, true);
        if let Some(Species::Piglin(state)) = piglin
            .living_mut()
            .and_then(|l| l.mob.as_mut())
            .map(|m| &mut m.species)
        {
            state.time_in_overworld = 123;
            state.cannot_hunt = true;
        }
        let saved = save_entity(&piglin, None).unwrap();
        assert_eq!(saved.get_bool("IsBaby"), Some(true));
        assert_eq!(saved.get_bool("CannotHunt"), Some(true));
        assert_eq!(saved.get_bool("IsImmuneToZombification"), Some(true));
        assert_eq!(saved.get_i32("TimeInOverworld"), Some(123));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(loaded.is_baby());
        assert!(is_immune_to_zombification(&loaded.base));
        let state = state(&loaded);
        assert_eq!((state.time_in_overworld, state.cannot_hunt), (123, true));
        assert_eq!(save_entity(&loaded, None), Some(saved));
    }

    #[test]
    fn a_piglin_without_a_pick_up_tag_still_picks_up_loot() {
        let piglin = spawn(EntityKind::Piglin, 9, SpawnReason::Natural);
        let mut saved = save_entity(&piglin, None).unwrap();
        saved.remove("CanPickUpLoot");
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(
            loaded
                .living()
                .unwrap()
                .mob
                .as_ref()
                .unwrap()
                .can_pick_up_loot
        );
    }

    #[test]
    fn the_brain_tag_and_the_inventory_survive_as_extras() {
        let piglin = spawn(EntityKind::Piglin, 4, SpawnReason::Natural);
        let mut saved = save_entity(&piglin, None).unwrap();
        saved.insert("Brain", Compound::new());
        saved.insert("Inventory", rspigot_nbt::Tag::List(Vec::new()));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(loaded.extra.contains_key("Brain"));
        assert!(loaded.extra.contains_key("Inventory"));
        let again = save_entity(&loaded, None).unwrap();
        assert!(again.contains_key("Brain") && again.contains_key("Inventory"));
    }
}
