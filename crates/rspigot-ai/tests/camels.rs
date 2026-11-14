use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, CamelAction, CamelPanic,
    CamelRandomSitting, GoalSettings, Living, MemoryModuleType, MemoryValue, MobAction, MobAi,
    MobBehavior, MobBody, MobControl, MobView, Status, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;

struct Field {
    others: Vec<Living>,
    time: i64,
}

impl Field {
    fn flat() -> Self {
        Self {
            others: Vec::new(),
            time: NOW,
        }
    }
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if (55..=63).contains(&pos.y) {
            BlockKind::Stone.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for Field {
    fn players(&self) -> &[Living] {
        &[]
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.others.iter().find(|living| living.id == id).copied()
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
        self.time
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }
}

fn view(kind: EntityKind, position: Vec3) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    MobView::of(&base, &state)
}

struct Bench {
    mob: MobView,
    world: Field,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(kind: EntityKind, at: Vec3, world: Field, seed: i64) -> Self {
        let mob = view(kind, at);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(kind, seed),
            state,
        }
    }

    fn with<R>(&mut self, run: impl FnOnce(&mut BehaviorCx<'_, '_, Field>) -> R) -> R {
        let mut ai = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.controls,
            body: &mut self.body,
        };
        let mut cx = BehaviorCx {
            ai: &mut ai,
            brain: &mut self.state,
        };
        run(&mut cx)
    }

    fn start(&mut self, control: &mut MobControl, now: i64) -> bool {
        self.with(|cx| control.try_start(cx, now))
    }

    fn tick(&mut self, control: &mut MobControl, now: i64) {
        self.with(|cx| control.tick_or_stop(cx, now));
    }

    fn get(&self, ty: MemoryModuleType) -> Option<&MemoryValue> {
        self.state.memories.get(ty)
    }

    fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.state.memories.set(ty, value);
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }
}

fn describe(control: &MobControl) -> String {
    format!("{control:?}")
}

fn behaviors(kind: EntityKind, activity: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    brain
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn camel_bench(kind: EntityKind) -> Bench {
    let mut bench = Bench::new(kind, Vec3::new(0.5, 64.0, 0.5), Field::flat(), 7);
    bench.mob.species.camel.can_change_pose = true;
    bench.mob.species.camel.pose_time = 1000;
    bench
}

fn sitting_control() -> MobControl {
    MobControl::behavior(MobBehavior::CamelRandomSitting(CamelRandomSitting::new(20)))
}

#[test]
fn a_camel_brain_registers_the_vanilla_core_and_idle_behaviors_in_order() {
    for kind in [EntityKind::Camel, EntityKind::CamelHusk] {
        let core = behaviors(kind, Activity::Core);
        let names: Vec<&str> = [
            "Swim",
            "CamelPanic",
            "LookAtTargetSink",
            "MoveToTargetSink",
            "CountDownCooldownTicks",
            "CountDownCooldownTicks",
        ]
        .into();
        assert_eq!(core.len(), names.len(), "{kind:?}");
        for ((_, text), name) in core.iter().zip(names) {
            assert!(text.contains(name), "{text} {name}");
        }
        assert!(core[0].1.contains("0.8"));
        assert!(core[1].1.contains("speed_multiplier: 4.0"));
        assert!(core[4].1.contains("TemptationCooldownTicks"));
        assert!(core[5].1.contains("GazeCooldownTicks"));
        let idle = behaviors(kind, Activity::Idle);
        assert_eq!(idle.len(), 5);
        assert!(idle[0].1.contains("SetEntityLookTargetSometimes"));
        assert!(idle[1].1.contains("AnimalMakeLove"));
        assert!(idle[2].1.contains("FollowTemptation"));
        assert!(idle[2].1.contains("baby: 2.5"));
        assert!(idle[2].1.contains("adult: 3.5"));
        assert!(idle[2].1.contains("BabyFollowAdult"));
        assert!(idle[3].1.contains("RandomLookAround"));
        assert!(idle[4].1.contains("RandomStroll"));
        assert!(idle[4].1.contains("SetWalkTargetFromLookTarget"));
        assert!(idle[4].1.contains("CamelRandomSitting"));
        assert!(idle[4].1.contains("DoNothing"));
    }
}

#[test]
fn the_camel_and_its_husk_tempt_with_different_food() {
    let camel = format!("{:?}", rspigot_ai::camel_sensors(EntityKind::Camel));
    let husk = format!("{:?}", rspigot_ai::camel_sensors(EntityKind::CamelHusk));
    assert!(camel.contains("minecraft:camel_food"), "{camel}");
    assert!(husk.contains("minecraft:camel_husk_food"), "{husk}");
    assert!(camel.contains("NearestAdult") && camel.contains("HurtBy"));
}

#[test]
fn random_sitting_waits_for_a_calm_camel_that_has_held_its_pose_for_twenty_seconds() {
    let mut bench = camel_bench(EntityKind::Camel);
    let mut control = sitting_control();
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Camel(CamelAction::SitDown)]
    );
    for blocked in 0..5 {
        let mut bench = camel_bench(EntityKind::Camel);
        match blocked {
            0 => bench.mob.species.camel.pose_time = 399,
            1 => bench.mob.leashed = true,
            2 => bench.mob.on_ground = false,
            3 => bench.mob.species.camel.controlled_by_player = true,
            _ => bench.mob.species.camel.can_change_pose = false,
        }
        let mut control = sitting_control();
        assert!(!bench.start(&mut control, NOW), "{blocked}");
    }
    let mut wet = camel_bench(EntityKind::Camel);
    wet.mob.fluid.in_water = true;
    assert!(!wet.start(&mut sitting_control(), NOW));
}

