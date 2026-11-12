use std::num::NonZeroU8;

use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::BiomeId;
use crate::entity::ageable::{self, BABY_START_AGE};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::save::{read_uuid, uuid_tag};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize, set_item_slot};
use crate::item::ItemStack;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_TYPE: u8 = 18;
pub const DATA_FLAGS: u8 = 19;
pub const DATA_TRUSTED_0: u8 = 20;
pub const DATA_TRUSTED_1: u8 = 21;
pub const FLAG_SITTING: u8 = 1;
pub const FLAG_CROUCHING: u8 = 4;
pub const FLAG_INTERESTED: u8 = 8;
pub const FLAG_POUNCING: u8 = 16;
pub const FLAG_SLEEPING: u8 = 32;
pub const FLAG_FACEPLANTED: u8 = 64;
pub const FLAG_DEFENDING: u8 = 128;
pub const TICKS_BEFORE_EAT: i32 = 600;
pub const MAX_CROUCH_AMOUNT: f32 = 5.0;
pub const SNOW_FOXES_BIOMES: &str = "minecraft:spawns_snow_foxes";
const CROUCH_STEP: f32 = 0.2;
const EQUIPMENT_CHANCE: f32 = 0.2;
const EMERALD_LIMIT: f32 = 0.05;
const EGG_LIMIT: f32 = 0.2;
const RABBIT_LIMIT: f32 = 0.4;
const WHEAT_LIMIT: f32 = 0.6;
const LEATHER_LIMIT: f32 = 0.8;
const GROUP_BABY_SIZE: i32 = 2;

pub const KEYS: &[&str] = &["Trusted", "Sleeping", "Type", "Sitting", "Crouching"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Red,
    Snow,
}

impl Variant {
    pub const fn id(self) -> u32 {
        match self {
            Self::Red => 0,
            Self::Snow => 1,
        }
    }

    pub const fn from_id(id: u32) -> Self {
        match id {
            1 => Self::Snow,
            _ => Self::Red,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Snow => "snow",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "red" => Some(Self::Red),
            "snow" => Some(Self::Snow),
            _ => None,
        }
    }

    pub fn by_biome(biome: BiomeId) -> Self {
        if crate::biome::in_tag(biome, SNOW_FOXES_BIOMES) {
            Self::Snow
        } else {
            Self::Red
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FoxState {
    pub ticks_since_eaten: i32,
    pub crouch_amount: f32,
    pub crouch_amount_old: f32,
    pub hunts: bool,
}

impl Eq for FoxState {}

impl FoxState {
    pub fn is_fully_crouched(&self) -> bool {
        self.crouch_amount == MAX_CROUCH_AMOUNT
    }

    pub fn tick_crouch(&mut self, crouching: bool) {
        self.crouch_amount_old = self.crouch_amount;
        if crouching {
            self.crouch_amount += CROUCH_STEP;
            if self.crouch_amount > MAX_CROUCH_AMOUNT {
                self.crouch_amount = MAX_CROUCH_AMOUNT;
            }
        } else {
            self.crouch_amount = 0.0;
        }
    }

    pub fn reset_crouch(&mut self) {
        self.crouch_amount = 0.0;
        self.crouch_amount_old = 0.0;
    }

    pub fn can_hold(&self, held: Option<&ItemStack>, stack: &ItemStack) -> bool {
        match held {
            None => true,
            Some(held) => {
                self.ticks_since_eaten > 0 && is_consumable_food(stack) && !is_consumable_food(held)
            }
        }
    }
}

pub fn is_consumable_food(stack: &ItemStack) -> bool {
    stack.item.kind().and_then(ItemKind::food).is_some()
}

pub fn define(base: &mut EntityBase) -> FoxState {
    base.data.define(DATA_TYPE, SyncedValue::Int(0));
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    base.data
        .define(DATA_TRUSTED_0, SyncedValue::OptionalEntity(None));
    base.data
        .define(DATA_TRUSTED_1, SyncedValue::OptionalEntity(None));
    FoxState::default()
}

pub fn variant(base: &EntityBase) -> Variant {
    Variant::from_id(base.data.int(DATA_TYPE) as u32)
}

pub fn set_variant(base: &mut EntityBase, variant: Variant) {
    base.data
        .set(DATA_TYPE, SyncedValue::Int(variant.id() as i32));
}

pub fn flags(base: &EntityBase) -> u8 {
    base.data.byte(DATA_FLAGS) as u8
}

pub fn set_flags(base: &mut EntityBase, flags: u8) {
    base.data.set(DATA_FLAGS, SyncedValue::Byte(flags as i8));
}

fn flag(base: &EntityBase, mask: u8) -> bool {
    base.data.flag(DATA_FLAGS, mask)
}

fn set_flag(base: &mut EntityBase, mask: u8, value: bool) {
    base.data.set_flag(DATA_FLAGS, mask, value);
}

pub fn is_sitting(base: &EntityBase) -> bool {
    flag(base, FLAG_SITTING)
}

pub fn set_sitting(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_SITTING, value);
}

pub fn is_crouching(base: &EntityBase) -> bool {
    flag(base, FLAG_CROUCHING)
}

pub fn set_crouching(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_CROUCHING, value);
}

pub fn is_interested(base: &EntityBase) -> bool {
    flag(base, FLAG_INTERESTED)
}

pub fn set_interested(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_INTERESTED, value);
}

