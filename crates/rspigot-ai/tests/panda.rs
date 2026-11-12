use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiContext, AiWorld, BlockView, DroppedItem, Facts, Flag, Goal, GoalSettings, Living, MobAction,
    MobAi, MobBody, MobGoal, MobView, PandaAction, PandaBrain, PandaFacts, PandaGoal, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::panda::Gene;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    aggressive: Vec<EntityId>,
    blocks: HashMap<BlockPos, StateId>,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            items: Vec::new(),
            aggressive: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(100),
        }
    }
}

impl BlockView for World {
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

    fn items_in(&self, area: &Aabb) -> Vec<DroppedItem> {
        self.items
            .iter()
            .filter(|item| item.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn aggressive_panda(&self, id: EntityId) -> bool {
        self.aggressive.contains(&id)
    }
}

const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

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

fn mob(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Panda);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn view(base: &EntityBase, state: &LivingState, panda: PandaFacts) -> MobView {
    let mut view = MobView::of(base, state);
    view.species.panda = panda;
    view
}

fn ai(seed: i64) -> MobAi {
    let mut ai = MobAi::new(EntityKind::Panda, seed, GoalSettings::default());
    ai.brain_mut().panda.active = true;
    ai
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

fn running(ai: &MobAi, matches: impl Fn(&PandaGoal) -> bool) -> bool {
    ai.goals().goals().iter().any(|goal| {
        goal.is_running()
            && match goal.goal() {
                MobGoal::Panda(panda) => matches(panda),
                _ => false,
            }
    })
}

fn pandas(actions: &[MobAction]) -> Vec<PandaAction> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Panda(action) => Some(*action),
            _ => None,
        })
        .collect()
}

fn with_context<R>(
    ai: &mut MobAi,
    view: &MobView,
    world: &World,
    run: impl FnOnce(&mut AiContext<'_, World>) -> R,
) -> R {
    let mut body = MobBody::of(view);
    let mut context = AiContext {
        mob: view,
        world,
        brain: ai.brain_mut(),
        body: &mut body,
    };
    run(&mut context)
}

fn panda_goal(ai: &MobAi, pick: impl Fn(&PandaGoal) -> bool) -> PandaGoal {
    ai.goals()
        .goals()
        .iter()
        .chain(ai.targets().goals())
        .find_map(|goal| match goal.goal() {
            MobGoal::Panda(panda) if pick(panda) => Some(panda.clone()),
            _ => None,
        })
        .expect("panda goal")
}

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::Tempt(_) => "tempt",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::FollowParent(_) => "follow_parent",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::Panda(panda) => match panda {
            PandaGoal::Panic(_) => "panic",
            PandaGoal::Breed(_) => "breed",
            PandaGoal::Attack(_) => "attack",
            PandaGoal::Avoid(_) => "avoid",
            PandaGoal::Sit(_) => "sit",
            PandaGoal::LieOnBack(_) => "lie_on_back",
            PandaGoal::Sneeze => "sneeze",
            PandaGoal::LookAtPlayer(_) => "look_at_player",
            PandaGoal::Roll => "roll",
            PandaGoal::HurtByTarget(_) => "hurt_by",
        },
        _ => "other",
    }
}

#[test]
fn pandas_register_the_vanilla_goals_in_order() {
    let ai = MobAi::new(EntityKind::Panda, 1, GoalSettings::default());
    let summary: Vec<(i32, &str)> = ai
        .goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect();
    assert_eq!(
        summary,
        vec![
            (0, "float"),
            (2, "panic"),
            (2, "breed"),
            (3, "attack"),
            (4, "tempt"),
            (6, "avoid"),
            (6, "avoid"),
            (7, "sit"),
            (8, "lie_on_back"),
            (8, "sneeze"),
            (9, "look_at_player"),
            (10, "look_around"),
            (12, "roll"),
            (13, "follow_parent"),
            (14, "stroll"),
        ]
    );
    let targets: Vec<(i32, &str)> = ai
        .targets()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect();
    assert_eq!(targets, vec![(1, "hurt_by")]);
    assert!(ai.can_float());
    assert!(ai.brain().panda.active);
}

