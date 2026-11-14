use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiContext, AiWorld, Brain, Facts, Flag, Flags, Goal, GoalSettings, Living, MergeWithStoneGoal,
    MobAction, MobAi, MobBody, MobGoal, MobRandom, MobView, RandomSource, SilverfishAction,
    SilverfishGoal, TargetKind, WakeUpFriendsGoal,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const PLAYER: EntityId = EntityId(100);
const ORIGIN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    time: Cell<i64>,
    griefing: bool,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(0),
            griefing: true,
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if (60..64).contains(&pos.y) {
            BlockKind::Dirt.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for World {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.id == id)
            .copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        let pool = if kind == TargetKind::Player {
            &self.players
        } else {
            &self.others
        };
        pool.iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        self.time.get()
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn mob_griefing(&self) -> bool {
        self.griefing
    }
}

fn living(id: EntityId, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id,
        player: kind == EntityKind::Player,
        kind,
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
        facts: Facts::default(),
    }
}

fn silverfish(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Silverfish);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn step(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    base.tick_count += 1;
    world.time.set(world.time.get() + 1);
    let mut view = MobView::of(base, state);
    adjust(&mut view);
    let mut body = MobBody::of(&view);
    ai.think(&view, world, &mut body);
    ai.take_actions()
}

fn friends(ai: &mut MobAi) -> &mut WakeUpFriendsGoal {
    ai.goals_mut()
        .goals_mut()
        .find_map(|goal| match goal {
            MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(friends)) => Some(friends),
            _ => None,
        })
        .expect("the wake up goal")
}

fn host_state() -> StateId {
    BlockKind::Stone.default_state()
}

fn seed_rolling(face: i32) -> i64 {
    (0..100_000)
        .find(|seed| {
            let mut random = MobRandom::new(*seed);
            random.next_int(5) == 0 && random.next_int(6) == face
        })
        .expect("a seed with the wanted rolls")
}

fn seed_where(rolls: &[bool]) -> i64 {
    (0..100_000)
        .find(|seed| {
            let mut random = MobRandom::new(*seed);
            rolls
                .iter()
                .all(|wanted| (random.next_int(5) == 0) == *wanted)
        })
        .expect("a seed with the wanted rolls")
}

fn after(seed: i64, draws: usize, bound: i32) -> MobRandom {
    let mut random = MobRandom::new(seed);
    for _ in 0..draws {
        random.next_int(bound);
    }
    random
}

struct Rig {
    mob: MobView,
    brain: Brain,
    body: MobBody,
}

impl Rig {
    fn new(seed: i64) -> Self {
        let (base, state) = silverfish(ORIGIN);
        let mob = MobView::of(&base, &state);
        let body = MobBody::of(&mob);
        Self {
            mob,
            brain: Brain::new(EntityKind::Silverfish, seed),
            body,
        }
    }

    fn context<'a>(&'a mut self, world: &'a World) -> AiContext<'a, World> {
        AiContext {
            mob: &self.mob,
            world,
            brain: &mut self.brain,
            body: &mut self.body,
        }
    }
}

fn merge_goal() -> MobGoal {
    MobGoal::Silverfish(SilverfishGoal::MergeWithStone(MergeWithStoneGoal::default()))
}

fn merge_state(goal: &MobGoal) -> bool {
    match goal {
        MobGoal::Silverfish(SilverfishGoal::MergeWithStone(goal)) => goal.do_merge(),
        _ => panic!("not the merge goal"),
    }
}

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::ClimbOnTopOfPowderSnow(_) => "climb_powder_snow",
        MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(_)) => "wake_up_friends",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::Silverfish(SilverfishGoal::MergeWithStone(_)) => "merge_with_stone",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::NearestAttackableTarget(_) => "nearest_target",
        _ => "other",
    }
}

#[test]
fn goals_follow_vanilla_priorities() {
    let ai = MobAi::new(EntityKind::Silverfish, 1, GoalSettings::default());
    let goals: Vec<_> = ai
        .goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect();
    assert_eq!(
        goals,
        vec![
            (1, "float"),
            (1, "climb_powder_snow"),
            (3, "wake_up_friends"),
            (4, "melee"),
            (5, "merge_with_stone"),
        ]
    );
    let targets: Vec<_> = ai
        .targets()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect();
    assert_eq!(targets, vec![(1, "hurt_by"), (2, "nearest_target")]);
    assert!(ai.can_float());
}

