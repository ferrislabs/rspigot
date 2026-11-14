use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, FollowOwnerGoal, GoalSelector, GoalSettings, Living, MobAction, MobAi, MobBody,
    MobGoal, MobView, NearestAttackableTargetGoal, OwnerTargetGoal, ResetUniversalAngerGoal,
    SitWhenOrderedToGoal, TargetFilter, TargetKind, is_angry_at,
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

fn blank_ai() -> MobAi {
    let mut ai = MobAi::new(EntityKind::Wolf, 5, GoalSettings::default());
    *ai.goals_mut() = GoalSelector::new();
    *ai.targets_mut() = GoalSelector::new();
    ai
}

#[test]
fn a_tamed_mob_walks_to_an_owner_ten_blocks_away_and_stops_two_blocks_short() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.goals_mut().add_goal(
        6,
        MobGoal::FollowOwner(FollowOwnerGoal::new(1.0, 10.0, 2.0)),
    );
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(10.4, 64.0, 0.5))];
    for _ in 0..4 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::FollowOwner(_)
    )));
    world.players = vec![owner_at(Vec3::new(10.6, 64.0, 0.5))];
    for _ in 0..4 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    assert!(running(&ai, |goal| matches!(goal, MobGoal::FollowOwner(_))));
    assert!(!ai.brain().navigation.is_done());
}

#[test]
fn sitting_or_leashed_or_spectated_owners_keep_a_mob_in_place() {
    let blocked = |adjust: fn(&mut MobView), spectator: bool| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = blank_ai();
        ai.goals_mut().add_goal(
            6,
            MobGoal::FollowOwner(FollowOwnerGoal::new(1.0, 10.0, 2.0)),
        );
        let mut world = World::new();
        let mut owner = owner_at(Vec3::new(20.5, 64.0, 0.5));
        owner.spectator = spectator;
        world.players = vec![owner];
        let mut ever = false;
        for _ in 0..6 {
            step2(&mut ai, &mut base, &state, &world, |view| {
                tamed(view);
                adjust(view);
            });
            ever |= running(&ai, |goal| matches!(goal, MobGoal::FollowOwner(_)));
        }
        !ever
    };
    assert!(blocked(|view| view.ordered_to_sit = true, false));
    assert!(blocked(|view| view.leashed = true, false));
    assert!(blocked(|_| {}, true));
    assert!(!blocked(|_| {}, false));
}

#[test]
fn an_owner_twelve_blocks_away_gets_a_teleport_next_to_them() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.goals_mut().add_goal(
        6,
        MobGoal::FollowOwner(FollowOwnerGoal::new(1.0, 10.0, 2.0)),
    );
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(12.5, 64.0, 0.5))];
    let mut teleports = Vec::new();
    for _ in 0..12 {
        teleports.extend(
            step2(&mut ai, &mut base, &state, &world, tamed)
                .into_iter()
                .filter_map(|action| match action {
                    MobAction::TeleportTo(at) => Some(at),
                    _ => None,
                }),
        );
    }
    assert!(!teleports.is_empty());
    for at in teleports {
        let dx = (at.x - 12.5).abs();
        let dz = (at.z - 0.5).abs();
        assert!(dx.max(dz) >= 1.5 && dx <= 3.5 && dz <= 3.5, "{at:?}");
        assert_eq!(at.x.fract(), 0.5);
        assert!((63.0..=65.0).contains(&at.y));
    }
}

#[test]
fn sit_goal_orders_the_sitting_pose_only_while_ordered() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.goals_mut()
        .add_goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
    let world = World::new();
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        view.tamed = true;
        view.ordered_to_sit = true;
    });
    assert!(actions.contains(&MobAction::SetSitting(true)));
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(3.5, 64.0, 0.5))];
    let actions = step2(&mut ai, &mut base, &state, &world, tamed);
    assert!(actions.contains(&MobAction::SetSitting(false)));
    assert!(!running(&ai, |goal| matches!(
        goal,
        MobGoal::SitWhenOrdered(_)
    )));
}

#[test]
fn a_wild_mob_never_sits_and_a_swimming_one_stays_up() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.goals_mut()
        .add_goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
    let world = World::new();
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        view.ordered_to_sit = true;
    });
    assert!(actions.contains(&MobAction::SetSitting(true)));
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.goals_mut()
        .add_goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
    let actions = step2(&mut ai, &mut base, &state, &world, |view| {
        view.tamed = true;
        view.ordered_to_sit = true;
        view.fluid.in_water = true;
    });
    assert!(actions.is_empty());
}

