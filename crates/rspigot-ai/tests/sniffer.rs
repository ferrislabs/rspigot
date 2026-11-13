use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, Facts, GlobalPos,
    GoalSettings, Living, Memories, MemoryModuleType, MemoryValue, MobAction, MobAi, MobBehavior,
    MobBody, MobControl, MobView, SnifferAction, SnifferFeelingHappy, SnifferScenting,
    SnifferSearching, SnifferSniffing, Status, TargetKind, WalkTarget,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::sniffer::State;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;
const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);
const HEAD_BELOW: BlockPos = BlockPos::new(0, 63, 2);
const OVERWORLD: &str = "minecraft:overworld";

struct Field {
    players: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
    floor: StateId,
}

impl Field {
    fn meadow() -> Self {
        Self {
            players: Vec::new(),
            edits: HashMap::new(),
            floor: BlockKind::GrassBlock.default_state(),
        }
    }

    fn stony() -> Self {
        Self {
            floor: BlockKind::Stone.default_state(),
            ..Self::meadow()
        }
    }

    fn put(&mut self, x: i32, y: i32, z: i32, state: StateId) {
        self.edits.insert(BlockPos::new(x, y, z), state);
    }
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.edits.get(&pos) {
            return Some(*state);
        }
        Some(if (55..=63).contains(&pos.y) {
            self.floor
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for Field {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players.iter().find(|living| living.id == id).copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        self.players
            .iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        NOW
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }
}

fn player(id: i32, position: Vec3) -> Living {
    let size = EntityKind::Player.dimensions();
    Living {
        id: EntityId(id),
        player: true,
        kind: EntityKind::Player,
        position,
        eye_height: f64::from(size.eye_height),
        bounds: Aabb::of_size(position, size.width, size.height),
        alive: true,
        spectator: false,
        creative: false,
        invulnerable: false,
        discrete: false,
        invisible: false,
        armor_cover: 0.0,
        main_hand: None,
        off_hand: None,
        yaw: 0.0,
        pitch: 0.0,
        in_water: false,
        baby: false,
        age: 0,
        in_love: false,
        panicking: false,
        has_target: false,
        scared: false,
        facts: Facts {
            uuid: Uuid::from_u128(id as u128 + 900),
            ..Facts::default()
        },
    }
}

fn view(position: Vec3) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Sniffer);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    MobView::of(&base, &state)
}

struct Bench {
    mob: MobView,
    world: Field,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(world: Field, seed: i64) -> Self {
        let mob = view(HOME);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(EntityKind::Sniffer, seed),
            state,
        }
    }

    fn meadow() -> Self {
        Self::new(Field::meadow(), 7)
    }

    fn with<R>(&mut self, run: impl FnOnce(&mut BehaviorCx<'_, '_, Field>) -> R) -> R {
        let mut ai = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.controls,
            body: &mut self.body,
        };
        let mut cx = BehaviorCx {
            ai: &mut ai,
            brain: &mut self.state,
        };
        run(&mut cx)
    }

    fn start(&mut self, control: &mut MobControl, now: i64) -> bool {
        self.with(|cx| control.try_start(cx, now))
    }

    fn tick(&mut self, control: &mut MobControl, now: i64) {
        self.with(|cx| control.tick_or_stop(cx, now));
    }

    fn memories(&self) -> &Memories {
        &self.state.memories
    }

    fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.state.memories.set(ty, value);
    }

    fn has(&self, ty: MemoryModuleType) -> bool {
        self.state.memories.has_value(ty)
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn explored(&self) -> Vec<BlockPos> {
        match self
            .state
            .memories
            .get(MemoryModuleType::SnifferExploredPositions)
        {
            Some(MemoryValue::GlobalPosList(positions)) => {
                positions.iter().map(|position| position.pos).collect()
            }
            _ => Vec::new(),
        }
    }

    fn state_is(&self, state: State) -> bool {
        self.body.sniffer.state == state
    }
}

