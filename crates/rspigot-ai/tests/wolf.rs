use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal, MobView, TargetKind,
    WolfAction, WolfGoal,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    time: Cell<i64>,
    universal_anger: bool,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(0),
            universal_anger: false,
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

fn wolf(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Wolf);
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

fn fresh() -> MobAi {
    MobAi::new(EntityKind::Wolf, 5, GoalSettings::default())
}

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::Panic(_) => "panic",
        MobGoal::SitWhenOrdered(_) => "sit",
        MobGoal::Wolf(WolfGoal::AvoidLlama(_)) => "avoid_llama",
        MobGoal::Wolf(WolfGoal::Beg(_)) => "beg",
        MobGoal::LeapAtTarget(_) => "leap",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::FollowOwner(_) => "follow_owner",
        MobGoal::Breed(_) => "breed",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::OwnerTarget(_) => "owner",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::NearestAttackableTarget(_) => "nearest",
        MobGoal::ResetUniversalAnger(_) => "reset_anger",
        _ => "other",
    }
}

fn ordered(goals: &[rspigot_ai::WrappedGoal<MobGoal>]) -> Vec<(i32, &'static str)> {
    let mut list: Vec<_> = goals
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect();
    list.sort();
    list
}

#[test]
fn goals_and_targets_use_the_vanilla_priorities() {
    let ai = fresh();
    assert_eq!(
        ordered(ai.goals().goals()),
        vec![
            (1, "float"),
            (1, "panic"),
            (2, "sit"),
            (3, "avoid_llama"),
            (4, "leap"),
            (5, "melee"),
            (6, "follow_owner"),
            (7, "breed"),
            (8, "stroll"),
            (9, "beg"),
            (10, "look"),
            (10, "look_around"),
        ]
    );
    assert_eq!(
        ordered(ai.targets().goals()),
        vec![
            (1, "owner"),
            (2, "owner"),
            (3, "hurt_by"),
            (4, "nearest"),
            (5, "nearest"),
            (6, "nearest"),
            (7, "nearest"),
            (8, "reset_anger"),
        ]
    );
}

fn holding(mut player: Living, item: Option<rspigot_items::ItemKind>) -> Living {
    player.main_hand = item.map(|kind| kind.id());
    player
}

fn begs(item: Option<rspigot_items::ItemKind>, distance: f64) -> (bool, Vec<MobAction>) {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    world.players = vec![holding(
        owner_at(Vec3::new(0.5 + distance, 64.0, 0.5)),
        item,
    )];
    let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
    (
        running(&ai, |goal| matches!(goal, MobGoal::Wolf(WolfGoal::Beg(_)))),
        actions,
    )
}

#[test]
fn a_wolf_begs_for_bones_and_wolf_food_within_eight_blocks() {
    use rspigot_items::ItemKind;
    for item in [ItemKind::Bone, ItemKind::Beef, ItemKind::RabbitStew] {
        let (begging, actions) = begs(Some(item), 6.0);
        assert!(begging, "{item:?}");
        assert!(actions.contains(&MobAction::Wolf(WolfAction::SetInterested(true))));
    }
    assert!(!begs(Some(ItemKind::Apple), 6.0).0);
    assert!(!begs(None, 6.0).0);
    assert!(!begs(Some(ItemKind::Bone), 9.0).0);
}

#[test]
fn begging_in_the_off_hand_counts_and_stops_when_the_item_goes_away() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    let mut player = owner_at(Vec3::new(4.5, 64.0, 0.5));
    player.off_hand = Some(rspigot_items::ItemKind::Bone.id());
    world.players = vec![player];
    step2(&mut ai, &mut base, &state, &world, |_| {});
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::Wolf(WolfGoal::Beg(_))
    )));
    world.players = vec![owner_at(Vec3::new(4.5, 64.0, 0.5))];
    let actions = step2(&mut ai, &mut base, &state, &world, |_| {});
    assert!(actions.contains(&MobAction::Wolf(WolfAction::SetInterested(false))));
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::Wolf(WolfGoal::Beg(_))
    )));
}

