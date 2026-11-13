use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::{BrainMessage, MobAction};
use crate::behavior::{
    Activity, ActivityData, AdultSensor, AnimalMakeLove, Behavior, BehaviorBrain, BehaviorCx,
    Condition, FollowTemptation, Gate, HurtBySensor, MemoryModuleType, MemoryStatus, MemoryValue,
    MobBehavior, MobControl, MobTrigger, NearestLivingEntitySensor, OrderPolicy, PlayerSensor,
    RandomStroll, RunningPolicy, SensorKind, SensorType, SetWalkTargetFromLookTarget,
    StartAttacking, TargetFinder, TemptDistance, TemptingSensor, UniformInt,
    is_entity_attackable_ignoring_line_of_sight,
};
use crate::goals::TemptItems;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind};

pub const PANIC_SPEED: f32 = 1.6;
pub const MAKE_LOVE_SPEED: f32 = 0.4;
pub const MAKE_LOVE_DISTANCE: i32 = 2;
pub const TEMPT_SPEED: f32 = 1.3;
pub const ZOMBIE_TEMPT_SPEED: f32 = 0.9;
pub const TEMPT_DISTANCE_BABY: f64 = 2.5;
pub const TEMPT_DISTANCE_ADULT: f64 = 3.5;
pub const SWIM_SPEED: f32 = 1.0;
pub const CHARGE_COOLDOWN: i32 = 80;
pub const CHARGE_SPEED: f32 = 0.6;
pub const ZOMBIE_CHARGE_SPEED: f32 = 0.5;
pub const CHARGE_KNOCKBACK: f32 = 2.0;
pub const MAX_CHARGE_DISTANCE: f64 = 12.0;
pub const MAX_TARGET_DETECTION_DISTANCE: f64 = 11.0;
pub const NON_PLAYER_ATTACKS: UniformInt = UniformInt::of(2400, 3600);
pub const ANGER_TICKS: i64 = 400;
pub const DASH_SOUND: &str = "minecraft:entity.nautilus.dash";
pub const ZOMBIE_DASH_SOUND: &str = "minecraft:entity.zombie_nautilus.dash";
pub const NAUTILUS_FOOD: &str = "minecraft:nautilus_food";
pub const HOSTILES: &str = "minecraft:nautilus_hostiles";
const ARMOR_STAND: EntityKind = EntityKind::ArmorStand;

pub const NAUTILUS_MEMORIES: [MemoryModuleType; 2] = [
    MemoryModuleType::AngryAt,
    MemoryModuleType::AttackTargetCooldown,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NautilusAction {
    ChargeHit {
        target: EntityId,
        speed: f32,
        knockback: f32,
        velocity: Vec3,
    },
}

pub fn init_memories(memories: &mut crate::behavior::Memories, attack_cooldown: i32) {
    memories.set(
        MemoryModuleType::AttackTargetCooldown,
        MemoryValue::Int(attack_cooldown),
    );
}

pub fn nautilus_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestAdult(AdultSensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::NautilusTemptations,
            TemptItems::Tag(NAUTILUS_FOOD),
        )),
    ]
}

fn in_entity_tag(kind: EntityKind, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:entity_type", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

fn is_hostile_target(target: &Living) -> bool {
    target.in_water && in_entity_tag(target.kind, HOSTILES)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NautilusFinder;

impl NautilusFinder {
    pub fn find<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
        if cx.brain.memories.has_value(MemoryModuleType::BreedTarget)
            || !cx.ai.mob.fluid.in_water
            || cx.ai.mob.baby
            || cx.ai.mob.tamed
        {
            return None;
        }
        let angry = cx
            .brain
            .memories
            .get(MemoryModuleType::AngryAt)
            .and_then(MemoryValue::as_uuid)
            .and_then(|uuid| cx.ai.world.living_by_uuid(uuid))
            .filter(|target| target.in_water)
            .filter(|target| {
                is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, target)
            });
        if angry.is_some() {
            return angry;
        }
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::AttackTargetCooldown)
        {
            return None;
        }
        let cooldown = NON_PLAYER_ATTACKS.sample(&mut cx.ai.brain.random);
        cx.brain.memories.set(
            MemoryModuleType::AttackTargetCooldown,
            MemoryValue::Int(cooldown),
        );
        if crate::random::RandomSource::next_float(&mut cx.ai.brain.random) < 0.5 {
            return None;
        }
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .cloned()?;
        visible.find_closest(cx.ai, &cx.brain.memories, is_hostile_target)
    }
}