#[test]
fn goal_flags_match_vanilla() {
    let wake = MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(WakeUpFriendsGoal::default()));
    assert_eq!(Goal::<AiContext<'_, World>>::flags(&wake), Flags::NONE);
    assert_eq!(
        Goal::<AiContext<'_, World>>::flags(&merge_goal()),
        Flags::of(Flag::Move)
    );
    assert!(Goal::<AiContext<'_, World>>::is_interruptable(&wake));
    assert!(!Goal::<AiContext<'_, World>>::requires_update_every_tick(
        &wake
    ));
    assert!(!Goal::<AiContext<'_, World>>::requires_update_every_tick(
        &merge_goal()
    ));
}

#[test]
fn being_hurt_arms_the_wake_up_goal_once() {
    let mut goal = WakeUpFriendsGoal::default();
    assert_eq!(goal.look_for_friends(), 0);
    goal.notify_hurt();
    assert_eq!(goal.look_for_friends(), 10);
    let mut rig = Rig::new(1);
    let world = World::new();
    let mut wake = MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(goal));
    let mut context = rig.context(&world);
    for _ in 0..4 {
        Goal::tick(&mut wake, &mut context);
    }
    if let MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(goal)) = &mut wake {
        assert_eq!(goal.look_for_friends(), 6);
        goal.notify_hurt();
        assert_eq!(goal.look_for_friends(), 6);
    }
    assert!(Goal::can_use(&mut wake, &mut context));
    assert!(Goal::can_continue_to_use(&mut wake, &mut context));
    assert!(context.brain.actions.is_empty());
}

#[test]
fn an_idle_wake_up_goal_cannot_start() {
    let mut rig = Rig::new(1);
    let world = World::new();
    let mut wake = MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(WakeUpFriendsGoal::default()));
    assert!(!Goal::can_use(&mut wake, &mut rig.context(&world)));
}

#[test]
fn the_wake_up_scan_fires_after_twenty_game_ticks_from_the_block_position() {
    let mut ai = MobAi::new(EntityKind::Silverfish, 1, GoalSettings::default());
    let (mut base, state) = silverfish(Vec3::new(3.5, 64.0, -2.5));
    let world = World::new();
    friends(&mut ai).notify_hurt();
    let mut ticks = 0;
    let mut found = None;
    while found.is_none() && ticks < 40 {
        ticks += 1;
        found = step(&mut ai, &mut base, &state, &world, |_| {})
            .into_iter()
            .find_map(|action| match action {
                MobAction::Silverfish(SilverfishAction::WakeUpFriends(origin)) => Some(origin),
                _ => None,
            });
    }
    assert_eq!(found, Some(BlockPos::new(3, 64, -3)));
    assert!((19..=21).contains(&ticks), "{ticks}");
    assert!(friends(&mut ai).look_for_friends() <= 0);
}

#[test]
fn merging_targets_the_rolled_neighbour() {
    let faces: [(i32, (i32, i32, i32)); 6] = [
        (0, (0, -1, 0)),
        (1, (0, 1, 0)),
        (2, (0, 0, -1)),
        (3, (0, 0, 1)),
        (4, (-1, 0, 0)),
        (5, (1, 0, 0)),
    ];
    for (face, (dx, dy, dz)) in faces {
        let mut rig = Rig::new(seed_rolling(face));
        let mut world = World::new();
        let target = BlockPos::new(dx, 64 + dy, dz);
        world.blocks.insert(target, host_state());
        let mut goal = merge_goal();
        let mut context = rig.context(&world);
        assert!(Goal::can_use(&mut goal, &mut context), "face {face}");
        assert!(merge_state(&goal));
        assert!(!Goal::can_continue_to_use(&mut goal, &mut context));
        Goal::start(&mut goal, &mut context);
        assert_eq!(
            context.brain.actions,
            vec![MobAction::Silverfish(SilverfishAction::Merge(target))],
            "face {face}"
        );
    }
}

#[test]
fn the_neighbour_is_taken_from_half_a_block_above_the_feet() {
    let (base, state) = silverfish(Vec3::new(0.5, 63.6, 0.5));
    let mob = MobView::of(&base, &state);
    let mut brain = Brain::new(EntityKind::Silverfish, seed_rolling(5));
    let mut body = MobBody::of(&mob);
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(1, 64, 0), host_state());
    let mut context = AiContext {
        mob: &mob,
        world: &world,
        brain: &mut brain,
        body: &mut body,
    };
    let mut goal = merge_goal();
    assert!(Goal::can_use(&mut goal, &mut context));
}

