use std::collections::{HashMap, HashSet};
use std::num::NonZeroU8;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, ContainerView,
    CopperGolemAction, Facts, GlobalPos, GoalSettings, InteractionState, Living, MemoryModuleType,
    MemoryValue, MobAction, MobAi, MobBehavior, MobBody, MobControl, MobView, SensorKind, Status,
    TargetKind, TargetVerdict, TransportItemsBetweenContainers, TransportState,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::copper_golem::GolemState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::{BlockPos, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

const GOLEM: EntityId = EntityId(1);
const OVERWORLD: &str = "minecraft:overworld";

#[derive(Default)]
struct Yard {
    containers: HashMap<BlockPos, Vec<Slot>>,
    compound: HashSet<BlockPos>,
    blocks: HashMap<BlockPos, StateId>,
    locked: HashSet<BlockPos>,
    openers: HashMap<BlockPos, Vec<EntityId>>,
    hand: Option<ItemStack>,
    others: Vec<Living>,
    time: i64,
}

impl Yard {
    fn new() -> Self {
        Self {
            time: 1_000,
            ..Self::default()
        }
    }

    fn chest(&mut self, pos: BlockPos, kind: BlockKind, slots: Vec<Slot>) {
        self.blocks.insert(pos, kind.default_state());
        let mut full = vec![None; 27];
        for (index, stack) in slots.into_iter().enumerate() {
            full[index] = stack;
        }
        self.containers.insert(pos, full);
    }
}

impl rspigot_ai::BlockView for Yard {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if pos.y < 64 {
            BlockKind::Stone.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for Yard {
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

    fn dimension(&self) -> &str {
        OVERWORLD
    }

    fn chests_in_chunk(&self, chunk: ChunkPos) -> Option<Vec<BlockPos>> {
        let mut found: Vec<BlockPos> = self
            .containers
            .keys()
            .filter(|pos| pos.chunk() == chunk)
            .copied()
            .collect();
        found.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        Some(found)
    }

    fn chest_container(&self, pos: BlockPos, ignore_blocked: bool) -> Option<ContainerView> {
        if !ignore_blocked && rspigot_ai::is_chest_blocked_at(self, pos) {
            return None;
        }
        self.containers.get(&pos).map(|slots| ContainerView {
            slots: slots.clone(),
            compound: self.compound.contains(&pos),
        })
    }

    fn container_locked(&self, pos: BlockPos) -> bool {
        self.locked.contains(&pos)
    }

    fn container_openers(&self, pos: BlockPos) -> Vec<EntityId> {
        self.openers.get(&pos).cloned().unwrap_or_default()
    }

    fn main_hand_stack(&self, _id: EntityId) -> Option<ItemStack> {
        self.hand.clone()
    }
}

fn stack(kind: ItemKind, count: u8) -> Slot {
    Some(ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap()))
}

fn golem_view(at: Vec3, hand: Option<ItemKind>) -> MobView {
    let mut base = EntityBase::new(GOLEM, Uuid::from_u128(1), EntityKind::CopperGolem);
    base.position = at;
    base.old_position = at;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    let mut view = MobView::of(&base, &state);
    view.main_hand = hand.map(ItemKind::id);
    view
}

struct Bench {
    mob: MobView,
    world: Yard,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
    control: MobControl,
    now: i64,
}

impl Bench {
    fn new(at: Vec3, hand: Option<ItemKind>, world: Yard) -> Self {
        let mob = golem_view(at, hand);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        let mut world = world;
        world.hand = hand.and_then(|kind| stack(kind, 12));
        let now = world.time;
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(EntityKind::CopperGolem, 7),
            state,
            control: MobControl::behavior(MobBehavior::TransportItems(
                TransportItemsBetweenContainers::new(),
            )),
            now,
        }
    }

    fn with<R>(&mut self, run: impl FnOnce(&mut BehaviorCx<'_, '_, Yard>) -> R) -> R {
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

    fn start(&mut self) -> bool {
        let now = self.now;
        let mut control = self.control.clone();
        let started = self.with(|cx| control.try_start(cx, now));
        self.control = control;
        started
    }

    fn tick(&mut self) -> Vec<MobAction> {
        self.now += 1;
        let now = self.now;
        let mut control = self.control.clone();
        self.with(|cx| control.tick_or_stop(cx, now));
        self.control = control;
        let actions = std::mem::take(&mut self.controls.actions);
        self.apply(&actions);
        actions
    }

    fn apply(&mut self, actions: &[MobAction]) {
        for action in actions {
            let MobAction::CopperGolem(action) = action else {
                continue;
            };
            match action {
                CopperGolemAction::SetContainer { pos, slots } => {
                    if let Some(container) = self.world.containers.get_mut(pos) {
                        for (index, stack) in slots {
                            container[*index] = stack.clone();
                        }
                    }
                }
                CopperGolemAction::SetMainHand { stack, .. } => {
                    self.world.hand = stack.clone();
                    self.mob.main_hand = stack.as_ref().map(|stack| stack.item);
                }
                CopperGolemAction::OpenContainer(pos) => {
                    self.world.openers.entry(*pos).or_default().push(GOLEM);
                }
                CopperGolemAction::CloseContainer(pos) => {
                    if let Some(list) = self.world.openers.get_mut(pos) {
                        list.retain(|id| *id != GOLEM);
                    }
                }
                _ => {}
            }
        }
    }

    fn run(&mut self, ticks: usize) -> Vec<(i64, MobAction)> {
        let mut all = Vec::new();
        for _ in 0..ticks {
            let actions = self.tick();
            all.extend(actions.into_iter().map(|action| (self.now, action)));
        }
        all
    }

    fn memory(&self, ty: MemoryModuleType) -> Option<&MemoryValue> {
        self.state.memories.get(ty)
    }

    fn behavior(&self) -> &TransportItemsBetweenContainers {
        let MobControl::Timed(timed) = &self.control else {
            panic!("a timed control");
        };
        match timed.inner() {
            MobBehavior::TransportItems(behavior) => behavior,
            other => panic!("not the transport behavior: {other:?}"),
        }
    }
}

const SOURCE: BlockPos = BlockPos::new(5, 64, 0);
const DESTINATION: BlockPos = BlockPos::new(-4, 64, 0);
const AT_SOURCE: Vec3 = Vec3::new(4.5, 64.0, 0.5);
const AT_DESTINATION: Vec3 = Vec3::new(-3.3, 64.0, 0.5);

fn golem_actions(actions: &[(i64, MobAction)]) -> Vec<(i64, &CopperGolemAction)> {
    actions
        .iter()
        .filter_map(|(at, action)| match action {
            MobAction::CopperGolem(action) => Some((*at, action)),
            _ => None,
        })
        .collect()
}

fn sounds(actions: &[(i64, MobAction)]) -> Vec<(i64, &'static str)> {
    actions
        .iter()
        .filter_map(|(at, action)| match action {
            MobAction::Sound { sound, .. } => Some((*at, *sound)),
            _ => None,
        })
        .collect()
}

fn yard_with_source(items: Vec<Slot>) -> Yard {
    let mut yard = Yard::new();
    yard.chest(SOURCE, BlockKind::CopperChest, items);
    yard
}

fn named(control: &MobControl) -> String {
    format!("{control:?}")
}

fn behaviors(activity: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(EntityKind::CopperGolem, 3, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, named(control)))
        .collect()
}

fn leading(text: &str) -> &'static str {
    const NAMES: [&str; 8] = [
        "AnimalPanic",
        "LookAtTargetSink",
        "MoveToTargetSink",
        "InteractWithDoor",
        "CountDownCooldownTicks",
        "TransportItems",
        "SetEntityLookTargetSometimes",
        "Gate",
    ];
    NAMES
        .into_iter()
        .filter_map(|name| text.find(name).map(|at| (at, name)))
        .min_by_key(|(at, _)| *at)
        .map_or("?", |(_, name)| name)
}