#[test]
fn pandas_declare_the_vanilla_control_flags_and_interruption() {
    let ai = MobAi::new(EntityKind::Panda, 1, GoalSettings::default());
    let flags = |name: &str| -> Vec<Flag> {
        let goal = ai
            .goals()
            .goals()
            .iter()
            .chain(ai.targets().goals())
            .find(|goal| label(goal.goal()) == name)
            .expect("goal");
        Goal::<AiContext<'_, World>>::flags(goal.goal())
            .iter()
            .collect()
    };
    assert_eq!(flags("panic"), vec![Flag::Move]);
    assert_eq!(flags("breed"), vec![Flag::Move, Flag::Look]);
    assert_eq!(flags("attack"), vec![Flag::Move, Flag::Look]);
    assert_eq!(flags("avoid"), vec![Flag::Move]);
    assert_eq!(flags("sit"), vec![Flag::Move]);
    assert_eq!(flags("lie_on_back"), vec![]);
    assert_eq!(flags("sneeze"), vec![]);
    assert_eq!(flags("look_at_player"), vec![Flag::Look]);
    assert_eq!(flags("roll"), vec![Flag::Move, Flag::Look, Flag::Jump]);
    assert_eq!(flags("hurt_by"), vec![Flag::Target]);
    let interruptable = |name: &str| {
        let goal = ai
            .goals()
            .goals()
            .iter()
            .find(|goal| label(goal.goal()) == name)
            .expect("goal");
        Goal::<AiContext<'_, World>>::is_interruptable(goal.goal())
    };
    assert!(!interruptable("roll"));
    assert!(interruptable("sit"));
    assert!(interruptable("panic"));
    let every_tick = |name: &str| {
        let goal = ai
            .goals()
            .goals()
            .iter()
            .find(|goal| label(goal.goal()) == name)
            .expect("goal");
        Goal::<AiContext<'_, World>>::requires_update_every_tick(goal.goal())
    };
    assert!(every_tick("attack"));
    assert!(every_tick("float"));
    assert!(!every_tick("sit"));
    assert!(!every_tick("panic"));
}

#[test]
fn can_perform_action_needs_no_blocking_state() {
    assert!(PandaFacts::default().can_perform_action());
    for blocked in [
        PandaFacts {
            sitting: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            on_back: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            rolling: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            eating: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            variant: Gene::Worried,
            thundering: true,
            ..PandaFacts::default()
        },
    ] {
        assert!(!blocked.can_perform_action(), "{blocked:?}");
    }
    let calm_worried = PandaFacts {
        variant: Gene::Worried,
        ..PandaFacts::default()
    };
    assert!(calm_worried.can_perform_action());
    let thundery_normal = PandaFacts {
        thundering: true,
        ..PandaFacts::default()
    };
    assert!(thundery_normal.can_perform_action());
}

#[test]
fn the_move_control_only_runs_when_the_panda_can_act() {
    let mut brain = PandaBrain::default();
    assert!(brain.can_move());
    brain.active = true;
    assert!(brain.can_move());
    brain.facts.sitting = true;
    assert!(!brain.can_move());
}

#[test]
fn panicking_pandas_run_at_twice_the_speed() {
    let (mut base, state) = mob(HOME);
    let mut ai = ai(7);
    let world = World::new();
    for _ in 0..6 {
        step(&mut ai, &mut base, &state, &world, |view| view.panic = true);
    }
    assert!(ai.is_panicking());
    assert!(running(&ai, PandaGoal::is_panic));
    assert_eq!(ai.brain().navigation.speed_modifier(), 2.0);
}

#[test]
fn a_sitting_panda_stops_panicking() {
    let (base, state) = mob(HOME);
    let mut ai = ai(7);
    let world = World::new();
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Panic(_)));
    let calm = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &calm, &world, |context| {
        context.brain.panda.facts = calm.species.panda;
        assert!(!Goal::can_continue_to_use(&mut goal, context));
        assert!(context.brain.navigation.is_done());
    });
    let sitting = view(
        &base,
        &state,
        PandaFacts {
            sitting: true,
            ..PandaFacts::default()
        },
    );
    with_context(&mut ai, &sitting, &world, |context| {
        context.brain.panda.facts = sitting.species.panda;
        assert!(!Goal::can_continue_to_use(&mut goal, context));
        assert!(context.brain.navigation.is_done());
    });
}