fn describe(control: &MobControl) -> String {
    format!("{control:?}")
}

fn activity(kind: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(EntityKind::Sniffer, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(kind)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn from_activity(kind: Activity, name: &str) -> MobControl {
    let ai = MobAi::new(EntityKind::Sniffer, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(kind)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(name))
        .expect("the behavior")
}

fn searching() -> MobControl {
    MobControl::behavior_for(MobBehavior::SnifferSearching(SnifferSearching), 600, 600)
}

fn sniffing() -> MobControl {
    MobControl::behavior_for(MobBehavior::SnifferSniffing(SnifferSniffing), 40, 80)
}

fn digging() -> MobControl {
    from_activity(Activity::Dig, "SnifferDigging(")
}

fn finished_digging() -> MobControl {
    from_activity(Activity::Dig, "SnifferFinishedDigging")
}

fn scenting() -> MobControl {
    MobControl::behavior_for(MobBehavior::SnifferScenting(SnifferScenting), 40, 80)
}

fn happy() -> MobControl {
    MobControl::behavior_for(
        MobBehavior::SnifferFeelingHappy(SnifferFeelingHappy),
        40,
        100,
    )
}

fn block(x: i32, y: i32, z: i32) -> BlockPos {
    BlockPos::new(x, y, z)
}

fn sounds(actions: &[MobAction]) -> Vec<(&'static str, f32)> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Sound { sound, pitch, .. } => Some((*sound, *pitch)),
            _ => None,
        })
        .collect()
}

fn time_to_live(bench: &Bench, ty: MemoryModuleType) -> Option<i64> {
    bench.state.memories.time_until_expiry(ty)
}

#[test]
fn the_core_activity_swims_panics_moves_and_counts_the_temptation_cooldown() {
    let core = activity(Activity::Core);
    assert_eq!(core.len(), 4);
    assert_eq!(core[0].0, 0);
    assert!(core[0].1.contains("Swim") && core[0].1.contains("chance: 0.8"));
    assert_eq!(core[1].0, 1);
    assert!(core[1].1.contains("SnifferPanic") && core[1].1.contains("speed_multiplier: 2.0"));
    assert!(core[1].1.contains("min_duration: 100") && core[1].1.contains("max_duration: 120"));
    assert!(core[2].1.contains("MoveToTargetSink"));
    assert!(core[2].1.contains("min_duration: 500") && core[2].1.contains("max_duration: 700"));
    assert!(core[3].1.contains("CountDownCooldownTicks"));
    assert!(core[3].1.contains("TemptationCooldownTicks"));
}

#[test]
fn the_idle_activity_follows_the_vanilla_priorities_and_timings() {
    let idle = activity(Activity::Idle);
    let priorities: Vec<i32> = idle.iter().map(|(priority, _)| *priority).collect();
    assert_eq!(priorities, vec![0, 1, 2, 3, 4]);
    assert!(idle[0].1.contains("SnifferMakeLove") && idle[0].1.contains("min_duration: 110"));
    assert!(idle[1].1.contains("SnifferFollowTemptation"));
    assert!(idle[1].1.contains("speed_modifier: 1.25"));
    assert!(idle[1].1.contains("baby: 2.5") && idle[1].1.contains("adult: 3.5"));
    assert!(idle[2].1.contains("LookAtTargetSink"));
    assert!(idle[2].1.contains("min_duration: 45") && idle[2].1.contains("max_duration: 90"));
    assert!(idle[3].1.contains("SnifferFeelingHappy"));
    assert!(idle[3].1.contains("min_duration: 40") && idle[3].1.contains("max_duration: 100"));
    let gate = &idle[4].1;
    let order: Vec<usize> = [
        "SetWalkTargetFromLookTarget",
        "SnifferScenting",
        "SnifferSniffing",
        "SetEntityLookTarget",
        "RandomStroll",
        "DoNothing",
    ]
    .iter()
    .map(|name| gate.find(name).unwrap_or_else(|| panic!("{name}")))
    .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    assert!(gate.contains("min_duration: 5, max_duration: 20"));
}

