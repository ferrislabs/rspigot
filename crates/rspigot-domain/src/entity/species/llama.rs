use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use crate::EntityId;
use crate::entity::ageable::{self, DEFAULT_BABY_CHANCE};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::species::equine;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_STRENGTH: u8 = 20;
pub const DATA_VARIANT: u8 = 21;
pub const MIN_STRENGTH: i32 = 1;
pub const MAX_STRENGTH: i32 = 5;
pub const MAX_TEMPER: i32 = 30;
pub const DEFAULT_DESPAWN_DELAY: i32 = 47_999;
pub const CARAVAN_SEARCH_XZ: f64 = 9.0;
pub const CARAVAN_SEARCH_Y: f64 = 4.0;
pub const SPIT_REQUIRED_PATH_LENGTH: f32 = 40.0;
pub const KEYS: &[&str] = &["Variant", "Strength", "DespawnDelay"];

const STRONG_ROLL_CHANCE: f32 = 0.04;
const STRONG_ROLL: i32 = 5;
const WEAK_ROLL: i32 = 3;
const BONUS_STRENGTH_CHANCE: f32 = 0.03;
const VARIANT_COUNT: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Creamy,
    White,
    Brown,
    Gray,
}

impl Variant {
    pub const ALL: [Self; 4] = [Self::Creamy, Self::White, Self::Brown, Self::Gray];

    pub const fn id(self) -> i32 {
        match self {
            Self::Creamy => 0,
            Self::White => 1,
            Self::Brown => 2,
            Self::Gray => 3,
        }
    }

