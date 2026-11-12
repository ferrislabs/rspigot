use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, DroppedItem, Facts, FoxAction, FoxGoal, GoalSettings, Living, MobAction, MobAi,
    MobBody, MobGoal, MobView, TargetKind, TargetReason,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::fox::{
    FLAG_CROUCHING, FLAG_DEFENDING, FLAG_FACEPLANTED, FLAG_INTERESTED, FLAG_POUNCING, FLAG_SITTING,
    FLAG_SLEEPING,
};
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    blocks: HashMap<BlockPos, StateId>,
    time: Cell<i64>,
    bright: bool,
    thunder: bool,
    village: i32,
    griefing: bool,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            items: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(0),
            bright: false,
            thunder: false,
            village: 7,
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
            BlockKind::GrassBlock.default_state()
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

    fn bright_outside(&self) -> bool {
        self.bright
    }

    fn thundering(&self) -> bool {
        self.thunder
    }

    fn mob_griefing(&self) -> bool {
        self.griefing
    }

    fn items_in(&self, area: &Aabb) -> Vec<DroppedItem> {
        self.items
            .iter()
            .filter(|item| item.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn sections_to_village(&self, _pos: BlockPos) -> i32 {
        self.village
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
        facts: Facts {
            uuid: Uuid::from_u128(id as u128 + 1000),
            ..Facts::default()
        },
    }
}

fn fox(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Fox);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn actions_of(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: &impl Fn(&mut MobView),
) -> (Vec<MobAction>, MobBody) {
    base.tick_count += 1;
    world.time.set(world.time.get() + 1);
    let mut view = MobView::of(base, state);
    adjust(&mut view);
    let mut body = MobBody::of(&view);
    ai.think(&view, world, &mut body);
    (ai.take_actions(), body)
}

fn step2(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    let (mut actions, _) = actions_of(ai, base, state, world, &adjust);
    actions.extend(actions_of(ai, base, state, world, &adjust).0);
    actions
}

fn new_ai() -> MobAi {
    MobAi::new(EntityKind::Fox, 9, GoalSettings::default())
}

fn running_fox(ai: &MobAi, matches: impl Fn(&FoxGoal) -> bool) -> bool {
    ai.goals()
        .goals()
        .iter()
        .any(|goal| goal.is_running() && matches!(goal.goal(), MobGoal::Fox(fox) if matches(fox)))
}

fn flags(view: &mut MobView, flags: u8) {
    view.species.fox.flags = flags;
}

fn set_flags_of(actions: &[MobAction]) -> Option<u8> {
    actions.iter().rev().find_map(|action| match action {
        MobAction::Fox(FoxAction::SetFlags(flags)) => Some(*flags),
        _ => None,
    })
}

fn sounds(actions: &[MobAction]) -> Vec<&'static str> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Fox(FoxAction::Sound(sound)) => Some(*sound),
            _ => None,
        })
        .collect()
}

fn pos() -> Vec3 {
    Vec3::new(0.5, 64.0, 0.5)
}

#[test]
fn goals_are_registered_with_the_vanilla_priorities() {
    let ai = new_ai();
    let priorities: Vec<i32> = ai
        .goals()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    assert_eq!(
        priorities,
        vec![
            0, 0, 1, 2, 3, 4, 4, 4, 5, 6, 6, 7, 7, 8, 9, 10, 10, 11, 11, 12, 13
        ]
    );
    let targets: Vec<i32> = ai
        .targets()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    assert_eq!(targets, vec![3]);
}

#[test]
fn red_foxes_hunt_land_prey_first_and_snow_foxes_fish_first() {
    let mut red = new_ai();
    red.fox_target_goals(true);
    let priorities = |ai: &MobAi| -> Vec<i32> {
        ai.targets()
            .goals()
            .iter()
            .map(rspigot_ai::WrappedGoal::priority)
            .collect()
    };
    assert_eq!(priorities(&red), vec![3, 4, 4, 6]);
    let mut snow = new_ai();
    snow.fox_target_goals(false);
    assert_eq!(priorities(&snow), vec![3, 4, 6, 6]);
}