pub fn is_pouncing(base: &EntityBase) -> bool {
    flag(base, FLAG_POUNCING)
}

pub fn set_pouncing(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_POUNCING, value);
}

pub fn is_sleeping(base: &EntityBase) -> bool {
    flag(base, FLAG_SLEEPING)
}

pub fn set_sleeping(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_SLEEPING, value);
}

pub fn is_faceplanted(base: &EntityBase) -> bool {
    flag(base, FLAG_FACEPLANTED)
}

pub fn set_faceplanted(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_FACEPLANTED, value);
}

pub fn is_defending(base: &EntityBase) -> bool {
    flag(base, FLAG_DEFENDING)
}

pub fn set_defending(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_DEFENDING, value);
}

pub fn can_move(base: &EntityBase) -> bool {
    !is_sleeping(base) && !is_sitting(base) && !is_faceplanted(base)
}

pub fn clear_states(base: &mut EntityBase) {
    set_interested(base, false);
    set_crouching(base, false);
    set_sitting(base, false);
    set_sleeping(base, false);
    set_defending(base, false);
    set_faceplanted(base, false);
}

pub fn trusted(base: &EntityBase) -> [Option<Uuid>; 2] {
    [
        base.data.entity(DATA_TRUSTED_0),
        base.data.entity(DATA_TRUSTED_1),
    ]
}

pub fn trusts(base: &EntityBase, uuid: Uuid) -> bool {
    trusted(base).contains(&Some(uuid))
}

pub fn add_trusted(base: &mut EntityBase, uuid: Uuid) {
    let slot = if base.data.entity(DATA_TRUSTED_0).is_some() {
        DATA_TRUSTED_1
    } else {
        DATA_TRUSTED_0
    };
    base.data.set(slot, SyncedValue::OptionalEntity(Some(uuid)));
}

pub fn clear_trusted(base: &mut EntityBase) {
    base.data
        .set(DATA_TRUSTED_0, SyncedValue::OptionalEntity(None));
    base.data
        .set(DATA_TRUSTED_1, SyncedValue::OptionalEntity(None));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut FoxState) {
    clear_trusted(base);
    if let Some(list) = nbt.get_list("Trusted") {
        for uuid in list.iter().filter_map(read_uuid) {
            add_trusted(base, uuid);
        }
    }
    set_sleeping(base, nbt.get_bool("Sleeping").unwrap_or(false));
    let variant = nbt
        .get_str("Type")
        .and_then(Variant::from_name)
        .unwrap_or_default();
    set_variant(base, variant);
    set_sitting(base, nbt.get_bool("Sitting").unwrap_or(false));
    set_crouching(base, nbt.get_bool("Crouching").unwrap_or(false));
    state.hunts = true;
}

pub fn write(base: &EntityBase, _state: &FoxState, out: &mut Compound) {
    let trusted: Vec<Tag> = trusted(base).into_iter().flatten().map(uuid_tag).collect();
    out.insert("Trusted", Tag::List(trusted));
    out.insert("Sleeping", i8::from(is_sleeping(base)));
    out.insert("Type", variant(base).name());
    out.insert("Sitting", i8::from(is_sitting(base)));
    out.insert("Crouching", i8::from(is_crouching(base)));
}

