use rspigot_ai::{
    AiContext, AiWorld, Brain, Facts, Flag, GoalSettings, Living, MobAi, MobBody, MobGoal, MobView,
    TargetKind, TemptItems, default_pos, land_pos,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

struct Field {
    players: Vec<Living>,
    difficulty: i32,
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
        self.players.iter().find(|player| player.id == id).copied()
    }

    fn living_of(&self, _kind: TargetKind, _area: &Aabb) -> Vec<Living> {
        Vec::new()
    }

    fn difficulty(&self) -> i32 {
        self.difficulty
    }

    fn game_time(&self) -> i64 {
        0
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }
}

fn player(position: Vec3, held: Option<ItemId>) -> Living {
    Living {
        id: EntityId(100),
        player: true,
        kind: EntityKind::Player,
        position,
        eye_height: 1.62,
        bounds: Aabb::of_size(position, 0.6, 1.8),
        alive: true,
        spectator: false,
        creative: false,
        invulnerable: false,
        discrete: false,
        invisible: false,
        armor_cover: 0.0,
        main_hand: held,
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

fn running(ai: &MobAi, pick: fn(&MobGoal) -> bool) -> bool {
    ai.goals()
        .goals()
        .iter()
        .any(|goal| goal.is_running() && pick(goal.goal()))
}

fn wheat() -> Option<ItemId> {
    Some(ItemKind::Wheat.id())
}

#[test]
fn random_positions_stay_on_the_floor_within_range() {
    let field = Field {
        players: Vec::new(),
        difficulty: 2,
    };
    let (base, living) = mob(EntityKind::Cow, Vec3::new(0.5, 64.0, 0.5));
    let view = MobView::of(&base, &living);
    let mut brain = Brain::new(EntityKind::Cow, 7);
    let mut body = MobBody::of(&view);
    let mut context = AiContext {
        mob: &view,
        world: &field,
        brain: &mut brain,
        body: &mut body,
    };
    let mut found = 0;
    for _ in 0..200 {
        for position in [
            land_pos(&mut context, 10, 7),
            default_pos(&mut context, 10, 7),
        ]
        .into_iter()
        .flatten()
        {
            found += 1;
            assert_eq!(position.y, 64.0);
            assert!(
                position.x.abs() <= 11.0 && position.z.abs() <= 11.0,
                "{position:?}"
            );
            assert_eq!(position.x.fract().abs(), 0.5);
        }
    }
    assert!(found > 250, "only {found} positions out of 400 tries");
}

#[test]
fn cow_is_tempted_by_wheat_only() {
    let (base, living) = mob(EntityKind::Cow, Vec3::new(0.5, 64.0, 0.5));
    let view = MobView::of(&base, &living);
    let mut ai = MobAi::new(EntityKind::Cow, 3, GoalSettings::default());
    let bare = Field {
        players: vec![player(Vec3::new(6.5, 64.0, 0.5), None)],
        difficulty: 2,
    };
    ai.think(&view, &bare, &mut MobBody::of(&view));
    assert!(!running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
    let tempting = Field {
        players: vec![player(Vec3::new(6.5, 64.0, 0.5), wheat())],
        difficulty: 2,
    };
    ai.think(&view, &tempting, &mut MobBody::of(&view));
    assert!(running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
    assert_eq!(
        ai.goals().locked(Flag::Move).map(|goal| goal.priority()),
        Some(3)
    );
    let target = ai.brain().navigation.target_pos();
    assert_eq!(target, Some(BlockPos::new(6, 64, 0)));
    assert!(ai.brain().movement.has_wanted());
    let far = Field {
        players: vec![player(Vec3::new(11.5, 64.0, 0.5), wheat())],
        difficulty: 2,
    };
    ai.think(&view, &far, &mut MobBody::of(&view));
    assert!(!running(&ai, |goal| matches!(goal, MobGoal::Tempt(_))));
    assert!(ai.brain().navigation.is_done());
    assert!(TemptItems::Tag("minecraft:cow_food").matches(wheat()));
    assert!(!TemptItems::Tag("minecraft:cow_food").matches(Some(ItemKind::Carrot.id())));
    assert!(
        TemptItems::Item(ItemKind::CarrotOnAStick).matches(Some(ItemKind::CarrotOnAStick.id()))
    );
}

#[test]
fn zombie_targets_survival_players_only() {
    let (mut base, living) = mob(EntityKind::Zombie, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Zombie, 11, GoalSettings::default());
    let survival = Field {
        players: vec![player(Vec3::new(5.5, 64.0, 0.5), None)],
        difficulty: 2,
    };
    for tick in 0..40 {
        base.tick_count = tick * 2 + 1;
        let view = MobView::of(&base, &living);
        ai.think(&view, &survival, &mut MobBody::of(&view));
        if ai.target().is_some() {
            break;
        }
    }
    assert_eq!(ai.target(), Some(EntityId(100)));
    let mut creative = player(Vec3::new(5.5, 64.0, 0.5), None);
    creative.creative = true;
    creative.invulnerable = true;
    let creative = Field {
        players: vec![creative],
        difficulty: 2,
    };
    for tick in 40..80 {
        base.tick_count = tick * 2 + 1;
        let view = MobView::of(&base, &living);
        ai.think(&view, &creative, &mut MobBody::of(&view));
        assert!(tick == 40 || ai.target().is_none());
    }
    assert_eq!(ai.target(), None);
    let peaceful = Field {
        players: vec![player(Vec3::new(5.5, 64.0, 0.5), None)],
        difficulty: 0,
    };
    for tick in 80..120 {
        base.tick_count = tick * 2 + 1;
        let view = MobView::of(&base, &living);
        ai.think(&view, &peaceful, &mut MobBody::of(&view));
        assert_eq!(ai.target(), None);
    }
}

#[test]
fn submerged_mob_floats_by_jumping() {
    let (mut base, living) = mob(EntityKind::Cow, Vec3::new(0.5, 64.0, 0.5));
    base.fluid.water_height = 1.0;
    base.fluid.in_water = true;
    let view = MobView::of(&base, &living);
    let mut ai = MobAi::new(EntityKind::Cow, 5, GoalSettings::default());
    let field = Field {
        players: Vec::new(),
        difficulty: 2,
    };
    let mut jumps = 0;
    for _ in 0..20 {
        ai.think(&view, &field, &mut MobBody::of(&view));
        assert!(running(&ai, |goal| matches!(goal, MobGoal::Float(_))));
        if ai.brain().jump.is_jumping() {
            jumps += 1;
        }
        ai.brain_mut().jump = Default::default();
    }
    assert!(jumps > 10);
    assert!(ai.can_float());
    let zombie = MobAi::new(EntityKind::Zombie, 5, GoalSettings::default());
    assert!(!zombie.can_float());
}
