use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, CatAction, CatGoal, Facts, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal,
    MobView, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    time: Cell<i64>,
    universal_anger: bool,
    chest_viewers: i32,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(0),
            universal_anger: false,
            chest_viewers: 0,
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

    fn universal_anger(&self) -> bool {
        self.universal_anger
    }

    fn chest_open_count(&self, _pos: BlockPos) -> i32 {
        self.chest_viewers
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

const OWNER: EntityId = EntityId(100);

fn owner_at(position: Vec3) -> Living {
    living(OWNER.0, EntityKind::Player, position)
}

fn cat(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cat);
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

fn step2(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    let mut actions = step(ai, base, state, world, &adjust);
    actions.extend(step(ai, base, state, world, &adjust));
    actions
}

fn tamed(view: &mut MobView) {
    view.tamed = true;
    view.owner = Some(OWNER);
}

fn running(ai: &MobAi, matches: impl Fn(&MobGoal) -> bool) -> bool {
    ai.goals()
        .goals()
        .iter()
        .chain(ai.targets().goals())
        .any(|goal| goal.is_running() && matches(goal.goal()))
}

fn cat_ai() -> MobAi {
    MobAi::new(EntityKind::Cat, 5, GoalSettings::default())
}

fn bed(facing: &str, part: &str) -> StateId {
    BlockKind::WhiteBed
        .state_with(&[("facing", facing), ("part", part), ("occupied", "false")])
        .expect("bed state")
}

fn owned_world() -> World {
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(4.5, 64.0, 0.5))];
    world
}

fn world_with(blocks: &[(BlockPos, StateId)]) -> World {
    let mut world = owned_world();
    world.blocks = blocks.iter().copied().collect();
    world
}

fn actions_of(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    ticks: usize,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    let mut all = Vec::new();
    for _ in 0..ticks {
        all.extend(step2(ai, base, state, world, &adjust));
    }
    all
}

fn kind_of(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::Panic(_) => "panic",
        MobGoal::SitWhenOrdered(_) => "sit",
        MobGoal::Cat(CatGoal::RelaxOnOwner(_)) => "relax",
        MobGoal::Tempt(_) => "tempt",
        MobGoal::Cat(CatGoal::LieOnBlock(_)) => "lie",
        MobGoal::FollowOwner(_) => "follow",
        MobGoal::Cat(CatGoal::SitOnBlock(_)) => "sit_on_block",
        MobGoal::LeapAtTarget(_) => "leap",
        MobGoal::OcelotAttack(_) => "attack",
        MobGoal::Breed(_) => "breed",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::AvoidEntity(_) => "avoid",
        MobGoal::NearestAttackableTarget(_) => "target",
        _ => "other",
    }
}

#[test]
fn goals_and_targets_use_the_vanilla_priorities() {
    let ai = cat_ai();
    let goals: Vec<(i32, &str)> = ai
        .goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), kind_of(goal.goal())))
        .collect();
    assert_eq!(
        goals,
        vec![
            (1, "float"),
            (1, "panic"),
            (2, "sit"),
            (3, "relax"),
            (4, "tempt"),
            (5, "lie"),
            (6, "follow"),
            (7, "sit_on_block"),
            (8, "leap"),
            (9, "attack"),
            (10, "breed"),
            (11, "stroll"),
            (12, "look"),
            (4, "avoid"),
        ]
    );
    let targets: Vec<(i32, &str)> = ai
        .targets()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), kind_of(goal.goal())))
        .collect();
    assert_eq!(targets, vec![(1, "target"), (1, "target")]);
    assert!(ai.can_float());
}

#[test]
fn taming_swaps_the_avoid_goal_and_untaming_brings_it_back() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = cat_ai();
    let world = World::new();
    let avoids = |ai: &MobAi| {
        ai.goals()
            .goals()
            .iter()
            .filter(|goal| matches!(goal.goal(), MobGoal::AvoidEntity(_)))
            .count()
    };
    ai.reassess_cat_tame_goals(true);
    step2(&mut ai, &mut base, &state, &world, tamed);
    assert_eq!(avoids(&ai), 0);
    ai.reassess_cat_tame_goals(false);
    step2(&mut ai, &mut base, &state, &world, |_| {});
    assert_eq!(avoids(&ai), 1);
    ai.reassess_cat_tame_goals(false);
    step2(&mut ai, &mut base, &state, &world, |_| {});
    assert_eq!(avoids(&ai), 1);
    let last = ai.goals().goals().last().unwrap();
    assert_eq!(last.priority(), 4);
}

