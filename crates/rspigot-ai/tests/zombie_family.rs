use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal, MobView, TargetKind,
    WrappedGoal,
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

const PLAYER: EntityId = EntityId(100);

fn player_at(position: Vec3) -> Living {
    living(PLAYER.0, EntityKind::Player, position)
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

fn name(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::RemoveTurtleEgg(_) => "turtle_egg",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::MoveThroughVillage(_) => "village",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::NearestAttackableTarget(_) => "nearest",
        MobGoal::ResetUniversalAnger(_) => "reset_anger",
        _ => "other",
    }
}

fn listed(goals: &[WrappedGoal<MobGoal>]) -> Vec<(i32, &'static str)> {
    let mut list: Vec<_> = goals
        .iter()
        .map(|goal| (goal.priority(), name(goal.goal())))
        .collect();
    list.sort_by_key(|(priority, _)| *priority);
    list
}

fn ai_of(kind: EntityKind) -> MobAi {
    MobAi::new(kind, 5, GoalSettings::default())
}

#[test]
fn a_zombified_piglin_has_no_village_golem_or_turtle_goals() {
    let ai = ai_of(EntityKind::ZombifiedPiglin);
    assert_eq!(
        listed(ai.goals().goals()),
        vec![
            (2, "melee"),
            (4, "turtle_egg"),
            (7, "stroll"),
            (8, "look"),
            (8, "look_around"),
        ]
    );
    assert_eq!(
        listed(ai.targets().goals()),
        vec![(1, "hurt_by"), (2, "nearest"), (3, "reset_anger")]
    );
}

#[test]
fn a_zombie_villager_registers_exactly_what_a_zombie_does() {
    let zombie = ai_of(EntityKind::Zombie);
    let villager = ai_of(EntityKind::ZombieVillager);
    assert_eq!(
        listed(villager.goals().goals()),
        listed(zombie.goals().goals())
    );
    assert_eq!(
        listed(villager.targets().goals()),
        listed(zombie.targets().goals())
    );
    assert!(
        listed(villager.goals().goals())
            .iter()
            .any(|(_, goal)| *goal == "village")
    );
}

fn hunts_player(angry: bool, target_anger: Option<EntityId>) -> bool {
    let (mut base, state) = mob(EntityKind::ZombifiedPiglin, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = ai_of(EntityKind::ZombifiedPiglin);
    let mut world = World::new();
    world.players = vec![player_at(Vec3::new(6.5, 64.0, 0.5))];
    for _ in 0..60 {
        step(&mut ai, &mut base, &state, &world, |view| {
            view.angry = angry;
            view.anger_target = target_anger;
        });
        if ai.target().is_some() {
            return true;
        }
    }
    false
}

#[test]
fn a_calm_piglin_ignores_players_but_an_angry_one_hunts_the_one_it_is_angry_at() {
    assert!(!hunts_player(false, None));
    assert!(hunts_player(true, Some(PLAYER)));
    assert!(!hunts_player(true, Some(EntityId(9))));
}

#[test]
fn universal_anger_makes_an_angry_piglin_hunt_every_player() {
    let (mut base, state) = mob(EntityKind::ZombifiedPiglin, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = ai_of(EntityKind::ZombifiedPiglin);
    let mut world = World::new();
    world.universal_anger = true;
    world.players = vec![player_at(Vec3::new(6.5, 64.0, 0.5))];
    for _ in 0..60 {
        step(&mut ai, &mut base, &state, &world, |view| view.angry = true);
    }
    assert_eq!(ai.target(), Some(PLAYER));
}

fn alerted(victim: EntityKind, others: Vec<Living>) -> Vec<EntityId> {
    let (mut base, state) = mob(victim, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = ai_of(victim);
    let mut world = World::new();
    world.others = others;
    world
        .others
        .push(living(7, EntityKind::Skeleton, Vec3::new(2.5, 64.0, 0.5)));
    (0..2)
        .flat_map(|_| {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.last_hurt_by_mob = Some(EntityId(7));
                view.last_hurt_by_mob_at = 5;
            })
        })
        .filter_map(|action| match action {
            MobAction::Alert { mob, .. } => Some(mob),
            _ => None,
        })
        .collect()
}

#[test]
fn a_hurt_piglin_calls_only_idle_piglins_nearby() {
    let piglin = |id, at| living(id, EntityKind::ZombifiedPiglin, at);
    let mut busy = piglin(3, Vec3::new(5.5, 64.0, 0.5));
    busy.has_target = true;
    let called = alerted(
        EntityKind::ZombifiedPiglin,
        vec![
            piglin(2, Vec3::new(3.5, 64.0, 0.5)),
            busy,
            piglin(4, Vec3::new(60.5, 64.0, 0.5)),
            living(5, EntityKind::Zombie, Vec3::new(3.5, 64.0, 1.5)),
        ],
    );
    assert_eq!(called, vec![EntityId(2)]);
}

#[test]
fn a_hurt_zombie_never_calls_zombified_piglins_but_calls_other_zombies() {
    let called = alerted(
        EntityKind::Zombie,
        vec![
            living(2, EntityKind::ZombifiedPiglin, Vec3::new(3.5, 64.0, 0.5)),
            living(3, EntityKind::Zombie, Vec3::new(4.5, 64.0, 0.5)),
            living(4, EntityKind::ZombieVillager, Vec3::new(4.5, 64.0, 1.5)),
        ],
    );
    assert_eq!(called, vec![EntityId(3), EntityId(4)]);
}
