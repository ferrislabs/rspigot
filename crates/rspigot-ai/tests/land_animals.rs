use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MORE_CARROT_TICKS, MobAction, MobAi, MobBody, MobGoal,
    MobView, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    griefing: bool,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            griefing: true,
            time: Cell::new(0),
        }
    }
}

impl rspigot_ai::BlockView for World {
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

fn living(id: i32, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id: EntityId(id),
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

fn player(position: Vec3) -> Living {
    living(100, EntityKind::Player, position)
}

fn mob(kind: EntityKind, position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
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

fn running(ai: &MobAi, matches: impl Fn(&MobGoal) -> bool) -> bool {
    ai.goals()
        .goals()
        .iter()
        .any(|goal| goal.is_running() && matches(goal.goal()))
}

fn ripe_garden(world: &mut World, at: BlockPos) {
    world
        .blocks
        .insert(at.offset(0, -1, 0), BlockKind::Farmland.default_state());
    world.blocks.insert(
        at,
        BlockKind::Carrots
            .state_with(&[("age", "7")])
            .expect("carrot age"),
    );
}

fn goal_summary(ai: &MobAi) -> Vec<(i32, &'static str)> {
    ai.goals()
        .goals()
        .iter()
        .map(|goal| {
            let label = match goal.goal() {
                MobGoal::Float(_) => "float",
                MobGoal::ClimbOnTopOfPowderSnow(_) => "climb_powder_snow",
                MobGoal::Panic(_) => "panic",
                MobGoal::Breed(_) => "breed",
                MobGoal::Tempt(_) => "tempt",
                MobGoal::AvoidEntity(_) => "avoid",
                MobGoal::RaidGarden(_) => "raid_garden",
                MobGoal::Stroll(_) => "stroll",
                MobGoal::LookAtPlayer(_) => "look",
                MobGoal::LeapAtTarget(_) => "leap",
                MobGoal::OcelotAttack(_) => "ocelot_attack",
                _ => "other",
            };
            (goal.priority(), label)
        })
        .collect()
}

#[test]
fn rabbits_register_the_vanilla_goals() {
    let ai = MobAi::new(EntityKind::Rabbit, 1, GoalSettings::default());
    assert_eq!(
        goal_summary(&ai),
        vec![
            (1, "float"),
            (1, "climb_powder_snow"),
            (1, "panic"),
            (2, "breed"),
            (3, "tempt"),
            (4, "avoid"),
            (4, "avoid"),
            (4, "avoid"),
            (5, "raid_garden"),
            (6, "stroll"),
            (11, "look"),
        ]
    );
    assert!(ai.targets().goals().is_empty());
    assert!(ai.can_float());
}

#[test]
fn ocelots_register_the_vanilla_goals_and_targets() {
    let ai = MobAi::new(EntityKind::Ocelot, 1, GoalSettings::default());
    assert_eq!(
        goal_summary(&ai),
        vec![
            (1, "float"),
            (3, "tempt"),
            (7, "leap"),
            (8, "ocelot_attack"),
            (9, "breed"),
            (10, "stroll"),
            (11, "look"),
            (4, "avoid"),
        ]
    );
    let targets: Vec<i32> = ai
        .targets()
        .goals()
        .iter()
        .map(|goal| goal.priority())
        .collect();
    assert_eq!(targets, vec![1, 1]);
}

#[test]
fn panicking_rabbits_run_at_twice_and_a_fifth_speed() {
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 7, GoalSettings::default());
    let world = World::new();
    for _ in 0..6 {
        step(&mut ai, &mut base, &state, &world, |view| view.panic = true);
    }
    assert!(ai.is_panicking());
    assert_eq!(ai.brain().navigation.speed_modifier(), 2.2);
    assert_eq!(ai.brain().movement.speed_modifier(), 2.2);
}

#[test]
fn rabbits_flee_players_within_eight_blocks_unless_evil() {
    let flees = |evil: bool, distance: f64| {
        let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Rabbit, 3, GoalSettings::default());
        let mut world = World::new();
        world.players = vec![player(Vec3::new(0.5 + distance, 64.0, 0.5))];
        let mut ever = false;
        for _ in 0..40 {
            step(&mut ai, &mut base, &state, &world, |view| view.evil = evil);
            ever |= running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_)));
        }
        ever
    };
    assert!(flees(false, 5.0));
    assert!(!flees(false, 10.0));
    assert!(!flees(true, 5.0));
}

