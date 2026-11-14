use rspigot_ai::{
    Activity, AdultSensorAnyType, AiContext, AiWorld, BlockView, Flag, Goal, GoalSettings, Living,
    MobAi, MobBody, MobGoal, MobView, MoveStyle, Operation, SensorKind, TargetKind, TemptItems,
    can_reach, can_reach_carefully, happy_ghast_sensors, wrap_degrees_90,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use std::collections::HashMap;
use uuid::Uuid;

struct Sky {
    blocks: HashMap<BlockPos, StateId>,
}

impl Sky {
    fn open() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    fn with(mut self, pos: BlockPos, kind: BlockKind) -> Self {
        self.blocks.insert(pos, kind.default_state());
        self
    }
}

impl BlockView for Sky {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(self.blocks.get(&pos).copied().unwrap_or(StateId::AIR))
    }
}

impl AiWorld for Sky {
    fn players(&self) -> &[Living] {
        &[]
    }

    fn living(&self, _id: EntityId) -> Option<Living> {
        None
    }

    fn living_of(&self, _kind: TargetKind, _area: &Aabb) -> Vec<Living> {
        Vec::new()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        100
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }
}

fn adult() -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::HappyGhast);
    base.position = Vec3::new(0.5, 100.0, 0.5);
    base.old_position = base.position;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn view_of(base: &EntityBase, state: &LivingState, baby: bool) -> MobView {
    let mut view = MobView::of(base, state);
    view.baby = baby;
    view
}

fn goals_of(ai: &MobAi) -> Vec<(i32, MobGoal)> {
    ai.goals()
        .goals()
        .iter()
        .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
        .collect()
}

#[test]
fn an_adult_registers_float_tempt_and_random_float_at_three_four_and_five() {
    let ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    let goals = goals_of(&ai);
    assert_eq!(goals.len(), 3);
    assert_eq!(goals[0].0, 3);
    assert!(format!("{:?}", goals[0].1).contains("HappyFloat"));
    assert_eq!(goals[1].0, 4);
    let MobGoal::Tempt(_) = &goals[1].1 else {
        panic!("a tempt goal");
    };
    let text = format!("{:?}", goals[1].1);
    assert!(text.contains("stop_distance: 7.0"), "{text}");
    assert!(text.contains("speed_modifier: 1.0"), "{text}");
    assert!(text.contains("HappyGhast"), "{text}");
    assert_eq!(goals[2].0, 5);
    let text = format!("{:?}", goals[2].1);
    assert!(text.contains("RandomFloatAround"), "{text}");
    assert!(text.contains("distance_to_blocks: 16"), "{text}");
    assert_eq!(ai.brain().movement.style(), MoveStyle::Ghast);
    assert!(ai.target().is_none());
}

#[test]
fn the_baby_brain_has_the_vanilla_core_idle_and_panic_activities() {
    let ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    let core: Vec<String> = brain
        .activity_behaviors(Activity::Core)
        .into_iter()
        .map(|(_, control)| format!("{control:?}"))
        .collect();
    assert_eq!(core.len(), 5);
    for (text, name) in core.iter().zip([
        "Swim",
        "AnimalPanic",
        "LookAtTargetSink",
        "MoveToTargetSink",
        "TemptationCooldownTicks",
    ]) {
        assert!(text.contains(name), "{text} {name}");
    }
    assert!(core[0].contains("0.8"));
    assert!(core[1].contains("speed_multiplier: 2.0"));
    assert!(core[1].contains("fly_height: Some(0)"));
    let idle = brain.activity_behaviors(Activity::Idle);
    let priorities: Vec<i32> = idle.iter().map(|(priority, _)| *priority).collect();
    assert_eq!(priorities, vec![1, 2, 3, 4]);
    let text: Vec<String> = idle.iter().map(|(_, c)| format!("{c:?}")).collect();
    assert!(text[0].contains("FollowTemptation"));
    assert!(text[0].contains("speed_modifier: 1.25"));
    assert!(text[0].contains("Constant(3.0)"));
    assert!(text[0].contains("look_in_the_eyes: true"));
    assert!(text[1].contains("BabyFollowAdult") && text[1].contains("NearestVisiblePlayer"));
    assert!(text[1].contains("speed_modifier: 1.1"));
    assert!(text[2].contains("NearestVisibleAdult"));
    assert!(text[3].contains("RandomStroll") && text[3].contains("flying: true"));
    assert!(text[3].contains("SetWalkTargetFromLookTarget"));
    assert!(brain.activity_behaviors(Activity::Panic).is_empty());
}

#[test]
fn the_baby_senses_players_food_and_any_followable_adult() {
    let sensors = happy_ghast_sensors();
    let text = format!("{sensors:?}");
    for name in [
        "NearestLivingEntities",
        "HurtBy",
        "minecraft:happy_ghast_food",
        "NearestAdultAnyType",
        "NearestPlayers",
    ] {
        assert!(text.contains(name), "{name} {text}");
    }
    assert!(
        sensors
            .iter()
            .any(|sensor| matches!(sensor, SensorKind::NearestAdultAnyType(AdultSensorAnyType)))
    );
}