#[test]
fn the_sniff_and_dig_activities_hold_the_search_and_the_dig_pair() {
    let sniff = activity(Activity::Sniff);
    assert_eq!(sniff.len(), 1);
    assert_eq!(sniff[0].0, 0);
    assert!(sniff[0].1.contains("SnifferSearching"));
    assert!(sniff[0].1.contains("min_duration: 600") && sniff[0].1.contains("max_duration: 600"));
    let dig = activity(Activity::Dig);
    assert_eq!(dig.iter().map(|(p, _)| *p).collect::<Vec<_>>(), vec![0, 0]);
    assert!(dig[0].1.contains("SnifferDigging(") && dig[0].1.contains("min_duration: 160"));
    assert!(dig[0].1.contains("max_duration: 180"));
    assert!(dig[1].1.contains("SnifferFinishedDigging"));
    assert!(dig[1].1.contains("min_duration: 40") && dig[1].1.contains("max_duration: 40"));
}

#[test]
fn a_sniffer_chooses_dig_then_sniff_then_idle_by_its_memories() {
    let mut ai = MobAi::new(EntityKind::Sniffer, 3, GoalSettings::default());
    let mob = view(HOME);
    let world = Field::meadow();
    let chosen = |ai: &MobAi| {
        ai.behavior_brain()
            .and_then(|brain| brain.state().active_non_core_activity())
    };
    let step = |ai: &mut MobAi| {
        let mut body = MobBody::of(&mob);
        ai.think(&mob, &world, &mut body);
    };
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Idle));
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(
            MemoryModuleType::SnifferSniffingTarget,
            MemoryValue::Block(block(3, 63, 3)),
        );
        memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::block(block(3, 63, 3), 1.25, 0)),
        );
    }
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Sniff));
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.erase(MemoryModuleType::WalkTarget);
        memories.erase(MemoryModuleType::SnifferSniffingTarget);
        memories.set(MemoryModuleType::SnifferDigging, MemoryValue::Bool(true));
    }
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Dig));
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Idle));
}

#[test]
fn searching_needs_a_walk_target_a_sniffing_target_and_a_calm_sniffer() {
    let mut bench = Bench::meadow();
    let mut control = searching();
    assert!(!bench.start(&mut control, NOW));
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(block(4, 63, 4)),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(block(4, 63, 4), 1.25, 0)),
    );
    bench.mob.in_love = true;
    assert!(!bench.start(&mut control, NOW));
    bench.mob.in_love = false;
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert!(!bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::IsPanicking);
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::Searching));
}

#[test]
fn searching_ends_when_the_walk_target_is_no_longer_the_sniffing_target() {
    let mut bench = Bench::meadow();
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Running);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(block(9, 63, 9), 1.0, 0)),
    );
    bench.tick(&mut control, NOW + 2);
    assert_eq!(control.status(), Status::Stopped);
}

#[test]
fn searching_goes_back_to_idling_when_the_sniffer_can_no_longer_sniff() {
    let mut bench = Bench::meadow();
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.mob.on_ground = false;
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
}

#[test]
fn searching_arrives_over_a_diggable_block_and_decides_to_dig() {
    let mut bench = Bench::meadow();
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 601);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.has(MemoryModuleType::SnifferDigging));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
}

#[test]
fn searching_over_stone_does_not_dig_but_still_forgets_the_target() {
    let mut bench = Bench::new(Field::stony(), 7);
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 601);
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
}

fn mark_explored(bench: &mut Bench, positions: Vec<BlockPos>) {
    let list = positions
        .into_iter()
        .map(|pos| GlobalPos {
            dimension: OVERWORLD.to_owned(),
            pos,
        })
        .collect();
    bench.set(
        MemoryModuleType::SnifferExploredPositions,
        MemoryValue::GlobalPosList(list),
    );
}

