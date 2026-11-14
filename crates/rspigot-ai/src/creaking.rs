use rspigot_blocks::Aabb;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, BehaviorCx, DoNothing, EntityFilter, Gate, MeleeAttack,
    MemoryModuleType, MemoryStatus, MemoryValue, MobControl, MobPredicate, MobTrigger,
    NearestLivingEntitySensor, PlayerSensor, RandomStroll, SensorKind,
    SetEntityLookTargetSometimes, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackCondition, StopAttackingIfTargetInvalid,
    TargetErased, TargetFinder, UniformInt,
};
use crate::targeting;
use crate::view::BlockView;
use crate::world::{AiWorld, Living};

pub const LOOK_CONE: f64 = 0.5;
pub const ACTIVATION_DISTANCE_SQR: f64 = 144.0;
pub const ATTACK_COOLDOWN: i32 = 40;
pub const SIGHT_RANGE: f64 = 128.0;
pub const PATH_HOME_RADIUS_SQR: f64 = 1024.0;
const ACTIVATE_SOUND: &str = "minecraft:entity.creaking.activate";
const DEACTIVATE_SOUND: &str = "minecraft:entity.creaking.deactivate";
const FREEZE_SOUND: &str = "minecraft:entity.creaking.freeze";
const UNFREEZE_SOUND: &str = "minecraft:entity.creaking.unfreeze";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreakingFacts {
    pub home_pos: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreakingState {
    pub can_move: bool,
    pub active: bool,
}

impl Default for CreakingState {
    fn default() -> Self {
        Self {
            can_move: true,
            active: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreakingAction {
    SetState { can_move: bool, active: bool },
}

pub fn blocks_path(home: BlockPos, node: BlockPos, mob: BlockPos) -> bool {
    let distance = |a: BlockPos, b: BlockPos| {
        let (dx, dy, dz) = (
            f64::from(a.x - b.x),
            f64::from(a.y - b.y),
            f64::from(a.z - b.z),
        );
        dx * dx + dy * dy + dz * dz
    };
    let to_node = distance(home, node);
    PATH_HOME_RADIUS_SQR < to_node && distance(home, mob) <= to_node
}

fn is_see_through(state: rspigot_blocks::StateId) -> bool {
    state.kind().is_some_and(|kind| {
        let name = format!("{kind:?}");
        name.contains("Glass") || name == "IronBars"
    })
}

fn visual_shape(state: rspigot_blocks::StateId) -> &'static [Aabb] {
    if is_see_through(state) {
        &[]
    } else {
        state.collision_shape()
    }
}

struct DomainView<'a, V>(&'a V);

impl<V: BlockView> rspigot_domain::entity::BlockView for DomainView<'_, V> {
    fn block(&self, pos: BlockPos) -> Option<rspigot_blocks::StateId> {
        self.0.state(pos)
    }
}

fn has_visual_line_of_sight(world: &impl AiWorld, player: &Living, at: Vec3, y: f64) -> bool {
    let from = Vec3::new(player.position.x, player.eye_y(), player.position.z);
    let to = Vec3::new(at.x, y, at.z);
    if to.distance_squared(from).sqrt() > SIGHT_RANGE {
        return false;
    }
    rspigot_domain::entity::raycast::clip_blocks_with(&DomainView(world), from, to, visual_shape)
        .is_none()
}

pub fn is_looking_at_me(
    world: &impl AiWorld,
    me: Vec3,
    player: &Living,
    cone: f64,
    gaze_heights: &[f64],
) -> bool {
    let view = mth::view_vector(player.pitch, player.yaw).normalize();
    gaze_heights.iter().any(|height| {
        let toward = Vec3::new(
            me.x - player.position.x,
            height - player.eye_y(),
            me.z - player.position.z,
        );
        let direction = toward.normalize();
        let dot = view.x * direction.x + view.y * direction.y + view.z * direction.z;
        1.0 - cone < dot && has_visual_line_of_sight(world, player, me, *height)
    })
}

fn set_state<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, can_move: bool, active: bool) {
    let before = cx.ai.brain.creaking;
    cx.ai.brain.creaking = CreakingState { can_move, active };
    if before != cx.ai.brain.creaking {
        cx.ai.act(MobAction::Creaking(CreakingAction::SetState {
            can_move,
            active,
        }));
    }
}

fn sound<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, sound: &'static str) {
    cx.ai.act(MobAction::Sound {
        sound,
        volume: 1.0,
        pitch: 1.0,
    });
}

fn activate<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, player: EntityId) {
    cx.brain
        .memories
        .set(MemoryModuleType::AttackTarget, MemoryValue::Entity(player));
    sound(cx, ACTIVATE_SOUND);
    let can_move = cx.ai.brain.creaking.can_move;
    set_state(cx, can_move, true);
}