#[test]
fn breeding_without_bamboo_makes_the_panda_unhappy_and_look_at_a_player() {
    let (base, state) = mob(HOME);
    let mut ai = ai(3);
    let mut world = World::new();
    let mut partner = living(2, EntityKind::Panda, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = true;
    world.others = vec![partner];
    world.players = vec![player(Vec3::new(4.5, 64.0, 0.5))];
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Breed(_)));
    let mut loving = view(&base, &state, PandaFacts::default());
    loving.in_love = true;
    with_context(&mut ai, &loving, &world, |context| {
        context.brain.panda.facts = loving.species.panda;
        assert!(!Goal::can_use(&mut goal, context));
        assert_eq!(context.brain.panda.facts.unhappy_counter, 32);
        assert_eq!(context.brain.panda.look_at, Some(EntityId(100)));
        let actions = std::mem::take(&mut context.brain.actions);
        assert_eq!(actions, vec![MobAction::Panda(PandaAction::Unhappy(32))]);
        context.brain.panda.facts.unhappy_counter = 0;
        assert!(!Goal::can_use(&mut goal, context));
        assert!(context.brain.actions.is_empty());
    });
}

#[test]
fn breeding_retries_unhappiness_after_six_hundred_ticks() {
    let (mut base, state) = mob(HOME);
    let mut ai = ai(3);
    let mut world = World::new();
    let mut partner = living(2, EntityKind::Panda, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = true;
    world.others = vec![partner];
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Breed(_)));
    base.tick_count = 10;
    let mut loving = view(&base, &state, PandaFacts::default());
    loving.in_love = true;
    with_context(&mut ai, &loving, &world, |context| {
        assert!(!Goal::can_use(&mut goal, context));
        context.brain.actions.clear();
        context.brain.panda.facts.unhappy_counter = 0;
        assert!(!Goal::can_use(&mut goal, context));
        assert!(context.brain.actions.is_empty());
    });
    base.tick_count = 10 + 600;
    let later = {
        let mut later = view(&base, &state, PandaFacts::default());
        later.in_love = true;
        later
    };
    with_context(&mut ai, &later, &world, |context| {
        assert!(!Goal::can_use(&mut goal, context));
        assert_eq!(context.brain.actions.len(), 1);
    });
}

#[test]
fn breeding_goes_ahead_when_bamboo_is_within_seven_blocks() {
    let (base, state) = mob(HOME);
    let mut loving = view(&base, &state, PandaFacts::default());
    loving.in_love = true;
    let mut partner = living(2, EntityKind::Panda, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = true;
    for (offset, found) in [
        ((7, 0, 0), true),
        ((-7, 2, 7), true),
        ((0, 1, -7), true),
        ((8, 0, 0), false),
        ((0, 3, 0), false),
        ((0, -1, 0), false),
        ((0, 0, 8), false),
    ] {
        let mut ai = ai(3);
        let mut world = World::new();
        world.others = vec![partner];
        world.blocks.insert(
            BlockPos::new(0, 64, 0).offset(offset.0, offset.1, offset.2),
            BlockKind::Bamboo.default_state(),
        );
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Breed(_)));
        let usable = with_context(&mut ai, &loving, &world, |context| {
            Goal::can_use(&mut goal, context)
        });
        assert_eq!(usable, found, "{offset:?}");
    }
}

#[test]
fn an_unhappy_panda_does_not_breed_even_with_bamboo() {
    let (base, state) = mob(HOME);
    let mut loving = view(
        &base,
        &state,
        PandaFacts {
            unhappy_counter: 12,
            ..PandaFacts::default()
        },
    );
    loving.in_love = true;
    let mut partner = living(2, EntityKind::Panda, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = true;
    let mut ai = ai(3);
    let mut world = World::new();
    world.others = vec![partner];
    world
        .blocks
        .insert(BlockPos::new(1, 64, 0), BlockKind::Bamboo.default_state());
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Breed(_)));
    with_context(&mut ai, &loving, &world, |context| {
        context.brain.panda.facts = loving.species.panda;
        assert!(!Goal::can_use(&mut goal, context));
        assert!(context.brain.actions.is_empty());
    });
}

