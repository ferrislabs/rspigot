use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, EndermanAction, EndermanGoal, Facts, GoalSettings, Living, MobAction, MobAi, MobBody,
    MobGoal, MobView, TargetKind, find_teleport, random_teleport, teleport_roll,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct Field {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    griefing: bool,
}

impl Field {
    fn flat() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            griefing: true,
        }
    }
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if (60..64).contains(&pos.y) {
            BlockKind::Stone.default_state()
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
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.id == id)
            .copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        self.others
            .iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        0
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn mob_griefing(&self) -> bool {
        self.griefing
    }
}

fn living(id: i32, kind: EntityKind, position: Vec3) -> Living {
    let (width, height) = (kind.dimensions().width, kind.dimensions().height);
    Living {
        id: EntityId(id),
        player: kind == EntityKind::Player,
        kind,
        position,
        eye_height: f64::from(kind.dimensions().eye_height),
        bounds: Aabb::of_size(position, width, height),
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

fn mob(kind: EntityKind, position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let living = LivingState::new(&mut base, 0.0);
    (base, living)
}

fn enderman_view(position: Vec3) -> MobView {
    let (base, state) = mob(EntityKind::Enderman, position);
    MobView::of(&base, &state)
}

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::NearestAttackableTarget(_) => "nearest_target",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::ClimbOnTopOfPowderSnow(_) => "climb_powder_snow",
        MobGoal::ResetUniversalAnger(_) => "reset_anger",
        MobGoal::Enderman(EndermanGoal::FreezeWhenLookedAt(_)) => "freeze",
        MobGoal::Enderman(EndermanGoal::LeaveBlock) => "leave",
        MobGoal::Enderman(EndermanGoal::TakeBlock) => "take",
        MobGoal::Enderman(EndermanGoal::LookForPlayer(_)) => "look_for_player",
        _ => "other",
    }
}

fn goals(ai: &MobAi) -> Vec<(i32, &'static str)> {
    ai.goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect()
}

fn targets(ai: &MobAi) -> Vec<(i32, &'static str)> {
    ai.targets()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect()
}

fn running(ai: &MobAi, name: &str) -> bool {
    ai.goals()
        .goals()
        .iter()
        .chain(ai.targets().goals())
        .any(|goal| goal.is_running() && label(goal.goal()) == name)
}

fn actions_over(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    field: &Field,
    ticks: i32,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        base.tick_count += 1;
        let mut view = MobView::of(base, state);
        adjust(&mut view);
        let mut body = MobBody::of(&view);
        ai.think(&view, field, &mut body);
        all.extend(ai.take_actions());
    }
    all
}

#[test]
fn enderman_goals_follow_vanilla_priorities() {
    let ai = MobAi::new(EntityKind::Enderman, 1, GoalSettings::default());
    assert_eq!(
        goals(&ai),
        vec![
            (0, "float"),
            (1, "freeze"),
            (2, "melee"),
            (7, "stroll"),
            (8, "look"),
            (8, "look_around"),
            (10, "leave"),
            (11, "take"),
        ]
    );
    assert_eq!(
        targets(&ai),
        vec![
            (1, "look_for_player"),
            (2, "hurt_by"),
            (3, "nearest_target"),
            (4, "reset_anger"),
        ]
    );
}

#[test]
fn endermite_goals_follow_vanilla_priorities() {
    let ai = MobAi::new(EntityKind::Endermite, 1, GoalSettings::default());
    assert_eq!(
        goals(&ai),
        vec![
            (1, "float"),
            (1, "climb_powder_snow"),
            (2, "melee"),
            (3, "stroll"),
            (7, "look"),
            (8, "look_around"),
        ]
    );
    assert_eq!(targets(&ai), vec![(1, "hurt_by"), (2, "nearest_target")]);
}

fn stared_scene(disguised: bool) -> (Field, EntityBase, LivingState) {
    let (base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 5.5));
    let mut player = living(100, EntityKind::Player, Vec3::new(0.5, 64.93, 0.5));
    player.facts.gaze_disguise = disguised;
    let mut field = Field::flat();
    field.players.push(player);
    (field, base, state)
}