#[test]
fn rabbits_flee_monsters_only_within_four_blocks() {
    let flees = |distance: f64| {
        let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Rabbit, 3, GoalSettings::default());
        let mut world = World::new();
        world.others = vec![living(
            2,
            EntityKind::Zombie,
            Vec3::new(0.5 + distance, 64.0, 0.5),
        )];
        let mut ever = false;
        for _ in 0..40 {
            step(&mut ai, &mut base, &state, &world, |_| {});
            ever |= running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_)));
        }
        ever
    };
    assert!(flees(2.5));
    assert!(!flees(6.0));
}

#[test]
fn rabbits_nibble_ripe_carrots_underfoot_then_want_no_more() {
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 11, GoalSettings::default());
    let mut world = World::new();
    ripe_garden(&mut world, BlockPos::new(0, 64, 0));
    let mut raided = Vec::new();
    for _ in 0..6 {
        raided.extend(
            step(&mut ai, &mut base, &state, &world, |_| {})
                .into_iter()
                .filter(|action| matches!(action, MobAction::RaidCarrot(_))),
        );
    }
    assert_eq!(raided, vec![MobAction::RaidCarrot(BlockPos::new(0, 64, 0))]);
}

#[test]
fn rabbits_ignore_carrots_while_they_still_have_a_full_belly() {
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 11, GoalSettings::default());
    let mut world = World::new();
    ripe_garden(&mut world, BlockPos::new(0, 64, 0));
    for _ in 0..6 {
        let actions = step(&mut ai, &mut base, &state, &world, |view| {
            view.more_carrot_ticks = MORE_CARROT_TICKS;
        });
        assert!(!actions.contains(&MobAction::RaidCarrot(BlockPos::new(0, 64, 0))));
    }
}

#[test]
fn rabbits_ignore_carrots_that_are_not_ripe_or_when_griefing_is_off() {
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 11, GoalSettings::default());
    let mut world = World::new();
    ripe_garden(&mut world, BlockPos::new(0, 64, 0));
    world.griefing = false;
    for _ in 0..6 {
        let actions = step(&mut ai, &mut base, &state, &world, |_| {});
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, MobAction::RaidCarrot(_)))
        );
    }
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 11, GoalSettings::default());
    let mut world = World::new();
    ripe_garden(&mut world, BlockPos::new(0, 64, 0));
    world.blocks.insert(
        BlockPos::new(0, 64, 0),
        BlockKind::Carrots
            .state_with(&[("age", "6")])
            .expect("carrot age"),
    );
    for _ in 0..6 {
        let actions = step(&mut ai, &mut base, &state, &world, |_| {});
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, MobAction::RaidCarrot(_)))
        );
    }
}

#[test]
fn the_carrot_appetite_fades_by_zero_to_two_ticks_each_tick() {
    let (mut base, mut state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 5, GoalSettings::default());
    let mut last = MORE_CARROT_TICKS;
    let mut now = last;
    for _ in 0..80 {
        ai.brain_mut()
            .rabbit_ai_step(&mut base, &mut state, false, None, &mut now);
        assert!((last - 2..=last).contains(&now) || now == 0);
        last = now;
    }
    assert_eq!(last, 0);
}

#[test]
fn rabbits_hop_out_of_powder_snow() {
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 5, GoalSettings::default());
    let world = World::new();
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.in_powder_snow = true;
        });
    }
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::ClimbOnTopOfPowderSnow(_)
    )));
    let mut world = World::new();
    world
        .blocks
        .insert(BlockPos::new(0, 65, 0), BlockKind::Stone.default_state());
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 5, GoalSettings::default());
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.in_powder_snow = true;
        });
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::ClimbOnTopOfPowderSnow(_)
    )));
}

#[test]
fn only_the_walkers_of_powder_snow_hop() {
    let (mut base, state) = mob(EntityKind::Pig, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Pig, 5, GoalSettings::default());
    let world = World::new();
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.in_powder_snow = true;
        });
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::ClimbOnTopOfPowderSnow(_)
    )));
}

fn ocelot_food() -> ItemId {
    ItemKind::Cod.id()
}

#[test]
fn ocelots_follow_a_still_player_holding_fish_but_flee_when_it_moves() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 2, GoalSettings::default());
    let mut world = World::new();
    let mut holder = player(Vec3::new(4.5, 64.0, 0.5));
    holder.main_hand = Some(ocelot_food());
    world.players = vec![holder];
    for _ in 0..6 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
    world.players[0].position = Vec3::new(4.5, 64.0, 1.5);
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(!running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
}