#[test]
fn a_block_that_was_already_dug_is_never_dug_again() {
    let mut bench = Bench::meadow();
    mark_explored(&mut bench, vec![HEAD_BELOW]);
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 601);
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
}

#[test]
fn a_position_explored_in_another_dimension_does_not_count() {
    let mut bench = Bench::meadow();
    bench.set(
        MemoryModuleType::SnifferExploredPositions,
        MemoryValue::GlobalPosList(vec![GlobalPos {
            dimension: "minecraft:the_nether".to_owned(),
            pos: HEAD_BELOW,
        }]),
    );
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut control = searching();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 601);
    assert!(bench.has(MemoryModuleType::SnifferDigging));
}

#[test]
fn a_calf_never_digs_and_never_starts_sniffing() {
    let mut bench = Bench::meadow();
    bench.mob.baby = true;
    let mut control = sniffing();
    assert!(!bench.start(&mut control, NOW));
    let target = block(4, 63, 4);
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(target),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(target, 1.25, 0)),
    );
    let mut search = searching();
    assert!(bench.start(&mut search, NOW));
    bench.tick(&mut search, NOW + 601);
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
}

#[test]
fn sniffing_needs_no_walk_target_no_sniffing_target_and_no_cooldown() {
    let mut control = sniffing();
    for blocker in [
        MemoryModuleType::WalkTarget,
        MemoryModuleType::SnifferSniffingTarget,
        MemoryModuleType::SniffCooldown,
    ] {
        let mut bench = Bench::meadow();
        let value = match blocker {
            MemoryModuleType::WalkTarget => {
                MemoryValue::WalkTarget(WalkTarget::block(block(2, 63, 2), 1.0, 0))
            }
            MemoryModuleType::SnifferSniffingTarget => MemoryValue::Block(block(2, 63, 2)),
            _ => MemoryValue::Unit,
        };
        bench.set(blocker, value);
        assert!(!bench.start(&mut control, NOW), "{blocker:?}");
    }
    let mut bench = Bench::meadow();
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::Sniffing));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.sniffer.sniffing", 1.0)]
    );
}

#[test]
fn a_sniffer_that_finished_sniffing_picks_a_diggable_spot_and_walks_there() {
    let mut bench = Bench::meadow();
    let mut control = sniffing();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 81);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
    let target = bench
        .memories()
        .get(MemoryModuleType::SnifferSniffingTarget)
        .and_then(MemoryValue::as_block)
        .expect("a sniffing target");
    assert_eq!(target.y, 63);
    let walk = bench
        .memories()
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .expect("a walk target");
    assert_eq!(walk.speed_modifier, 1.25);
    assert_eq!(walk.close_enough_dist, 0);
    assert_eq!(
        walk.target.current_block_position(&bench.world),
        Some(target)
    );
    let distance = target.x.abs().max(target.z.abs());
    assert!(distance >= 1, "{target:?}");
}

#[test]
fn a_sniffer_interrupted_while_sniffing_digs_nowhere() {
    let mut bench = Bench::meadow();
    let mut control = sniffing();
    assert!(bench.start(&mut control, NOW));
    bench.mob.in_love = true;
    bench.tick(&mut control, NOW + 5);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
}

#[test]
fn nothing_diggable_nearby_means_no_target() {
    let mut bench = Bench::new(Field::stony(), 5);
    let mut control = sniffing();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 81);
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
}

fn ready_to_dig() -> Bench {
    let mut bench = Bench::meadow();
    bench.set(MemoryModuleType::SnifferDigging, MemoryValue::Bool(true));
    bench
}

#[test]
fn digging_starts_the_dig_animation_and_asks_for_the_seed_timer() {
    let mut bench = ready_to_dig();
    let mut control = digging();
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::Digging));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Sniffer(SnifferAction::StartDigging)]
    );
}