#[test]
fn every_infestable_block_can_be_merged_into() {
    for host in [
        BlockKind::Stone,
        BlockKind::Cobblestone,
        BlockKind::StoneBricks,
        BlockKind::MossyStoneBricks,
        BlockKind::CrackedStoneBricks,
        BlockKind::ChiseledStoneBricks,
        BlockKind::Deepslate,
    ] {
        let mut rig = Rig::new(seed_rolling(5));
        let mut world = World::new();
        world
            .blocks
            .insert(BlockPos::new(1, 64, 0), host.default_state());
        let mut goal = merge_goal();
        assert!(
            Goal::can_use(&mut goal, &mut rig.context(&world)),
            "{host:?}"
        );
    }
}

#[test]
fn a_neighbour_that_cannot_host_falls_back_to_the_stroll_roll() {
    let seed = (0..100_000)
        .find(|seed| {
            let mut random = MobRandom::new(*seed);
            random.next_int(5) == 0 && random.next_int(6) == 5 && random.next_int(5) != 0
        })
        .expect("a seed");
    let mut rig = Rig::new(seed);
    let mut world = World::new();
    world
        .blocks
        .insert(BlockPos::new(1, 64, 0), BlockKind::Andesite.default_state());
    let mut goal = merge_goal();
    let mut context = rig.context(&world);
    assert!(!Goal::can_use(&mut goal, &mut context));
    assert!(!merge_state(&goal));
    let mut expected = after(seed, 3, 5);
    assert_eq!(context.brain.random.next_int(1000), expected.next_int(1000));
}

#[test]
fn without_mob_griefing_no_merge_is_rolled() {
    let seed = seed_where(&[false, false]);
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(1, 64, 0), host_state());
    world.griefing = false;
    let mut rig = Rig::new(seed);
    let mut goal = merge_goal();
    let mut context = rig.context(&world);
    assert!(!Goal::can_use(&mut goal, &mut context));
    assert!(!merge_state(&goal));
    let mut expected = after(seed, 1, 5);
    assert_eq!(context.brain.random.next_int(1000), expected.next_int(1000));
    world.griefing = true;
    let mut rig = Rig::new(seed);
    let mut goal = merge_goal();
    let mut context = rig.context(&world);
    assert!(!Goal::can_use(&mut goal, &mut context));
    let mut expected = after(seed, 2, 5);
    assert_eq!(context.brain.random.next_int(1000), expected.next_int(1000));
}

#[test]
fn a_target_or_a_path_in_progress_blocks_the_goal_without_rolling() {
    let seed = seed_rolling(5);
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(1, 64, 0), host_state());
    world.players = vec![living(
        PLAYER,
        EntityKind::Player,
        Vec3::new(4.5, 64.0, 0.5),
    )];
    let mut rig = Rig::new(seed);
    rig.brain.target = Some(PLAYER);
    let mut goal = merge_goal();
    let mut context = rig.context(&world);
    assert!(!Goal::can_use(&mut goal, &mut context));
    let mut fresh = MobRandom::new(seed);
    assert_eq!(context.brain.random.next_int(1000), fresh.next_int(1000));

    let mut rig = Rig::new(seed);
    let mut context = rig.context(&world);
    assert!(context.move_to_pos(Vec3::new(5.5, 64.0, 0.5), 1.0));
    assert!(!context.brain.navigation.is_done());
    let mut goal = merge_goal();
    assert!(!Goal::can_use(&mut goal, &mut context));
    let mut fresh = MobRandom::new(seed);
    assert_eq!(context.brain.random.next_int(1000), fresh.next_int(1000));
}

#[test]
fn the_stroll_branch_keeps_walking_until_the_path_is_done() {
    let mut rig = Rig::new(1);
    let world = World::new();
    let mut goal = merge_goal();
    let mut context = rig.context(&world);
    assert!(!Goal::can_continue_to_use(&mut goal, &mut context));
    assert!(context.move_to_pos(Vec3::new(5.5, 64.0, 0.5), 1.0));
    assert!(Goal::can_continue_to_use(&mut goal, &mut context));
    Goal::stop(&mut goal, &mut context);
    assert!(context.brain.navigation.is_done());
}