#[test]
fn a_wild_cat_runs_from_players_but_a_tamed_one_does_not() {
    let flees = |tame: bool| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = cat_ai();
        let world = owned_world();
        for _ in 0..6 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                if tame {
                    tamed(view);
                }
            });
            if running(&ai, |goal| matches!(goal, MobGoal::AvoidEntity(_))) {
                return true;
            }
        }
        false
    };
    assert!(flees(false));
    assert!(!flees(true));
}

#[test]
fn a_wild_cat_follows_cat_food_and_a_tamed_one_ignores_it() {
    let follows = |tame: bool, item: ItemKind| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = cat_ai();
        ai.reassess_cat_tame_goals(tame);
        let mut world = World::new();
        let mut player = owner_at(Vec3::new(5.5, 64.0, 0.5));
        player.main_hand = Some(item.id());
        world.players = vec![player];
        for _ in 0..6 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                if tame {
                    tamed(view);
                }
            });
            if running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))) {
                return true;
            }
        }
        false
    };
    assert!(follows(false, ItemKind::Cod));
    assert!(follows(false, ItemKind::Salmon));
    assert!(!follows(false, ItemKind::Bone));
    assert!(!follows(true, ItemKind::Cod));
}

#[test]
fn a_tamed_cat_goes_to_a_bed_found_by_the_vanilla_vertical_scan_and_lies_down_on_arrival() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let world = world_with(&[(BlockPos::new(0, 66, 0), bed("north", "foot"))]);
    let mut ai = cat_ai();
    ai.reassess_cat_tame_goals(true);
    let before = actions_of(&mut ai, &mut base, &state, &world, 6, tamed);
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::Cat(CatGoal::LieOnBlock(_))
    )));
    assert!(before.contains(&MobAction::SetSitting(false)));
    assert!(!before.contains(&MobAction::Cat(CatAction::SetLying(true))));
    base.position = Vec3::new(0.5, 67.0, 0.5);
    let after = actions_of(&mut ai, &mut base, &state, &world, 2, tamed);
    assert!(after.contains(&MobAction::Cat(CatAction::SetLying(true))));
}

#[test]
fn a_cat_standing_right_on_a_bed_skips_it_because_the_scan_never_checks_offset_zero() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let world = world_with(&[(BlockPos::new(0, 63, 0), bed("north", "head"))]);
    let mut ai = cat_ai();
    actions_of(&mut ai, &mut base, &state, &world, 6, tamed);
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::Cat(CatGoal::LieOnBlock(_))
    )));
}

#[test]
fn a_wild_or_seated_cat_never_lies_down() {
    for (tame, sit) in [(false, false), (true, true)] {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let world = world_with(&[(BlockPos::new(0, 66, 0), bed("north", "foot"))]);
        let mut ai = cat_ai();
        actions_of(&mut ai, &mut base, &state, &world, 40, |view| {
            if tame {
                tamed(view);
            }
            view.ordered_to_sit = sit;
        });
        assert!(!running(&ai, |goal| matches!(
            goal,
            MobGoal::Cat(CatGoal::LieOnBlock(_) | CatGoal::SitOnBlock(_))
        )));
    }
}

#[test]
fn a_cat_already_lying_does_not_start_looking_for_a_bed() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let world = world_with(&[(BlockPos::new(0, 66, 0), bed("north", "foot"))]);
    let mut ai = cat_ai();
    actions_of(&mut ai, &mut base, &state, &world, 40, |view| {
        tamed(view);
        view.species.cat.lying = true;
    });
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::Cat(CatGoal::LieOnBlock(_))
    )));
}

fn sit_runs(block: StateId, viewers: i32, head: bool) -> bool {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let mut world = world_with(&[(BlockPos::new(0, 63, 0), block)]);
    world.chest_viewers = viewers;
    let mut ai = cat_ai();
    let _ = head;
    let actions = actions_of(&mut ai, &mut base, &state, &world, 40, |view| {
        tamed(view);
        view.species.cat.lying = true;
    });
    running(&ai, |goal| {
        matches!(goal, MobGoal::Cat(CatGoal::SitOnBlock(_)))
    }) && actions.contains(&MobAction::SetSitting(true))
}