#[test]
fn an_unharnessed_adult_follows_harnesses_and_snowballs_while_a_harnessed_one_only_snowballs() {
    let snowball = Some(ItemKind::Snowball.id());
    let harness = Some(ItemKind::BlueHarness.id());
    let items = TemptItems::HappyGhast;
    assert!(items.matches_for(false, snowball));
    assert!(items.matches_for(false, harness));
    assert!(items.matches_for(true, snowball));
    assert!(!items.matches_for(true, harness));
    assert!(!items.matches_for(false, None));
}

#[test]
fn the_happy_ghast_float_goal_stands_down_while_the_ghast_stays_still() {
    let (mut base, state) = adult();
    base.fluid.water_height = 1.0;
    base.fluid.in_water = true;
    let mut view = view_of(&base, &state, false);
    let world = Sky::open();
    let mut ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    let mut body = MobBody::of(&view);
    let mut float = goals_of(&ai)[0].1.clone();
    assert_eq!(
        Goal::<AiContext<'_, Sky>>::flags(&float)
            .iter()
            .collect::<Vec<_>>(),
        vec![Flag::Jump]
    );
    let mut context = AiContext {
        mob: &view,
        world: &world,
        brain: ai.brain_mut(),
        body: &mut body,
    };
    assert!(Goal::can_use(&mut float, &mut context));
    view.species.happy_ghast.on_still_timeout = true;
    let mut context = AiContext {
        mob: &view,
        world: &world,
        brain: ai.brain_mut(),
        body: &mut body,
    };
    assert!(!Goal::can_use(&mut float, &mut context));
}

#[test]
fn growing_up_swaps_the_goals_the_move_control_and_the_navigation() {
    let (base, state) = adult();
    let world = Sky::open();
    let mut ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    let baby = view_of(&base, &state, true);
    let mut body = MobBody::of(&baby);
    ai.think(&baby, &world, &mut body);
    assert!(goals_of(&ai).is_empty());
    assert_eq!(ai.brain().movement.style(), MoveStyle::Flying);
    assert_eq!(ai.brain().movement.max_turn(), 180);
    assert!(ai.brain().movement.hovers_in_place());
    let grown = view_of(&base, &state, false);
    ai.think(&grown, &world, &mut body);
    let goals = goals_of(&ai);
    assert_eq!(
        goals.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    assert_eq!(ai.brain().movement.style(), MoveStyle::Ghast);
}

#[test]
fn a_still_adult_snaps_to_a_quarter_turn_and_looks_straight_ahead() {
    let (mut base, mut state) = adult();
    let world = Sky::open();
    let mut ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    base.set_yaw(100.0);
    if let Some(rspigot_domain::entity::species::Species::HappyGhast(ghast)) =
        state.mob.as_mut().map(|mob| &mut mob.species)
    {
        rspigot_domain::entity::species::happy_ghast::set_server_still_timeout(&mut base, ghast, 5);
    }
    ai.act(&mut base, &mut state, &world);
    assert_eq!(base.yaw, 100.0 - wrap_degrees_90(100.0));
    assert_eq!(base.head_yaw, base.yaw);
}

#[test]
fn an_adult_that_is_not_still_faces_its_motion() {
    let (mut base, mut state) = adult();
    let world = Sky::open();
    let mut ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    base.velocity = Vec3::new(1.0, 0.0, 0.0);
    ai.act(&mut base, &mut state, &world);
    assert!((base.yaw + 90.0).abs() < 0.01, "{}", base.yaw);
    assert_eq!(state.body_yaw, base.yaw);
}

#[test]
fn quarter_turn_wrapping_keeps_the_remainder_within_forty_five_degrees() {
    assert_eq!(wrap_degrees_90(0.0), 0.0);
    assert_eq!(wrap_degrees_90(44.0), 44.0);
    assert_eq!(wrap_degrees_90(45.0), -45.0);
    assert_eq!(wrap_degrees_90(100.0), 10.0);
    assert_eq!(wrap_degrees_90(-100.0), -10.0);
    assert_eq!(wrap_degrees_90(-50.0), 40.0);
}

#[test]
fn the_careful_move_control_refuses_harmful_blocks_the_plain_one_flies_through() {
    let (base, _) = adult();
    let world = Sky::open().with(BlockPos::new(0, 100, 3), BlockKind::Fire);
    let toward = Vec3::new(0.0, 0.0, 6.0);
    assert!(can_reach(&base, &world, toward));
    assert!(!can_reach_carefully(&base, &world, toward));
    let clear = Sky::open();
    assert!(can_reach_carefully(&base, &clear, toward));
    let water = Sky::open().with(BlockPos::new(0, 100, 3), BlockKind::Water);
    assert!(can_reach(&base, &water, toward));
    assert!(!can_reach_carefully(&base, &water, toward));
}

#[test]
fn the_careful_control_stops_a_still_adult_in_place() {
    let (mut base, mut state) = adult();
    let world = Sky::open();
    let mut ai = MobAi::new(EntityKind::HappyGhast, 1, GoalSettings::default());
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(0.5, 100.0, 10.5), 1.0);
    base.velocity = Vec3::new(0.2, 0.1, 0.3);
    if let Some(rspigot_domain::entity::species::Species::HappyGhast(ghast)) =
        state.mob.as_mut().map(|mob| &mut mob.species)
    {
        rspigot_domain::entity::species::happy_ghast::set_server_still_timeout(&mut base, ghast, 5);
    }
    ai.act(&mut base, &mut state, &world);
    assert_eq!(ai.brain().movement.operation(), Operation::Wait);
    assert_eq!(base.velocity, Vec3::ZERO);
}
