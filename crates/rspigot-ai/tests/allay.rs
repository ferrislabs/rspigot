use std::collections::HashMap;

use rspigot_ai::{
    ALLAY_MEMORIES, Activity, AiContext, AiWorld, AirAction, AllayAction, AllayFacts,
    BehaviorControl, BehaviorCx, BrainState, Facts, GlobalPos, GoalSettings, Living,
    MemoryModuleType, MemoryValue, MobAction, MobAi, MobBody, MobControl, MobView,
    StayCloseToTarget, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;
const PLAYER_UUID: u128 = 4242;

#[derive(Default)]
struct Field {
    players: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(self.edits.get(&pos).copied().unwrap_or(StateId::AIR))
    }
}

impl AiWorld for Field {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players.iter().find(|living| living.id == id).copied()
    }

    fn living_by_uuid(&self, uuid: Uuid) -> Option<Living> {
        self.players
            .iter()
            .find(|living| living.facts.uuid == uuid)
            .copied()
    }

    fn living_of(&self, _kind: TargetKind, _area: &Aabb) -> Vec<Living> {
        Vec::new()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        NOW
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn mob_griefing(&self) -> bool {
        true
    }

    fn dimension(&self) -> &str {
        "minecraft:overworld"
    }
}

fn player(id: i32, position: Vec3) -> Living {
    let size = EntityKind::Player.dimensions();
    Living {
        id: EntityId(id),
        player: true,
        kind: EntityKind::Player,
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
            uuid: Uuid::from_u128(PLAYER_UUID),
            ..Facts::default()
        },
    }
}

fn view(at: Vec3, facts: AllayFacts) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Allay);
    base.position = at;
    base.old_position = at;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    let mut view = MobView::of(&base, &state);
    view.species.allay = facts;
    view
}

struct Bench {
    mob: MobView,
    world: Field,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(facts: AllayFacts, world: Field) -> Self {
        let mob = view(Vec3::new(0.5, 70.0, 0.5), facts);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(EntityKind::Allay, 1),
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

    fn throws(&mut self) -> Vec<Vec3> {
        std::mem::take(&mut self.controls.actions)
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Air(AirAction::Allay(AllayAction::ThrowItem { target })) => Some(target),
                _ => None,
            })
            .collect()
    }
}

fn controls(activity: Activity) -> Vec<(i32, MobControl)> {
    let ai = MobAi::new(EntityKind::Allay, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    brain
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, control.clone()))
        .collect()
}

fn named(activity: Activity) -> Vec<(i32, String)> {
    controls(activity)
        .into_iter()
        .map(|(priority, control)| (priority, format!("{control:?}")))
        .collect()
}

fn noteblock() -> StateId {
    BlockKind::NoteBlock.default_state()
}

fn pos_of(x: i32, y: i32, z: i32) -> GlobalPos {
    GlobalPos {
        dimension: "minecraft:overworld".to_owned(),
        pos: BlockPos::new(x, y, z),
    }
}

fn carrying() -> AllayFacts {
    AllayFacts {
        has_item_to_throw: true,
        ..AllayFacts::default()
    }
}

fn liked_player_world(at: Vec3) -> Field {
    Field {
        players: vec![player(7, at)],
        ..Field::default()
    }
}

fn like_player(bench: &mut Bench) {
    bench.set(
        MemoryModuleType::LikedPlayer,
        MemoryValue::Uuid(Uuid::from_u128(PLAYER_UUID)),
    );
}

#[test]
fn the_core_activity_holds_swim_panic_look_move_and_two_cooldown_countdowns() {
    let core = named(Activity::Core);
    let names: Vec<(i32, &str)> = core
        .iter()
        .map(|(priority, text)| {
            let known = [
                "Swim",
                "AnimalPanic",
                "LookAtTargetSink",
                "MoveToTargetSink",
                "CountDownCooldownTicks",
            ];
            let name = known.iter().find(|name| text.contains(*name)).unwrap();
            (*priority, *name)
        })
        .collect();
    assert_eq!(
        names,
        vec![
            (0, "Swim"),
            (1, "AnimalPanic"),
            (2, "LookAtTargetSink"),
            (3, "MoveToTargetSink"),
            (4, "CountDownCooldownTicks"),
            (5, "CountDownCooldownTicks"),
        ]
    );
    assert!(core[4].1.contains("LikedNoteblockCooldownTicks"));
    assert!(core[5].1.contains("ItemPickupCooldownTicks"));
    assert!(core[0].1.contains("0.8"));
    assert!(core[1].1.contains("2.5"));
}