#[test]
fn only_an_idle_panda_attacks() {
    let attacks = |panda: PandaFacts| {
        let (mut base, state) = mob(HOME);
        let mut ai = ai(5);
        let mut world = World::new();
        world.others = vec![living(2, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5))];
        ai.set_target(Some(EntityId(2)));
        let mut ever = false;
        for _ in 0..6 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.species.panda = panda;
            });
            ever |= running(&ai, |goal| matches!(goal, PandaGoal::Attack(_)));
        }
        ever
    };
    assert!(attacks(PandaFacts::default()));
    for blocked in [
        PandaFacts {
            sitting: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            on_back: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            eating: true,
            ..PandaFacts::default()
        },
        PandaFacts {
            rolling: true,
            ..PandaFacts::default()
        },
    ] {
        assert!(!attacks(blocked), "{blocked:?}");
    }
}

#[test]
fn only_worried_pandas_flee_players_and_monsters() {
    let flees = |variant: Gene, creative: bool, spectator: bool, monster: bool| {
        let (mut base, state) = mob(HOME);
        let mut ai = ai(3);
        let mut world = World::new();
        if monster {
            world.others = vec![living(2, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5))];
        } else {
            let mut near = player(Vec3::new(5.5, 64.0, 0.5));
            near.creative = creative;
            near.spectator = spectator;
            world.players = vec![near];
        }
        let facts = PandaFacts {
            variant,
            ..PandaFacts::default()
        };
        let mut ever = false;
        for _ in 0..40 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.species.panda = facts;
            });
            ever |= running(&ai, |goal| matches!(goal, PandaGoal::Avoid(_)));
        }
        ever
    };
    assert!(flees(Gene::Worried, false, false, false));
    assert!(flees(Gene::Worried, true, false, false));
    assert!(!flees(Gene::Worried, false, true, false));
    assert!(!flees(Gene::Normal, false, false, false));
    assert!(!flees(Gene::Lazy, false, false, true));
    assert!(flees(Gene::Worried, false, false, true));
}

#[test]
fn pandas_flee_players_from_eight_blocks_but_monsters_from_four() {
    let flees = |monster: bool, distance: f64| {
        let (mut base, state) = mob(HOME);
        let mut ai = ai(3);
        let mut world = World::new();
        if monster {
            world.others = vec![living(
                2,
                EntityKind::Zombie,
                Vec3::new(0.5 + distance, 64.0, 0.5),
            )];
        } else {
            world.players = vec![player(Vec3::new(0.5 + distance, 64.0, 0.5))];
        }
        let facts = PandaFacts {
            variant: Gene::Worried,
            ..PandaFacts::default()
        };
        let mut ever = false;
        for _ in 0..40 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.species.panda = facts;
            });
            ever |= running(&ai, |goal| matches!(goal, PandaGoal::Avoid(_)));
        }
        ever
    };
    assert!(flees(false, 7.0));
    assert!(!flees(false, 10.0));
    assert!(flees(true, 3.0));
    assert!(!flees(true, 6.0));
}

#[test]
fn a_panda_holding_food_sits_down_and_stops_walking() {
    let (mut base, state) = mob(HOME);
    let mut ai = ai(9);
    let world = World::new();
    let holding = PandaFacts {
        holding: true,
        ..PandaFacts::default()
    };
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.extend(pandas(&step(&mut ai, &mut base, &state, &world, |view| {
            view.species.panda = holding;
        })));
    }
    assert!(running(&ai, |goal| matches!(goal, PandaGoal::Sit(_))));
    assert!(seen.contains(&PandaAction::TryToSit));
    assert!(ai.brain().navigation.is_done());
}

