use std::collections::HashMap;

use rspigot_ai::{
    AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, Facts, GlobalPos,
    InteractWithDoor, Living, MemoryModuleType, MemoryValue, MobAction, MobBody, MobControl,
    MobTrigger, MobView, Path, PathNode, PathType, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 1_000;
const OVERWORLD: &str = "minecraft:overworld";

#[derive(Default)]
struct Field {
    others: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
    paths: HashMap<EntityId, (BlockPos, BlockPos)>,
    dimension: Option<&'static str>,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.edits.get(&pos) {
            return Some(*state);
        }
        Some(if pos.y < 64 {
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

    fn dimension(&self) -> &str {
        self.dimension.unwrap_or(OVERWORLD)
    }

    fn path_door_nodes(&self, mob: EntityId) -> Option<(BlockPos, BlockPos)> {
        self.paths.get(&mob).copied()
    }
}

fn door(kind: BlockKind, open: bool) -> StateId {
    let lower = kind.default_state().with_property("half", "lower").unwrap();
    lower
        .with_property("open", if open { "true" } else { "false" })
        .unwrap()
}

fn other(id: i32, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id: EntityId(id),
        player: false,
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
            uuid: Uuid::from_u128(id as u128 + 900),
            ..Facts::default()
        },
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
    fn new(kind: EntityKind, world: Field) -> Self {
        let mob = view(kind, Vec3::new(0.5, 64.0, 0.5));
        let mut state = BrainState::default();
        for ty in [
            MemoryModuleType::Path,
            MemoryModuleType::DoorsToClose,
            MemoryModuleType::NearestLivingEntities,
        ] {
            state.memories.register(ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(kind, 7),
            state,
        }
    }

    fn control() -> MobControl {
        MobControl::one_shot(MobTrigger::InteractWithDoor(InteractWithDoor::default()))
    }

    fn walk(&mut self, nodes: &[BlockPos], advanced: usize) {
        let nodes = nodes
            .iter()
            .map(|pos| PathNode::new(*pos, PathType::Walkable))
            .collect();
        let mut path = Path::new(nodes, BlockPos::new(9, 64, 0), true);
        for _ in 0..advanced {
            path.advance();
        }
        self.state
            .memories
            .set(MemoryModuleType::Path, MemoryValue::Path(path));
    }

    fn start(&mut self, control: &mut MobControl) -> bool {
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
        control.try_start(&mut cx, NOW)
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn remembered(&self) -> Vec<BlockPos> {
        match self.state.memories.get(MemoryModuleType::DoorsToClose) {
            Some(MemoryValue::GlobalPosList(doors)) => doors.iter().map(|door| door.pos).collect(),
            _ => Vec::new(),
        }
    }

    fn remember(&mut self, positions: &[BlockPos]) {
        let doors = positions
            .iter()
            .map(|pos| GlobalPos {
                dimension: OVERWORLD.to_owned(),
                pos: *pos,
            })
            .collect();
        self.state.memories.set(
            MemoryModuleType::DoorsToClose,
            MemoryValue::GlobalPosList(doors),
        );
    }

    fn see(&mut self, entities: &[Living]) {
        self.world.others.extend(entities.iter().copied());
        let ids = entities.iter().map(|living| living.id).collect();
        self.state.memories.set(
            MemoryModuleType::NearestLivingEntities,
            MemoryValue::Entities(ids),
        );
    }
}

fn opens(pos: BlockPos) -> MobAction {
    MobAction::SetDoorOpen {
        pos,
        open: true,
        interact: true,
    }
}

fn closes(pos: BlockPos) -> MobAction {
    MobAction::SetDoorOpen {
        pos,
        open: false,
        interact: false,
    }
}

fn at(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 0)
}

#[test]
fn a_closed_door_on_the_node_ahead_is_opened_and_remembered() {
    let mut world = Field::default();
    world.edits.insert(at(2), door(BlockKind::OakDoor, false));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![opens(at(2))]);
    assert_eq!(bench.remembered(), vec![at(2)]);
}

#[test]
fn a_closed_door_on_the_node_just_left_is_opened_and_remembered() {
    let mut world = Field::default();
    world.edits.insert(at(1), door(BlockKind::OakDoor, false));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.walk(&[at(0), at(1), at(2)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![opens(at(1))]);
    assert_eq!(bench.remembered(), vec![at(1)]);
}

#[test]
fn an_already_open_door_on_the_node_just_left_is_remembered_without_opening() {
    let mut world = Field::default();
    world.edits.insert(at(1), door(BlockKind::OakDoor, true));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.walk(&[at(0), at(1), at(2)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert_eq!(bench.remembered(), vec![at(1)]);
}

#[test]
fn an_already_open_door_on_the_node_ahead_is_not_remembered() {
    let mut world = Field::default();
    world.edits.insert(at(2), door(BlockKind::OakDoor, true));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}

#[test]
fn iron_doors_and_plain_blocks_are_left_alone() {
    let mut world = Field::default();
    world.edits.insert(at(2), door(BlockKind::IronDoor, false));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}

#[test]
fn copper_doors_are_opened_like_wooden_ones() {
    let mut world = Field::default();
    world
        .edits
        .insert(at(2), door(BlockKind::CopperDoor, false));
    let mut bench = Bench::new(EntityKind::Piglin, world);
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![opens(at(2))]);
}

#[test]
fn nothing_happens_before_the_path_started_or_after_it_is_done() {
    let mut bench = Bench::new(EntityKind::Villager, Field::default());
    bench.walk(&[at(0), at(1)], 0);
    assert!(!bench.start(&mut Bench::control()));
    bench.walk(&[at(0), at(1)], 2);
    assert!(!bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
}

#[test]
fn it_needs_a_path_memory() {
    let mut bench = Bench::new(EntityKind::Villager, Field::default());
    assert!(!bench.start(&mut Bench::control()));
}

#[test]
fn a_new_next_node_waits_twenty_ticks_but_the_same_node_runs_every_tick() {
    let mut bench = Bench::new(EntityKind::Villager, Field::default());
    let mut control = Bench::control();
    bench.walk(&[at(0), at(1), at(2), at(3)], 1);
    assert!(bench.start(&mut control));
    assert!(bench.start(&mut control));
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    for _ in 0..19 {
        assert!(!bench.start(&mut control));
    }
    assert!(bench.start(&mut control));
    assert!(bench.start(&mut control));
}

#[test]
fn doors_left_behind_are_closed_and_forgotten() {
    let mut world = Field::default();
    world.edits.insert(at(0), door(BlockKind::OakDoor, true));
    world.edits.insert(at(2), door(BlockKind::OakDoor, false));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(0)]);
    bench.walk(&[at(1), at(2), at(3)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![opens(at(2)), closes(at(0))]);
    assert_eq!(bench.remembered(), vec![at(2)]);
}

#[test]
fn doors_on_the_previous_and_next_nodes_are_kept() {
    let mut world = Field::default();
    world.edits.insert(at(1), door(BlockKind::OakDoor, true));
    world.edits.insert(at(2), door(BlockKind::OakDoor, true));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(1), at(2)]);
    bench.walk(&[at(0), at(1), at(2), at(3)], 2);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert_eq!(bench.remembered(), vec![at(1), at(2)]);
}

#[test]
fn doors_more_than_three_blocks_away_are_forgotten_without_closing() {
    let mut world = Field::default();
    world.edits.insert(at(6), door(BlockKind::OakDoor, true));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(6)]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}