#[test]
fn a_silverfish_next_to_stone_eventually_asks_to_merge() {
    let mut ai = MobAi::new(EntityKind::Silverfish, 3, GoalSettings::default());
    let (mut base, state) = silverfish(ORIGIN);
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(0, 63, 0), host_state());
    world.blocks.insert(BlockPos::new(1, 64, 0), host_state());
    world.blocks.insert(BlockPos::new(-1, 64, 0), host_state());
    let mut merges = Vec::new();
    for _ in 0..600 {
        merges.extend(
            step(&mut ai, &mut base, &state, &world, |_| {})
                .into_iter()
                .filter_map(|action| match action {
                    MobAction::Silverfish(SilverfishAction::Merge(pos)) => Some(pos),
                    _ => None,
                }),
        );
    }
    assert!(!merges.is_empty());
    for pos in merges {
        assert!(
            [
                BlockPos::new(0, 63, 0),
                BlockPos::new(1, 64, 0),
                BlockPos::new(-1, 64, 0)
            ]
            .contains(&pos),
            "{pos:?}"
        );
    }
}

#[test]
fn a_silverfish_on_plain_ground_never_merges_but_wanders() {
    let mut ai = MobAi::new(EntityKind::Silverfish, 3, GoalSettings::default());
    let (mut base, state) = silverfish(ORIGIN);
    let world = World::new();
    let mut walked = false;
    for _ in 0..600 {
        let actions = step(&mut ai, &mut base, &state, &world, |_| {});
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::Silverfish(SilverfishAction::Merge(_))))
        );
        walked |= !ai.brain().navigation.is_done();
    }
    assert!(walked);
}

#[test]
fn a_nearby_player_becomes_the_target() {
    let mut ai = MobAi::new(EntityKind::Silverfish, 7, GoalSettings::default());
    let (mut base, state) = silverfish(ORIGIN);
    let mut world = World::new();
    world.players = vec![living(
        PLAYER,
        EntityKind::Player,
        Vec3::new(5.5, 64.0, 0.5),
    )];
    for _ in 0..80 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert_eq!(ai.target(), Some(PLAYER));
}

#[test]
fn a_hurt_silverfish_alerts_idle_silverfish_but_not_busy_ones() {
    let mut ai = MobAi::new(EntityKind::Silverfish, 7, GoalSettings::default());
    let (mut base, state) = silverfish(ORIGIN);
    let mut world = World::new();
    world.players = vec![living(
        PLAYER,
        EntityKind::Player,
        Vec3::new(5.5, 64.0, 0.5),
    )];
    let idle = living(
        EntityId(2),
        EntityKind::Silverfish,
        Vec3::new(2.5, 64.0, 2.5),
    );
    let mut busy = living(
        EntityId(3),
        EntityKind::Silverfish,
        Vec3::new(-2.5, 64.0, 2.5),
    );
    busy.has_target = true;
    let far = living(
        EntityId(4),
        EntityKind::Silverfish,
        Vec3::new(60.5, 64.0, 0.5),
    );
    let cow = living(EntityId(5), EntityKind::Cow, Vec3::new(1.5, 64.0, 1.5));
    world.others = vec![idle, busy, far, cow];
    let mut alerts = Vec::new();
    for _ in 0..8 {
        alerts.extend(
            step(&mut ai, &mut base, &state, &world, |view| {
                view.last_hurt_by_mob = Some(PLAYER);
                view.last_hurt_by_mob_at = 1;
            })
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Alert { mob, target } => Some((mob, target)),
                _ => None,
            }),
        );
    }
    assert_eq!(alerts, vec![(EntityId(2), PLAYER)]);
}

#[test]
fn silverfish_prefer_to_walk_above_infestable_blocks() {
    let mut world = World::new();
    let pos = BlockPos::new(0, 64, 0);
    world.blocks.insert(BlockPos::new(0, 63, 0), host_state());
    assert_eq!(
        rspigot_ai::walk_target_value(EntityKind::Silverfish, &world, pos),
        10.0
    );
    world.blocks.insert(
        BlockPos::new(0, 63, 0),
        BlockKind::InfestedStone.default_state(),
    );
    assert_eq!(
        rspigot_ai::walk_target_value(EntityKind::Silverfish, &world, pos),
        rspigot_ai::walk_target_value(EntityKind::Zombie, &world, pos)
    );
}

#[test]
fn giants_have_no_goals_and_prefer_lit_places() {
    let giant = MobAi::new(EntityKind::Giant, 1, GoalSettings::default());
    assert!(giant.goals().goals().is_empty());
    assert!(giant.targets().goals().is_empty());
    assert!(!giant.can_float());
    let world = World::new();
    let pos = BlockPos::new(0, 64, 0);
    let lit = rspigot_ai::walk_target_value(EntityKind::Giant, &world, pos);
    let monster = rspigot_ai::walk_target_value(EntityKind::Zombie, &world, pos);
    assert!(lit > 0.0);
    assert_eq!(lit, -monster);
}