#[test]
fn a_sitting_camel_stands_up_and_a_panicking_one_never_sits() {
    let mut bench = camel_bench(EntityKind::Camel);
    bench.mob.species.camel.sitting = true;
    let mut control = sitting_control();
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Camel(CamelAction::StandUp)]
    );
    let mut panicking = camel_bench(EntityKind::Camel);
    panicking.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    let mut control = sitting_control();
    assert!(panicking.start(&mut control, NOW));
    assert!(panicking.actions().is_empty());
}

#[test]
fn a_hurt_camel_panics_by_standing_up_instantly_unless_a_mob_rides_it() {
    let hurt = |bench: &mut Bench| {
        let damage = rspigot_ai::LastDamage {
            kind: rspigot_domain::damage::DamageType::MobAttack,
            entity: None,
        };
        bench.set(MemoryModuleType::HurtBy, MemoryValue::HurtBy(damage));
    };
    let mut bench = camel_bench(EntityKind::Camel);
    hurt(&mut bench);
    let mut control = MobControl::behavior(MobBehavior::CamelPanic(CamelPanic::new(4.0)));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Camel(CamelAction::StandUpInstantly)]
    );
    let mut ridden = camel_bench(EntityKind::CamelHusk);
    ridden.mob.species.camel.mob_controlled = true;
    hurt(&mut ridden);
    let mut control = MobControl::behavior(MobBehavior::CamelPanic(CamelPanic::new(4.0)));
    assert!(!ridden.start(&mut control, NOW));
}

#[test]
fn a_camel_refusing_to_move_neither_strolls_nor_follows_an_adult() {
    let ai = MobAi::new(EntityKind::Camel, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    let wander = brain
        .activity_behaviors(Activity::Idle)
        .into_iter()
        .find(|(priority, _)| *priority == 4)
        .map(|(_, control)| control.clone())
        .expect("the wander gate");
    let mut still = camel_bench(EntityKind::Camel);
    still.mob.species.camel.sitting = true;
    let mut gate = wander.clone();
    let mut ran = false;
    for _ in 0..40 {
        still.start(&mut gate, NOW);
        still.tick(&mut gate, NOW);
        ran |= gate.status() == Status::Running;
        gate = wander.clone();
    }
    assert!(ran);
    assert!(still.get(MemoryModuleType::WalkTarget).is_none());
}
