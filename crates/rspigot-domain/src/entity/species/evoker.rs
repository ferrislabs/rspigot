use rspigot_items::EquipmentSlot;
use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use super::spellcaster::{self, SpellcasterState};
use crate::EntityId;
use crate::entity::base::EntityBase;
use crate::entity::living::{LivingState, slot_index};
use crate::entity::spawn::{mob_finalize, set_item_slot};
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const KEYS: &[&str] = spellcaster::KEYS;
pub const AMBIENT_SOUND: &str = "minecraft:entity.evoker.ambient";
pub const HURT_SOUND: &str = "minecraft:entity.evoker.hurt";
pub const DEATH_SOUND: &str = "minecraft:entity.evoker.death";
pub const CELEBRATE_SOUND: &str = "minecraft:entity.evoker.celebrate";
pub const CAST_SOUND: &str = "minecraft:entity.evoker.cast_spell";
pub const PREPARE_SUMMON_SOUND: &str = "minecraft:entity.evoker.prepare_summon";
pub const PREPARE_ATTACK_SOUND: &str = "minecraft:entity.evoker.prepare_attack";
pub const PREPARE_WOLOLO_SOUND: &str = "minecraft:entity.evoker.prepare_wololo";
pub const FANGS_ATTACK_SOUND: &str = "minecraft:entity.evoker_fangs.attack";
pub const SUMMON_VEX_COUNT: i32 = 3;
pub const SUMMON_SPREAD: i32 = 5;
pub const SUMMON_HEIGHT: i32 = 1;
pub const SUMMON_COUNT_RANGE: f64 = 16.0;
pub const SUMMON_COUNT_RANDOM_BOUND: i32 = 8;
pub const SUMMON_LIFE_BASE_SECONDS: i32 = 30;
pub const SUMMON_LIFE_RANDOM_SECONDS: i32 = 90;
pub const SUMMON_CASTING_TICKS: i32 = 100;
pub const SUMMON_INTERVAL: i32 = 340;
pub const ATTACK_CASTING_TICKS: i32 = 40;
pub const ATTACK_INTERVAL: i32 = 100;
pub const WOLOLO_RANGE: f64 = 16.0;
pub const WOLOLO_INFLATE_Y: f64 = 4.0;
pub const WOLOLO_WARMUP_TICKS: i32 = 40;
pub const WOLOLO_CASTING_TICKS: i32 = 60;
pub const WOLOLO_INTERVAL: i32 = 140;
pub const WOLOLO_FROM_COLOR: u8 = 11;
pub const WOLOLO_TO_COLOR: u8 = 14;
pub const FANGS_LINE_COUNT: i32 = 16;
pub const FANGS_LINE_STEP: f64 = 1.25;
pub const FANGS_NEAR_DISTANCE_SQR: f64 = 9.0;
pub const FANGS_NEAR_RING_COUNT: i32 = 5;
pub const FANGS_NEAR_RING_RADIUS: f64 = 1.5;
pub const FANGS_FAR_RING_COUNT: i32 = 8;
pub const FANGS_FAR_RING_RADIUS: f64 = 2.5;
pub const FANGS_FAR_RING_DELAY: i32 = 3;
pub const FANGS_Y_RANGE_TOP: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EvokerState {
    pub raider: RaiderState,
    pub spell: SpellcasterState,
    pub wololo_target: Option<EntityId>,
}

pub fn define(base: &mut EntityBase) -> EvokerState {
    raider::define(base);
    spellcaster::define(base);
    EvokerState::default()
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut EvokerState) {
    raider::read(nbt, &mut state.raider);
    spellcaster::read(nbt, &mut state.spell);
}