#[test]
fn a_tamed_cat_sits_on_beds_feet_lit_furnaces_and_unopened_chests() {
    assert!(sit_runs(bed("north", "foot"), 0, false));
    assert!(!sit_runs(bed("north", "head"), 0, true));
    assert!(sit_runs(
        BlockKind::Furnace
            .state_with(&[("facing", "north"), ("lit", "true")])
            .unwrap(),
        0,
        false
    ));
    assert!(!sit_runs(
        BlockKind::Furnace
            .state_with(&[("facing", "north"), ("lit", "false")])
            .unwrap(),
        0,
        false
    ));
    assert!(sit_runs(BlockKind::Chest.default_state(), 0, false));
    assert!(!sit_runs(BlockKind::Chest.default_state(), 1, false));
    assert!(!sit_runs(BlockKind::Stone.default_state(), 0, false));
}

fn sleeping_owner(position: Vec3, timer: i32) -> Living {
    let mut owner = owner_at(position);
    owner.facts.sleeping = true;
    owner.facts.sleep_timer = timer;
    owner
}

#[test]
fn a_tamed_cat_joins_a_sleeping_owner_then_lies_and_leaves_a_gift_after_a_long_sleep() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let owner_pos = BlockPos::new(1, 64, 0);
    let world_blocks = [(owner_pos, bed("north", "foot"))];
    let mut world = world_with(&world_blocks);
    world.players = vec![sleeping_owner(Vec3::new(1.5, 64.0, 0.5), 100)];
    let mut ai = cat_ai();
    ai.reassess_cat_tame_goals(true);
    let actions = actions_of(&mut ai, &mut base, &state, &world, 24, tamed);
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::Cat(CatGoal::RelaxOnOwner(_))
    )));
    assert!(actions.contains(&MobAction::Cat(CatAction::SetRelaxStateOne(true))));
    assert!(actions.contains(&MobAction::Cat(CatAction::SetLying(true))));
    let mut awake = owner_at(Vec3::new(1.5, 64.0, 0.5));
    awake.facts.sleep_timer = 100;
    world.players = vec![awake];
    let mut woke = Vec::new();
    for _ in 0..3 {
        woke.extend(step2(&mut ai, &mut base, &state, &world, tamed));
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::Cat(CatGoal::RelaxOnOwner(_))
    )));
    assert!(woke.contains(&MobAction::Cat(CatAction::SetLying(false))));
    assert!(woke.contains(&MobAction::Cat(CatAction::MorningGift)));
    assert!(woke.contains(&MobAction::Cat(CatAction::SetRelaxStateOne(false))));
}

#[test]
fn leaving_the_bed_early_gives_no_gift_roll() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let mut world = world_with(&[(BlockPos::new(1, 64, 0), bed("north", "foot"))]);
    world.players = vec![sleeping_owner(Vec3::new(1.5, 64.0, 0.5), 60)];
    let mut ai = cat_ai();
    actions_of(&mut ai, &mut base, &state, &world, 4, tamed);
    world.players = vec![owner_at(Vec3::new(1.5, 64.0, 0.5))];
    let woke = actions_of(&mut ai, &mut base, &state, &world, 3, tamed);
    assert!(!woke.contains(&MobAction::Cat(CatAction::MorningGift)));
    assert!(woke.contains(&MobAction::Cat(CatAction::SetLying(false))));
}

#[test]
fn relaxing_needs_a_tame_standing_cat_a_near_sleeping_owner_and_a_free_bed() {
    let blocked = |adjust: fn(&mut MobView), distance: f64, other_cat: bool, sleeping: bool| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut world = world_with(&[(BlockPos::new(1, 64, 0), bed("north", "foot"))]);
        let mut owner = owner_at(Vec3::new(1.5 + distance, 64.0, 0.5));
        owner.facts.sleeping = sleeping;
        world.players = vec![owner];
        if other_cat {
            let mut other = living(2, EntityKind::Cat, Vec3::new(1.5, 64.0, 1.5));
            other.facts.cat_relaxing = true;
            world.others = vec![other];
        }
        world.blocks.insert(
            BlockPos::new(1 + distance as i32, 64, 0),
            bed("north", "foot"),
        );
        let mut ai = cat_ai();
        let mut ever = false;
        for _ in 0..8 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                tamed(view);
                adjust(view);
            });
            ever |= running(&ai, |goal| {
                matches!(goal, MobGoal::Cat(CatGoal::RelaxOnOwner(_)))
            });
        }
        !ever
    };
    assert!(!blocked(|_| {}, 0.0, false, true));
    assert!(blocked(|view| view.ordered_to_sit = true, 0.0, false, true));
    assert!(blocked(|_| {}, 11.0, false, true));
    assert!(blocked(|_| {}, 0.0, true, true));
    assert!(blocked(|_| {}, 0.0, false, false));
}