#[test]
fn sitting_does_not_start_for_babies_swimmers_blocked_or_unhappy_pandas() {
    let (base, state) = mob(HOME);
    let world = World::new();
    let holding = PandaFacts {
        holding: true,
        ..PandaFacts::default()
    };
    let starts = |baby: bool, panda: PandaFacts, in_water: bool| {
        let mut ai = ai(9);
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sit(_)));
        let mut subject = view(&base, &state, panda);
        subject.baby = baby;
        subject.fluid.in_water = in_water;
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = panda;
            Goal::can_use(&mut goal, context)
        })
    };
    assert!(starts(false, holding, false));
    assert!(!starts(true, holding, false));
    assert!(!starts(false, holding, true));
    assert!(!starts(
        false,
        PandaFacts {
            unhappy_counter: 1,
            ..holding
        },
        false
    ));
    assert!(!starts(
        false,
        PandaFacts {
            on_back: true,
            ..holding
        },
        false
    ));
    assert!(!starts(false, PandaFacts::default(), false));
}

#[test]
fn an_empty_handed_panda_sits_only_for_edible_items_within_six_blocks() {
    let (base, state) = mob(HOME);
    let item = |x: f64, id: i32| DroppedItem {
        id: EntityId(id),
        position: Vec3::new(x, 64.0, 0.5),
        bounds: Aabb::of_size(Vec3::new(x, 64.0, 0.5), 0.25, 0.25),
        item: ItemKind::Bamboo.id(),
        alive: true,
        has_pickup_delay: false,
    };
    let starts = |items: Vec<DroppedItem>| {
        let mut ai = ai(9);
        let mut world = World::new();
        world.items = items;
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sit(_)));
        let subject = view(&base, &state, PandaFacts::default());
        with_context(&mut ai, &subject, &world, |context| {
            Goal::can_use(&mut goal, context)
        })
    };
    assert!(starts(vec![item(4.5, 5)]));
    assert!(!starts(vec![item(12.5, 5)]));
    assert!(!starts(vec![]));
    let mut delayed = item(2.5, 5);
    delayed.has_pickup_delay = true;
    assert!(!starts(vec![delayed]));
    let mut dead = item(2.5, 5);
    dead.alive = false;
    assert!(!starts(vec![dead]));
    let mut other = item(2.5, 5);
    other.item = ItemKind::Stick.id();
    assert!(!starts(vec![other]));
}

#[test]
fn an_empty_handed_panda_walks_to_the_nearest_listed_item_when_it_starts_sitting() {
    let (base, state) = mob(HOME);
    let mut ai = ai(9);
    let mut world = World::new();
    world.items = vec![DroppedItem {
        id: EntityId(5),
        position: Vec3::new(6.5, 64.0, 0.5),
        bounds: Aabb::of_size(Vec3::new(6.5, 64.0, 0.5), 0.25, 0.25),
        item: ItemKind::Bamboo.id(),
        alive: true,
        has_pickup_delay: false,
    }];
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sit(_)));
    let subject = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &subject, &world, |context| {
        Goal::start(&mut goal, context);
        assert!(!context.brain.navigation.is_done());
        assert_eq!(
            context.brain.navigation.speed_modifier(),
            1.2000000476837158
        );
        assert!(context.brain.actions.is_empty());
    });
}

#[test]
fn leaving_the_sit_goal_drops_the_item_and_stands_up() {
    let (base, state) = mob(HOME);
    let mut ai = ai(9);
    let world = World::new();
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sit(_)));
    let holding = PandaFacts {
        holding: true,
        sitting: true,
        variant: Gene::Lazy,
        ..PandaFacts::default()
    };
    let subject = view(&base, &state, holding);
    with_context(&mut ai, &subject, &world, |context| {
        context.brain.panda.facts = holding;
        Goal::stop(&mut goal, context);
        let actions = pandas(&context.brain.actions);
        assert_eq!(
            actions,
            vec![PandaAction::DropHeld, PandaAction::Sit(false)]
        );
        assert!(!context.brain.panda.facts.holding);
        assert!(!context.brain.panda.facts.sitting);
    });
    let PandaGoal::Sit(sit) = &goal else {
        panic!("sit");
    };
    let cooldown = format!("{sit:?}");
    assert!(!cooldown.contains("cooldown: 0"), "{cooldown}");
    let again = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &again, &world, |context| {
        context.brain.panda.facts.holding = true;
        assert!(!Goal::can_use(&mut goal, context));
    });
}