#[test]
fn the_copper_golem_core_and_idle_activities_follow_the_vanilla_priorities() {
    let core: Vec<(i32, &str)> = behaviors(Activity::Core)
        .iter()
        .map(|(priority, text)| (*priority, leading(text)))
        .collect();
    assert_eq!(
        core,
        vec![
            (0, "AnimalPanic"),
            (1, "LookAtTargetSink"),
            (2, "MoveToTargetSink"),
            (3, "InteractWithDoor"),
            (4, "CountDownCooldownTicks"),
            (5, "CountDownCooldownTicks"),
        ]
    );
    let idle: Vec<(i32, &str)> = behaviors(Activity::Idle)
        .iter()
        .map(|(priority, text)| (*priority, leading(text)))
        .collect();
    assert_eq!(
        idle,
        vec![
            (0, "TransportItems"),
            (1, "SetEntityLookTargetSometimes"),
            (2, "Gate"),
        ]
    );
    let all = behaviors(Activity::Core);
    assert!(all[0].1.contains("speed_multiplier: 1.5"));
    assert!(all[4].1.contains("GazeCooldownTicks"));
    assert!(all[5].1.contains("TransportItemsCooldownTicks"));
    let idle = behaviors(Activity::Idle);
    assert!(idle[1].1.contains("Kind(Player)"));
    assert!(idle[1].1.contains("6.0"));
    assert!(idle[2].1.contains("TransportItemsCooldownTicks"));
    assert!(idle[2].1.contains("ValueAbsent"));
}

