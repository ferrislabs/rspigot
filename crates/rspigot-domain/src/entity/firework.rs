use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::item::ItemStack;
use crate::random::{LegacyRandom, RandomSource};
use crate::{EntityId, Vec3};

pub const DATA_ITEM: u8 = 8;
pub const DATA_ATTACHED: u8 = 9;
pub const DATA_SHOT_AT_ANGLE: u8 = 10;
pub const FIREWORKS: &str = "minecraft:fireworks";
pub const LAUNCH_SOUND: &str = "minecraft:entity.firework_rocket.launch";
pub const LAUNCH_VOLUME: f32 = 3.0;
pub const EXPLODE_STATUS: i8 = 17;
pub const RISE_SPEED: f64 = 0.05;
pub const SPREAD: f64 = 0.002_297;
pub const ACCELERATION: f64 = 1.15;
pub const LIFT: f64 = 0.04;
pub const BLAST_RADIUS_SQR: f64 = 25.0;
pub const BASE_DAMAGE: f32 = 5.0;
pub const DAMAGE_PER_EXPLOSION: f32 = 2.0;
pub const BLAST_RANGE: f64 = 5.0;
pub const DISPENSE_POWER: f32 = 0.5;
pub const DISPENSE_UNCERTAINTY: f32 = 1.0;
pub const FACE_OFFSET: f64 = 0.15;

#[derive(Debug, Clone, PartialEq)]
pub struct FireworkState {
    pub owner: Option<Uuid>,
    pub life: i32,
    pub lifetime: i32,
    pub attached: Option<EntityId>,
    pub left_owner: bool,
    pub random: LegacyRandom,
}

pub fn default_item() -> ItemStack {
    ItemStack::new(ItemKind::FireworkRocket.id(), NonZeroU8::MIN)
}

pub fn flight_duration(stack: &ItemStack) -> i32 {
    stack
        .components
        .get_compound(FIREWORKS)
        .and_then(|fireworks| fireworks.get_i32("flight_duration"))
        .unwrap_or(0)
}

pub fn explosion_count(stack: &ItemStack) -> usize {
    stack
        .components
        .get_compound(FIREWORKS)
        .and_then(|fireworks| fireworks.get_list("explosions"))
        .map_or(0, <[_]>::len)
}

pub fn item(base: &EntityBase) -> ItemStack {
    match base.data.get(DATA_ITEM) {
        Some(SyncedValue::Item(Some(stack))) => stack.clone(),
        _ => default_item(),
    }
}

pub fn set_item(base: &mut EntityBase, stack: &ItemStack) {
    base.data
        .set(DATA_ITEM, SyncedValue::Item(Some(stack.clone())));
}

pub fn shot_at_angle(base: &EntityBase) -> bool {
    matches!(
        base.data.get(DATA_SHOT_AT_ANGLE),
        Some(SyncedValue::Boolean(true))
    )
}

pub fn set_shot_at_angle(base: &mut EntityBase, angled: bool) {
    base.data
        .set(DATA_SHOT_AT_ANGLE, SyncedValue::Boolean(angled));
}

pub fn set_attached(base: &mut EntityBase, state: &mut FireworkState, target: Option<EntityId>) {
    state.attached = target;
    let value = target.and_then(|id| u32::try_from(id.0).ok());
    base.data
        .set(DATA_ATTACHED, SyncedValue::OptionalUnsignedInt(value));
}

impl FireworkState {
    pub fn new(base: &mut EntityBase, seed: i64, stack: &ItemStack) -> Self {
        base.data
            .define(DATA_ITEM, SyncedValue::Item(Some(stack.clone())));
        base.data
            .define(DATA_ATTACHED, SyncedValue::OptionalUnsignedInt(None));
        base.data
            .define(DATA_SHOT_AT_ANGLE, SyncedValue::Boolean(false));
        let mut random = LegacyRandom::new(seed);
        let sideways = random.triangle(0.0, SPREAD);
        let forward = random.triangle(0.0, SPREAD);
        base.velocity = Vec3::new(sideways, RISE_SPEED, forward);
        let lifetime = (1 + flight_duration(stack)) * 10 + random.next_int(6) + random.next_int(7);
        Self {
            owner: None,
            life: 0,
            lifetime,
            attached: None,
            left_owner: false,
            random,
        }
    }

    pub fn blank(base: &mut EntityBase, seed: i64) -> Self {
        Self::new(base, seed, &default_item())
    }
}

pub fn blast_damage(count: usize) -> f32 {
    count as f32 * DAMAGE_PER_EXPLOSION + BASE_DAMAGE
}

pub fn damage_at(base: f32, distance: f64) -> f32 {
    base * ((BLAST_RANGE - distance) / BLAST_RANGE).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use rspigot_nbt::{Compound, Tag};

    use super::*;

    fn rocket(stack: &ItemStack) -> (EntityBase, FireworkState) {
        let mut base = EntityBase::new(EntityId(3), Uuid::from_u128(3), EntityKind::FireworkRocket);
        let state = FireworkState::new(&mut base, 11, stack);
        (base, state)
    }

    #[test]
    fn a_rocket_rises_and_lives_ten_ticks_per_flight_level_plus_a_roll() {
        let (base, state) = rocket(&default_item());
        assert_eq!(base.velocity.y, RISE_SPEED);
        assert!((10..=10 + 5 + 6).contains(&state.lifetime));
        let mut long = default_item();
        long.components.insert(
            FIREWORKS,
            Compound::new()
                .with("flight_duration", 3i8)
                .with("explosions", Tag::List(Vec::new())),
        );
        let (_, state) = rocket(&long);
        assert!((40..=40 + 5 + 6).contains(&state.lifetime));
    }

    #[test]
    fn the_blast_hurts_less_with_distance() {
        let base = blast_damage(2);
        assert_eq!(base, 9.0);
        assert_eq!(damage_at(base, 0.0), base);
        assert!(damage_at(base, 4.0) < damage_at(base, 1.0));
        assert_eq!(damage_at(base, 5.0), 0.0);
    }

    #[test]
    fn attaching_publishes_the_target_id() {
        let (mut base, mut state) = rocket(&default_item());
        set_attached(&mut base, &mut state, Some(EntityId(7)));
        assert_eq!(
            base.data.get(DATA_ATTACHED),
            Some(&SyncedValue::OptionalUnsignedInt(Some(7)))
        );
        assert_eq!(state.attached, Some(EntityId(7)));
    }
}