pub fn set_anger_target<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    target: EntityId,
    uuid: uuid::Uuid,
) {
    let Some(living) = cx.ai.world.living(target) else {
        return;
    };
    if !is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, &living) {
        return;
    }
    cx.brain
        .memories
        .erase(MemoryModuleType::CantReachWalkTargetSince);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AngryAt,
        MemoryValue::Uuid(uuid),
        ANGER_TICKS,
    );
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    if let BrainMessage::NautilusAnger { attacker, uuid } = message {
        set_anger_target(cx, attacker, uuid);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeAttack {
    time_between_attacks: i32,
    speed: f32,
    knockback_force: f32,
    max_charge_distance: f64,
    max_target_detection_distance: f64,
    sound: &'static str,
    velocity: Vec3,
    start_position: Vec3,
}

impl ChargeAttack {
    const CONDITION: [Condition; 2] = [
        (
            MemoryModuleType::ChargeCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
    ];

    pub fn new(
        time_between_attacks: i32,
        speed: f32,
        knockback_force: f32,
        max_charge_distance: f64,
        max_target_detection_distance: f64,
        sound: &'static str,
    ) -> Self {
        Self {
            time_between_attacks,
            speed,
            knockback_force,
            max_charge_distance,
            max_target_detection_distance,
            sound,
            velocity: Vec3::ZERO,
            start_position: Vec3::ZERO,
        }
    }

    fn target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
        cx.brain
            .memories
            .get(MemoryModuleType::AttackTarget)
            .and_then(MemoryValue::as_entity)
            .and_then(|id| cx.ai.world.living(id))
    }

    fn selector<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        (cx.ai.world.mob_griefing() || target.kind != ARMOR_STAND)
            && cx.ai.world.within_world_border(target.position.block())
    }

    fn still_usable<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(target) = Self::target(cx) else {
            return false;
        };
        if cx.ai.mob.tamed {
            return false;
        }
        let travelled = cx.ai.mob.position - self.start_position;
        if self.max_charge_distance * self.max_charge_distance <= travelled.length_sqr() {
            return false;
        }
        let gap = target.position - cx.ai.mob.position;
        if self.max_target_detection_distance * self.max_target_detection_distance
            <= gap.length_sqr()
        {
            return false;
        }
        let sees = cx
            .ai
            .brain
            .sensing
            .has_line_of_sight(cx.ai.world, cx.ai.mob, &target);
        sees && !cx
            .brain
            .memories
            .has_value(MemoryModuleType::ChargeCooldownTicks)
    }

    fn face<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
        let eye = cx.ai.mob.eye_position();
        let dx = target.position.x - eye.x;
        let dy = target.eye_y() - eye.y;
        let dz = target.position.z - eye.z;
        let horizontal = (dx * dx + dz * dz).sqrt();
        let yaw = (dz.atan2(dx).to_degrees() as f32) - 90.0;
        let pitch = -(dy.atan2(horizontal).to_degrees() as f32);
        cx.ai.body.yaw = yaw;
        cx.ai.body.head_yaw = yaw;
        cx.ai.body.pitch = pitch;
    }
}