pub fn default_equipment(level: &mut impl RandomSource) -> Option<ItemKind> {
    if level.next_float() >= EQUIPMENT_CHANCE {
        return None;
    }
    let roll = level.next_float();
    Some(if roll < EMERALD_LIMIT {
        ItemKind::Emerald
    } else if roll < EGG_LIMIT {
        ItemKind::Egg
    } else if roll < RABBIT_LIMIT {
        if level.next_boolean() {
            ItemKind::RabbitFoot
        } else {
            ItemKind::RabbitHide
        }
    } else if roll < WHEAT_LIMIT {
        ItemKind::Wheat
    } else if roll < LEATHER_LIMIT {
        ItemKind::Leather
    } else {
        ItemKind::Feather
    })
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut chosen = Variant::by_biome(environment.biome);
    let mut group_size = 0;
    let mut baby = false;
    if let Some(SpawnGroupData::Fox {
        variant,
        group_size: size,
    }) = group
    {
        chosen = variant;
        group_size = size;
        baby = size >= GROUP_BABY_SIZE;
    }
    set_variant(base, chosen);
    if baby && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
        ageable::set_age(base, ageable, BABY_START_AGE);
    }
    if let Some(mob) = living.mob.as_mut()
        && let super::Species::Fox(state) = &mut mob.species
    {
        state.hunts = true;
    }
    if let Some(kind) = default_equipment(level) {
        let stack = ItemStack::new(kind.id(), NonZeroU8::MIN);
        set_item_slot(living, EquipmentSlot::Mainhand, Some(stack));
    }
    let inner = Some(SpawnGroupData::Ageable {
        group_size,
        should_spawn_baby: false,
        baby_chance: ageable::DEFAULT_BABY_CHANCE,
    });
    let finished = ageable_finalize(base, living, inner, level);
    let size = match finished {
        Some(SpawnGroupData::Ageable { group_size, .. }) => group_size,
        _ => group_size + 1,
    };
    Some(SpawnGroupData::Fox {
        variant: chosen,
        group_size: size,
    })
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;
    use crate::entity::species::Species;
    use crate::random::LegacyRandom;

    fn fox() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Fox);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn state(living: &LivingState) -> FoxState {
        match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Fox(state)) => state,
            other => panic!("not a fox: {other:?}"),
        }
    }

    #[test]
    fn synced_data_sits_at_the_vanilla_indexes() {
        let (mut base, _) = fox();
        set_variant(&mut base, Variant::Snow);
        set_crouching(&mut base, true);
        add_trusted(&mut base, Uuid::from_u128(7));
        assert_eq!(base.data.int(18), 1);
        assert_eq!(base.data.byte(19), 4);
        assert_eq!(base.data.entity(20), Some(Uuid::from_u128(7)));
        assert_eq!(base.data.entity(21), None);
    }

    #[test]
    fn each_flag_owns_its_bit() {
        let (mut base, _) = fox();
        set_sitting(&mut base, true);
        set_crouching(&mut base, true);
        set_interested(&mut base, true);
        set_pouncing(&mut base, true);
        set_sleeping(&mut base, true);
        set_faceplanted(&mut base, true);
        set_defending(&mut base, true);
        assert_eq!(flags(&base), 253);
        set_pouncing(&mut base, false);
        assert_eq!(flags(&base), 253 - 16);
        assert!(!can_move(&base));
        clear_states(&mut base);
        assert_eq!(flags(&base), 0);
        assert!(can_move(&base));
    }

    #[test]
    fn the_third_trusted_entity_replaces_the_second() {
        let (mut base, _) = fox();
        add_trusted(&mut base, Uuid::from_u128(1));
        add_trusted(&mut base, Uuid::from_u128(2));
        add_trusted(&mut base, Uuid::from_u128(3));
        assert_eq!(
            trusted(&base),
            [Some(Uuid::from_u128(1)), Some(Uuid::from_u128(3))]
        );
        assert!(trusts(&base, Uuid::from_u128(3)));
        assert!(!trusts(&base, Uuid::from_u128(2)));
        clear_trusted(&mut base);
        assert_eq!(trusted(&base), [None, None]);
    }

    #[test]
    fn nbt_round_trips_every_field() {
        let (mut base, mut living) = fox();
        set_variant(&mut base, Variant::Snow);
        set_sleeping(&mut base, true);
        set_sitting(&mut base, true);
        set_crouching(&mut base, true);
        add_trusted(&mut base, Uuid::from_u128(11));
        add_trusted(&mut base, Uuid::from_u128(12));
        let mut out = Compound::new();
        write(&base, &state(&living), &mut out);
        assert_eq!(out.get_str("Type"), Some("snow"));
        assert_eq!(out.get_list("Trusted").map(<[Tag]>::len), Some(2));
        let (mut other, mut other_living) = fox();
        let mut other_state = state(&other_living);
        read(&out, &mut other, &mut other_state);
        assert_eq!(variant(&other), Variant::Snow);
        assert!(is_sleeping(&other) && is_sitting(&other) && is_crouching(&other));
        assert_eq!(
            trusted(&other),
            [Some(Uuid::from_u128(11)), Some(Uuid::from_u128(12))]
        );
        assert!(other_state.hunts);
        let _ = (&mut living, &mut other_living);
    }

    #[test]
    fn missing_fields_read_as_defaults() {
        let (mut base, living) = fox();
        set_variant(&mut base, Variant::Snow);
        set_sleeping(&mut base, true);
        let mut fresh = state(&living);
        read(&Compound::new(), &mut base, &mut fresh);
        assert_eq!(variant(&base), Variant::Red);
        assert!(!is_sleeping(&base));
        assert_eq!(trusted(&base), [None, None]);
    }

    #[test]
    fn unknown_variants_fall_back_to_red() {
        assert_eq!(Variant::from_id(9), Variant::Red);
        assert_eq!(Variant::from_name("arctic"), None);
        assert_eq!(Variant::Snow.name(), "snow");
    }

    #[test]
    fn crouching_ramps_up_to_the_cap_and_resets() {
        let mut state = FoxState::default();
        for _ in 0..25 {
            state.tick_crouch(true);
        }
        assert!(!state.is_fully_crouched());
        state.tick_crouch(true);
        assert!(state.is_fully_crouched());
        assert!(state.crouch_amount_old < MAX_CROUCH_AMOUNT);
        state.tick_crouch(false);
        assert_eq!(state.crouch_amount, 0.0);
    }

    #[test]
    fn a_fox_holds_food_only_over_a_non_food_item() {
        let apple = ItemStack::new(ItemKind::Apple.id(), NonZeroU8::MIN);
        let wheat = ItemStack::new(ItemKind::Wheat.id(), NonZeroU8::MIN);
        let mut state = FoxState::default();
        assert!(state.can_hold(None, &wheat));
        assert!(!state.can_hold(Some(&wheat), &apple));
        state.ticks_since_eaten = 1;
        assert!(state.can_hold(Some(&wheat), &apple));
        assert!(!state.can_hold(Some(&apple), &apple));
        assert!(!state.can_hold(Some(&wheat), &wheat));
    }

    #[test]
    fn default_equipment_follows_the_roll_table() {
        struct Rolls(Vec<f32>);
        impl RandomSource for Rolls {
            fn next_float(&mut self) -> f32 {
                self.0.remove(0)
            }
            fn next_int(&mut self, _: i32) -> i32 {
                0
            }
            fn next_double(&mut self) -> f64 {
                0.0
            }
            fn next_boolean(&mut self) -> bool {
                true
            }
            fn next_gaussian(&mut self) -> f64 {
                0.0
            }
        }
        assert_eq!(default_equipment(&mut Rolls(vec![0.3])), None);
        let pick = |roll: f32| default_equipment(&mut Rolls(vec![0.1, roll]));
        assert_eq!(pick(0.01), Some(ItemKind::Emerald));
        assert_eq!(pick(0.1), Some(ItemKind::Egg));
        assert_eq!(pick(0.3), Some(ItemKind::RabbitFoot));
        assert_eq!(pick(0.5), Some(ItemKind::Wheat));
        assert_eq!(pick(0.7), Some(ItemKind::Leather));
        assert_eq!(pick(0.9), Some(ItemKind::Feather));
    }

    #[test]
    fn the_third_fox_of_a_group_is_a_baby_and_shares_the_variant() {
        let environment = SpawnEnvironment {
            biome: BiomeId(rspigot_data::registries::biome_id("minecraft:snowy_taiga").unwrap()),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(5);
        let mut group = None;
        let mut babies = Vec::new();
        for _ in 0..3 {
            let (mut base, mut living) = fox();
            group = finalize(&mut base, &mut living, &environment, group, &mut level);
            assert_eq!(variant(&base), Variant::Snow);
            assert!(state(&living).hunts);
            babies.push(living.mob.as_ref().unwrap().ageable.as_ref().unwrap().age < 0);
        }
        assert_eq!(babies, vec![false, false, true]);
        assert_eq!(
            group,
            Some(SpawnGroupData::Fox {
                variant: Variant::Snow,
                group_size: 3
            })
        );
    }

    #[test]
    fn a_plains_fox_is_red() {
        let environment = SpawnEnvironment {
            biome: BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap()),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let (mut base, mut living) = fox();
        let mut level = LegacyRandom::new(1);
        finalize(&mut base, &mut living, &environment, None, &mut level);
        assert_eq!(variant(&base), Variant::Red);
    }
}