#[test]
fn doors_of_another_dimension_are_forgotten() {
    let mut world = Field::default();
    world.edits.insert(at(-1), door(BlockKind::OakDoor, true));
    world.dimension = Some("minecraft:the_nether");
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(-1)]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}

#[test]
fn a_door_is_left_open_while_a_mob_of_the_same_kind_is_coming_through() {
    let mut world = Field::default();
    world.edits.insert(at(-1), door(BlockKind::OakDoor, true));
    world.paths.insert(EntityId(5), (at(-1), at(0)));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(-1)]);
    bench.see(&[other(5, EntityKind::Villager, Vec3::new(-0.5, 64.0, 0.5))]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}

#[test]
fn another_kind_of_mob_does_not_keep_the_door_open() {
    let mut world = Field::default();
    world.edits.insert(at(-1), door(BlockKind::OakDoor, true));
    world.paths.insert(EntityId(5), (at(-1), at(0)));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(-1)]);
    bench.see(&[other(5, EntityKind::Piglin, Vec3::new(-0.5, 64.0, 0.5))]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![closes(at(-1))]);
}

#[test]
fn a_same_kind_mob_that_is_too_far_or_not_crossing_does_not_keep_the_door_open() {
    let mut world = Field::default();
    world.edits.insert(at(-1), door(BlockKind::OakDoor, true));
    world.paths.insert(EntityId(5), (at(7), at(8)));
    world.paths.insert(EntityId(6), (at(-1), at(0)));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(-1)]);
    bench.see(&[
        other(5, EntityKind::Villager, Vec3::new(-0.5, 64.0, 0.5)),
        other(6, EntityKind::Villager, Vec3::new(-4.5, 64.0, 0.5)),
    ]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert_eq!(bench.actions(), vec![closes(at(-1))]);
}

#[test]
fn a_remembered_door_that_is_already_shut_is_just_forgotten() {
    let mut world = Field::default();
    world.edits.insert(at(-1), door(BlockKind::OakDoor, false));
    let mut bench = Bench::new(EntityKind::Villager, world);
    bench.remember(&[at(-1)]);
    bench.walk(&[at(0), at(1), at(2)], 1);
    assert!(bench.start(&mut Bench::control()));
    assert!(bench.actions().is_empty());
    assert!(bench.remembered().is_empty());
}
