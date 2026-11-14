use rspigot_ai::{
    AiWorld, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal, MobView, TargetKind,
    bow_power,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct Field {
    players: Vec<Living>,
    others: Vec<Living>,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
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

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Float(_) => "float",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::Tempt(_) => "tempt",
        MobGoal::NearestAttackableTarget(_) => "nearest_target",
        MobGoal::Panic(_) => "panic",
        MobGoal::Breed(_) => "breed",
        MobGoal::FollowParent(_) => "follow_parent",
        MobGoal::EatBlock(_) => "eat_block",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::RangedBowAttack(_) => "bow",
        MobGoal::Swell(_) => "swell",
        MobGoal::LeapAtTarget(_) => "leap",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::AvoidEntity(_) => "avoid",
        MobGoal::FleeSun(_) => "flee_sun",
        MobGoal::RestrictSun(_) => "restrict_sun",
        MobGoal::RemoveTurtleEgg(_) => "turtle_egg",
        MobGoal::BreakDoor(_) => "break_door",
        MobGoal::MoveThroughVillage(_) => "village",
        MobGoal::ClimbOnTopOfPowderSnow(_) => "climb_powder_snow",
        MobGoal::OcelotAttack(_) => "ocelot_attack",
        MobGoal::RaidGarden(_) => "raid_garden",
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

#[test]
fn animal_goals_follow_vanilla_priorities() {
    let animal = |breed: i32, tempts: &[i32], parent: i32, rest: &[(i32, &'static str)]| {
        let mut expected = vec![(0, "float"), (1, "panic"), (breed, "breed")];
        expected.extend(tempts.iter().map(|priority| (*priority, "tempt")));
        expected.push((parent, "follow_parent"));
        expected.extend_from_slice(rest);
        expected
    };
    let settings = GoalSettings::default();
    let cow = MobAi::new(EntityKind::Cow, 1, settings);
    assert_eq!(
        goals(&cow),
        animal(
            2,
            &[3],
            4,
            &[(5, "stroll"), (6, "look"), (7, "look_around")]
        )
    );
    let pig = MobAi::new(EntityKind::Pig, 1, settings);
    assert_eq!(
        goals(&pig),
        animal(
            3,
            &[4, 4],
            5,
            &[(6, "stroll"), (7, "look"), (8, "look_around")]
        )
    );
    let sheep = MobAi::new(EntityKind::Sheep, 1, settings);
    assert_eq!(
        goals(&sheep),
        animal(
            2,
            &[3],
            4,
            &[
                (5, "eat_block"),
                (6, "stroll"),
                (7, "look"),
                (8, "look_around")
            ]
        )
    );
    let chicken = MobAi::new(EntityKind::Chicken, 1, settings);
    assert_eq!(
        goals(&chicken),
        animal(
            2,
            &[3],
            4,
            &[(5, "stroll"), (6, "look"), (7, "look_around")]
        )
    );
    for ai in [&cow, &pig, &sheep, &chicken] {
        assert!(targets(ai).is_empty());
    }
}

#[test]
fn monster_goals_follow_vanilla_priorities() {
    let settings = GoalSettings::default();
    let zombie = MobAi::new(EntityKind::Zombie, 1, settings);
    assert_eq!(
        goals(&zombie),
        vec![
            (4, "turtle_egg"),
            (8, "look"),
            (8, "look_around"),
            (3, "melee"),
            (6, "village"),
            (7, "stroll")
        ]
    );
    assert_eq!(
        targets(&zombie),
        vec![
            (1, "hurt_by"),
            (2, "nearest_target"),
            (3, "nearest_target"),
            (3, "nearest_target"),
            (5, "nearest_target")
        ]
    );
    let peaceful_villagers = MobAi::new(
        EntityKind::Zombie,
        1,
        GoalSettings {
            zombie_aggressive_towards_villager: false,
        },
    );
    assert_eq!(targets(&peaceful_villagers).len(), 4);
    let mut skeleton = MobAi::new(EntityKind::Skeleton, 1, settings);
    assert_eq!(
        targets(&skeleton),
        vec![
            (1, "hurt_by"),
            (2, "nearest_target"),
            (3, "nearest_target"),
            (3, "nearest_target")
        ]
    );
    let (mut bones, bones_state) = mob(EntityKind::Skeleton, Vec3::new(0.5, 64.0, 0.5));
    let empty = Field {
        players: Vec::new(),
        others: Vec::new(),
    };
    skeleton.reassess_weapon(true, false);
    run(&mut skeleton, &mut bones, &bones_state, &empty, 1, |_| {});
    let weapons: Vec<_> = goals(&skeleton)
        .into_iter()
        .filter(|(_, label)| matches!(*label, "bow" | "melee"))
        .collect();
    assert_eq!(weapons, vec![(4, "bow")]);
    assert_eq!(
        goals(&skeleton)
            .into_iter()
            .filter(|(_, label)| !matches!(*label, "bow" | "melee"))
            .collect::<Vec<_>>(),
        vec![
            (2, "restrict_sun"),
            (3, "flee_sun"),
            (3, "avoid"),
            (5, "stroll"),
            (6, "look"),
            (6, "look_around")
        ]
    );
    skeleton.reassess_weapon(false, false);
    run(&mut skeleton, &mut bones, &bones_state, &empty, 1, |_| {});
    let weapons: Vec<_> = goals(&skeleton)
        .into_iter()
        .filter(|(_, label)| matches!(*label, "bow" | "melee"))
        .collect();
    assert_eq!(weapons, vec![(4, "melee")]);
    let creeper = MobAi::new(EntityKind::Creeper, 1, settings);
    assert_eq!(
        goals(&creeper),
        vec![
            (1, "float"),
            (2, "swell"),
            (3, "avoid"),
            (3, "avoid"),
            (4, "melee"),
            (5, "stroll"),
            (6, "look"),
            (6, "look_around")
        ]
    );
    assert_eq!(
        targets(&creeper),
        vec![(1, "nearest_target"), (2, "hurt_by")]
    );
    let spider = MobAi::new(EntityKind::Spider, 1, settings);
    assert_eq!(
        goals(&spider),
        vec![
            (1, "float"),
            (2, "avoid"),
            (3, "leap"),
            (4, "melee"),
            (5, "stroll"),
            (6, "look"),
            (6, "look_around")
        ]
    );
    assert_eq!(
        targets(&spider),
        vec![(1, "hurt_by"), (2, "nearest_target"), (3, "nearest_target")]
    );
    assert!(spider.brain().navigation.is_climber());
}

#[test]
fn variants_share_the_goals_of_their_base_mob() {
    let settings = GoalSettings::default();
    for (variant, base) in [
        (EntityKind::Husk, EntityKind::Zombie),
        (EntityKind::Stray, EntityKind::Skeleton),
        (EntityKind::Bogged, EntityKind::Skeleton),
        (EntityKind::Parched, EntityKind::Skeleton),
        (EntityKind::CaveSpider, EntityKind::Spider),
        (EntityKind::Mooshroom, EntityKind::Cow),
    ] {
        let variant = MobAi::new(variant, 1, settings);
        let base = MobAi::new(base, 1, settings);
        assert_eq!(goals(&variant), goals(&base));
        assert_eq!(targets(&variant), targets(&base));
        assert_eq!(
            variant.brain().navigation.is_climber(),
            base.brain().navigation.is_climber()
        );
    }
}

#[test]
fn wither_skeleton_also_hunts_piglins_and_always_melees() {
    let settings = GoalSettings::default();
    let skeleton = MobAi::new(EntityKind::Skeleton, 1, settings);
    let wither = MobAi::new(EntityKind::WitherSkeleton, 1, settings);
    assert_eq!(goals(&wither), goals(&skeleton));
    let mut expected = targets(&skeleton);
    expected.push((3, "nearest_target"));
    expected.sort_by_key(|(priority, _)| *priority);
    assert_eq!(targets(&wither), expected);
}

#[test]
fn bow_power_grows_with_the_draw_time() {
    assert_eq!(bow_power(0), 0.0);
    assert!((bow_power(10) - (0.25 + 1.0) / 3.0).abs() < 1.0e-6);
    assert_eq!(bow_power(20), 1.0);
    assert_eq!(bow_power(40), 1.0);
}

fn run(
    ai: &mut MobAi,
    base: &mut EntityBase,
    living: &LivingState,
    field: &Field,
    ticks: i32,
    adjust: impl Fn(&mut MobView),
) -> MobBody {
    let mut view = MobView::of(base, living);
    adjust(&mut view);
    let mut last = MobBody::of(&view);
    for _ in 0..ticks {
        base.tick_count += 1;
        let mut view = MobView::of(base, living);
        adjust(&mut view);
        view.swell_dir = last.swell_dir;
        let mut body = MobBody::of(&view);
        ai.think(&view, field, &mut body);
        ai.take_actions();
        last = body;
    }
    last
}

#[test]
fn creeper_swells_only_close_to_its_target() {
    let (mut base, living) = mob(EntityKind::Creeper, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Creeper, 3, GoalSettings::default());
    let near = Field {
        players: vec![living_player(Vec3::new(2.5, 64.0, 0.5))],
        others: Vec::new(),
    };
    ai.set_target(Some(EntityId(100)));
    let body = run(&mut ai, &mut base, &living, &near, 4, |_| {});
    assert_eq!(body.swell_dir, 1);
    assert!(
        ai.goals()
            .goals()
            .iter()
            .any(|g| g.is_running() && matches!(g.goal(), MobGoal::Swell(_)))
    );
    let far = Field {
        players: vec![living_player(Vec3::new(12.5, 64.0, 0.5))],
        others: Vec::new(),
    };
    let body = run(&mut ai, &mut base, &living, &far, 4, |_| {});
    assert_eq!(body.swell_dir, -1);
    assert!(
        !ai.goals()
            .goals()
            .iter()
            .any(|g| g.is_running() && matches!(g.goal(), MobGoal::Swell(_)))
    );
}

fn living_player(position: Vec3) -> Living {
    living(100, EntityKind::Player, position)
}

#[test]
fn cows_in_love_breed_after_sixty_ticks() {
    let (mut base, living_state) = mob(EntityKind::Cow, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Cow, 5, GoalSettings::default());
    let mut partner = living(2, EntityKind::Cow, Vec3::new(1.5, 64.0, 0.5));
    partner.in_love = true;
    let field = Field {
        players: Vec::new(),
        others: vec![partner],
    };
    let mut bred = None;
    for tick in 0..80 {
        base.tick_count += 1;
        let mut view = MobView::of(&base, &living_state);
        view.in_love = true;
        let mut body = MobBody::of(&view);
        ai.think(&view, &field, &mut body);
        if ai
            .take_actions()
            .into_iter()
            .any(|action| action == MobAction::Breed(EntityId(2)))
        {
            bred = Some(tick);
            break;
        }
    }
    let bred = bred.expect("the cows never bred");
    assert!((59..=62).contains(&bred), "bred at tick {bred}");
}

#[test]
fn hurt_animals_panic() {
    let (mut base, mut living_state) = mob(EntityKind::Sheep, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Sheep, 9, GoalSettings::default());
    let field = Field {
        players: vec![living_player(Vec3::new(2.5, 64.0, 0.5))],
        others: Vec::new(),
    };
    living_state.damage.last_hurt_by_mob = Some(EntityId(100));
    living_state.damage.last_hurt_by_mob_at = 1;
    run(&mut ai, &mut base, &living_state, &field, 3, |view| {
        view.panic = true;
    });
    assert!(ai.is_panicking());
    assert!(ai.brain().navigation.target_pos().is_some());
}