#[test]
fn a_begging_wolf_keeps_looking_until_its_timer_runs_out() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    world.players = vec![holding(
        owner_at(Vec3::new(4.5, 64.0, 0.5)),
        Some(rspigot_items::ItemKind::Bone),
    )];
    let mut stopped = None;
    for tick in 0..120 {
        let actions = step(&mut ai, &mut base, &state, &world, |_| {});
        if actions.contains(&MobAction::Wolf(WolfAction::SetInterested(false))) {
            stopped = Some(tick);
            break;
        }
    }
    let tick = stopped.expect("the begging ends");
    assert!((20..=100).contains(&tick), "{tick}");
}

fn llama(strength: i32) -> Living {
    let mut llama = living(30, EntityKind::Llama, Vec3::new(6.5, 64.0, 0.5));
    llama.facts.llama_strength = strength;
    llama
}

#[test]
fn a_wild_wolf_flees_a_strong_llama_and_drops_its_target() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    ai.set_target(Some(EntityId(40)));
    let mut world = World::new();
    world.others = vec![llama(5)];
    for _ in 0..4 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert!(running(&ai, |goal| matches!(
        goal,
        MobGoal::Wolf(WolfGoal::AvoidLlama(_))
    )));
    assert_eq!(ai.target(), None);
}

#[test]
fn a_tamed_wolf_ignores_llamas() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    world.others = vec![llama(5)];
    for _ in 0..8 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::Wolf(WolfGoal::AvoidLlama(_))
    )));
}

#[test]
fn a_llama_without_strength_is_only_avoided_when_the_roll_is_zero() {
    let avoided = |seed: i64| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Wolf, seed, GoalSettings::default());
        let mut world = World::new();
        world.others = vec![llama(0)];
        step2(&mut ai, &mut base, &state, &world, |_| {});
        running(&ai, |goal| {
            matches!(goal, MobGoal::Wolf(WolfGoal::AvoidLlama(_)))
        })
    };
    let hits = (0..200).filter(|seed| avoided(*seed)).count();
    assert!((10..=80).contains(&hits), "{hits}");
}

#[test]
fn environmental_panic_needs_the_environmental_damage_tag() {
    let panics = |adjust: fn(&mut MobView)| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = fresh();
        let world = World::new();
        for _ in 0..4 {
            step2(&mut ai, &mut base, &state, &world, adjust);
        }
        running(&ai, |goal| matches!(goal, MobGoal::Panic(_)))
    };
    assert!(panics(|view| view.panic_environmental = true));
    assert!(!panics(|view| view.panic = true));
    assert!(!panics(|_| {}));
}

#[test]
fn a_panicking_tamed_wolf_far_from_its_owner_teleports_first() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(14.5, 64.0, 0.5))];
    let mut teleported = false;
    for _ in 0..8 {
        let actions = step2(&mut ai, &mut base, &state, &world, |view| {
            tamed(view);
            view.panic_environmental = true;
        });
        teleported |= actions
            .iter()
            .any(|action| matches!(action, MobAction::TeleportTo(_)));
    }
    assert!(teleported);
}

#[test]
fn a_sitting_wolf_cannot_teleport_while_panicking() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(14.5, 64.0, 0.5))];
    for _ in 0..8 {
        let actions = step2(&mut ai, &mut base, &state, &world, |view| {
            tamed(view);
            view.ordered_to_sit = true;
            view.panic_environmental = true;
        });
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::TeleportTo(_)))
        );
    }
}

fn targets_after(world: &World, adjust: impl Fn(&mut MobView)) -> Option<EntityId> {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    for _ in 0..30 {
        step2(&mut ai, &mut base, &state, world, &adjust);
    }
    ai.target()
}

#[test]
fn a_wild_wolf_hunts_sheep_rabbits_and_foxes_but_not_cows() {
    for (kind, hunted) in [
        (EntityKind::Sheep, true),
        (EntityKind::Rabbit, true),
        (EntityKind::Fox, true),
        (EntityKind::Cow, false),
        (EntityKind::Pig, false),
    ] {
        let mut world = World::new();
        world.others = vec![living(5, kind, Vec3::new(3.5, 64.0, 0.5))];
        let target = targets_after(&world, |_| {});
        assert_eq!(target.is_some(), hunted, "{kind:?}");
    }
}