#[test]
fn a_fox_in_water_floats_and_forgets_its_states() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let world = World::new();
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        flags(view, FLAG_SITTING | FLAG_CROUCHING);
        view.fluid.water_height = 0.3;
        view.fluid.in_water = true;
    });
    assert!(running_fox(&ai, |goal| matches!(goal, FoxGoal::Float)));
    assert_eq!(set_flags_of(&actions), Some(0));
}

#[test]
fn shallow_water_does_not_make_a_fox_float() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let world = World::new();
    step2(&mut ai, &mut base, &state, &world, |view| {
        view.fluid.water_height = 0.2;
        view.fluid.in_water = true;
    });
    assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::Float)));
}

#[test]
fn a_faceplanted_fox_stays_down_for_twenty_ticks_then_gets_up() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let world = World::new();
    let mut last = None;
    let mut ticks = 0;
    for _ in 0..40 {
        let planted = last != Some(0);
        let actions = step2(&mut ai, &mut base, &state, &world, |view| {
            flags(view, if planted { FLAG_FACEPLANTED } else { 0 });
        });
        if running_fox(&ai, |goal| matches!(goal, FoxGoal::Faceplant { .. })) {
            ticks += 1;
        }
        if let Some(flags) = set_flags_of(&actions) {
            last = Some(flags);
        }
    }
    assert_eq!(last, Some(0));
    assert!((19..=21).contains(&ticks), "ran for {ticks}");
}

#[test]
fn defending_foxes_neither_panic_nor_follow_their_parent() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let world = World::new();
    step2(&mut ai, &mut base, &state, &world, |view| {
        flags(view, FLAG_DEFENDING);
        view.panic = true;
        view.age = -100;
    });
    assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::Panic(_))));
    step2(&mut ai, &mut base, &state, &world, |view| {
        flags(view, 0);
        view.panic = true;
    });
    assert!(running_fox(&ai, |goal| matches!(goal, FoxGoal::Panic(_))));
}

#[test]
fn foxes_avoid_players_that_are_not_trusted() {
    let player = living(100, EntityKind::Player, Vec3::new(6.5, 64.0, 0.5));
    let run = |trusted: bool, discrete: bool| {
        let (mut base, state) = fox(pos());
        let mut ai = new_ai();
        let mut world = World::new();
        let mut player = player;
        player.discrete = discrete;
        world.players = vec![player];
        let mut ever = false;
        for _ in 0..40 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                if trusted {
                    view.species.fox.trusted[0] = Some(rspigot_ai::TrustedRef {
                        uuid: player.facts.uuid,
                        id: Some(player.id),
                    });
                }
            });
            ever |=
                ai.goals().goals().iter().any(|goal| {
                    goal.is_running() && matches!(goal.goal(), MobGoal::AvoidEntity(_))
                });
        }
        ever
    };
    assert!(run(false, false));
    assert!(!run(true, false));
    assert!(!run(false, true));
}

#[test]
fn foxes_avoid_wild_wolves_and_polar_bears_unless_defending() {
    for (kind, tamed) in [
        (EntityKind::Wolf, false),
        (EntityKind::Wolf, true),
        (EntityKind::PolarBear, false),
    ] {
        for defending in [false, true] {
            let (mut base, state) = fox(pos());
            let mut ai = new_ai();
            let mut world = World::new();
            let mut other = living(50, kind, Vec3::new(4.5, 64.0, 0.5));
            other.facts.tamed = tamed;
            world.others = vec![other];
            let mut avoiding = false;
            for _ in 0..40 {
                step2(&mut ai, &mut base, &state, &world, |view| {
                    flags(view, if defending { FLAG_DEFENDING } else { 0 });
                });
                avoiding |= ai.goals().goals().iter().any(|goal| {
                    goal.is_running() && matches!(goal.goal(), MobGoal::AvoidEntity(_))
                });
            }
            assert_eq!(
                avoiding,
                !tamed && !defending,
                "{kind:?} {tamed} {defending}"
            );
        }
    }
}

