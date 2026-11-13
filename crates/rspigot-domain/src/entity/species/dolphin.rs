use rspigot_nbt::Compound;

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_GOT_FISH: u8 = 18;
pub const DATA_MOISTNESS: u8 = 19;
pub const MAX_MOISTNESS: i32 = 2400;
pub const BABY_CHANCE: f32 = 0.1;
pub const TREASURE_EVENT: i8 = 38;
pub const TREASURE_SEARCH_RADIUS: i32 = 50;
pub const MAX_HEAD_ROT: i32 = 1;
pub const GRACE_TICKS: i32 = 100;
pub const KEYS: &[&str] = &["GotFish", "Moistness"];
pub const SWIM_SPEED_PUSH_FACTOR: f64 = 0.9;
pub const DRY_FLOP_SPREAD: f32 = 0.2;
pub const DRY_FLOP_VERTICAL: f64 = 0.5;
pub const ITEM_PICKUP_DELAY: i32 = 40;
pub const BABY_AGE_SCALE: f32 = 0.65;
pub const FEEDING_SOUND: &str = "minecraft:entity.dolphin.eat";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinState {
    pub treasure_pos: Option<BlockPos>,
}

pub fn define(base: &mut EntityBase) -> DolphinState {
    base.data.define(DATA_GOT_FISH, SyncedValue::Boolean(false));
    base.data
        .define(DATA_MOISTNESS, SyncedValue::Int(MAX_MOISTNESS));
    DolphinState::default()
}

pub fn got_fish(base: &EntityBase) -> bool {
    base.data.boolean(DATA_GOT_FISH)
}

pub fn set_got_fish(base: &mut EntityBase, got_fish: bool) {
    base.data.set(DATA_GOT_FISH, SyncedValue::Boolean(got_fish));
}

pub fn moistness(base: &EntityBase) -> i32 {
    base.data.int(DATA_MOISTNESS)
}

pub fn set_moistness(base: &mut EntityBase, level: i32) {
    base.data.set(DATA_MOISTNESS, SyncedValue::Int(level));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut DolphinState) {
    set_got_fish(base, nbt.get_bool("GotFish").unwrap_or(false));
    set_moistness(base, nbt.get_i32("Moistness").unwrap_or(MAX_MOISTNESS));
}

pub fn write(base: &EntityBase, _state: &DolphinState, out: &mut Compound) {
    out.insert("GotFish", got_fish(base));
    out.insert("Moistness", moistness(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    base.set_air_supply(base.max_air_ticks);
    base.set_pitch(0.0);
    let group = group.unwrap_or(SpawnGroupData::Ageable {
        group_size: 0,
        should_spawn_baby: true,
        baby_chance: BABY_CHANCE,
    });
    ageable_finalize(base, living, Some(group), level)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinTick {
    pub dry_out: bool,
    pub flopped: bool,
}

pub fn tick_moistness(
    base: &mut EntityBase,
    wet: bool,
    random: &mut impl RandomSource,
) -> DolphinTick {
    let mut tick = DolphinTick::default();
    if wet {
        set_moistness(base, MAX_MOISTNESS);
        return tick;
    }
    let level = moistness(base) - 1;
    set_moistness(base, level);
    if level < 1 {
        tick.dry_out = true;
    }
    if base.on_ground {
        let x = f64::from((random.next_float() * 2.0 - 1.0) * DRY_FLOP_SPREAD);
        let z = f64::from((random.next_float() * 2.0 - 1.0) * DRY_FLOP_SPREAD);
        base.velocity = base.velocity + crate::Vec3::new(x, DRY_FLOP_VERTICAL, z);
        base.set_yaw(random.next_float() * 360.0);
        base.on_ground = false;
        base.needs_sync = true;
        tick.flopped = true;
    }
    tick
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn dolphin(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:dolphin"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn a_dolphin_breathes_for_four_thousand_eight_hundred_ticks() {
        let entity = dolphin(Compound::new());
        assert_eq!(entity.base.max_air_ticks, 4800);
        let fresh = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Dolphin);
        assert_eq!(fresh.air_supply(), 4800);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Bukkit.MaxAirSupply"), None);
        let cow = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cow);
        assert_eq!(cow.air_supply(), 300);
    }

    #[test]
    fn the_fish_flag_and_moistness_round_trip_with_the_vanilla_defaults() {
        let entity = dolphin(Compound::new());
        assert!(!got_fish(&entity.base));
        assert_eq!(moistness(&entity.base), 2400);
        assert_eq!(
            entity.base.data.get(DATA_GOT_FISH),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(
            entity.base.data.get(DATA_MOISTNESS),
            Some(&SyncedValue::Int(2400))
        );
        let fed = dolphin(Compound::new().with("GotFish", true).with("Moistness", 77));
        assert!(got_fish(&fed.base));
        assert_eq!(moistness(&fed.base), 77);
        let saved = save_entity(&fed, None).unwrap();
        assert_eq!(saved.get_bool("GotFish"), Some(true));
        assert_eq!(saved.get_i32("Moistness"), Some(77));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_baby_dolphin_is_sixty_five_hundredths_of_the_adult_with_a_low_eye() {
        let adult = EntityKind::Dolphin.dimensions();
        let baby = crate::entity::ageable::dimensions(EntityKind::Dolphin, true);
        assert_eq!(baby.width, adult.width * 0.65);
        assert_eq!(baby.height, adult.height * 0.65);
        assert_eq!(baby.eye_height, 0.09375);
    }

    #[test]
    fn a_wet_dolphin_refills_its_moistness_and_a_dry_one_dries_and_flops() {
        let mut entity = dolphin(Compound::new());
        let mut random = LegacyRandom::new(5);
        set_moistness(&mut entity.base, 100);
        let tick = tick_moistness(&mut entity.base, true, &mut random);
        assert_eq!(moistness(&entity.base), 2400);
        assert_eq!(tick, DolphinTick::default());
        entity.base.on_ground = true;
        let tick = tick_moistness(&mut entity.base, false, &mut random);
        assert_eq!(moistness(&entity.base), 2399);
        assert!(!tick.dry_out && tick.flopped);
        assert!(!entity.base.on_ground);
        assert_eq!(entity.base.velocity.y, 0.5);
        assert!(entity.base.velocity.x.abs() <= 0.2 && entity.base.velocity.z.abs() <= 0.2);
        assert!((0.0..360.0).contains(&entity.base.yaw));
    }

    #[test]
    fn a_dolphin_that_ran_out_of_moisture_takes_the_dry_out_damage_each_tick() {
        let mut entity = dolphin(Compound::new());
        let mut random = LegacyRandom::new(5);
        set_moistness(&mut entity.base, 1);
        assert!(tick_moistness(&mut entity.base, false, &mut random).dry_out);
        assert_eq!(moistness(&entity.base), 0);
        assert!(tick_moistness(&mut entity.base, false, &mut random).dry_out);
        assert_eq!(moistness(&entity.base), -1);
    }
}