#[test]
fn digging_needs_the_digging_memory_and_no_cooldown_walk_target_or_panic() {
    let mut control = digging();
    let mut bench = Bench::meadow();
    assert!(!bench.start(&mut control, NOW));
    for blocker in [
        MemoryModuleType::SniffCooldown,
        MemoryModuleType::IsPanicking,
        MemoryModuleType::WalkTarget,
    ] {
        let mut bench = ready_to_dig();
        let value = match blocker {
            MemoryModuleType::SniffCooldown => MemoryValue::Unit,
            MemoryModuleType::IsPanicking => MemoryValue::Bool(true),
            _ => MemoryValue::WalkTarget(WalkTarget::block(block(2, 63, 2), 1.0, 0)),
        };
        bench.set(blocker, value);
        assert!(!bench.start(&mut control, NOW), "{blocker:?}");
    }
}

#[test]
fn a_sniffer_in_love_will_not_start_digging() {
    let mut bench = ready_to_dig();
    bench.mob.in_love = true;
    assert!(!bench.start(&mut digging(), NOW));
}

#[test]
fn a_dig_that_runs_its_course_sets_the_long_sniff_cooldown_and_keeps_digging() {
    let mut bench = ready_to_dig();
    let mut control = digging();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 181);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.has(MemoryModuleType::SniffCooldown));
    assert!(bench.has(MemoryModuleType::SnifferDigging));
    let left = time_to_live(&bench, MemoryModuleType::SniffCooldown).expect("a timer");
    assert_eq!(left, 9600);
}

#[test]
fn a_dig_interrupted_early_forgets_everything_and_goes_back_to_idling() {
    let mut bench = ready_to_dig();
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(block(1, 63, 1)),
    );
    let mut control = digging();
    assert!(bench.start(&mut control, NOW));
    bench.mob.in_love = true;
    bench.tick(&mut control, NOW + 10);
    assert_eq!(control.status(), Status::Stopped);
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
    assert!(!bench.has(MemoryModuleType::SniffCooldown));
    assert!(bench.state_is(State::Idling));
}

#[test]
fn a_dig_in_the_water_or_on_a_fully_dug_spot_is_interrupted() {
    for explored in [false, true] {
        let mut bench = ready_to_dig();
        let mut control = digging();
        assert!(bench.start(&mut control, NOW));
        if explored {
            mark_explored(&mut bench, vec![HEAD_BELOW]);
        } else {
            bench.mob.fluid.in_water = true;
        }
        bench.tick(&mut control, NOW + 10);
        assert_eq!(control.status(), Status::Stopped, "{explored}");
        assert!(!bench.has(MemoryModuleType::SnifferDigging));
    }
}

fn dug_out() -> Bench {
    let mut bench = ready_to_dig();
    bench.set(MemoryModuleType::SniffCooldown, MemoryValue::Unit);
    bench
}

#[test]
fn finishing_a_dig_needs_the_cooldown_and_rises_with_the_stop_sound() {
    let mut control = finished_digging();
    let mut bench = ready_to_dig();
    assert!(!bench.start(&mut control, NOW));
    let mut bench = dug_out();
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::Rising));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.sniffer.digging_stop", 1.0)]
    );
}

#[test]
fn a_finished_dig_remembers_the_spot_and_makes_the_sniffer_happy() {
    let mut bench = dug_out();
    let mut control = finished_digging();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 41);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
    assert_eq!(bench.explored(), vec![block(0, 63, 0)]);
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(bench.has(MemoryModuleType::SnifferHappy));
}

#[test]
fn a_rise_cut_short_does_not_remember_the_spot_but_still_ends_happy() {
    let mut bench = dug_out();
    let mut control = finished_digging();
    assert!(bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::SnifferDigging);
    bench.tick(&mut control, NOW + 3);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.explored().is_empty());
    assert!(bench.has(MemoryModuleType::SnifferHappy));
}

