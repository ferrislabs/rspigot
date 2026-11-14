use rspigot_entities::EntityDimensions;
use rspigot_nbt::Compound;

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{ATTACK_DAMAGE, LivingAttributes, LivingState};
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_SIZE: u8 = 16;
pub const KEYS: &[&str] = &["anchor_pos", "size"];
pub const MAX_SIZE: i32 = 64;
pub const BASE_ATTACK_DAMAGE: f64 = 6.0;
pub const SIZE_SCALE: f32 = 0.15;
pub const ANCHOR_HEIGHT: i32 = 5;
pub const SWOOP_EVENT: i32 = 1039;
pub const SWOOP_SOUND: &str = "minecraft:entity.phantom.swoop";
pub const FLY_SPEED_AIR: f32 = 0.2;
pub const FLY_SPEED_FLUID: f32 = 0.02;
pub const TICKS_PER_FLAP: i32 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhantomState {
    pub anchor: Option<BlockPos>,
}

pub fn define(base: &mut EntityBase) -> PhantomState {
    base.data.define(DATA_SIZE, SyncedValue::Int(0));
    PhantomState::default()
}

pub fn size(base: &EntityBase) -> i32 {
    match base.data.get(DATA_SIZE) {
        Some(SyncedValue::Int(size)) => *size,
        _ => 0,
    }
}

pub fn scaled_dimensions(dimensions: EntityDimensions, size: i32) -> EntityDimensions {
    let factor = size as f32 * SIZE_SCALE + 1.0;
    EntityDimensions {
        width: dimensions.width * factor,
        height: dimensions.height * factor,
        eye_height: dimensions.eye_height * factor,
    }
}

pub fn set_size(base: &mut EntityBase, attributes: &mut LivingAttributes, size: i32) {
    let size = size.clamp(0, MAX_SIZE);
    base.data.set(DATA_SIZE, SyncedValue::Int(size));
    base.dimensions = scaled_dimensions(base.kind.dimensions(), size);
    attributes.set_base(ATTACK_DAMAGE, BASE_ATTACK_DAMAGE + f64::from(size));
}

pub fn read(
    nbt: &Compound,
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    state: &mut PhantomState,
) {
    state.anchor = nbt
        .get_int_array("anchor_pos")
        .filter(|pos| pos.len() == 3)
        .map(|pos| BlockPos::new(pos[0], pos[1], pos[2]));
    set_size(base, attributes, nbt.get_i32("size").unwrap_or(0));
}

pub fn write(base: &EntityBase, state: &PhantomState, out: &mut Compound) {
    if let Some(pos) = state.anchor {
        out.insert("anchor_pos", vec![pos.x, pos.y, pos.z]);
    }
    out.insert("size", size(base));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let anchor = base.position.block().offset(0, ANCHOR_HEIGHT, 0);
    if let Some(crate::entity::species::Species::Phantom(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.anchor = Some(anchor);
    }
    set_size(base, &mut living.attributes, 0);
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};

    fn phantom(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:phantom"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn the_size_sits_at_index_sixteen_defaults_to_zero_and_is_always_saved() {
        let entity = phantom(Compound::new());
        assert_eq!(entity.base.data.get(DATA_SIZE), Some(&SyncedValue::Int(0)));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("size"), Some(0));
        assert_eq!(saved.get("anchor_pos"), None);
    }

    #[test]
    fn the_size_and_anchor_round_trip_and_the_size_is_clamped_to_sixty_four() {
        let entity = phantom(
            Compound::new()
                .with("size", 100)
                .with("anchor_pos", vec![3, 90, -4]),
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("size"), Some(64));
        assert_eq!(saved.get_int_array("anchor_pos"), Some(&[3, 90, -4][..]));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn each_size_grows_the_box_by_fifteen_hundredths_and_adds_one_attack_damage() {
        let entity = phantom(Compound::new().with("size", 10));
        let base = rspigot_entities::EntityKind::Phantom.dimensions();
        assert_eq!(entity.base.dimensions.width, base.width * 2.5);
        assert_eq!(entity.base.dimensions.height, base.height * 2.5);
        let mut entity = entity;
        let living = entity.living_mut().unwrap();
        assert_eq!(living.attribute(ATTACK_DAMAGE), 16.0);
    }

    #[test]
    fn a_fresh_phantom_anchors_five_blocks_above_where_it_spawned() {
        use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
        let mut entity = phantom(Compound::new());
        entity.base.position = crate::Vec3::new(4.5, 90.0, 4.5);
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
        let mut level = crate::random::LegacyRandom::new(5);
        let mut own = crate::random::LegacyRandom::new(6);
        finalize_spawn(
            &mut entity,
            &environment,
            crate::spawn::SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_int_array("anchor_pos"), Some(&[4, 95, 4][..]));
    }
}