#[test]
fn a_fox_sleeps_when_sheltered_by_day_and_wakes_when_it_has_a_reason() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    world.bright = true;
    let mut slept = None;
    for _ in 0..60 {
        let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
        if let Some(flags) = set_flags_of(&actions)
            && flags & FLAG_SLEEPING != 0
        {
            slept = Some(flags);
            break;
        }
    }
    assert_eq!(slept, Some(FLAG_SLEEPING));
    assert!(running_fox(&ai, |goal| matches!(goal, FoxGoal::Sleep(_))));
    world.others = vec![living(7, EntityKind::Chicken, Vec3::new(3.5, 64.0, 0.5))];
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        flags(view, FLAG_SLEEPING);
    });
    assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::Sleep(_))));
    assert_eq!(
        set_flags_of(&actions).map(|flags| flags & FLAG_SLEEPING),
        Some(0)
    );
}

#[test]
fn sleeping_needs_a_still_fox() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    world.bright = true;
    for _ in 0..60 {
        step2(&mut ai, &mut base, &state, &world, |view| {
            view.species.fox.moving = true;
        });
    }
    assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::Sleep(_))));
}

#[test]
fn a_fox_stalks_distant_prey_then_crouches_when_the_path_is_clear() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    ai.fox_target_goals(true);
    let mut world = World::new();
    let chicken = living(9, EntityKind::Chicken, Vec3::new(9.5, 64.0, 0.5));
    world.others = vec![chicken];
    ai.set_target(Some(chicken.id));
    let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
    assert!(running_fox(&ai, |goal| matches!(goal, FoxGoal::StalkPrey)));
    assert!(!ai.brain().navigation.is_done());
    assert!(set_flags_of(&actions).is_none());
    world.others = vec![living(9, EntityKind::Chicken, Vec3::new(3.5, 64.0, 0.5))];
    let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
    assert_eq!(
        set_flags_of(&actions).map(|flags| flags & (FLAG_CROUCHING | FLAG_INTERESTED)),
        Some(FLAG_CROUCHING | FLAG_INTERESTED)
    );
}

#[test]
fn a_fox_does_not_stalk_when_it_is_already_crouching_or_the_prey_is_not_stalkable() {
    for (kind, crouching) in [(EntityKind::Chicken, true), (EntityKind::Cow, false)] {
        let (mut base, state) = fox(pos());
        let mut ai = new_ai();
        let mut world = World::new();
        let prey = living(9, kind, Vec3::new(9.5, 64.0, 0.5));
        world.others = vec![prey];
        ai.set_target(Some(prey.id));
        step2(&mut ai, &mut base, &state, &world, |view| {
            flags(view, if crouching { FLAG_CROUCHING } else { 0 });
        });
        assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::StalkPrey)));
    }
}

#[test]
fn a_fully_crouched_fox_with_a_clear_path_pounces() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let chicken = living(9, EntityKind::Chicken, Vec3::new(4.5, 64.0, 0.5));
    world.others = vec![chicken];
    ai.set_target(Some(chicken.id));
    let (actions, body) = {
        let adjust = |view: &mut MobView| {
            flags(view, FLAG_CROUCHING | FLAG_INTERESTED);
            view.species.fox.fully_crouched = true;
        };
        let mut all = Vec::new();
        let mut last = None;
        for _ in 0..2 {
            let (actions, body) = actions_of(&mut ai, &mut base, &state, &world, &adjust);
            all.extend(actions);
            if running_fox(&ai, |goal| matches!(goal, FoxGoal::Pounce)) && last.is_none() {
                last = Some(body);
            }
        }
        (all, last)
    };
    assert!(running_fox(&ai, |goal| matches!(goal, FoxGoal::Pounce)));
    let flags = set_flags_of(&actions).unwrap();
    assert_eq!(flags & FLAG_POUNCING, FLAG_POUNCING);
    assert_eq!(flags & FLAG_INTERESTED, 0);
    let body = body.unwrap();
    assert!(body.velocity.y >= 0.9);
    assert!(body.velocity.x > 0.7);
}

#[test]
fn a_blocked_path_cancels_the_crouch() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    for y in 65..68 {
        for x in 1..4 {
            world
                .blocks
                .insert(BlockPos::new(x, y, 0), BlockKind::Stone.default_state());
        }
    }
    let chicken = living(9, EntityKind::Chicken, Vec3::new(4.5, 64.0, 0.5));
    world.others = vec![chicken];
    ai.set_target(Some(chicken.id));
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        flags(view, FLAG_CROUCHING | FLAG_INTERESTED);
        view.species.fox.fully_crouched = true;
    });
    assert!(!running_fox(&ai, |goal| matches!(goal, FoxGoal::Pounce)));
    assert_eq!(set_flags_of(&actions), Some(0));
}