#[test]
fn a_tamed_mob_sits_unless_its_owner_is_hurt_close_by() {
    let sits = |hurt: bool, distance: f64| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = blank_ai();
        ai.goals_mut()
            .add_goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
        let mut world = World::new();
        let mut owner = owner_at(Vec3::new(0.5 + distance, 64.0, 0.5));
        owner.facts.last_hurt_by_mob = hurt.then_some(EntityId(9));
        world.players = vec![owner];
        step2(&mut ai, &mut base, &state, &world, |view| {
            tamed(view);
            view.ordered_to_sit = true;
        });
        running(&ai, |goal| matches!(goal, MobGoal::SitWhenOrdered(_)))
    };
    assert!(sits(false, 3.0));
    assert!(sits(true, 20.0));
    assert!(!sits(true, 3.0));
}

#[test]
fn the_owner_hurt_goal_targets_whoever_hurt_the_owner() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut()
        .add_goal(1, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_by()));
    let mut world = World::new();
    let mut owner = owner_at(Vec3::new(4.5, 64.0, 0.5));
    owner.facts.last_hurt_by_mob = Some(EntityId(7));
    owner.facts.last_hurt_by_mob_at = 5;
    owner.facts.retaliable = true;
    world.players = vec![owner];
    world.others = vec![living(7, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5))];
    step2(&mut ai, &mut base, &state, &world, tamed);
    assert_eq!(ai.target(), Some(EntityId(7)));
}

#[test]
fn the_owner_hurt_goal_ignores_stale_unretaliable_sat_down_and_wild_cases() {
    let targets = |retaliable: bool, sitting: bool, wild: bool, same_stamp: bool| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = blank_ai();
        ai.targets_mut()
            .add_goal(1, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_by()));
        let mut world = World::new();
        let mut owner = owner_at(Vec3::new(4.5, 64.0, 0.5));
        owner.facts.last_hurt_by_mob = Some(EntityId(7));
        owner.facts.last_hurt_by_mob_at = if same_stamp { 0 } else { 5 };
        owner.facts.retaliable = retaliable;
        world.players = vec![owner];
        world.others = vec![living(7, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5))];
        step2(&mut ai, &mut base, &state, &world, |view| {
            if !wild {
                tamed(view);
            }
            view.ordered_to_sit = sitting;
        });
        ai.target().is_some()
    };
    assert!(targets(true, false, false, false));
    assert!(!targets(false, false, false, false));
    assert!(!targets(true, true, false, false));
    assert!(!targets(true, false, true, false));
    assert!(!targets(true, false, false, true));
}

#[test]
fn the_owner_attack_goal_follows_what_the_owner_last_hit() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut()
        .add_goal(2, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_target()));
    let mut world = World::new();
    let mut owner = owner_at(Vec3::new(4.5, 64.0, 0.5));
    owner.facts.last_hurt_mob = Some(EntityId(8));
    owner.facts.last_hurt_mob_at = 3;
    world.players = vec![owner];
    world.others = vec![living(8, EntityKind::Skeleton, Vec3::new(3.5, 64.0, 0.5))];
    step2(&mut ai, &mut base, &state, &world, tamed);
    assert_eq!(ai.target(), Some(EntityId(8)));
}

#[test]
fn a_wolf_never_attacks_creepers_or_other_pack_mates_for_its_owner() {
    let attacks = |kind: EntityKind, tamed_target: bool, owner_of_target: Option<EntityId>| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = blank_ai();
        ai.targets_mut()
            .add_goal(2, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_target()));
        let mut world = World::new();
        let mut owner = owner_at(Vec3::new(4.5, 64.0, 0.5));
        owner.facts.last_hurt_mob = Some(EntityId(8));
        owner.facts.last_hurt_mob_at = 3;
        world.players = vec![owner];
        let mut victim = living(8, kind, Vec3::new(3.5, 64.0, 0.5));
        victim.facts.tamed = tamed_target;
        victim.facts.owner = owner_of_target;
        world.others = vec![victim];
        step2(&mut ai, &mut base, &state, &world, tamed);
        ai.target().is_some()
    };
    assert!(attacks(EntityKind::Zombie, false, None));
    assert!(!attacks(EntityKind::Creeper, false, None));
    assert!(!attacks(EntityKind::Ghast, false, None));
    assert!(attacks(EntityKind::Wolf, false, None));
    assert!(!attacks(EntityKind::Wolf, true, Some(OWNER)));
    assert!(attacks(EntityKind::Wolf, true, Some(EntityId(55))));
    assert!(!attacks(EntityKind::Cat, true, Some(EntityId(55))));
}