#[test]
fn ocelots_ignore_the_scare_once_they_trust() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 2, GoalSettings::default());
    let mut world = World::new();
    let mut holder = player(Vec3::new(4.5, 64.0, 0.5));
    holder.main_hand = Some(ocelot_food());
    world.players = vec![holder];
    for _ in 0..6 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.trusting = true;
        });
    }
    world.players[0].position = Vec3::new(4.5, 64.0, 1.5);
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.trusting = true;
        });
    }
    assert!(running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
}

#[test]
fn ocelots_avoid_players_within_sixteen_blocks_until_they_trust() {
    let flees = |trusting: bool| {
        let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Ocelot, 4, GoalSettings::default());
        let mut world = World::new();
        world.players = vec![player(Vec3::new(10.5, 64.0, 0.5))];
        let mut ever = false;
        for _ in 0..40 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.trusting = trusting;
            });
            ever |= running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_)));
        }
        ever
    };
    assert!(flees(false));
    assert!(!flees(true));
}

#[test]
fn ocelots_strike_a_target_every_twenty_ticks_once_in_reach() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 6, GoalSettings::default());
    let mut world = World::new();
    world.others = vec![living(2, EntityKind::Chicken, Vec3::new(1.5, 64.0, 0.5))];
    ai.set_target(Some(EntityId(2)));
    let mut strikes = 0;
    for _ in 0..30 {
        strikes += step(&mut ai, &mut base, &state, &world, |_| {})
            .iter()
            .filter(|action| **action == MobAction::Attack(EntityId(2)))
            .count();
    }
    assert_eq!(strikes, 2);
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::OcelotAttack(_)
    )));
}

#[test]
fn ocelots_stalk_before_the_pounce_and_sneak_in_between() {
    let speed = |distance: f64| {
        let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Ocelot, 6, GoalSettings::default());
        let mut world = World::new();
        world.others = vec![living(
            2,
            EntityKind::Chicken,
            Vec3::new(0.5 + distance, 64.0, 0.5),
        )];
        ai.set_target(Some(EntityId(2)));
        for _ in 0..4 {
            step(&mut ai, &mut base, &state, &world, |_| {});
        }
        ai.brain().navigation.speed_modifier()
    };
    assert_eq!(speed(3.0), 1.33);
    assert_eq!(speed(8.0), 0.6);
    assert_eq!(speed(20.0), 0.8);
}

#[test]
fn ocelots_stop_hunting_once_the_target_is_gone() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 6, GoalSettings::default());
    let mut world = World::new();
    world.others = vec![living(2, EntityKind::Chicken, Vec3::new(8.5, 64.0, 0.5))];
    ai.set_target(Some(EntityId(2)));
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::OcelotAttack(_)
    )));
    world.others[0].alive = false;
    for _ in 0..4 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::OcelotAttack(_)
    )));
}

#[test]
fn ocelots_pick_chickens_as_targets() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 8, GoalSettings::default());
    let mut world = World::new();
    world.others = vec![living(2, EntityKind::Chicken, Vec3::new(3.5, 64.0, 0.5))];
    for _ in 0..80 {
        step(&mut ai, &mut base, &state, &world, |_| {});
        if ai.target().is_some() {
            break;
        }
    }
    assert_eq!(ai.target(), Some(EntityId(2)));
}

fn rabbit_at_rest() -> (EntityBase, LivingState, MobAi) {
    let (base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Rabbit, 21, GoalSettings::default());
    let (mut base, mut state) = (base, state);
    for _ in 0..11 {
        hop_step(&mut ai, &mut base, &mut state, false, None);
    }
    ai.brain_mut().take_hop_sounds();
    (base, state, ai)
}

fn hop_step(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &mut LivingState,
    evil: bool,
    target: Option<Vec3>,
) {
    let mut carrots = 0;
    ai.brain_mut()
        .rabbit_ai_step(base, state, evil, target, &mut carrots);
}

#[test]
fn a_rabbit_with_a_destination_hops_towards_it_and_faces_it() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.6);
    hop_step(&mut ai, &mut base, &mut state, false, None);
    assert!(state.jumping);
    assert!((base.yaw - (-90.0)).abs() < 1.0e-3);
    let sounds = ai.brain_mut().take_hop_sounds();
    assert_eq!(sounds.len(), 1);
    assert!((0.64..=0.96).contains(&sounds[0].pitch));
    assert_eq!(ai.brain().hop().map(|hop| hop.jump_duration()), Some(15));
}