#[test]
fn the_sit_goal_ends_at_random_unless_in_water() {
    let (base, state) = mob(HOME);
    let world = World::new();
    let ends = |variant: Gene, in_water: bool| {
        let mut ai = ai(21);
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sit(_)));
        let facts = PandaFacts {
            variant,
            sitting: true,
            holding: true,
            ..PandaFacts::default()
        };
        let mut subject = view(&base, &state, facts);
        subject.fluid.in_water = in_water;
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = facts;
            (0..4000).any(|_| !Goal::can_continue_to_use(&mut goal, context))
        })
    };
    assert!(ends(Gene::Normal, false));
    assert!(ends(Gene::Lazy, false));
    assert!(ends(Gene::Normal, true));
}

#[test]
fn lazy_pandas_lie_on_their_back_then_rest_before_trying_again() {
    let (mut base, state) = mob(HOME);
    let mut ai = ai(13);
    let world = World::new();
    let lazy = PandaFacts {
        variant: Gene::Lazy,
        ..PandaFacts::default()
    };
    let mut started = false;
    for _ in 0..4000 {
        let actions = step(&mut ai, &mut base, &state, &world, |view| {
            view.species.panda = lazy;
        });
        if pandas(&actions).contains(&PandaAction::OnBack(true)) {
            started = true;
            break;
        }
    }
    assert!(started);
    assert!(running(&ai, |goal| matches!(goal, PandaGoal::LieOnBack(_))));
    assert!(ai.brain().panda.facts.on_back);
}

#[test]
fn only_lazy_pandas_that_can_act_lie_on_their_back() {
    let ever = |panda: PandaFacts| {
        let (mut base, state) = mob(HOME);
        let mut ai = ai(13);
        let world = World::new();
        (0..3000).any(|_| {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.species.panda = panda;
            })
            .iter()
            .any(|action| matches!(action, MobAction::Panda(PandaAction::OnBack(true))))
        })
    };
    assert!(!ever(PandaFacts::default()));
    assert!(!ever(PandaFacts {
        variant: Gene::Playful,
        ..PandaFacts::default()
    }));
    assert!(!ever(PandaFacts {
        variant: Gene::Lazy,
        sitting: true,
        ..PandaFacts::default()
    }));
}

#[test]
fn getting_up_from_the_back_waits_two_hundred_ticks() {
    let (mut base, state) = mob(HOME);
    base.tick_count = 1000;
    let mut ai = ai(13);
    let world = World::new();
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::LieOnBack(_)));
    let lazy = PandaFacts {
        variant: Gene::Lazy,
        on_back: true,
        ..PandaFacts::default()
    };
    let subject = view(&base, &state, lazy);
    with_context(&mut ai, &subject, &world, |context| {
        context.brain.panda.facts = lazy;
        Goal::stop(&mut goal, context);
        assert_eq!(
            pandas(&context.brain.actions),
            vec![PandaAction::OnBack(false)]
        );
        assert!(!context.brain.panda.facts.on_back);
    });
    let PandaGoal::LieOnBack(back) = &goal else {
        panic!("back");
    };
    assert!(format!("{back:?}").contains("cooldown: 1200"));
    let rested = PandaFacts {
        variant: Gene::Lazy,
        ..PandaFacts::default()
    };
    let subject = view(&base, &state, rested);
    with_context(&mut ai, &subject, &world, |context| {
        context.brain.panda.facts = rested;
        assert!((0..4000).all(|_| !Goal::can_use(&mut goal, context)));
    });
}

#[test]
fn only_babies_sneeze_and_weak_ones_more_often() {
    let count = |baby: bool, variant: Gene, blocked: bool| {
        let mut ai = ai(17);
        let (base, state) = mob(HOME);
        let world = World::new();
        let facts = PandaFacts {
            variant,
            sitting: blocked,
            ..PandaFacts::default()
        };
        let mut subject = view(&base, &state, facts);
        subject.baby = baby;
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sneeze));
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = facts;
            (0..30_000)
                .filter(|_| Goal::can_use(&mut goal, context))
                .count()
        })
    };
    assert_eq!(count(false, Gene::Weak, false), 0);
    assert_eq!(count(true, Gene::Weak, true), 0);
    let normal = count(true, Gene::Normal, false);
    let weak = count(true, Gene::Weak, false);
    assert!(normal > 0);
    assert!(weak > normal * 5);
}

