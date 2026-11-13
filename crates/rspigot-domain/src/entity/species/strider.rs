use std::borrow::Cow;
use std::sync::OnceLock;

use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingAttributes, LivingState, MOVEMENT_SPEED, slot_index};
use crate::entity::physics::BlockView;
use crate::entity::spawn::{Jockey, SpawnEnvironment, ageable_finalize, set_item_slot, stack};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;
use crate::world::Difficulty;
use crate::{BlockPos, BlockStateId};

pub const DATA_BOOST_TIME: u8 = 18;
pub const DATA_SUFFOCATING: u8 = 19;
pub const SUFFOCATING_MODIFIER: &str = "minecraft:suffocating";
pub const SUFFOCATING_AMOUNT: f64 = -0.34;
pub const GUARANTEED_DROP: f32 = 2.0;
pub const BABY_AGE: i32 = -24_000;
pub const PIGLIN_JOCKEY_ODDS: i32 = 30;
pub const BABY_JOCKEY_ODDS: i32 = 10;
pub const PANIC_SPEED: f64 = 1.65;
pub const TEMPT_SPEED: f64 = 1.4;
pub const STROLL_INTERVAL: i32 = 60;
pub const WARM_SPEED: f32 = 0.55;
pub const COLD_SPEED: f32 = 0.35;
pub const HAPPY_ODDS: i32 = 140;
pub const RETREAT_ODDS: i32 = 60;
pub const LAVA_WALK_TARGET_VALUE: f32 = 10.0;
pub const KEYS: &[&str] = &[];
pub const BOOST_BASE: i32 = 140;
pub const BOOST_SPREAD: i32 = 841;
pub const BOOST_GAIN: f32 = 1.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StriderState {
    pub boosting: bool,
    pub boost_time: i32,
    pub boost_total: i32,
}

impl StriderState {
    pub fn boost(&mut self, base: &mut EntityBase, random: &mut impl RandomSource) -> bool {
        if self.boosting {
            return false;
        }
        self.boosting = true;
        self.boost_time = 0;
        self.boost_total = random.next_int(BOOST_SPREAD) + BOOST_BASE;
        base.data
            .set(DATA_BOOST_TIME, SyncedValue::Int(self.boost_total));
        true
    }

    pub fn tick_boost(&mut self) {
        if self.boosting {
            let spent = self.boost_time;
            self.boost_time += 1;
            if spent > self.boost_total {
                self.boosting = false;
            }
        }
    }

    pub fn boost_factor(&self) -> f32 {
        if self.boosting {
            1.0 + BOOST_GAIN
                * (std::f32::consts::PI / self.boost_total as f32 * self.boost_time as f32).sin()
        } else {
            1.0
        }
    }
}

pub fn define(base: &mut EntityBase) -> StriderState {
    base.data.define(DATA_BOOST_TIME, SyncedValue::Int(0));
    base.data
        .define(DATA_SUFFOCATING, SyncedValue::Boolean(false));
    StriderState::default()
}

pub fn is_suffocating(base: &EntityBase) -> bool {
    base.data.boolean(DATA_SUFFOCATING)
}

pub fn set_suffocating(base: &mut EntityBase, attributes: &mut LivingAttributes, value: bool) {
    base.data.set(DATA_SUFFOCATING, SyncedValue::Boolean(value));
    attributes.remove_modifier(MOVEMENT_SPEED, SUFFOCATING_MODIFIER);
    if value {
        attributes.add_modifier(
            MOVEMENT_SPEED,
            AttributeModifier {
                id: Cow::Borrowed(SUFFOCATING_MODIFIER),
                amount: SUFFOCATING_AMOUNT,
                operation: AttributeOperation::AddMultipliedBase,
            },
        );
    }
}

pub fn walk_target_value(lava_here: bool, in_lava: bool) -> f32 {
    if lava_here {
        LAVA_WALK_TARGET_VALUE
    } else if in_lava {
        f32::NEG_INFINITY
    } else {
        0.0
    }
}

pub fn ridden_speed(attribute: f64, suffocating: bool, boost_factor: f32) -> f32 {
    let steering = if suffocating { COLD_SPEED } else { WARM_SPEED };
    (attribute * f64::from(steering) * f64::from(boost_factor)) as f32
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let baby = living
        .mob
        .as_ref()
        .and_then(|mob| mob.ageable.as_ref())
        .is_some_and(|ageable| ageable.age < 0);
    if baby {
        return ageable_finalize(base, living, group, level);
    }
    let mut group = group;
    if level.next_int(PIGLIN_JOCKEY_ODDS) == 0 {
        if environment.difficulty.base != Difficulty::Peaceful {
            let baby = level.next_float() < crate::entity::spawn::ZOMBIE_BABY_CHANCE;
            if let Some(mob) = living.mob.as_mut() {
                mob.jockey = Some(Jockey::StriderPiglin { baby });
            }
            set_item_slot(living, EquipmentSlot::Saddle, Some(stack(ItemKind::Saddle)));
            if let Some(mob) = living.mob.as_mut() {
                mob.drop_chances[slot_index(EquipmentSlot::Saddle)] = GUARANTEED_DROP;
            }
            group = Some(ageable_group(0.0));
        }
    } else if level.next_int(BABY_JOCKEY_ODDS) == 0 {
        if let Some(mob) = living.mob.as_mut() {
            mob.jockey = Some(Jockey::StriderBaby);
        }
        group = Some(ageable_group(0.0));
    } else {
        group = Some(ageable_group(0.5));
    }
    ageable_finalize(base, living, group, level)
}