#[test]
fn a_gaze_on_the_eyes_angers_it_after_the_aggro_delay() {
    let (field, mut base, state) = stared_scene(false);
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 12, |_| {});
    assert!(actions.contains(&MobAction::Enderman(EndermanAction::BeingStaredAt)));
    assert_eq!(ai.target(), Some(EntityId(100)));
    assert!(running(&ai, "freeze"));
}

#[test]
fn a_carved_pumpkin_hides_the_gaze() {
    let (field, mut base, state) = stared_scene(true);
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 40, |_| {});
    assert!(!actions.contains(&MobAction::Enderman(EndermanAction::BeingStaredAt)));
    assert_eq!(ai.target(), None);
}

#[test]
fn looking_away_does_not_anger_it() {
    let (mut field, mut base, state) = stared_scene(false);
    field.players[0].yaw = 180.0;
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 40, |_| {});
    assert!(!actions.contains(&MobAction::Enderman(EndermanAction::BeingStaredAt)));
    assert_eq!(ai.target(), None);
}

#[test]
fn a_wall_between_blocks_the_gaze() {
    let (mut field, mut base, state) = stared_scene(false);
    for y in 64..70 {
        field
            .blocks
            .insert(BlockPos::new(0, y, 3), BlockKind::Stone.default_state());
    }
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 40, |_| {});
    assert!(!actions.contains(&MobAction::Enderman(EndermanAction::BeingStaredAt)));
}

#[test]
fn angry_endermen_target_the_player_they_remember() {
    let (base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 5.5));
    let mut field = Field::flat();
    let mut player = living(100, EntityKind::Player, Vec3::new(0.5, 64.93, 0.5));
    player.yaw = 180.0;
    field.players.push(player);
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    let mut base = base;
    actions_over(&mut ai, &mut base, &state, &field, 12, |view| {
        view.angry = true;
        view.anger_target = Some(EntityId(100));
    });
    assert_eq!(ai.target(), Some(EntityId(100)));
}

#[test]
fn endermites_are_hunted_within_sight() {
    let (mut base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 0.5));
    let mut field = Field::flat();
    field
        .others
        .push(living(7, EntityKind::Endermite, Vec3::new(4.5, 64.0, 0.5)));
    let mut ai = MobAi::new(EntityKind::Enderman, 9, GoalSettings::default());
    actions_over(&mut ai, &mut base, &state, &field, 60, |_| {});
    assert_eq!(ai.target(), Some(EntityId(7)));
}

#[test]
fn it_takes_holdable_blocks_only_when_griefing_is_allowed() {
    let mut field = Field::flat();
    for x in 1..=3 {
        for z in -3..=3 {
            field
                .blocks
                .insert(BlockPos::new(x, 64, z), BlockKind::Dirt.default_state());
        }
    }
    let (mut base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Enderman, 11, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 600, |_| {});
    let taken = actions.iter().find_map(|action| match action {
        MobAction::Enderman(EndermanAction::TakeBlock { carried, .. }) => Some(*carried),
        _ => None,
    });
    assert_eq!(taken, Some(BlockKind::Dirt.default_state()));
    field.griefing = false;
    let mut ai = MobAi::new(EntityKind::Enderman, 11, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 600, |_| {});
    assert!(!actions.iter().any(|action| matches!(
        action,
        MobAction::Enderman(EndermanAction::TakeBlock { .. })
    )));
}

#[test]
fn a_covered_block_is_not_taken() {
    let mut field = Field::flat();
    let target = BlockPos::new(2, 64, 0);
    field.blocks.insert(target, BlockKind::Dirt.default_state());
    field
        .blocks
        .insert(BlockPos::new(1, 64, 0), BlockKind::Stone.default_state());
    let (mut base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Enderman, 13, GoalSettings::default());
    let actions = actions_over(&mut ai, &mut base, &state, &field, 2000, |_| {});
    assert!(!actions.iter().any(|action| matches!(
        action,
        MobAction::Enderman(EndermanAction::TakeBlock { pos, .. }) if *pos == target
    )));
}

#[test]
fn a_carried_block_is_set_down_on_solid_ground() {
    let field = Field::flat();
    let (mut base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Enderman, 21, GoalSettings::default());
    let dirt = BlockKind::Dirt.default_state();
    let actions = actions_over(&mut ai, &mut base, &state, &field, 30_000, |view| {
        view.species.enderman.carried = Some(dirt);
    });
    let placed: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Enderman(EndermanAction::PlaceBlock { pos, state }) => Some((*pos, *state)),
            _ => None,
        })
        .collect();
    assert!(!placed.is_empty());
    for (pos, state) in placed {
        assert_eq!(state, dirt);
        assert!(pos.y >= 64 && pos.y <= 65);
        assert!((-1..=1).contains(&pos.x) && (-1..=1).contains(&pos.z));
    }
}

