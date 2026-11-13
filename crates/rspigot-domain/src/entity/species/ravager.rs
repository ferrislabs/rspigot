use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use crate::Vec3;
use crate::entity::base::EntityBase;
use crate::entity::living::LivingState;
use crate::entity::spawn::mob_finalize;
use crate::mth;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const KEYS: &[&str] = &[
    "Wave",
    "CanJoinRaid",
    "patrol_target",
    "PatrolLeader",
    "Patrolling",
    "RaidId",
    ATTACK_TICK_KEY,
    STUN_TICK_KEY,
    ROAR_TICK_KEY,
];
pub const ATTACK_TICK_KEY: &str = "AttackTick";
pub const STUN_TICK_KEY: &str = "StunTick";
pub const ROAR_TICK_KEY: &str = "RoarTick";
pub const AMBIENT_SOUND: &str = "minecraft:entity.ravager.ambient";
pub const HURT_SOUND: &str = "minecraft:entity.ravager.hurt";
pub const DEATH_SOUND: &str = "minecraft:entity.ravager.death";
pub const STEP_SOUND: &str = "minecraft:entity.ravager.step";
pub const ATTACK_SOUND: &str = "minecraft:entity.ravager.attack";
pub const STUNNED_SOUND: &str = "minecraft:entity.ravager.stunned";
pub const ROAR_SOUND: &str = "minecraft:entity.ravager.roar";
pub const CELEBRATE_SOUND: &str = "minecraft:entity.ravager.celebrate";
pub const STEP_VOLUME: f32 = 0.15;
pub const ATTACK_TICKS: i32 = 10;
pub const STUN_TICKS: i32 = 40;
pub const ROAR_TICKS: i32 = 20;
pub const ROAR_FIRES_AT: i32 = 10;
pub const ATTACK_EVENT: i8 = 4;
pub const STUN_EVENT: i8 = 39;
pub const ROAR_EVENT: i8 = 69;
pub const ROAR_RANGE: f64 = 4.0;
pub const ROAR_DAMAGE: f32 = 6.0;
pub const KNOCKBACK_STRENGTH: f64 = 4.0;
pub const KNOCKBACK_LIFT: f64 = 0.2;
pub const KNOCKBACK_MIN_DISTANCE_SQR: f64 = 0.001;
pub const STUN_CHANCE: f64 = 0.5;
pub const MAX_HEAD_YAW: i32 = 45;
pub const CHASING_SPEED: f64 = 0.35;
pub const IDLE_SPEED: f64 = 0.3;
pub const SPEED_BLEND: f64 = 0.1;
pub const LEAF_REACH: f64 = 0.2;
pub const ATTACK_BOX_DEFLATE_HORIZONTAL: f64 = 0.05;
pub const EXPERIENCE_REWARD: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RavagerState {
    pub raider: RaiderState,
    pub attack_tick: i32,
    pub stunned_tick: i32,
    pub roar_tick: i32,
}

impl RavagerState {
    pub const fn blocks_actions(&self) -> bool {
        0 < self.attack_tick || 0 < self.stunned_tick || 0 < self.roar_tick
    }

    pub const fn can_see(&self) -> bool {
        self.stunned_tick < 1 && self.roar_tick < 1
    }
}

pub fn define(base: &mut EntityBase) -> RavagerState {
    raider::define(base);
    RavagerState::default()
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut RavagerState) {
    raider::read(nbt, &mut state.raider);
    state.attack_tick = nbt.get_i32(ATTACK_TICK_KEY).unwrap_or(0);
    state.stunned_tick = nbt.get_i32(STUN_TICK_KEY).unwrap_or(0);
    state.roar_tick = nbt.get_i32(ROAR_TICK_KEY).unwrap_or(0);
}