#[test]
fn sneezing_flags_the_panda_and_never_continues() {
    let (base, state) = mob(HOME);
    let mut ai = ai(17);
    let world = World::new();
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Sneeze));
    let subject = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &subject, &world, |context| {
        Goal::start(&mut goal, context);
        assert_eq!(
            pandas(&context.brain.actions),
            vec![PandaAction::Sneeze(true)]
        );
        assert!(!Goal::can_continue_to_use(&mut goal, context));
    });
}

#[test]
fn babies_and_playful_pandas_roll_off_ledges_at_once() {
    let (base, state) = mob(HOME);
    let facts = PandaFacts::default();
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(0, 63, 1), StateId::AIR);
    let mut baby = view(&base, &state, facts);
    baby.baby = true;
    let mut ai = ai(1);
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Roll));
    let rolls = with_context(&mut ai, &baby, &world, |context| {
        Goal::can_use(&mut goal, context)
    });
    assert!(rolls);
    let adult = view(&base, &state, facts);
    let rolls = with_context(&mut ai, &adult, &world, |context| {
        Goal::can_use(&mut goal, context)
    });
    assert!(!rolls);
}

#[test]
fn rolling_needs_the_ground_and_a_free_panda() {
    let (base, state) = mob(HOME);
    let mut world = World::new();
    world.blocks.insert(BlockPos::new(0, 63, 1), StateId::AIR);
    let rolls = |baby: bool, panda: PandaFacts, on_ground: bool| {
        let mut ai = ai(1);
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Roll));
        let mut subject = view(&base, &state, panda);
        subject.baby = baby;
        subject.on_ground = on_ground;
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = panda;
            Goal::can_use(&mut goal, context)
        })
    };
    assert!(rolls(true, PandaFacts::default(), true));
    assert!(!rolls(true, PandaFacts::default(), false));
    assert!(!rolls(
        true,
        PandaFacts {
            sitting: true,
            ..PandaFacts::default()
        },
        true
    ));
    assert!(rolls(
        false,
        PandaFacts {
            variant: Gene::Playful,
            ..PandaFacts::default()
        },
        true
    ));
}

#[test]
fn rolling_on_flat_ground_is_random_and_only_for_the_young_or_playful() {
    let (base, state) = mob(HOME);
    let world = World::new();
    let count = |baby: bool, variant: Gene| {
        let mut ai = ai(8);
        let facts = PandaFacts {
            variant,
            ..PandaFacts::default()
        };
        let mut subject = view(&base, &state, facts);
        subject.baby = baby;
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Roll));
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = facts;
            (0..20_000)
                .filter(|_| Goal::can_use(&mut goal, context))
                .count()
        })
    };
    assert_eq!(count(false, Gene::Normal), 0);
    let baby = count(true, Gene::Normal);
    let playful = count(false, Gene::Playful);
    assert!(baby > 20 && baby < 100, "{baby}");
    assert!(playful > baby * 3, "{playful}");
}

#[test]
fn rolling_starts_the_roll_and_ends_immediately() {
    let (base, state) = mob(HOME);
    let mut ai = ai(8);
    let world = World::new();
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::Roll));
    let subject = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &subject, &world, |context| {
        Goal::start(&mut goal, context);
        assert_eq!(
            pandas(&context.brain.actions),
            vec![PandaAction::Roll(true)]
        );
        assert!(context.brain.panda.facts.rolling);
        assert!(!Goal::can_continue_to_use(&mut goal, context));
    });
}