#[test]
fn the_explored_list_keeps_the_twenty_latest_spots_and_the_new_one_in_front() {
    let mut bench = dug_out();
    mark_explored(
        &mut bench,
        (0..25).map(|i| block(100 + i, 63, 100)).collect(),
    );
    let mut control = finished_digging();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 41);
    let explored = bench.explored();
    assert_eq!(explored.len(), 21);
    assert_eq!(explored[0], block(0, 63, 0));
    assert_eq!(explored[1], block(100, 63, 100));
    assert_eq!(explored[20], block(119, 63, 100));
}

#[test]
fn a_calm_sniffer_may_scent_unless_tempted_or_busy() {
    let mut control = scenting();
    let mut bench = Bench::meadow();
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::Scenting));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.sniffer.scenting", 1.0)]
    );
    for blocker in [
        MemoryModuleType::SnifferDigging,
        MemoryModuleType::SnifferSniffingTarget,
        MemoryModuleType::SnifferHappy,
        MemoryModuleType::BreedTarget,
        MemoryModuleType::IsPanicking,
        MemoryModuleType::TemptingPlayer,
    ] {
        let mut bench = Bench::meadow();
        let value = match blocker {
            MemoryModuleType::SnifferSniffingTarget => MemoryValue::Block(block(1, 63, 1)),
            MemoryModuleType::BreedTarget | MemoryModuleType::TemptingPlayer => {
                MemoryValue::Entity(EntityId(5))
            }
            _ => MemoryValue::Bool(true),
        };
        bench.set(blocker, value);
        let mut control = scenting();
        assert!(!bench.start(&mut control, NOW), "{blocker:?}");
    }
}

#[test]
fn a_calf_scents_with_a_higher_pitch() {
    let mut bench = Bench::meadow();
    bench.mob.baby = true;
    assert!(bench.start(&mut scenting(), NOW));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.sniffer.scenting", 1.3)]
    );
}

#[test]
fn scenting_lasts_until_it_times_out_and_then_idles_again() {
    let mut bench = Bench::meadow();
    let mut control = scenting();
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 40);
    assert_eq!(control.status(), Status::Running);
    bench.tick(&mut control, NOW + 81);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
}

#[test]
fn feeling_happy_plays_the_happy_sound_and_clears_the_memory_when_done() {
    let mut control = happy();
    let mut bench = Bench::meadow();
    assert!(!bench.start(&mut control, NOW));
    bench.set(MemoryModuleType::SnifferHappy, MemoryValue::Bool(true));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.state_is(State::FeelingHappy));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.sniffer.happy", 1.0)]
    );
    bench.tick(&mut control, NOW + 101);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.state_is(State::Idling));
    assert!(!bench.has(MemoryModuleType::SnifferHappy));
}

#[test]
fn being_tempted_resets_any_sniffing_in_progress() {
    let mut bench = ready_to_dig();
    bench.body.sniffer.state = State::Digging;
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(block(1, 63, 1)),
    );
    bench
        .world
        .players
        .push(player(5, Vec3::new(4.5, 64.0, 0.5)));
    bench.set(
        MemoryModuleType::TemptingPlayer,
        MemoryValue::Entity(EntityId(5)),
    );
    let mut control = from_activity(Activity::Idle, "SnifferFollowTemptation");
    assert!(bench.start(&mut control, NOW));
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
    assert!(bench.state_is(State::Idling));
}

#[test]
fn panicking_resets_any_sniffing_in_progress() {
    let mut bench = ready_to_dig();
    bench.body.sniffer.state = State::Searching;
    bench.set(
        MemoryModuleType::SnifferSniffingTarget,
        MemoryValue::Block(block(1, 63, 1)),
    );
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    let mut control = from_activity(Activity::Core, "SnifferPanic");
    assert!(bench.start(&mut control, NOW));
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(!bench.has(MemoryModuleType::SnifferSniffingTarget));
    assert!(bench.state_is(State::Idling));
}