pub fn write(_base: &EntityBase, state: &EvokerState, out: &mut Compound) {
    raider::write(&state.raider, out);
    spellcaster::write(&state.spell, out);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let leader = match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(crate::entity::species::Species::Evoker(state)) => {
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

pub fn summon_positions(
    origin: crate::BlockPos,
    random: &mut impl RandomSource,
) -> [crate::BlockPos; 3] {
    let mut next = || {
        let dx = random.next_int(SUMMON_SPREAD) - 2;
        let dz = random.next_int(SUMMON_SPREAD) - 2;
        origin.offset(dx, SUMMON_HEIGHT, dz)
    };
    [next(), next(), next()]
}

pub fn summon_life_ticks(random: &mut impl RandomSource) -> i32 {
    (random.next_int(SUMMON_LIFE_RANDOM_SECONDS) + SUMMON_LIFE_BASE_SECONDS) * 20
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FangSpawn {
    pub x: f64,
    pub z: f64,
    pub angle: f32,
    pub delay: i32,
}

pub fn fang_pattern(origin_x: f64, origin_z: f64, angle: f32, close: bool) -> Vec<FangSpawn> {
    let mut spawns = Vec::new();
    if !close {
        for i in 0..FANGS_LINE_COUNT {
            let reach = f64::from(i + 1) * FANGS_LINE_STEP;
            spawns.push(FangSpawn {
                x: origin_x + f64::from(crate::mth::cos(f64::from(angle))) * reach,
                z: origin_z + f64::from(crate::mth::sin(f64::from(angle))) * reach,
                angle,
                delay: i,
            });
        }
        return spawns;
    }
    for i in 0..FANGS_NEAR_RING_COUNT {
        let ring = angle + i as f32 * std::f32::consts::PI * 0.4;
        spawns.push(FangSpawn {
            x: origin_x + f64::from(crate::mth::cos(f64::from(ring))) * FANGS_NEAR_RING_RADIUS,
            z: origin_z + f64::from(crate::mth::sin(f64::from(ring))) * FANGS_NEAR_RING_RADIUS,
            angle: ring,
            delay: 0,
        });
    }
    for i in 0..FANGS_FAR_RING_COUNT {
        let ring = angle + i as f32 * std::f32::consts::PI * 2.0 / 8.0 + 1.256_637_1;
        spawns.push(FangSpawn {
            x: origin_x + f64::from(crate::mth::cos(f64::from(ring))) * FANGS_FAR_RING_RADIUS,
            z: origin_z + f64::from(crate::mth::sin(f64::from(ring))) * FANGS_FAR_RING_RADIUS,
            angle: ring,
            delay: FANGS_FAR_RING_DELAY,
        });
    }
    spawns
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::BlockPos;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};
    use crate::random::LegacyRandom;

    fn evoker() -> Entity {
        load_entity(
            &Compound::new().with("id", "minecraft:evoker"),
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

    fn state_of(entity: &Entity) -> EvokerState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Evoker(state)) => state,
            other => panic!("{other:?}"),
        }
    }

    fn state_mut(entity: &mut Entity) -> &mut EvokerState {
        match entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            Some(Species::Evoker(state)) => state,
            _ => panic!("an evoker has the evoker species"),
        }
    }

    #[test]
    fn an_evoker_has_the_celebration_flag_and_the_spell_byte_after_it() {
        use crate::entity::data::SyncedValue;
        let entity = evoker();
        assert_eq!(
            entity.base.data.get(raider::DATA_IS_CELEBRATING),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(
            entity.base.data.get(spellcaster::DATA_SPELL_CASTING),
            Some(&SyncedValue::Byte(0))
        );
        assert_eq!(
            spellcaster::DATA_SPELL_CASTING,
            raider::DATA_IS_CELEBRATING + 1
        );
    }

    #[test]
    fn the_saved_layout_is_the_raider_fields_and_the_spell_ticks() {
        let mut entity = evoker();
        {
            let state = state_mut(&mut entity);
            state.raider.wave = 4;
            state.raider.raid_id = Some(9);
            state.raider.can_join_raid = true;
            state.spell.casting_ticks = 55;
        }
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("SpellTicks"), Some(55));
        assert_eq!(saved.get_i32("Wave"), Some(4));
        assert_eq!(saved.get_i32("RaidId"), Some(9));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        let state = state_of(&loaded);
        assert_eq!(state.spell.casting_ticks, 55);
        assert_eq!(state.raider.wave, 4);
        assert_eq!(state.raider.raid_id, Some(9));
        assert_eq!(save_entity(&loaded, None), Some(saved));
    }

    #[test]
    fn the_wololo_target_and_the_current_spell_are_not_persisted() {
        let mut entity = evoker();
        {
            let state = state_mut(&mut entity);
            state.wololo_target = Some(EntityId(8));
            state.spell.current = spellcaster::IllagerSpell::Fangs;
        }
        let saved = save_entity(&entity, None).unwrap();
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(state_of(&loaded).wololo_target, None);
        assert_eq!(
            state_of(&loaded).spell.current,
            spellcaster::IllagerSpell::None
        );
    }

    #[test]
    fn an_evoker_spawns_without_equipment_and_a_leader_wears_the_banner() {
        let mut entity = evoker();
        let mut level = LegacyRandom::new(4);
        let mut own = LegacyRandom::new(3);
        finalize_spawn(
            &mut entity,
            &environment(),
            SpawnReason::Structure,
            None,
            &mut level,
            &mut own,
        );
        assert!(
            entity
                .living()
                .unwrap()
                .equipment(EquipmentSlot::Mainhand)
                .is_none()
        );
        assert!(state_of(&entity).raider.can_join_raid);
        let seed = (0..100_000)
            .find(|seed| LegacyRandom::new(*seed).next_float() < raider::PATROL_LEADER_ODDS)
            .unwrap();
        let mut leader = evoker();
        let mut level = LegacyRandom::new(seed);
        finalize_spawn(
            &mut leader,
            &environment(),
            SpawnReason::Natural,
            None,
            &mut level,
            &mut own,
        );
        assert!(state_of(&leader).raider.patrol_leader);
        assert!(
            leader
                .living()
                .unwrap()
                .equipment(EquipmentSlot::Head)
                .is_some()
        );
        assert!(matches!(leader.state, EntityState::Living(_)));
    }

    #[test]
    fn the_summon_spreads_three_vexes_over_a_five_by_five_square_one_block_up() {
        let mut random = LegacyRandom::new(11);
        let mut mirror = LegacyRandom::new(11);
        let positions = summon_positions(BlockPos::new(10, 64, -3), &mut random);
        for position in positions {
            let dx = mirror.next_int(5) - 2;
            let dz = mirror.next_int(5) - 2;
            assert_eq!(position, BlockPos::new(10 + dx, 65, -3 + dz));
        }
    }

    #[test]
    fn a_summoned_vex_lives_thirty_to_one_hundred_nineteen_seconds() {
        let mut random = LegacyRandom::new(5);
        let mut mirror = LegacyRandom::new(5);
        let ticks = summon_life_ticks(&mut random);
        assert_eq!(ticks, (mirror.next_int(90) + 30) * 20);
        assert!((600..=2400).contains(&ticks));
    }

    #[test]
    fn a_far_target_gets_a_line_of_sixteen_fangs_with_a_growing_delay() {
        let spawns = fang_pattern(0.0, 0.0, 0.0, false);
        assert_eq!(spawns.len(), 16);
        for (i, spawn) in spawns.iter().enumerate() {
            assert_eq!(spawn.delay, i as i32);
            assert_eq!(spawn.angle, 0.0);
            assert!((spawn.x - (i as f64 + 1.0) * 1.25).abs() < 1e-6);
            assert!(spawn.z.abs() < 1e-6);
        }
    }

    #[test]
    fn a_close_target_gets_a_ring_of_five_then_a_ring_of_eight() {
        let spawns = fang_pattern(0.0, 0.0, 0.0, true);
        assert_eq!(spawns.len(), 13);
        for spawn in &spawns[..5] {
            assert_eq!(spawn.delay, 0);
            assert!((spawn.x.hypot(spawn.z) - 1.5).abs() < 1e-5);
        }
        for spawn in &spawns[5..] {
            assert_eq!(spawn.delay, 3);
            assert!((spawn.x.hypot(spawn.z) - 2.5).abs() < 1e-5);
        }
        let step = std::f32::consts::PI * 0.4;
        assert!((spawns[1].angle - step).abs() < 1e-6);
        assert!((spawns[5].angle - 1.256_637_1).abs() < 1e-6);
    }
}