#[test]
fn a_rabbit_without_a_destination_stays_put() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    hop_step(&mut ai, &mut base, &mut state, false, None);
    assert!(!state.jumping);
    assert!(ai.brain_mut().take_hop_sounds().is_empty());
}

#[test]
fn landing_sets_a_ten_tick_delay_or_three_when_fleeing() {
    let delay_after_landing = |speed: f64| {
        let (mut base, mut state, mut ai) = rabbit_at_rest();
        base.on_ground = false;
        hop_step(&mut ai, &mut base, &mut state, false, None);
        ai.brain_mut()
            .movement
            .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), speed);
        base.on_ground = true;
        state.jumping = true;
        hop_step(&mut ai, &mut base, &mut state, false, None);
        assert!(!state.jumping);
        ai.brain().hop().map(|hop| hop.jump_delay_ticks()).unwrap()
    };
    assert_eq!(delay_after_landing(0.6), 10);
    assert_eq!(delay_after_landing(2.2), 3);
}

#[test]
fn the_hop_delay_counts_down_before_the_next_hop() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    base.on_ground = false;
    hop_step(&mut ai, &mut base, &mut state, false, None);
    base.on_ground = true;
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.6);
    hop_step(&mut ai, &mut base, &mut state, false, None);
    assert!(!state.jumping);
    let mut jumped_after = None;
    for tick in 1..=12 {
        hop_step(&mut ai, &mut base, &mut state, false, None);
        if state.jumping {
            jumped_after = Some(tick);
            break;
        }
    }
    assert_eq!(jumped_after, Some(10));
}

#[test]
fn a_killer_bunny_lunges_at_a_target_within_four_blocks() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    hop_step(
        &mut ai,
        &mut base,
        &mut state,
        true,
        Some(Vec3::new(3.5, 64.0, 0.5)),
    );
    assert!(state.jumping);
    assert!((base.yaw - (-90.0)).abs() < 1.0e-3);
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    hop_step(
        &mut ai,
        &mut base,
        &mut state,
        true,
        Some(Vec3::new(4.5, 64.0, 0.5)),
    );
    assert!(!state.jumping);
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    hop_step(
        &mut ai,
        &mut base,
        &mut state,
        false,
        Some(Vec3::new(2.5, 64.0, 0.5)),
    );
    assert!(!state.jumping);
}

#[test]
fn the_jump_flag_is_released_after_fifteen_ticks_of_hop() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.6);
    hop_step(&mut ai, &mut base, &mut state, false, None);
    assert!(state.jumping);
    for _ in 0..15 {
        ai.brain_mut().rabbit_post_ai_step(&mut state);
        assert!(state.jumping);
    }
    ai.brain_mut().rabbit_post_ai_step(&mut state);
    assert!(!state.jumping);
    assert_eq!(ai.brain().hop().map(|hop| hop.jump_duration()), Some(0));
}

#[test]
fn the_move_control_of_a_swimming_rabbit_always_asks_for_one_and_a_half() {
    let (_, _, mut ai) = rabbit_at_rest();
    ai.brain_mut().movement.set_in_water(true);
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.6);
    assert_eq!(ai.brain().movement.speed_modifier(), 1.5);
    assert_eq!(ai.brain().movement.next_jump_speed(), 1.5);
    ai.brain_mut().movement.set_in_water(false);
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.0);
    assert_eq!(ai.brain().movement.speed_modifier(), 0.0);
    assert_eq!(ai.brain().movement.next_jump_speed(), 1.5);
}

#[test]
fn other_mobs_keep_the_plain_move_control() {
    let mut ai = MobAi::new(EntityKind::Cow, 1, GoalSettings::default());
    ai.brain_mut().movement.set_in_water(true);
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 0.6);
    assert_eq!(ai.brain().movement.speed_modifier(), 0.6);
    assert!(ai.brain().hop().is_none());
}

#[test]
fn acting_tunes_the_jump_of_a_rabbit_to_its_speed_and_the_terrain() {
    let tuned = |speed: f64, wanted_y: f64, jumping: bool| {
        let (mut base, mut state, mut ai) = rabbit_at_rest();
        let view = World::new();
        ai.brain_mut()
            .movement
            .set_wanted_position(Vec3::new(5.5, wanted_y, 0.5), speed);
        state.jumping = jumping;
        ai.act(&mut base, &mut state, &view);
        state.jump_tuning
    };
    let slow = tuned(0.6, 64.0, true);
    assert!((slow.multiplier - 0.2 / 0.42).abs() < 1.0e-6);
    let quick = tuned(1.0, 64.0, true);
    assert!((quick.multiplier - 0.3 / 0.42).abs() < 1.0e-6);
    let climbing = tuned(1.0, 65.0, true);
    assert!((climbing.multiplier - 0.5 / 0.42).abs() < 1.0e-6);
    assert_eq!(quick.hop, Some(1.5));
}