#[test]
fn the_copper_golem_brain_senses_nearby_mobs_and_who_hurt_it() {
    let ai = MobAi::new(EntityKind::CopperGolem, 3, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    let kinds: Vec<&str> = brain
        .sensors()
        .iter()
        .map(|slot| match slot.sensor() {
            SensorKind::NearestLivingEntities(_) => "mobs",
            SensorKind::HurtBy(_) => "hurt_by",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["mobs", "hurt_by"]);
}

#[test]
fn a_new_copper_golem_waits_sixty_to_ninety_nine_ticks_before_transporting() {
    let cooldowns: HashSet<i32> = (0..200)
        .map(|seed| {
            let ai = MobAi::new(EntityKind::CopperGolem, seed, GoalSettings::default());
            ai.behavior_brain()
                .unwrap()
                .memories()
                .get(MemoryModuleType::TransportItemsCooldownTicks)
                .and_then(MemoryValue::as_int)
                .expect("an initial cooldown")
        })
        .collect();
    assert!(cooldowns.iter().all(|value| (60..=99).contains(value)));
    assert!(cooldowns.len() > 20);
}

#[test]
fn the_golem_has_no_target_until_it_is_off_cooldown_and_not_panicking() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 5)]),
    );
    bench.state.memories.set(
        MemoryModuleType::TransportItemsCooldownTicks,
        MemoryValue::Int(10),
    );
    assert!(!bench.start());
    bench
        .state
        .memories
        .erase(MemoryModuleType::TransportItemsCooldownTicks);
    bench
        .state
        .memories
        .set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert!(!bench.start());
    bench.state.memories.erase(MemoryModuleType::IsPanicking);
    bench.mob.leashed = true;
    assert!(!bench.start());
    bench.mob.leashed = false;
    assert!(bench.start());
    assert_eq!(bench.control.status(), Status::Running);
}