#[test]
fn path_clearness_checks_three_blocks_above_the_ground_along_the_line() {
    let mut world = World::new();
    let (base, state) = fox(pos());
    let view = MobView::of(&base, &state);
    let target = living(9, EntityKind::Chicken, Vec3::new(5.5, 64.0, 0.5));
    assert!(rspigot_ai::is_path_clear(&world, &view, &target));
    world
        .blocks
        .insert(BlockPos::new(2, 66, 0), BlockKind::Stone.default_state());
    assert!(!rspigot_ai::is_path_clear(&world, &view, &target));
}

#[test]
fn trusted_players_are_defended_against_their_attackers() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let mut owner = living(100, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    owner.facts.last_hurt_by_mob = Some(EntityId(60));
    owner.facts.last_hurt_by_mob_at = 40;
    let mut zombie = living(60, EntityKind::Zombie, Vec3::new(5.5, 64.0, 0.5));
    zombie.facts.last_hurt_mob = Some(EntityId(100));
    zombie.facts.last_hurt_mob_at = 40;
    zombie.facts.tick_count = 50;
    world.players = vec![owner];
    world.others = vec![zombie];
    let trusted = Some(rspigot_ai::TrustedRef {
        uuid: owner.facts.uuid,
        id: Some(owner.id),
    });
    let mut all = Vec::new();
    for _ in 0..30 {
        all.extend(step2(&mut ai, &mut base, &state, &world, |view| {
            view.species.fox.trusted[0] = trusted;
        }));
        if ai.target().is_some() {
            break;
        }
    }
    assert_eq!(ai.target(), Some(EntityId(60)));
    assert!(sounds(&all).contains(&"minecraft:entity.fox.aggro"));
    assert_eq!(
        set_flags_of(&all).map(|flags| flags & FLAG_DEFENDING),
        Some(FLAG_DEFENDING)
    );
    let requests = ai.take_target_requests();
    assert!(
        requests
            .iter()
            .any(|request| request.to == Some(EntityId(60))
                && request.reason == TargetReason::ClosestEntity)
    );
}

#[test]
fn a_fox_does_not_defend_against_a_trusted_attacker() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let mut owner = living(100, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    owner.facts.last_hurt_by_mob = Some(EntityId(101));
    owner.facts.last_hurt_by_mob_at = 40;
    let mut friend = living(101, EntityKind::Player, Vec3::new(5.5, 64.0, 0.5));
    friend.facts.last_hurt_mob = Some(EntityId(100));
    friend.facts.tick_count = 50;
    world.players = vec![owner, friend];
    for _ in 0..30 {
        step2(&mut ai, &mut base, &state, &world, |view| {
            view.species.fox.trusted = [
                Some(rspigot_ai::TrustedRef {
                    uuid: owner.facts.uuid,
                    id: Some(owner.id),
                }),
                Some(rspigot_ai::TrustedRef {
                    uuid: friend.facts.uuid,
                    id: Some(friend.id),
                }),
            ];
        });
    }
    assert!(ai.target().is_none());
}

#[test]
fn a_fox_picks_up_the_trail_of_dropped_items_only_with_an_empty_mouth() {
    let item = DroppedItem {
        id: EntityId(70),
        position: Vec3::new(6.5, 64.0, 0.5),
        bounds: Aabb::of_size(Vec3::new(6.5, 64.0, 0.5), 0.25, 0.25),
        has_pickup_delay: false,
        alive: true,
    };
    let walks = |held: bool, delay: bool| {
        let (mut base, state) = fox(pos());
        let mut ai = new_ai();
        let mut world = World::new();
        world.items = vec![DroppedItem {
            has_pickup_delay: delay,
            ..item
        }];
        for _ in 0..80 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                if held {
                    view.main_hand = Some(rspigot_domain::ItemId(5));
                }
            });
            if running_fox(&ai, |goal| matches!(goal, FoxGoal::SearchForItems)) {
                return !ai.brain().navigation.is_done();
            }
        }
        false
    };
    assert!(walks(false, false));
    assert!(!walks(true, false));
    assert!(!walks(false, true));
}