pub fn write(_base: &EntityBase, state: &RavagerState, out: &mut Compound) {
    raider::write(&state.raider, out);
    out.insert(ATTACK_TICK_KEY, state.attack_tick);
    out.insert(STUN_TICK_KEY, state.stunned_tick);
    out.insert(ROAR_TICK_KEY, state.roar_tick);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(crate::entity::species::Species::Ravager(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.raider.finalize(reason, false, true, level);
    }
    mob_finalize(base, living, group, level)
}

pub fn blended_speed(current: f64, has_target: bool, immobile: bool) -> f64 {
    if immobile {
        return 0.0;
    }
    let wanted = if has_target {
        CHASING_SPEED
    } else {
        IDLE_SPEED
    };
    mth::lerp(SPEED_BLEND, current, wanted)
}

pub fn strong_knockback(ravager: Vec3, target: Vec3) -> Vec3 {
    let dx = target.x - ravager.x;
    let dz = target.z - ravager.z;
    let distance = (dx * dx + dz * dz).max(KNOCKBACK_MIN_DISTANCE_SQR);
    Vec3::new(
        dx / distance * KNOCKBACK_STRENGTH,
        KNOCKBACK_LIFT,
        dz / distance * KNOCKBACK_STRENGTH,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Roar,
    StunnedEnd,
    None,
}

pub fn count_down(state: &mut RavagerState) -> [Tick; 2] {
    let mut events = [Tick::None, Tick::None];
    if 0 < state.roar_tick {
        state.roar_tick -= 1;
        if state.roar_tick == ROAR_FIRES_AT {
            events[0] = Tick::Roar;
        }
    }
    if 0 < state.attack_tick {
        state.attack_tick -= 1;
    }
    if 0 < state.stunned_tick {
        state.stunned_tick -= 1;
        if state.stunned_tick == 0 {
            state.roar_tick = ROAR_TICKS;
            events[1] = Tick::StunnedEnd;
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::living::LivingState;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};

    fn ravager() -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Ravager);
        let living = LivingState::new(&mut base, 0.0);
        Entity::new(base, EntityState::Living(Box::new(living)))
    }

    fn state_of(entity: &Entity) -> RavagerState {
        match entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            Some(Species::Ravager(state)) => state,
            _ => panic!("a ravager has the ravager species"),
        }
    }

    #[test]
    fn the_ticks_are_saved_under_their_vanilla_keys_and_read_back() {
        let mut entity = ravager();
        if let Some(Species::Ravager(state)) = entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            state.attack_tick = 7;
            state.stunned_tick = 30;
            state.roar_tick = 12;
            state.raider.wave = 2;
        }
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("AttackTick"), Some(7));
        assert_eq!(saved.get_i32("StunTick"), Some(30));
        assert_eq!(saved.get_i32("RoarTick"), Some(12));
        assert_eq!(saved.get_i32("Wave"), Some(2));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        let state = state_of(&loaded);
        assert_eq!(
            (state.attack_tick, state.stunned_tick, state.roar_tick),
            (7, 30, 12)
        );
    }

    #[test]
    fn missing_ticks_default_to_zero() {
        let mut state = RavagerState::default();
        let mut base = ravager().base;
        read(&Compound::new(), &mut base, &mut state);
        assert_eq!(state, RavagerState::default());
        assert!(!state.blocks_actions());
        assert!(state.can_see());
    }

    #[test]
    fn any_running_tick_blocks_actions_and_stun_or_roar_blocks_sight() {
        let attacking = RavagerState {
            attack_tick: 1,
            ..RavagerState::default()
        };
        assert!(attacking.blocks_actions());
        assert!(attacking.can_see());
        let roaring = RavagerState {
            roar_tick: 1,
            ..RavagerState::default()
        };
        assert!(roaring.blocks_actions());
        assert!(!roaring.can_see());
        let stunned = RavagerState {
            stunned_tick: 1,
            ..RavagerState::default()
        };
        assert!(stunned.blocks_actions());
        assert!(!stunned.can_see());
    }

    #[test]
    fn the_speed_blends_ten_percent_toward_the_wanted_speed() {
        let idle = blended_speed(0.3, false, false);
        assert!((idle - 0.3).abs() < 1e-12);
        let chasing = blended_speed(0.3, true, false);
        assert!((chasing - 0.305).abs() < 1e-12);
        assert_eq!(blended_speed(0.3, true, true), 0.0);
    }

    #[test]
    fn the_strong_knockback_pushes_away_with_a_small_lift() {
        let push = strong_knockback(Vec3::new(0.0, 64.0, 0.0), Vec3::new(2.0, 64.0, 0.0));
        assert!((push.x - 2.0).abs() < 1e-12);
        assert_eq!(push.y, 0.2);
        assert_eq!(push.z, 0.0);
        let overlapping = strong_knockback(Vec3::new(1.0, 64.0, 1.0), Vec3::new(1.0, 64.0, 1.0));
        assert_eq!((overlapping.x, overlapping.z), (0.0, 0.0));
    }

    #[test]
    fn a_stun_ends_with_a_roar_that_fires_ten_ticks_later() {
        let mut state = RavagerState {
            stunned_tick: 1,
            ..RavagerState::default()
        };
        let events = count_down(&mut state);
        assert_eq!(events[1], Tick::StunnedEnd);
        assert_eq!(state.roar_tick, ROAR_TICKS);
        let mut fired = 0;
        for _ in 0..ROAR_TICKS {
            if count_down(&mut state)[0] == Tick::Roar {
                fired += 1;
                assert_eq!(state.roar_tick, ROAR_FIRES_AT);
            }
        }
        assert_eq!(fired, 1);
        assert_eq!(state.roar_tick, 0);
    }

    #[test]
    fn the_attack_tick_counts_down_to_zero() {
        let mut state = RavagerState {
            attack_tick: ATTACK_TICKS,
            ..RavagerState::default()
        };
        for _ in 0..ATTACK_TICKS + 3 {
            count_down(&mut state);
        }
        assert_eq!(state.attack_tick, 0);
    }
}