#[test]
fn the_idle_activity_goes_to_items_gives_them_stays_close_looks_and_wanders() {
    let idle = named(Activity::Idle);
    let priorities: Vec<i32> = idle.iter().map(|(priority, _)| *priority).collect();
    assert_eq!(priorities, vec![0, 1, 2, 3, 4]);
    assert!(idle[0].1.contains("GoToWantedItem") && idle[0].1.contains("1.75"));
    assert!(idle[0].1.contains("32"));
    assert!(idle[1].1.contains("AllayGiveItems"));
    assert!(idle[2].1.contains("StayCloseToTarget"));
    assert!(idle[3].1.contains("SetEntityLookTargetSometimes"));
    assert!(idle[3].1.contains("6.0"));
    assert!(idle[4].1.contains("Gate"));
    assert!(idle[4].1.contains("RandomStroll") && idle[4].1.contains("flying: true"));
    assert!(idle[4].1.contains("SetWalkTargetFromLookTarget"));
    assert!(idle[4].1.contains("DoNothing"));
}

#[test]
fn an_allay_registers_the_liked_memories_and_the_four_sensors() {
    let ai = MobAi::new(EntityKind::Allay, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    for memory in ALLAY_MEMORIES {
        assert!(brain.memories().is_registered(memory), "{memory:?}");
    }
    for memory in [
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryModuleType::NearestPlayers,
        MemoryModuleType::HurtBy,
        MemoryModuleType::NearestVisibleWantedItem,
        MemoryModuleType::WalkTarget,
        MemoryModuleType::LookTarget,
    ] {
        assert!(brain.memories().is_registered(memory), "{memory:?}");
    }
}

#[test]
fn hearing_a_noteblock_remembers_it_for_six_hundred_ticks_and_only_the_same_one_refreshes() {
    let mut bench = Bench::new(AllayFacts::default(), Field::default());
    bench.with(|cx| rspigot_ai::hear_noteblock(cx, BlockPos::new(5, 64, 5)));
    assert_eq!(
        bench
            .get(MemoryModuleType::LikedNoteblockPosition)
            .and_then(MemoryValue::as_global_pos),
        Some(&pos_of(5, 64, 5))
    );
    assert_eq!(
        bench
            .get(MemoryModuleType::LikedNoteblockCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(600)
    );
    bench.set(
        MemoryModuleType::LikedNoteblockCooldownTicks,
        MemoryValue::Int(10),
    );
    bench.with(|cx| rspigot_ai::hear_noteblock(cx, BlockPos::new(9, 64, 9)));
    assert_eq!(
        bench
            .get(MemoryModuleType::LikedNoteblockCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(10)
    );
    assert_eq!(
        bench
            .get(MemoryModuleType::LikedNoteblockPosition)
            .and_then(MemoryValue::as_global_pos),
        Some(&pos_of(5, 64, 5))
    );
    bench.with(|cx| rspigot_ai::hear_noteblock(cx, BlockPos::new(5, 64, 5)));
    assert_eq!(
        bench
            .get(MemoryModuleType::LikedNoteblockCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(600)
    );
}

#[test]
fn the_deposit_position_is_above_a_live_liked_noteblock_and_the_noteblock_is_forgotten_otherwise() {
    let mut world = Field::default();
    world.edits.insert(BlockPos::new(5, 64, 5), noteblock());
    let mut bench = Bench::new(carrying(), world);
    bench.set(
        MemoryModuleType::LikedNoteblockPosition,
        MemoryValue::GlobalPos(pos_of(5, 64, 5)),
    );
    bench.set(
        MemoryModuleType::LikedNoteblockCooldownTicks,
        MemoryValue::Int(100),
    );
    let spot = bench.with(rspigot_ai::item_deposit_position);
    let at = spot.and_then(|tracker| tracker.current_position(&bench.world));
    assert_eq!(at, Some(Vec3::new(5.5, 65.5, 5.5)));
    bench
        .state
        .memories
        .erase(MemoryModuleType::LikedNoteblockCooldownTicks);
    let spot = bench.with(rspigot_ai::item_deposit_position);
    assert!(spot.is_none());
    assert!(
        bench
            .get(MemoryModuleType::LikedNoteblockPosition)
            .is_none()
    );
}

#[test]
fn a_noteblock_that_was_broken_is_forgotten_and_the_liked_player_takes_over() {
    let mut bench = Bench::new(carrying(), liked_player_world(Vec3::new(10.5, 64.0, 0.5)));
    bench.set(
        MemoryModuleType::LikedNoteblockPosition,
        MemoryValue::GlobalPos(pos_of(5, 64, 5)),
    );
    bench.set(
        MemoryModuleType::LikedNoteblockCooldownTicks,
        MemoryValue::Int(100),
    );
    like_player(&mut bench);
    let spot = bench.with(rspigot_ai::item_deposit_position);
    assert!(spot.is_some());
    assert!(
        bench
            .get(MemoryModuleType::LikedNoteblockPosition)
            .is_none()
    );
}

#[test]
fn the_liked_player_must_be_a_non_spectator_within_sixty_four_blocks() {
    let mut bench = Bench::new(carrying(), liked_player_world(Vec3::new(10.5, 64.0, 0.5)));
    like_player(&mut bench);
    assert!(bench.with(|cx| rspigot_ai::liked_player(cx)).is_some());
    bench.world.players[0].position = Vec3::new(70.5, 70.0, 0.5);
    assert!(bench.with(|cx| rspigot_ai::liked_player(cx)).is_none());
    bench.world.players[0].position = Vec3::new(10.5, 64.0, 0.5);
    bench.world.players[0].spectator = true;
    assert!(bench.with(|cx| rspigot_ai::liked_player(cx)).is_none());
}

fn give_control() -> MobControl {
    controls(Activity::Idle)
        .into_iter()
        .find(|(priority, _)| *priority == 1)
        .map(|(_, control)| control)
        .unwrap()
}

#[test]
fn giving_walks_to_the_target_at_two_point_two_five_and_throws_within_three_blocks() {
    let mut bench = Bench::new(carrying(), liked_player_world(Vec3::new(10.5, 64.0, 0.5)));
    like_player(&mut bench);
    let mut give = give_control();
    assert!(bench.start(&mut give, NOW));
    let walk = bench
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .unwrap();
    assert_eq!(walk.speed_modifier, 2.25);
    assert_eq!(walk.close_enough_dist, 3);
    assert!(bench.get(MemoryModuleType::LookTarget).is_some());
    bench.tick(&mut give, NOW + 1);
    assert!(bench.throws().is_empty());
    bench.world.players[0].position = Vec3::new(1.5, 69.0, 0.5);
    bench.tick(&mut give, NOW + 2);
    assert_eq!(bench.throws().len(), 1);
    assert_eq!(
        bench
            .get(MemoryModuleType::ItemPickupCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(60)
    );
}

#[test]
fn giving_needs_an_item_in_the_inventory_and_a_target() {
    let mut empty = Bench::new(
        AllayFacts::default(),
        liked_player_world(Vec3::new(10.5, 64.0, 0.5)),
    );
    like_player(&mut empty);
    assert!(!empty.start(&mut give_control(), NOW));
    let mut alone = Bench::new(carrying(), Field::default());
    assert!(!alone.start(&mut give_control(), NOW));
}

#[test]
fn staying_close_walks_to_the_target_when_farther_than_four_blocks_and_nothing_wanted_is_around() {
    let stay = || {
        MobControl::one_shot(rspigot_ai::MobTrigger::StayCloseToTarget(
            StayCloseToTarget::new(),
        ))
    };
    let mut bench = Bench::new(carrying(), liked_player_world(Vec3::new(10.5, 70.0, 0.5)));
    like_player(&mut bench);
    assert!(bench.start(&mut stay(), NOW));
    let walk = bench
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .unwrap();
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (2.25, 4));
    let mut near = Bench::new(carrying(), liked_player_world(Vec3::new(2.5, 70.0, 0.5)));
    like_player(&mut near);
    assert!(!near.start(&mut stay(), NOW));
    let mut busy = Bench::new(carrying(), liked_player_world(Vec3::new(10.5, 70.0, 0.5)));
    like_player(&mut busy);
    busy.set(
        MemoryModuleType::NearestVisibleWantedItem,
        MemoryValue::Entity(EntityId(9)),
    );
    assert!(!busy.start(&mut stay(), NOW));
}

#[test]
fn an_allay_wants_only_the_item_it_holds_and_only_while_its_inventory_has_room() {
    let hand = ItemId(10);
    let other = ItemId(11);
    let facts = AllayFacts {
        hand_item: Some(hand),
        ..AllayFacts::default()
    };
    assert!(facts.wants(hand, true));
    assert!(!facts.wants(other, true));
    assert!(!facts.wants(hand, false));
    assert!(!AllayFacts::default().wants(hand, true));
    let stacked = AllayFacts {
        hand_item: Some(hand),
        inventory_item: Some(hand),
        inventory_full: false,
        ..AllayFacts::default()
    };
    assert!(stacked.wants(hand, true));
    let full = AllayFacts {
        inventory_full: true,
        ..stacked
    };
    assert!(!full.wants(hand, true));
    let different = AllayFacts {
        inventory_item: Some(other),
        ..stacked
    };
    assert!(!different.wants(hand, true));
}