#[test]
fn a_baby_rabbit_hops_with_a_shorter_push() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    base.data
        .set(16, rspigot_domain::entity::SyncedValue::Boolean(true));
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
    state.jumping = true;
    ai.act(&mut base, &mut state, &World::new());
    assert_eq!(state.jump_tuning.hop, Some(0.5));
}

#[test]
fn an_idle_rabbit_on_the_ground_drops_its_speed_modifier_to_zero_when_acting() {
    let (mut base, mut state, mut ai) = rabbit_at_rest();
    ai.brain_mut()
        .movement
        .set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
    ai.act(&mut base, &mut state, &World::new());
    assert_eq!(ai.brain().movement.next_jump_speed(), 1.0);
    assert_eq!(ai.brain().movement.speed_modifier(), 0.0);
    assert_eq!(state.speed, 0.0);
}

#[test]
fn the_killer_variant_adds_its_melee_goal_and_its_targets() {
    let mut ai = MobAi::new(EntityKind::Rabbit, 1, GoalSettings::default());
    ai.become_killer_bunny();
    let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
    step(&mut ai, &mut base, &state, &World::new(), |_| {});
    let summary = goal_summary(&ai);
    assert_eq!(summary.len(), 12);
    assert_eq!(summary.last(), Some(&(4, "other")));
    let targets: Vec<i32> = ai
        .targets()
        .goals()
        .iter()
        .map(|goal| goal.priority())
        .collect();
    assert_eq!(targets, vec![1, 2, 2]);
    assert!(matches!(
        ai.targets().goals()[0].goal(),
        MobGoal::HurtByTarget(_)
    ));
}

#[test]
fn a_killer_bunny_no_longer_flees_players() {
    let flees = |evil: bool| {
        let (mut base, state) = mob(EntityKind::Rabbit, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Rabbit, 9, GoalSettings::default());
        let mut world = World::new();
        world.players = vec![player(Vec3::new(3.5, 64.0, 0.5))];
        let mut ever = false;
        for _ in 0..20 {
            step(&mut ai, &mut base, &state, &world, |view| view.evil = evil);
            ever |= running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_)));
        }
        ever
    };
    assert!(flees(false));
    assert!(!flees(true));
}

#[test]
fn trust_removes_the_avoid_goal_and_losing_it_adds_it_back_last() {
    let mut ai = MobAi::new(EntityKind::Ocelot, 1, GoalSettings::default());
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    ai.reassess_trusting(true);
    step(&mut ai, &mut base, &state, &World::new(), |_| {});
    assert!(goal_summary(&ai).iter().all(|(_, label)| *label != "avoid"));
    ai.reassess_trusting(false);
    step(&mut ai, &mut base, &state, &World::new(), |_| {});
    assert_eq!(goal_summary(&ai).last(), Some(&(4, "avoid")));
    ai.reassess_trusting(false);
    step(&mut ai, &mut base, &state, &World::new(), |_| {});
    let avoids = goal_summary(&ai)
        .iter()
        .filter(|(_, label)| *label == "avoid")
        .count();
    assert_eq!(avoids, 1);
}

#[test]
fn trusting_while_fleeing_stops_the_flight_at_once() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 4, GoalSettings::default());
    let mut world = World::new();
    world.players = vec![player(Vec3::new(10.5, 64.0, 0.5))];
    for _ in 0..40 {
        step(&mut ai, &mut base, &state, &world, |_| {});
        if running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_))) {
            break;
        }
    }
    assert!(running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_))));
    ai.reassess_trusting(true);
    step(&mut ai, &mut base, &state, &world, |_| {});
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::AvoidEntity(_)
    )));
}

#[test]
fn the_tempt_goal_reports_when_it_is_running() {
    let (mut base, state) = mob(EntityKind::Ocelot, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Ocelot, 2, GoalSettings::default());
    assert!(!ai.is_tempting());
    let mut world = World::new();
    let mut holder = player(Vec3::new(4.5, 64.0, 0.5));
    holder.main_hand = Some(ocelot_food());
    world.players = vec![holder];
    for _ in 0..6 {
        step(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(ai.is_tempting());
}