    pub fn by_id(id: i32) -> Self {
        Self::ALL[id.clamp(0, VARIANT_COUNT - 1) as usize]
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Creamy => "creamy",
            Self::White => "white",
            Self::Brown => "brown",
            Self::Gray => "gray",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlamaState {
    pub did_spit: bool,
    pub caravan_head: Option<EntityId>,
    pub caravan_tail: Option<EntityId>,
    pub despawn_delay: i32,
}

pub fn is_llama(kind: EntityKind) -> bool {
    kind.extends("Llama")
}

pub fn is_trader(kind: EntityKind) -> bool {
    kind == EntityKind::TraderLlama
}

pub fn define(base: &mut EntityBase) -> LlamaState {
    base.data.define(DATA_STRENGTH, SyncedValue::Int(0));
    base.data.define(DATA_VARIANT, SyncedValue::Int(0));
    LlamaState {
        did_spit: false,
        caravan_head: None,
        caravan_tail: None,
        despawn_delay: DEFAULT_DESPAWN_DELAY,
    }
}

impl LlamaState {
    pub fn in_caravan(&self) -> bool {
        self.caravan_head.is_some()
    }

    pub fn has_caravan_tail(&self) -> bool {
        self.caravan_tail.is_some()
    }
}

pub fn strength(base: &EntityBase) -> i32 {
    base.data.int(DATA_STRENGTH)
}

pub fn set_strength(base: &mut EntityBase, value: i32) {
    base.data.set(
        DATA_STRENGTH,
        SyncedValue::Int(value.clamp(MIN_STRENGTH, MAX_STRENGTH)),
    );
}

pub fn set_random_strength(base: &mut EntityBase, random: &mut impl RandomSource) {
    let bound = if random.next_float() < STRONG_ROLL_CHANCE {
        STRONG_ROLL
    } else {
        WEAK_ROLL
    };
    let roll = random.next_int(bound);
    set_strength(base, roll + 1);
}

pub fn variant(base: &EntityBase) -> Variant {
    Variant::by_id(base.data.int(DATA_VARIANT))
}

pub fn set_variant(base: &mut EntityBase, value: Variant) {
    base.data.set(DATA_VARIANT, SyncedValue::Int(value.id()));
}

pub fn random_variant(random: &mut impl RandomSource) -> Variant {
    Variant::ALL[random.next_int(VARIANT_COUNT) as usize]
}

pub fn inventory_columns(base: &EntityBase) -> i32 {
    if equine::has_chest(base) {
        strength(base)
    } else {
        0
    }
}

pub fn offspring_strength(own: i32, other: i32, random: &mut impl RandomSource) -> i32 {
    let mut value = random.next_int(own.max(other)) + 1;
    if random.next_float() < BONUS_STRENGTH_CHANCE {
        value += 1;
    }
    value
}

pub fn offspring_variant(own: Variant, other: Variant, random: &mut impl RandomSource) -> Variant {
    if random.next_boolean() { own } else { other }
}

pub fn read_strength(nbt: &Compound, base: &mut EntityBase) {
    set_strength(base, nbt.get_i32("Strength").unwrap_or(0));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut LlamaState) {
    read_strength(nbt, base);
    let variant = nbt
        .get_i32("Variant")
        .map_or(Variant::Creamy, Variant::by_id);
    set_variant(base, variant);
    if is_trader(base.kind) {
        state.despawn_delay = nbt.get_i32("DespawnDelay").unwrap_or(DEFAULT_DESPAWN_DELAY);
    }
}

pub fn write(base: &EntityBase, state: &LlamaState, out: &mut Compound) {
    out.insert("Variant", variant(base).id());
    out.insert("Strength", strength(base));
    if is_trader(base.kind) {
        out.insert("DespawnDelay", state.despawn_delay);
    }
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut group = group;
    if is_trader(base.kind) {
        if reason == SpawnReason::Event
            && let Some(age) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::set_age(base, age, 0);
        }
        if group.is_none() {
            group = Some(SpawnGroupData::ageable(false));
        }
    }
    set_random_strength(base, level);
    let (chosen, size) = match group {
        Some(SpawnGroupData::Llama {
            variant,
            group_size,
        }) => (variant, group_size),
        _ => (random_variant(level), 0),
    };
    set_variant(base, chosen);
    let inherited = SpawnGroupData::Ageable {
        group_size: size,
        should_spawn_baby: true,
        baby_chance: DEFAULT_BABY_CHANCE,
    };
    let result = equine::finalize(base, living, reason, Some(inherited), level);
    match result {
        Some(SpawnGroupData::Ageable { group_size, .. }) => Some(SpawnGroupData::Llama {
            variant: chosen,
            group_size,
        }),
        other => other,
    }
}

pub fn can_mate(kind: EntityKind, partner: EntityKind) -> bool {
    is_llama(kind) && is_llama(partner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::living::LivingState;
    use crate::random::LegacyRandom;
    use uuid::Uuid;

    fn base(kind: EntityKind) -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), kind)
    }

    fn llama(kind: EntityKind) -> (EntityBase, LivingState) {
        let mut base = base(kind);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn the_variant_ids_follow_the_vanilla_order_and_clamp() {
        assert_eq!(Variant::ALL.map(Variant::id), [0, 1, 2, 3]);
        assert_eq!(Variant::by_id(-4), Variant::Creamy);
        assert_eq!(Variant::by_id(9), Variant::Gray);
        assert_eq!(Variant::White.name(), "white");
    }

    #[test]
    fn the_strength_and_variant_sit_after_the_chest_flag() {
        let (base, _) = llama(EntityKind::Llama);
        assert_eq!(base.data.int(DATA_STRENGTH), 0);
        assert_eq!(DATA_STRENGTH, equine::DATA_CHEST + 1);
        assert_eq!(DATA_VARIANT, DATA_STRENGTH + 1);
    }

    #[test]
    fn the_strength_is_clamped_between_one_and_five() {
        let (mut base, _) = llama(EntityKind::Llama);
        set_strength(&mut base, 0);
        assert_eq!(strength(&base), 1);
        set_strength(&mut base, 9);
        assert_eq!(strength(&base), 5);
        set_strength(&mut base, 3);
        assert_eq!(strength(&base), 3);
    }

    #[test]
    fn a_random_strength_rolls_the_float_first_then_the_int() {
        let mut mirror = LegacyRandom::new(8);
        let mut random = LegacyRandom::new(8);
        let (mut base, _) = llama(EntityKind::Llama);
        let strong = mirror.next_float() < 0.04;
        let roll = mirror.next_int(if strong { 5 } else { 3 });
        set_random_strength(&mut base, &mut random);
        assert_eq!(strength(&base), roll + 1);
    }

    #[test]
    fn the_inventory_has_one_column_per_strength_point_once_chested() {
        let (mut base, _) = llama(EntityKind::Llama);
        set_strength(&mut base, 4);
        assert_eq!(inventory_columns(&base), 0);
        equine::set_chest(&mut base, true);
        assert_eq!(inventory_columns(&base), 4);
    }

    #[test]
    fn the_state_round_trips_in_the_vanilla_layout() {
        let (mut base, _) = llama(EntityKind::TraderLlama);
        let mut state = define(&mut base);
        set_strength(&mut base, 4);
        set_variant(&mut base, Variant::Gray);
        state.despawn_delay = 1200;
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i32("Variant"), Some(3));
        assert_eq!(out.get_i32("Strength"), Some(4));
        assert_eq!(out.get_i32("DespawnDelay"), Some(1200));
        let (mut again_base, _) = llama(EntityKind::TraderLlama);
        let mut again = define(&mut again_base);
        read(&out, &mut again_base, &mut again);
        assert_eq!(strength(&again_base), 4);
        assert_eq!(variant(&again_base), Variant::Gray);
        assert_eq!(again.despawn_delay, 1200);
    }

    #[test]
    fn a_plain_llama_does_not_write_the_despawn_delay_and_reads_zero_strength_as_one() {
        let (mut base, _) = llama(EntityKind::Llama);
        let mut state = define(&mut base);
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert!(!out.contains_key("DespawnDelay"));
        let empty = Compound::new();
        read(&empty, &mut base, &mut state);
        assert_eq!(strength(&base), 1);
        assert_eq!(variant(&base), Variant::Creamy);
        assert_eq!(state.despawn_delay, DEFAULT_DESPAWN_DELAY);
    }

    #[test]
    fn offspring_strength_rolls_up_to_the_stronger_parent_plus_a_rare_bonus() {
        let mut random = LegacyRandom::new(5);
        for _ in 0..200 {
            let value = offspring_strength(2, 4, &mut random);
            assert!((1..=5).contains(&value));
        }
        let mut mirror = LegacyRandom::new(5);
        let mut again = LegacyRandom::new(5);
        let mut expected = mirror.next_int(4) + 1;
        if mirror.next_float() < 0.03 {
            expected += 1;
        }
        assert_eq!(offspring_strength(2, 4, &mut again), expected);
    }

    #[test]
    fn a_new_llama_rolls_its_strength_then_picks_the_group_variant() {
        let (mut first_base, mut first) = llama(EntityKind::Llama);
        let mut level = LegacyRandom::new(21);
        let group = finalize(
            &mut first_base,
            &mut first,
            SpawnReason::Natural,
            None,
            &mut level,
        );
        let Some(SpawnGroupData::Llama {
            variant: chosen,
            group_size,
        }) = group
        else {
            panic!("a llama group is expected");
        };
        assert_eq!(group_size, 1);
        assert_eq!(variant(&first_base), chosen);
        assert!((1..=5).contains(&strength(&first_base)));
        let (mut second_base, mut second) = llama(EntityKind::Llama);
        let again = finalize(
            &mut second_base,
            &mut second,
            SpawnReason::Natural,
            group,
            &mut level,
        );
        assert_eq!(variant(&second_base), chosen);
        assert!(matches!(
            again,
            Some(SpawnGroupData::Llama { group_size: 2, .. })
        ));
    }

    #[test]
    fn a_trader_llama_spawned_by_an_event_is_an_adult() {
        let (mut base, mut living) = llama(EntityKind::TraderLlama);
        if let Some(age) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
            ageable::set_age(&mut base, age, -24_000);
        }
        let mut level = LegacyRandom::new(2);
        finalize(&mut base, &mut living, SpawnReason::Event, None, &mut level);
        let age = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .map(|age| age.age);
        assert_eq!(age, Some(0));
    }

    #[test]
    fn only_llamas_mate_with_llamas() {
        assert!(can_mate(EntityKind::Llama, EntityKind::TraderLlama));
        assert!(!can_mate(EntityKind::Llama, EntityKind::Horse));
        assert_eq!(MAX_TEMPER, 30);
    }
}