#[test]
fn a_tamed_wolf_does_not_hunt_prey_turtles_or_skeletons_it_has_no_reason_to() {
    let mut world = World::new();
    world.others = vec![living(5, EntityKind::Sheep, Vec3::new(3.5, 64.0, 0.5))];
    assert_eq!(targets_after(&world, tamed), None);
    let mut turtle = living(6, EntityKind::Turtle, Vec3::new(3.5, 64.0, 0.5));
    turtle.baby = true;
    world.others = vec![turtle];
    assert_eq!(targets_after(&world, tamed), None);
    assert_eq!(targets_after(&world, |_| {}), Some(EntityId(6)));
    turtle.baby = false;
    world.others = vec![turtle];
    assert_eq!(targets_after(&world, |_| {}), None);
    turtle.baby = true;
    turtle.in_water = true;
    world.others = vec![turtle];
    assert_eq!(targets_after(&world, |_| {}), None);
}

#[test]
fn skeletons_are_hunted_even_by_tamed_wolves() {
    let mut world = World::new();
    world.others = vec![living(7, EntityKind::Stray, Vec3::new(3.5, 64.0, 0.5))];
    assert_eq!(targets_after(&world, tamed), Some(EntityId(7)));
    assert_eq!(targets_after(&world, |_| {}), Some(EntityId(7)));
}

#[test]
fn an_angry_wolf_targets_only_the_player_it_is_angry_at() {
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(3.5, 64.0, 0.5))];
    assert_eq!(targets_after(&world, |_| {}), None);
    assert_eq!(
        targets_after(&world, |view| {
            view.angry = true;
            view.anger_target = Some(OWNER);
        }),
        Some(OWNER)
    );
    assert_eq!(
        targets_after(&world, |view| {
            view.angry = true;
            view.anger_target = Some(EntityId(77));
        }),
        None
    );
}

fn alerted(wolf_owner: Option<EntityId>, other_owner: Option<EntityId>, attacker: i32) -> bool {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    let mut other = living(2, EntityKind::Wolf, Vec3::new(2.5, 64.0, 0.5));
    other.facts.tamed = other_owner.is_some();
    other.facts.owner = other_owner;
    world.others = vec![
        other,
        living(7, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5)),
    ];
    world.players = vec![owner_at(Vec3::new(30.5, 64.0, 0.5))];
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        if wolf_owner.is_some() {
            tamed(view);
            view.owner = wolf_owner;
        }
        view.last_hurt_by_mob = Some(EntityId(attacker));
        view.last_hurt_by_mob_at = 4;
    });
    actions
        .iter()
        .any(|action| matches!(action, MobAction::Alert { mob, .. } if *mob == EntityId(2)))
}

#[test]
fn a_hurt_wolf_only_alerts_wolves_with_the_same_owner() {
    assert!(alerted(None, None, 7));
    assert!(!alerted(None, Some(EntityId(9)), 7));
    assert!(!alerted(Some(OWNER), None, 7));
    assert!(alerted(Some(OWNER), Some(OWNER), 7));
    assert!(!alerted(Some(OWNER), Some(EntityId(9)), 7));
}

fn mates(mine_tamed: bool, partner_tamed: bool, partner_sitting: bool, partner_love: bool) -> bool {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = fresh();
    let mut world = World::new();
    let mut partner = living(2, EntityKind::Wolf, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = partner_love;
    partner.facts.tamed = partner_tamed;
    partner.facts.sitting = partner_sitting;
    world.others = vec![partner];
    world.players = vec![owner_at(Vec3::new(3.5, 64.0, 2.5))];
    step2(&mut ai, &mut base, &state, &world, |view| {
        view.in_love = true;
        if mine_tamed {
            tamed(view);
        }
    });
    running(&ai, |goal| matches!(goal, MobGoal::Breed(_)))
}

#[test]
fn wolves_only_mate_when_both_are_tamed_standing_and_in_love() {
    assert!(mates(true, true, false, true));
    assert!(!mates(false, true, false, true));
    assert!(!mates(true, false, false, true));
    assert!(!mates(true, true, true, true));
    assert!(!mates(true, true, false, false));
}

#[test]
fn a_sitting_wolf_turns_its_head_less() {
    let (base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut view = MobView::of(&base, &state);
    assert_eq!(view.max_head_x_rot(), 40.0);
    view.sitting = true;
    assert_eq!(view.max_head_x_rot(), 20.0);
    view.kind = EntityKind::Cow;
    assert_eq!(view.max_head_x_rot(), 40.0);
}