#[test]
fn berry_bushes_are_walked_to_and_picked_after_a_pause() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let bush = BlockPos::new(1, 64, 0);
    world.blocks.insert(
        bush,
        BlockKind::SweetBerryBush
            .state_with(&[("age", "3")])
            .unwrap(),
    );
    let mut picked = None;
    let mut sniffed = false;
    for _ in 0..3000 {
        let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
        sniffed |= sounds(&actions).contains(&"minecraft:entity.fox.sniff");
        picked = actions.iter().find_map(|action| match action {
            MobAction::Fox(FoxAction::PickBerries(pos)) => Some(*pos),
            _ => None,
        });
        if picked.is_some() {
            break;
        }
    }
    assert_eq!(picked, Some(bush));
    assert!(sniffed || picked.is_some());
}

#[test]
fn unripe_bushes_and_griefing_off_never_lead_to_picking() {
    for (age, griefing) in [("1", true), ("3", false)] {
        let (mut base, state) = fox(pos());
        let mut ai = new_ai();
        let mut world = World::new();
        world.griefing = griefing;
        world.blocks.insert(
            BlockPos::new(1, 64, 0),
            BlockKind::SweetBerryBush
                .state_with(&[("age", age)])
                .unwrap(),
        );
        for _ in 0..1500 {
            let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, MobAction::Fox(FoxAction::PickBerries(_))))
            );
        }
    }
}

#[test]
fn a_fox_bites_without_swinging() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let chicken = living(9, EntityKind::Chicken, Vec3::new(1.0, 64.0, 0.5));
    world.others = vec![chicken];
    ai.set_target(Some(chicken.id));
    let mut all = Vec::new();
    for _ in 0..12 {
        all.extend(step2(&mut ai, &mut base, &state, &world, |_| {}));
    }
    assert!(all.contains(&MobAction::Attack(chicken.id)));
    assert!(sounds(&all).contains(&"minecraft:entity.fox.bite"));
    assert!(
        !all.iter()
            .any(|action| matches!(action, MobAction::Swing(_)))
    );
}

#[test]
fn a_crouching_fox_does_not_melee() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    let chicken = living(9, EntityKind::Chicken, Vec3::new(1.0, 64.0, 0.5));
    world.others = vec![chicken];
    ai.set_target(Some(chicken.id));
    for _ in 0..12 {
        let actions = step2(&mut ai, &mut base, &state, &world, |view| {
            flags(view, FLAG_CROUCHING);
        });
        assert!(!actions.contains(&MobAction::Attack(chicken.id)));
    }
}

#[test]
fn foxes_flee_the_sun_into_shelter_and_perch_when_idle() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    world.thunder = true;
    let mut shelter = false;
    for _ in 0..40 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
        shelter |= running_fox(&ai, |goal| matches!(goal, FoxGoal::SeekShelter(_)));
    }
    assert!(!shelter, "no sky access means no hurry");
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut perched = false;
    for _ in 0..400 {
        let actions = step2(&mut ai, &mut base, &state, &World::new(), |_| {});
        perched |= set_flags_of(&actions).is_some_and(|flags| flags & FLAG_SITTING != 0);
    }
    assert!(perched);
}

#[test]
fn villages_never_attract_a_fox_when_none_is_close() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let world = World::new();
    for _ in 0..600 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
        assert!(!running_fox(&ai, |goal| matches!(
            goal,
            FoxGoal::StrollThroughVillage(_)
        )));
    }
}

#[test]
fn a_fox_at_night_strolls_towards_a_nearby_village() {
    let (mut base, state) = fox(pos());
    let mut ai = new_ai();
    let mut world = World::new();
    world.village = 6;
    let mut walked = false;
    for _ in 0..800 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
        walked |= running_fox(&ai, |goal| matches!(goal, FoxGoal::StrollThroughVillage(_)));
    }
    assert!(walked);
}

#[test]
fn only_sleepy_wild_flags_exist() {
    assert_eq!(
        FLAG_SITTING
            | FLAG_CROUCHING
            | FLAG_INTERESTED
            | FLAG_POUNCING
            | FLAG_SLEEPING
            | FLAG_FACEPLANTED
            | FLAG_DEFENDING,
        253
    );
}