#[test]
fn picking_up_takes_sixteen_items_after_sixty_ticks_of_opening_the_chest() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![None, stack(ItemKind::Apple, 40)]),
    );
    assert!(bench.start());
    let actions = bench.run(70);
    let golem = golem_actions(&actions);
    let opened = golem
        .iter()
        .find(|(_, action)| matches!(action, CopperGolemAction::OpenContainer(_)))
        .expect("the chest opens");
    assert_eq!(*opened.1, CopperGolemAction::OpenContainer(SOURCE));
    let first = opened.0;
    assert!(golem.iter().any(|(at, action)| *at == first
        && **action == CopperGolemAction::SetState(GolemState::GettingItem)));
    assert_eq!(
        sounds(&actions),
        vec![(first + 8, "minecraft:entity.copper_golem.no_item_get")]
    );
    let closed = golem
        .iter()
        .find(|(_, action)| matches!(action, CopperGolemAction::CloseContainer(_)))
        .expect("the chest closes");
    assert_eq!(closed.0, first + 59);
    assert_eq!(*closed.1, CopperGolemAction::CloseContainer(SOURCE));
    let taken = golem
        .iter()
        .find_map(|(at, action)| match action {
            CopperGolemAction::SetContainer { pos, slots } => Some((*at, *pos, slots.clone())),
            _ => None,
        })
        .expect("the stack is taken");
    assert_eq!(taken.0, first + 59);
    assert_eq!(taken.1, SOURCE);
    assert_eq!(taken.2, vec![(1, stack(ItemKind::Apple, 24))]);
    assert!(golem.iter().any(|(_, action)| matches!(
        action,
        CopperGolemAction::SetMainHand { stack: held, guaranteed: true }
            if *held == stack(ItemKind::Apple, 16)
    )));
    assert_eq!(bench.world.hand, stack(ItemKind::Apple, 16));
    assert!(golem.iter().any(|(at, action)| *at == first + 59
        && **action == CopperGolemAction::SetState(GolemState::Idle)));
    assert!(
        bench
            .memory(MemoryModuleType::VisitedBlockPositions)
            .is_none()
    );
}