impl Behavior for ChargeAttack {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::AttackTarget)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.still_usable(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.start_position = cx.ai.mob.position;
        let Some(target) = Self::target(cx) else {
            return;
        };
        self.velocity = (target.position - cx.ai.mob.position)
            .normalize()
            .scale(f64::from(self.speed));
        if self.still_usable(cx) {
            cx.ai.act(MobAction::Sound {
                sound: self.sound,
                volume: 1.0,
                pitch: 1.0,
            });
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(target) = Self::target(cx) else {
            return;
        };
        Self::face(cx, &target);
        cx.ai.body.velocity = self.velocity;
        let mob = *cx.ai.mob;
        let area = rspigot_domain::entity::Aabb::of_size(mob.position, mob.width, mob.height);
        let candidates = cx
            .ai
            .world
            .living_of(TargetKind::Class("LivingEntity"), &area);
        let conditions = TargetingConditions::for_combat();
        let mut hit = None;
        for candidate in candidates {
            let selected = Self::selector(cx, &candidate);
            if conditions.test(cx.ai, &candidate, |_| selected) {
                hit = Some(candidate);
                break;
            }
        }
        let Some(hit) = hit else {
            return;
        };
        cx.ai.act(MobAction::Nautilus(NautilusAction::ChargeHit {
            target: hit.id,
            speed: self.speed,
            knockback: self.knockback_force,
            velocity: self.velocity,
        }));
        self.stop(cx, now);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.set(
            MemoryModuleType::ChargeCooldownTicks,
            MemoryValue::Int(self.time_between_attacks),
        );
        cx.brain.memories.erase(MemoryModuleType::AttackTarget);
    }
}

fn tempt_distance() -> TemptDistance {
    TemptDistance::ByAge {
        baby: TEMPT_DISTANCE_BABY,
        adult: TEMPT_DISTANCE_ADULT,
    }
}

fn find_target() -> MobControl {
    MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
        TargetFinder::Nautilus,
    )))
}

fn swim_gate() -> MobControl {
    MobControl::Gate(Gate::new(
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)],
        Vec::new(),
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::swim(SWIM_SPEED))),
                2,
            ),
            (
                MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(SWIM_SPEED, 3),
                )),
                3,
            ),
        ],
    ))
}

fn fight_activity(speed: f32, sound: &'static str) -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Fight,
        vec![(
            0,
            MobControl::behavior(MobBehavior::NautilusCharge(ChargeAttack::new(
                CHARGE_COOLDOWN,
                speed,
                CHARGE_KNOCKBACK,
                MAX_CHARGE_DISTANCE,
                MAX_TARGET_DETECTION_DISTANCE,
                sound,
            ))),
        )],
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (MemoryModuleType::TemptingPlayer, MemoryStatus::ValueAbsent),
            (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::ChargeCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
        ],
    )
}

fn counters() -> Vec<MobControl> {
    vec![
        MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
        MobControl::count_down_cooldown_ticks(MemoryModuleType::ChargeCooldownTicks),
        MobControl::count_down_cooldown_ticks(MemoryModuleType::AttackTargetCooldown),
    ]
}

pub fn nautilus_activities() -> Vec<ActivityData<MobControl>> {
    let mut core = vec![
        MobControl::animal_panic(PANIC_SPEED),
        MobControl::look_at_target_sink(45, 90),
        MobControl::move_to_target_sink(),
    ];
    core.extend(counters());
    vec![
        ActivityData::create(Activity::Core, 0, core),
        ActivityData::from_pairs(
            Activity::Idle,
            vec![
                (
                    1,
                    MobControl::animal_make_love(AnimalMakeLove::with_walk(
                        EntityKind::Nautilus,
                        MAKE_LOVE_SPEED,
                        MAKE_LOVE_DISTANCE,
                    )),
                ),
                (
                    2,
                    MobControl::follow_temptation(FollowTemptation::with_distance(
                        TEMPT_SPEED,
                        tempt_distance(),
                        false,
                    )),
                ),
                (3, find_target()),
                (4, swim_gate()),
            ],
        ),
        fight_activity(CHARGE_SPEED, DASH_SOUND),
    ]
}

pub fn zombie_nautilus_activities() -> Vec<ActivityData<MobControl>> {
    let mut core = vec![
        MobControl::look_at_target_sink(45, 90),
        MobControl::move_to_target_sink(),
    ];
    core.extend(counters());
    vec![
        ActivityData::create(Activity::Core, 0, core),
        ActivityData::from_pairs(
            Activity::Idle,
            vec![
                (
                    1,
                    MobControl::follow_temptation(FollowTemptation::with_distance(
                        ZOMBIE_TEMPT_SPEED,
                        tempt_distance(),
                        false,
                    )),
                ),
                (2, find_target()),
                (3, swim_gate()),
            ],
        ),
        fight_activity(ZOMBIE_CHARGE_SPEED, ZOMBIE_DASH_SOUND),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}