fn ageable_group(baby_chance: f32) -> SpawnGroupData {
    SpawnGroupData::Ageable {
        group_size: 0,
        should_spawn_baby: true,
        baby_chance,
    }
}

pub fn is_strider(kind: EntityKind) -> bool {
    kind == EntityKind::Strider
}

const STABLE_HEIGHT: f64 = 0.5;
const ABOVE_EPSILON: f64 = 1.0e-5;

fn stable_surface() -> BlockStateId {
    static STATE: OnceLock<BlockStateId> = OnceLock::new();
    *STATE.get_or_init(|| {
        BlockKind::StoneSlab
            .state_with(&[("type", "bottom"), ("waterlogged", "false")])
            .unwrap_or_default()
    })
}

pub fn is_above_surface(bottom: f64, pos: BlockPos) -> bool {
    bottom > f64::from(pos.y) + STABLE_HEIGHT - ABOVE_EPSILON
}

fn is_lava(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::LAVA)
}

pub struct LavaStriding<'a, V> {
    inner: &'a V,
    bottom: f64,
}

impl<'a, V> LavaStriding<'a, V> {
    pub fn new(inner: &'a V, bottom: f64) -> Self {
        Self { inner, bottom }
    }
}

impl<V: BlockView> BlockView for LavaStriding<'_, V> {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        let state = self.inner.block(pos)?;
        let source = is_lava(state) && state.property("level") == Some("0");
        if source
            && is_above_surface(self.bottom, pos)
            && !self
                .inner
                .block_or_air(pos.offset(0, 1, 0))
                .flags()
                .contains(StateFlags::LAVA)
        {
            return Some(stable_surface());
        }
        Some(state)
    }
}

pub fn floats(base: &mut EntityBase, above_is_lava: bool) {
    if !base.fluid.in_lava() {
        return;
    }
    if is_above_surface(base.position.y, base.position.block()) && !above_is_lava {
        base.on_ground = true;
    } else {
        let v = base.velocity.scale(0.5);
        base.velocity = crate::Vec3::new(v.x, v.y + 0.05, v.z);
    }
}

pub fn should_shiver(warm: bool, vehicle_warm_strider: bool) -> bool {
    !(warm || vehicle_warm_strider)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lake;

    impl BlockView for Lake {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos.y < 64 {
                BlockKind::Lava.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    #[test]
    fn a_strider_above_a_lava_source_meets_a_half_block_floor() {
        let view = LavaStriding::new(&Lake, 63.6);
        let top = view.block(BlockPos::new(0, 63, 0)).unwrap();
        assert!(!is_lava(top));
        assert!(top.collision_shape().iter().any(|shape| shape.max_y == 0.5));
        let below = LavaStriding::new(&Lake, 63.2);
        assert!(is_lava(below.block(BlockPos::new(0, 63, 0)).unwrap()));
        let sunk = view.block(BlockPos::new(0, 62, 0)).unwrap();
        assert!(is_lava(sunk));
    }

    #[test]
    fn a_boost_lasts_until_its_total_and_swells_the_speed() {
        let mut base = EntityBase::new(
            crate::EntityId(1),
            uuid::Uuid::from_u128(1),
            EntityKind::Strider,
        );
        let mut state = define(&mut base);
        struct Fixed;
        impl RandomSource for Fixed {
            fn next_float(&mut self) -> f32 {
                0.0
            }
            fn next_int(&mut self, _: i32) -> i32 {
                10
            }
            fn next_double(&mut self) -> f64 {
                0.0
            }
            fn next_boolean(&mut self) -> bool {
                false
            }
            fn next_gaussian(&mut self) -> f64 {
                0.0
            }
        }
        assert!(state.boost(&mut base, &mut Fixed));
        assert!(!state.boost(&mut base, &mut Fixed));
        assert_eq!(state.boost_total, 150);
        assert_eq!(base.data.int(DATA_BOOST_TIME), 150);
        for _ in 0..75 {
            state.tick_boost();
        }
        assert!(state.boost_factor() > 2.0);
        for _ in 0..80 {
            state.tick_boost();
        }
        assert!(!state.boosting);
        assert_eq!(state.boost_factor(), 1.0);
    }

    #[test]
    fn cold_and_warm_strider_speeds_follow_the_ridden_formula() {
        assert!((ridden_speed(0.175, false, 1.0) - 0.175 * 0.55).abs() < 1e-6);
        assert!((ridden_speed(0.175, true, 1.0) - 0.175 * 0.35).abs() < 1e-6);
        assert!(should_shiver(false, false));
        assert!(!should_shiver(true, false));
        assert!(!should_shiver(false, true));
        assert_eq!(walk_target_value(true, false), 10.0);
        assert_eq!(walk_target_value(false, true), f32::NEG_INFINITY);
    }
}
