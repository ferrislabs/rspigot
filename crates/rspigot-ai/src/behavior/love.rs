use rspigot_domain::EntityId;
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::control::{Behavior, BehaviorCx};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::support::get_entity;
use crate::behavior::tracker::PositionTracker;
use crate::random::RandomSource;
use crate::world::{AiWorld, Living};

pub const MAKE_LOVE_DURATION: i32 = 110;
const MIN_SPAWN_DELAY: i64 = 60;
const SPAWN_DELAY_BOUND: i32 = 50;
const BREED_DISTANCE: f64 = 3.0;
const TEMPTATION_COOLDOWN: i32 = 100;
const TEMPTATION_WALK_CLOSE_ENOUGH: i32 = 2;
const DEFAULT_CLOSE_ENOUGH_DISTANCE: f64 = 2.5;

#[derive(Debug, Clone, PartialEq)]
pub struct AnimalMakeLove {
    partner_type: EntityKind,
    speed_modifier: f32,
    close_enough_distance: i32,
    spawn_child_at_time: i64,
}

impl AnimalMakeLove {
    const CONDITION: [Condition; 5] = [
        (
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryStatus::ValuePresent,
        ),
        (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
    ];

    pub fn new(partner_type: EntityKind) -> Self {
        Self::with_walk(partner_type, 1.0, 2)
    }

    pub fn with_walk(
        partner_type: EntityKind,
        speed_modifier: f32,
        close_enough_distance: i32,
    ) -> Self {
        Self {
            partner_type,
            speed_modifier,
            close_enough_distance,
            spawn_child_at_time: 0,
        }
    }

    fn can_mate(cx: &BehaviorCx<'_, '_, impl AiWorld>, other: &Living) -> bool {
        other.id != cx.ai.mob.id
            && other.kind == cx.ai.mob.kind
            && cx.ai.mob.in_love
            && other.in_love
            && crate::sniffer::can_mate_in_states(
                cx.ai.mob.kind,
                cx.ai.body.sniffer.state,
                other.facts.sniffer_state,
            )
    }

    fn find_valid_breed_partner<W: AiWorld>(
        &self,
        cx: &mut BehaviorCx<'_, '_, W>,
    ) -> Option<Living> {
        let partner_type = self.partner_type;
        let mob = cx.ai.mob;
        let in_love = mob.in_love;
        let kind = mob.kind;
        let id = mob.id;
        let own_state = cx.ai.body.sniffer.state;
        cx.brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .and_then(|visible| {
                visible.find_closest(cx.ai, &cx.brain.memories, |other| {
                    other.kind == partner_type
                        && other.id != id
                        && other.kind == kind
                        && in_love
                        && other.in_love
                        && crate::sniffer::can_mate_in_states(
                            kind,
                            own_state,
                            other.facts.sniffer_state,
                        )
                        && !other.panicking
                })
            })
    }

    fn breed_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
        get_entity(cx, MemoryModuleType::BreedTarget).and_then(|id| cx.ai.world.living(id))
    }

    fn lock_gaze_and_walk_to_each_other<W: AiWorld>(
        &self,
        cx: &mut BehaviorCx<'_, '_, W>,
        other: EntityId,
    ) {
        let me = cx.ai.mob.id;
        let walk = |target: EntityId| WalkTarget {
            target: PositionTracker::of_entity(target, false),
            speed_modifier: self.speed_modifier,
            close_enough_dist: self.close_enough_distance,
        };
        let look = |target: EntityId| PositionTracker::of_entity(target, true);
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(look(other)),
        );
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(walk(other)),
        );
        cx.ai.act(MobAction::SetMemory {
            mob: other,
            memory: MemoryModuleType::LookTarget,
            value: Some(MemoryValue::Tracker(look(me))),
        });
        cx.ai.act(MobAction::SetMemory {
            mob: other,
            memory: MemoryModuleType::WalkTarget,
            value: Some(MemoryValue::WalkTarget(walk(me))),
        });
    }
}