#[test]
fn a_flower_without_ground_is_not_kept() {
    let field = Field::flat();
    let (mut base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 66.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Enderman, 21, GoalSettings::default());
    let poppy = BlockKind::Poppy.default_state();
    let actions = actions_over(&mut ai, &mut base, &state, &field, 30_000, |view| {
        view.species.enderman.carried = Some(poppy);
    });
    assert!(!actions.iter().any(|action| matches!(
        action,
        MobAction::Enderman(EndermanAction::PlaceBlock { state, .. }) if *state == poppy
    )));
}

#[test]
fn teleport_rolls_draw_x_then_y_then_z() {
    let view = enderman_view(Vec3::new(10.0, 64.0, -10.0));
    let mut random = LegacyRandom::new(77);
    let rolled = teleport_roll(&view, &mut random);
    let mut replay = LegacyRandom::new(77);
    let x = 10.0 + (replay.next_double() - 0.5) * 64.0;
    let y = 64.0 + f64::from(replay.next_int(64) - 32);
    let z = -10.0 + (replay.next_double() - 0.5) * 64.0;
    assert_eq!(rolled, Vec3::new(x, y, z));
}

#[test]
fn it_lands_above_the_first_supporting_block_below_the_roll() {
    let field = Field::flat();
    let view = enderman_view(Vec3::new(0.5, 64.0, 0.5));
    let landed = random_teleport(&field, &view, Vec3::new(8.5, 70.0, 8.5));
    assert_eq!(landed, Some(Vec3::new(8.5, 64.0, 8.5)));
}

#[test]
fn bedrock_water_and_walls_refuse_the_teleport() {
    let view = enderman_view(Vec3::new(0.5, 64.0, 0.5));
    let mut field = Field::flat();
    field
        .blocks
        .insert(BlockPos::new(8, 63, 8), BlockKind::Bedrock.default_state());
    assert_eq!(
        random_teleport(&field, &view, Vec3::new(8.5, 64.0, 8.5)),
        None
    );
    let mut water = Field::flat();
    water
        .blocks
        .insert(BlockPos::new(8, 63, 8), BlockKind::Water.default_state());
    assert_eq!(
        random_teleport(&water, &view, Vec3::new(8.5, 64.0, 8.5)),
        None
    );
    let mut walled = Field::flat();
    walled
        .blocks
        .insert(BlockPos::new(8, 65, 8), BlockKind::Stone.default_state());
    assert_eq!(
        random_teleport(&walled, &view, Vec3::new(8.5, 64.0, 8.5)),
        None
    );
}

#[test]
fn passengers_never_teleport_but_still_consume_the_roll() {
    let field = Field::flat();
    let mut view = enderman_view(Vec3::new(0.5, 64.0, 0.5));
    view.passenger = true;
    let mut random = LegacyRandom::new(5);
    assert_eq!(find_teleport(&field, &view, &mut random), None);
    let mut replay = LegacyRandom::new(5);
    replay.next_double();
    replay.next_int(64);
    replay.next_double();
    assert_eq!(random.next_int(1000), replay.next_int(1000));
}

#[test]
fn staring_endermen_teleport_away_when_the_player_is_close() {
    let (base, state) = mob(EntityKind::Enderman, Vec3::new(0.5, 64.0, 3.5));
    let mut base = base;
    let mut field = Field::flat();
    field
        .players
        .push(living(100, EntityKind::Player, Vec3::new(0.5, 64.93, 0.5)));
    let mut ai = MobAi::new(EntityKind::Enderman, 4, GoalSettings::default());
    ai.set_target(Some(EntityId(100)));
    let actions = actions_over(&mut ai, &mut base, &state, &field, 80, |_| {});
    let teleported = actions
        .iter()
        .any(|action| matches!(action, MobAction::Enderman(EndermanAction::Teleport(_))));
    let started = actions.contains(&MobAction::Enderman(EndermanAction::BeingStaredAt));
    assert!(started);
    assert!(teleported);
}