#[test]
fn the_panda_look_goal_follows_the_player_picked_by_the_breed_goal() {
    let (base, state) = mob(HOME);
    let mut ai = ai(30);
    let mut world = World::new();
    world.players = vec![player(Vec3::new(30.5, 64.0, 0.5))];
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::LookAtPlayer(_)));
    let subject = view(&base, &state, PandaFacts::default());
    with_context(&mut ai, &subject, &world, |context| {
        assert!((0..2000).all(|_| !Goal::can_use(&mut goal, context)));
        assert_eq!(context.brain.panda.look_at, None);
        context.brain.panda.look_at = Some(EntityId(100));
        assert!((0..2000).any(|_| Goal::can_use(&mut goal, context)));
        Goal::start(&mut goal, context);
        assert!(!Goal::can_continue_to_use(&mut goal, context));
        Goal::stop(&mut goal, context);
        assert_eq!(context.brain.panda.look_at, None);
    });
}

#[test]
fn the_panda_look_goal_finds_nearby_players_but_not_when_busy() {
    let (base, state) = mob(HOME);
    let mut world = World::new();
    world.players = vec![player(Vec3::new(4.5, 64.0, 0.5))];
    let finds = |panda: PandaFacts| {
        let mut ai = ai(30);
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::LookAtPlayer(_)));
        let subject = view(&base, &state, panda);
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = panda;
            (0..2000).any(|_| Goal::can_use(&mut goal, context))
        })
    };
    assert!(finds(PandaFacts::default()));
    assert!(!finds(PandaFacts {
        sitting: true,
        ..PandaFacts::default()
    }));
    let mut far = World::new();
    far.players = vec![player(Vec3::new(9.5, 64.0, 0.5))];
    let mut ai = ai(30);
    let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::LookAtPlayer(_)));
    let subject = view(&base, &state, PandaFacts::default());
    let any = with_context(&mut ai, &subject, &far, |context| {
        (0..2000).any(|_| Goal::can_use(&mut goal, context))
    });
    assert!(!any);
}

#[test]
fn a_hurt_panda_targets_its_attacker_and_calls_only_aggressive_pandas() {
    let (mut base, state) = mob(HOME);
    let mut ai = ai(4);
    let mut world = World::new();
    let attacker = living(2, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5));
    let mut friend = living(3, EntityKind::Panda, Vec3::new(5.5, 64.0, 0.5));
    friend.has_target = false;
    let mut bystander = living(4, EntityKind::Panda, Vec3::new(6.5, 64.0, 0.5));
    bystander.has_target = false;
    let mut busy = living(5, EntityKind::Panda, Vec3::new(7.5, 64.0, 0.5));
    busy.has_target = true;
    world.others = vec![attacker, friend, bystander, busy];
    world.aggressive = vec![EntityId(3), EntityId(5)];
    let mut actions = Vec::new();
    for _ in 0..4 {
        actions.extend(step(&mut ai, &mut base, &state, &world, |view| {
            view.last_hurt_by_mob = Some(EntityId(2));
            view.last_hurt_by_mob_at = 90;
        }));
    }
    assert_eq!(ai.target(), Some(EntityId(2)));
    let alerts: Vec<PandaAction> = pandas(&actions)
        .into_iter()
        .filter(|action| matches!(action, PandaAction::Alert { .. }))
        .collect();
    assert_eq!(
        alerts,
        vec![PandaAction::Alert {
            mob: EntityId(3),
            target: EntityId(2)
        }]
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, MobAction::Alert { .. }))
    );
}

#[test]
fn a_panda_that_got_bamboo_or_bit_drops_its_target() {
    let (base, state) = mob(HOME);
    let world = {
        let mut world = World::new();
        world.others = vec![living(2, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5))];
        world
    };
    let outcome = |got_bamboo: bool, did_bite: bool| {
        let mut ai = ai(4);
        ai.set_target(Some(EntityId(2)));
        let mut goal = panda_goal(&ai, |goal| matches!(goal, PandaGoal::HurtByTarget(_)));
        let facts = PandaFacts {
            got_bamboo,
            did_bite,
            ..PandaFacts::default()
        };
        let subject = view(&base, &state, facts);
        with_context(&mut ai, &subject, &world, |context| {
            context.brain.panda.facts = facts;
            let keeps = Goal::can_continue_to_use(&mut goal, context);
            let requests = context.brain.target_requests.len();
            (keeps, requests)
        })
    };
    assert_eq!(outcome(true, false), (false, 1));
    assert_eq!(outcome(false, true), (false, 1));
    let (keeps, _) = outcome(false, false);
    assert!(keeps);
}