#[test]
fn breeding_resets_any_sniffing_in_progress() {
    let mut bench = ready_to_dig();
    bench.body.sniffer.state = State::Digging;
    bench.mob.in_love = true;
    let mate = EntityId(8);
    bench.state.memories.set(
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryValue::Visible(rspigot_ai::NearestVisibleLivingEntities::new(vec![mate])),
    );
    let mut partner = player(8, Vec3::new(2.5, 64.0, 0.5));
    partner.kind = EntityKind::Sniffer;
    partner.player = false;
    partner.in_love = true;
    bench.world.players.push(partner);
    let mut control = from_activity(Activity::Idle, "SnifferMakeLove");
    assert!(bench.start(&mut control, NOW));
    assert!(!bench.has(MemoryModuleType::SnifferDigging));
    assert!(bench.state_is(State::Idling));
    assert!(bench.has(MemoryModuleType::BreedTarget));
}

#[test]
fn the_head_block_follows_where_the_sniffer_looks() {
    for (yaw, expected) in [
        (0.0, block(0, 63, 2)),
        (90.0, block(-2, 63, 0)),
        (180.0, block(0, 63, -2)),
        (270.0, block(2, 63, 0)),
    ] {
        let mut bench = ready_to_dig();
        bench.mob.yaw = yaw;
        bench
            .world
            .put(expected.x, 63, expected.z, BlockKind::Stone.default_state());
        let mut control = digging();
        assert!(bench.start(&mut control, NOW));
        bench.tick(&mut control, NOW + 1);
        assert_eq!(control.status(), Status::Stopped, "{yaw}");
    }
}

#[test]
fn only_the_explored_positions_are_persisted_and_they_round_trip_in_the_vanilla_layout() {
    let mut ai = MobAi::new(EntityKind::Sniffer, 1, GoalSettings::default());
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(
            MemoryModuleType::SnifferExploredPositions,
            MemoryValue::GlobalPosList(vec![
                GlobalPos {
                    dimension: OVERWORLD.to_owned(),
                    pos: block(12, 63, -7),
                },
                GlobalPos {
                    dimension: "minecraft:the_nether".to_owned(),
                    pos: block(1, 70, 2),
                },
            ]),
        );
        memories.set(MemoryModuleType::SnifferDigging, MemoryValue::Bool(true));
        memories.set(MemoryModuleType::SnifferHappy, MemoryValue::Bool(true));
        memories.set(MemoryModuleType::SniffCooldown, MemoryValue::Unit);
    }
    let saved = ai.save_behavior_brain().expect("a brain");
    let memories = saved.get_compound("memories").expect("memories");
    assert!(memories.get_compound("minecraft:sniffer_digging").is_none());
    assert!(memories.get_compound("minecraft:sniffer_happy").is_none());
    assert!(memories.get_compound("minecraft:sniff_cooldown").is_some());
    let listed = memories
        .get_compound("minecraft:sniffer_explored_positions")
        .and_then(|entry| entry.get_list("value"))
        .expect("a list");
    assert_eq!(listed.len(), 2);
    let first = listed[0].as_compound().expect("a compound");
    assert_eq!(first.get_str("dimension"), Some(OVERWORLD));
    assert_eq!(first.get_int_array("pos"), Some(&[12, 63, -7][..]));
    let mut loaded = MobAi::new(EntityKind::Sniffer, 2, GoalSettings::default());
    loaded.load_behavior_brain(&saved);
    let restored = loaded.behavior_brain().unwrap().memories();
    match restored.get(MemoryModuleType::SnifferExploredPositions) {
        Some(MemoryValue::GlobalPosList(positions)) => {
            assert_eq!(positions.len(), 2);
            assert_eq!(positions[1].dimension, "minecraft:the_nether");
            assert_eq!(positions[1].pos, block(1, 70, 2));
        }
        other => panic!("{other:?}"),
    }
    assert!(!restored.has_value(MemoryModuleType::SnifferDigging));
}