#[test]
fn a_panicking_tame_cat_teleports_to_a_far_owner() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(20.5, 64.0, 0.5))];
    let mut ai = cat_ai();
    ai.reassess_cat_tame_goals(true);
    let actions = actions_of(&mut ai, &mut base, &state, &world, 12, |view| {
        tamed(view);
        view.panic = true;
    });
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, MobAction::TeleportTo(_)))
    );
}

#[test]
fn a_wild_cat_never_teleports_while_panicking_and_a_near_owner_keeps_it_in_place() {
    let teleports = |tame: bool, distance: f64| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut world = World::new();
        world.players = vec![owner_at(Vec3::new(0.5 + distance, 64.0, 0.5))];
        let mut ai = cat_ai();
        let actions = actions_of(&mut ai, &mut base, &state, &world, 12, |view| {
            if tame {
                tamed(view);
            }
            view.panic = true;
        });
        actions
            .iter()
            .any(|a| matches!(a, MobAction::TeleportTo(_)))
    };
    assert!(!teleports(false, 20.0));
    assert!(!teleports(true, 5.0));
}

#[test]
fn a_wild_cat_hunts_rabbits_and_a_tamed_one_does_not() {
    let hunts = |tame: bool| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut world = owned_world();
        world.others = vec![living(7, EntityKind::Rabbit, Vec3::new(3.5, 64.0, 0.5))];
        let mut ai = cat_ai();
        for _ in 0..80 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                if tame {
                    tamed(view);
                }
            });
            if ai.target() == Some(EntityId(7)) {
                return true;
            }
        }
        false
    };
    assert!(hunts(false));
    assert!(!hunts(true));
}

#[test]
fn a_wild_cat_hunts_baby_turtles_on_land_only() {
    let hunts = |baby: bool, in_water: bool| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut world = World::new();
        let mut turtle = living(8, EntityKind::Turtle, Vec3::new(3.5, 64.0, 0.5));
        turtle.baby = baby;
        turtle.in_water = in_water;
        world.others = vec![turtle];
        let mut ai = cat_ai();
        for _ in 0..80 {
            step2(&mut ai, &mut base, &state, &world, |_| {});
            if ai.target() == Some(EntityId(8)) {
                return true;
            }
        }
        false
    };
    assert!(hunts(true, false));
    assert!(!hunts(false, false));
    assert!(!hunts(true, true));
}

#[test]
fn only_tame_cats_breed_with_tame_cats() {
    let breeds = |tame: bool, partner_tame: bool| {
        let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
        let mut world = owned_world();
        let mut partner = living(9, EntityKind::Cat, Vec3::new(2.5, 64.0, 0.5));
        partner.in_love = true;
        partner.facts.tamed = partner_tame;
        world.others = vec![partner];
        let mut ai = cat_ai();
        for _ in 0..4 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                view.in_love = true;
                if tame {
                    tamed(view);
                }
            });
            if running(&ai, |goal| matches!(goal, MobGoal::Breed(_))) {
                return true;
            }
        }
        false
    };
    assert!(breeds(true, true));
    assert!(!breeds(true, false));
    assert!(!breeds(false, true));
}

#[test]
fn the_tempt_goal_reports_running_while_following() {
    let (mut base, state) = cat(Vec3::new(0.5, 64.0, 0.5));
    let mut world = World::new();
    let mut player = owner_at(Vec3::new(5.5, 64.0, 0.5));
    player.main_hand = Some(ItemKind::Cod.id());
    world.players = vec![player];
    let mut ai = cat_ai();
    assert!(!ai.is_tempting());
    actions_of(&mut ai, &mut base, &state, &world, 4, |_| {});
    assert!(ai.is_tempting());
}