impl Behavior for AnimalMakeLove {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.in_love && self.find_valid_breed_partner(cx).is_some()
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(partner) = self.find_valid_breed_partner(cx) else {
            return;
        };
        cx.brain.memories.set(
            MemoryModuleType::BreedTarget,
            MemoryValue::Entity(partner.id),
        );
        let me = cx.ai.mob.id;
        cx.ai.act(MobAction::SetMemory {
            mob: partner.id,
            memory: MemoryModuleType::BreedTarget,
            value: Some(MemoryValue::Entity(me)),
        });
        self.lock_gaze_and_walk_to_each_other(cx, partner.id);
        let delay = cx.ai.brain.random.next_int(SPAWN_DELAY_BOUND);
        self.spawn_child_at_time = now + MIN_SPAWN_DELAY + i64::from(delay);
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let Some(partner) = Self::breed_target(cx) else {
            return false;
        };
        if partner.kind != self.partner_type || !partner.alive || !Self::can_mate(cx, &partner) {
            return false;
        }
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, partner.id));
        visible
            && now <= self.spawn_child_at_time
            && !cx.brain.memories.has_value(MemoryModuleType::IsPanicking)
            && !partner.panicking
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(partner) = Self::breed_target(cx) else {
            return;
        };
        self.lock_gaze_and_walk_to_each_other(cx, partner.id);
        if cx.ai.mob.position.distance_squared(partner.position) >= BREED_DISTANCE * BREED_DISTANCE
            || self.spawn_child_at_time > now
        {
            return;
        }
        cx.ai.act(MobAction::Breed(partner.id));
        cx.brain.memories.erase(MemoryModuleType::BreedTarget);
        cx.ai.act(MobAction::SetMemory {
            mob: partner.id,
            memory: MemoryModuleType::BreedTarget,
            value: None,
        });
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::BreedTarget);
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
        self.spawn_child_at_time = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TemptDistance {
    Constant(f64),
    ByAge { baby: f64, adult: f64 },
}

impl TemptDistance {
    fn get(self, baby: bool) -> f64 {
        match self {
            Self::Constant(distance) => distance,
            Self::ByAge { baby: young, adult } => {
                if baby {
                    young
                } else {
                    adult
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowTemptation {
    speed_modifier: f32,
    close_enough_distance: TemptDistance,
    look_in_the_eyes: bool,
}

impl FollowTemptation {
    const CONDITION: [Condition; 6] = [
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::TemptationCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::TemptingPlayer, MemoryStatus::ValuePresent),
        (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
    ];

    pub fn new(speed_modifier: f32) -> Self {
        Self::with_distance(
            speed_modifier,
            TemptDistance::Constant(DEFAULT_CLOSE_ENOUGH_DISTANCE),
            false,
        )
    }

    pub fn with_distance(
        speed_modifier: f32,
        close_enough_distance: TemptDistance,
        look_in_the_eyes: bool,
    ) -> Self {
        Self {
            speed_modifier,
            close_enough_distance,
            look_in_the_eyes,
        }
    }
}

impl Behavior for FollowTemptation {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain
            .memories
            .has_value(MemoryModuleType::TemptingPlayer)
            && !cx.brain.memories.has_value(MemoryModuleType::BreedTarget)
            && !cx.brain.memories.has_value(MemoryModuleType::IsPanicking)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(player) =
            get_entity(cx, MemoryModuleType::TemptingPlayer).and_then(|id| cx.ai.world.living(id))
        else {
            return;
        };
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_entity(player.id, true)),
        );
        let close_enough = self.close_enough_distance.get(cx.ai.mob.baby);
        if close_enough * close_enough <= cx.ai.mob.distance_to_sqr(player.position) {
            let tracker = if self.look_in_the_eyes {
                PositionTracker::of_entity_targeting_eyes(player.id, true)
            } else {
                PositionTracker::of_entity(player.id, false)
            };
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget {
                    target: tracker,
                    speed_modifier: self.speed_modifier,
                    close_enough_dist: TEMPTATION_WALK_CLOSE_ENOUGH,
                }),
            );
        } else {
            cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.set(
            MemoryModuleType::TemptationCooldownTicks,
            MemoryValue::Int(TEMPTATION_COOLDOWN),
        );
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
    }
}