#[test]
fn an_empty_source_chest_plays_the_no_get_sound_and_changes_nothing() {
    let mut bench = Bench::new(AT_SOURCE, None, yard_with_source(Vec::new()));
    assert!(bench.start());
    let actions = bench.run(65);
    let golem = golem_actions(&actions);
    assert!(
        golem
            .iter()
            .any(|(_, action)| **action == CopperGolemAction::SetState(GolemState::GettingNoItem))
    );
    assert_eq!(
        sounds(&actions)
            .iter()
            .map(|(_, sound)| *sound)
            .collect::<Vec<_>>(),
        vec!["minecraft:entity.copper_golem.no_item_no_get"]
    );
    assert!(
        golem
            .iter()
            .all(|(_, action)| !matches!(action, CopperGolemAction::SetContainer { .. }))
    );
    assert!(bench.world.hand.is_none());
    assert_eq!(
        bench
            .memory(MemoryModuleType::TransportItemsCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(140)
    );
}

#[test]
fn dropping_merges_into_a_chest_holding_the_same_item_and_empties_the_hand() {
    let mut yard = Yard::new();
    yard.chest(
        DESTINATION,
        BlockKind::Chest,
        vec![stack(ItemKind::Apple, 60), stack(ItemKind::Stick, 3)],
    );
    let mut bench = Bench::new(AT_DESTINATION, Some(ItemKind::Apple), yard);
    assert!(bench.start());
    let actions = bench.run(70);
    let golem = golem_actions(&actions);
    assert!(
        golem
            .iter()
            .any(|(_, action)| **action == CopperGolemAction::SetState(GolemState::DroppingItem))
    );
    assert_eq!(
        sounds(&actions)
            .iter()
            .map(|(_, sound)| *sound)
            .collect::<Vec<_>>(),
        vec!["minecraft:entity.copper_golem.item_drop"]
    );
    let container = &bench.world.containers[&DESTINATION];
    assert_eq!(container[0], stack(ItemKind::Apple, 64));
    assert_eq!(container[1], stack(ItemKind::Stick, 3));
    assert_eq!(container[2], stack(ItemKind::Apple, 8));
    assert!(bench.world.hand.is_none());
}

#[test]
fn a_destination_without_the_held_item_is_refused_and_the_hand_is_kept() {
    let mut yard = Yard::new();
    yard.chest(
        DESTINATION,
        BlockKind::TrappedChest,
        vec![stack(ItemKind::Stick, 3)],
    );
    let mut bench = Bench::new(AT_DESTINATION, Some(ItemKind::Apple), yard);
    assert!(bench.start());
    let actions = bench.run(70);
    let golem = golem_actions(&actions);
    assert!(
        golem
            .iter()
            .any(|(_, action)| **action == CopperGolemAction::SetState(GolemState::DroppingNoItem))
    );
    assert_eq!(
        sounds(&actions)
            .iter()
            .map(|(_, sound)| *sound)
            .collect::<Vec<_>>(),
        vec!["minecraft:entity.copper_golem.item_no_drop"]
    );
    assert_eq!(
        bench.world.containers[&DESTINATION][0],
        stack(ItemKind::Stick, 3)
    );
    assert_eq!(bench.world.hand, stack(ItemKind::Apple, 12));
}

#[test]
fn an_empty_destination_takes_the_whole_stack_in_its_first_slot() {
    let mut yard = Yard::new();
    yard.chest(DESTINATION, BlockKind::Chest, Vec::new());
    let mut bench = Bench::new(AT_DESTINATION, Some(ItemKind::Stick), yard);
    assert!(bench.start());
    bench.run(70);
    assert_eq!(
        bench.world.containers[&DESTINATION][0],
        stack(ItemKind::Stick, 12)
    );
    assert!(bench.world.hand.is_none());
}

#[test]
fn a_full_destination_hands_back_the_remainder_and_stops_targeting_it() {
    let mut yard = Yard::new();
    yard.chest(
        DESTINATION,
        BlockKind::Chest,
        std::iter::repeat_n(stack(ItemKind::Apple, 64), 27).collect(),
    );
    let mut bench = Bench::new(AT_DESTINATION, Some(ItemKind::Apple), yard);
    assert!(bench.start());
    bench.run(70);
    assert_eq!(bench.world.hand, stack(ItemKind::Apple, 12));
    assert!(bench.behavior().target().is_none());
}

#[test]
fn only_copper_chests_are_sources_and_only_plain_chests_are_destinations() {
    let mut yard = Yard::new();
    yard.chest(SOURCE, BlockKind::Chest, vec![stack(ItemKind::Apple, 3)]);
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
    assert_eq!(
        bench
            .memory(MemoryModuleType::TransportItemsCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(140)
    );
    let mut yard = Yard::new();
    yard.chest(DESTINATION, BlockKind::CopperChest, Vec::new());
    let mut bench = Bench::new(AT_DESTINATION, Some(ItemKind::Apple), yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
}

#[test]
fn a_chest_with_a_solid_block_above_or_a_sitting_cat_on_it_is_not_a_target() {
    let mut yard = yard_with_source(vec![stack(ItemKind::Apple, 3)]);
    yard.blocks
        .insert(SOURCE.offset(0, 1, 0), BlockKind::Stone.default_state());
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
    let mut yard = yard_with_source(vec![stack(ItemKind::Apple, 3)]);
    let mut cat = other(5, EntityKind::Cat, Vec3::new(5.5, 65.0, 0.5));
    cat.facts.sitting = true;
    yard.others.push(cat);
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
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

#[test]
fn locked_containers_are_skipped() {
    let mut yard = yard_with_source(vec![stack(ItemKind::Apple, 3)]);
    yard.locked.insert(SOURCE);
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
}

#[test]
fn chests_beyond_thirty_two_blocks_across_or_eight_up_are_out_of_range() {
    let mut yard = Yard::new();
    yard.chest(
        BlockPos::new(33, 64, 0),
        BlockKind::CopperChest,
        vec![stack(ItemKind::Apple, 3)],
    );
    yard.chest(
        BlockPos::new(2, 73, 0),
        BlockKind::CopperChest,
        vec![stack(ItemKind::Apple, 3)],
    );
    let mut bench = Bench::new(Vec3::new(0.5, 64.0, 0.5), None, yard);
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
    let mut yard = Yard::new();
    yard.chest(
        BlockPos::new(32, 64, 0),
        BlockKind::CopperChest,
        vec![stack(ItemKind::Apple, 3)],
    );
    let mut bench = Bench::new(Vec3::new(0.5, 64.0, 0.5), None, yard);
    assert!(bench.start());
    bench.run(2);
    assert_eq!(
        bench.behavior().target().map(|target| target.pos),
        Some(BlockPos::new(32, 64, 0))
    );
}

#[test]
fn the_nearest_valid_chest_is_picked_and_the_choice_is_remembered_as_visited() {
    let mut yard = Yard::new();
    let near = BlockPos::new(6, 64, 0);
    let far = BlockPos::new(12, 64, 3);
    yard.chest(far, BlockKind::CopperChest, vec![stack(ItemKind::Apple, 3)]);
    yard.chest(
        near,
        BlockKind::CopperChest,
        vec![stack(ItemKind::Apple, 3)],
    );
    let mut bench = Bench::new(Vec3::new(0.5, 64.0, 0.5), None, yard);
    assert!(bench.start());
    let actions = bench.run(1);
    assert_eq!(
        bench.behavior().target().map(|target| target.pos),
        Some(near)
    );
    assert!(
        golem_actions(&actions)
            .iter()
            .any(|(_, action)| **action == CopperGolemAction::TargetBlock(near))
    );
    let Some(MemoryValue::GlobalPosList(visited)) =
        bench.memory(MemoryModuleType::VisitedBlockPositions)
    else {
        panic!("visited positions");
    };
    assert_eq!(
        visited,
        &vec![GlobalPos {
            dimension: OVERWORLD.to_owned(),
            pos: near
        }]
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::VisitedBlockPositions),
        Some(6000)
    );
}

#[test]
fn a_visited_or_unreachable_chest_is_not_chosen_again() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    bench.state.memories.set(
        MemoryModuleType::UnreachableTransportBlockPositions,
        MemoryValue::GlobalPosList(vec![GlobalPos {
            dimension: OVERWORLD.to_owned(),
            pos: SOURCE,
        }]),
    );
    assert!(bench.start());
    bench.run(2);
    assert!(bench.behavior().target().is_none());
    assert!(
        bench
            .memory(MemoryModuleType::UnreachableTransportBlockPositions)
            .is_none()
    );
}

#[test]
fn more_than_ten_visited_chests_send_the_golem_on_cooldown() {
    let visited: Vec<GlobalPos> = (0..10)
        .map(|x| GlobalPos {
            dimension: OVERWORLD.to_owned(),
            pos: BlockPos::new(100 + x, 64, 0),
        })
        .collect();
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    bench.state.memories.set(
        MemoryModuleType::VisitedBlockPositions,
        MemoryValue::GlobalPosList(visited),
    );
    assert!(bench.start());
    bench.run(1);
    assert_eq!(
        bench
            .memory(MemoryModuleType::TransportItemsCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(140)
    );
    assert!(
        bench
            .memory(MemoryModuleType::VisitedBlockPositions)
            .is_none()
    );
}

#[test]
fn a_golem_close_to_a_busy_chest_queues_until_it_is_free() {
    let mut bench = Bench::new(
        Vec3::new(3.0, 64.0, 0.5),
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    bench.world.openers.insert(SOURCE, vec![EntityId(9)]);
    assert!(bench.start());
    bench.run(3);
    assert_eq!(bench.behavior().state(), TransportState::Queuing);
    assert_eq!(bench.behavior().interaction(), None);
    bench.world.openers.clear();
    bench.run(1);
    assert_ne!(bench.behavior().state(), TransportState::Queuing);
}

#[test]
fn the_interaction_state_is_chosen_when_the_golem_reaches_the_chest() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    assert!(bench.start());
    bench.run(2);
    assert_eq!(bench.behavior().state(), TransportState::Interacting);
    assert_eq!(
        bench.behavior().interaction(),
        Some(InteractionState::PickupItem)
    );
    assert_eq!(bench.behavior().ticks_since_reaching_target(), 1);
}

#[test]
fn a_double_chest_is_not_closed_by_the_golem_but_left_to_the_opener_recheck() {
    let mut yard = yard_with_source(vec![stack(ItemKind::Apple, 3)]);
    yard.compound.insert(SOURCE);
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    let actions = bench.run(70);
    let golem = golem_actions(&actions);
    assert!(
        golem
            .iter()
            .any(|(_, action)| matches!(action, CopperGolemAction::OpenContainer(_)))
    );
    assert!(
        !golem
            .iter()
            .any(|(_, action)| matches!(action, CopperGolemAction::CloseContainer(_)))
    );
    assert!(
        golem
            .iter()
            .any(|(_, action)| **action == CopperGolemAction::ClearOpenedChest)
    );
}

#[test]
fn a_cancelled_target_event_puts_the_golem_on_cooldown_and_a_retarget_swaps_the_chest() {
    let mut yard = yard_with_source(vec![stack(ItemKind::Apple, 3)]);
    let other_chest = BlockPos::new(5, 64, 3);
    yard.chest(
        other_chest,
        BlockKind::CopperChest,
        vec![stack(ItemKind::Apple, 3)],
    );
    let mut bench = Bench::new(AT_SOURCE, None, yard);
    assert!(bench.start());
    bench.run(1);
    let chosen = bench.behavior().target().expect("a target").pos;
    let mut control = bench.control.clone();
    control.set_transport_verdict(TargetVerdict::Retarget {
        from: chosen,
        to: if chosen == SOURCE {
            other_chest
        } else {
            SOURCE
        },
    });
    bench.control = control;
    bench.run(1);
    let retargeted = bench.behavior().target().expect("a new target").pos;
    assert_ne!(retargeted, chosen);
    let mut control = bench.control.clone();
    control.set_transport_verdict(TargetVerdict::Cancel(retargeted));
    bench.control = control;
    bench.run(1);
    assert!(bench.behavior().target().is_none());
    assert_eq!(
        bench
            .memory(MemoryModuleType::TransportItemsCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(140)
    );
}

#[test]
fn stopping_the_behavior_resets_the_golem_state_and_closes_nothing_by_itself() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    assert!(bench.start());
    bench.run(20);
    let now = bench.now;
    let mut control = bench.control.clone();
    bench.with(|cx| control.do_stop(cx, now));
    bench.control = control;
    let actions: Vec<MobAction> = std::mem::take(&mut bench.controls.actions);
    assert!(actions.contains(&MobAction::CopperGolem(CopperGolemAction::ClearOpenedChest)));
    assert!(
        actions.contains(&MobAction::CopperGolem(CopperGolemAction::SetState(
            GolemState::Idle
        )))
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        MobAction::CopperGolem(CopperGolemAction::CloseContainer(_))
    )));
    assert!(!bench.controls.navigation.path_to_targets_below_surface());
}

#[test]
fn the_golem_navigates_to_targets_below_the_surface_while_transporting() {
    let mut bench = Bench::new(
        AT_SOURCE,
        None,
        yard_with_source(vec![stack(ItemKind::Apple, 3)]),
    );
    assert!(!bench.controls.navigation.path_to_targets_below_surface());
    assert!(bench.start());
    assert!(bench.controls.navigation.path_to_targets_below_surface());
}

#[test]
fn the_idle_activity_is_always_selected_after_the_brain_tick() {
    let ai = MobAi::new(EntityKind::CopperGolem, 3, GoalSettings::default());
    let brain = ai.behavior_brain().unwrap();
    assert!(brain.state().is_active(Activity::Idle));
    assert!(brain.state().is_active(Activity::Core));
}