fn deactivate<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    cx.brain.memories.erase(MemoryModuleType::AttackTarget);
    sound(cx, DEACTIVATE_SOUND);
    let can_move = cx.ai.brain.creaking.can_move;
    set_state(cx, can_move, false);
}

pub fn check_can_move<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> bool {
    let players: Vec<EntityId> = cx
        .brain
        .memories
        .get(MemoryModuleType::NearestPlayers)
        .and_then(MemoryValue::as_entities)
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    let was_active = cx.ai.brain.creaking.active;
    if players.is_empty() {
        if was_active {
            deactivate(cx);
        }
        return true;
    }
    let mob = *cx.ai.mob;
    let mut any_attackable = false;
    for id in players {
        let Some(player) = cx.ai.world.living(id) else {
            continue;
        };
        if !targeting::mob_can_attack(&mob, cx.ai.world, &player) {
            continue;
        }
        any_attackable = true;
        if was_active && player.facts.gaze_disguise {
            continue;
        }
        let gaze_heights = [
            mob.eye_y(),
            mob.position.y + 0.5,
            (mob.eye_y() + mob.position.y) / 2.0,
        ];
        if is_looking_at_me(cx.ai.world, mob.position, &player, LOOK_CONE, &gaze_heights) {
            if was_active {
                return false;
            }
            if mob.position.distance_squared(player.position) < ACTIVATION_DISTANCE_SQR {
                activate(cx, player.id);
                return false;
            }
        }
    }
    if !any_attackable && was_active {
        deactivate(cx);
    }
    true
}

pub fn apply_can_move<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    let was = cx.ai.brain.creaking.can_move;
    let can_move = check_can_move(cx);
    let active = cx.ai.brain.creaking.active;
    if can_move != was {
        if can_move {
            sound(cx, UNFREEZE_SOUND);
        } else {
            cx.ai.stop_navigation();
            cx.ai.body.velocity = Vec3::ZERO;
            cx.ai.brain.movement.stop_in_place();
            sound(cx, FREEZE_SOUND);
        }
    }
    set_state(cx, can_move, active);
}

pub fn creaking_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::creaking_swim(0.8),
            MobControl::look_at_target_sink(45, 90),
            MobControl::move_to_target_sink(),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::when(
                MobPredicate::CreakingActive,
                TargetFinder::Memory(MemoryModuleType::NearestVisibleAttackablePlayer),
            ))),
            MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(30, 60)),
            )),
            MobControl::Gate(Gate::run_one(vec![
                (
                    MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.3))),
                    2,
                ),
                (
                    MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                        SetWalkTargetFromLookTarget::new(0.3, 3),
                    )),
                    2,
                ),
                (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
            ])),
        ],
    )
}

fn fight_activity() -> ActivityData<MobControl> {
    ActivityData::create_when(
        Activity::Fight,
        10,
        vec![
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
            )),
            MobControl::one_shot(MobTrigger::MeleeAttack(MeleeAttack::when(
                MobPredicate::CreakingCanMove,
                ATTACK_COOLDOWN,
            ))),
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::new(
                    StopAttackCondition::NotInNearestAttackablePlayers,
                    TargetErased::Nothing,
                    true,
                ),
            )),
        ],
        vec![(MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent)],
    )
}

pub fn creaking_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity(), fight_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>, can_move: bool) {
    if can_move {
        brain
            .state_mut()
            .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
    } else {
        brain.state_mut().use_default_activity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_beyond_thirty_two_blocks_of_the_heart_is_blocked_unless_the_creaking_is_closer() {
        let home = BlockPos::new(0, 64, 0);
        let near = BlockPos::new(10, 64, 0);
        let far = BlockPos::new(40, 64, 0);
        assert!(!blocks_path(home, near, near));
        assert!(blocks_path(home, far, near));
        assert!(blocks_path(home, far, far));
        assert!(!blocks_path(home, far, BlockPos::new(60, 64, 0)));
        assert!(!blocks_path(home, BlockPos::new(32, 64, 0), near));
        assert!(blocks_path(home, BlockPos::new(33, 64, 0), near));
    }

    #[test]
    fn glass_and_bars_let_the_gaze_through_and_stone_does_not() {
        use rspigot_blocks::BlockKind;
        assert!(is_see_through(BlockKind::Glass.default_state()));
        assert!(is_see_through(BlockKind::TintedGlass.default_state()));
        assert!(is_see_through(BlockKind::GlassPane.default_state()));
        assert!(is_see_through(BlockKind::IronBars.default_state()));
        assert!(!is_see_through(BlockKind::Stone.default_state()));
        assert!(visual_shape(BlockKind::Glass.default_state()).is_empty());
        assert!(!visual_shape(BlockKind::Stone.default_state()).is_empty());
    }
}