#[test]
fn a_tamed_mob_never_targets_its_owner() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut().add_goal(
        4,
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
            TargetKind::Player,
            1,
            false,
            TargetFilter::Any,
        )),
    );
    let mut world = World::new();
    world.players = vec![owner_at(Vec3::new(3.5, 64.0, 0.5))];
    for _ in 0..8 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    assert_eq!(ai.target(), None);
    for _ in 0..8 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert_eq!(ai.target(), Some(OWNER));
}

#[test]
fn non_tame_targets_stop_once_the_mob_is_tamed() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut().add_goal(
        5,
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::non_tame(
            TargetKind::Class("Sheep"),
            false,
            TargetFilter::Any,
        )),
    );
    let mut world = World::new();
    world.others = vec![living(5, EntityKind::Sheep, Vec3::new(3.5, 64.0, 0.5))];
    for _ in 0..8 {
        step2(&mut ai, &mut base, &state, &world, |_| {});
    }
    assert_eq!(ai.target(), Some(EntityId(5)));
    for _ in 0..4 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut().add_goal(
        5,
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::non_tame(
            TargetKind::Class("Sheep"),
            false,
            TargetFilter::Any,
        )),
    );
    for _ in 0..8 {
        step2(&mut ai, &mut base, &state, &world, tamed);
    }
    assert_eq!(ai.target(), None);
}

#[test]
fn angry_at_matches_the_remembered_target_or_every_player_under_universal_anger() {
    let (base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let player = owner_at(Vec3::new(3.5, 64.0, 0.5));
    let zombie = living(7, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5));
    let mut world = World::new();
    let mut view = MobView::of(&base, &state);
    assert!(!is_angry_at(&view, &world, &player));
    view.angry = true;
    view.anger_target = Some(EntityId(7));
    assert!(is_angry_at(&view, &world, &zombie));
    assert!(!is_angry_at(&view, &world, &player));
    view.anger_target = None;
    assert!(!is_angry_at(&view, &world, &player));
    world.universal_anger = true;
    assert!(is_angry_at(&view, &world, &player));
    let mut creative = player;
    creative.creative = true;
    assert!(!is_angry_at(&view, &world, &creative));
    view.angry = false;
    assert!(!is_angry_at(&view, &world, &player));
}

#[test]
fn universal_anger_reset_forgets_the_grudge_of_the_mob_and_its_pack() {
    let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = blank_ai();
    ai.targets_mut().add_goal(
        8,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(true)),
    );
    let mut world = World::new();
    world.universal_anger = true;
    world.players = vec![owner_at(Vec3::new(5.5, 64.0, 0.5))];
    let mut pack_mate = living(2, EntityKind::Wolf, Vec3::new(2.5, 64.0, 0.5));
    pack_mate.facts.uuid = Uuid::from_u128(2);
    let mut spectator = living(3, EntityKind::Wolf, Vec3::new(2.5, 64.0, 1.5));
    spectator.spectator = true;
    world.others = vec![pack_mate, spectator];
    let hurt = |view: &mut MobView| {
        view.last_hurt_by_mob = Some(OWNER);
        view.last_hurt_by_mob_at = 4;
    };
    let actions = step2(&mut ai, &mut base, &state, &world, hurt);
    let resets: Vec<EntityId> = actions
        .iter()
        .filter_map(|action| match action {
            MobAction::ForgetAndRefreshAnger(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(resets, vec![EntityId(1), EntityId(2)]);
    let actions = step2(&mut ai, &mut base, &state, &world, hurt);
    assert!(actions.is_empty());
}

#[test]
fn universal_anger_reset_needs_the_game_rule_and_a_player_attacker() {
    let resets = |rule: bool, attacker_is_player: bool| {
        let (mut base, state) = wolf(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = blank_ai();
        ai.targets_mut().add_goal(
            8,
            MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(false)),
        );
        let mut world = World::new();
        world.universal_anger = rule;
        world.players = vec![owner_at(Vec3::new(5.5, 64.0, 0.5))];
        world.others = vec![living(7, EntityKind::Zombie, Vec3::new(2.5, 64.0, 0.5))];
        let attacker = if attacker_is_player {
            OWNER
        } else {
            EntityId(7)
        };
        let actions = step2(&mut ai, &mut base, &state, &world, |view| {
            view.last_hurt_by_mob = Some(attacker);
            view.last_hurt_by_mob_at = 4;
        });
        !actions.is_empty()
    };
    assert!(resets(true, true));
    assert!(!resets(false, true));
    assert!(!resets(true, false));
}
