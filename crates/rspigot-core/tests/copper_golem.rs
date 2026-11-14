mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{
    ClientEvent, Direction, EntityKind as WireKind, Hand, Inbound, MetadataValue, PlayerInput,
};
use rspigot_core::plugins::{EventKind, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

const SOURCE: BlockPos = BlockPos::new(10, 64, 3);
const DESTINATION: BlockPos = BlockPos::new(3, 64, 10);
const STATUE: BlockPos = BlockPos::new(12, 64, 12);
const WAXED_STATUE: BlockPos = BlockPos::new(13, 64, 12);
const COPPER_BASE: BlockPos = BlockPos::new(2, 64, 12);
const GOLEM_AT: Vec3 = Vec3::new(6.5, 64.0, 6.5);
const DOUBLE_LEFT: BlockPos = BlockPos::new(-10, 64, 3);
const DOUBLE_RIGHT: BlockPos = BlockPos::new(-9, 64, 3);

struct Yard;

fn chest_entity(pos: BlockPos, items: Vec<Tag>) -> BlockEntity {
    BlockEntity {
        pos,
        kind: Identifier::minecraft("chest"),
        data: Compound::new().with("Items", Tag::List(items)),
    }
}

fn apples(slot: i8, count: i32) -> Tag {
    Tag::Compound(
        Compound::new()
            .with("Slot", slot)
            .with("id", "minecraft:apple")
            .with("count", count),
    )
}

impl ChunkGenerator for Yard {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        if DOUBLE_LEFT.chunk() == pos {
            for (at, side) in [(DOUBLE_LEFT, "left"), (DOUBLE_RIGHT, "right")] {
                let state = BlockKind::CopperChest.default_state();
                let state = state.with_property("type", side).unwrap_or(state);
                chunk.set_block(at, state);
                chunk.set_block_entity(chest_entity(at, vec![apples(0, 20)]));
            }
        }
        if SOURCE.chunk() == pos {
            chunk.set_block(SOURCE, BlockKind::CopperChest.default_state());
            chunk.set_block_entity(chest_entity(SOURCE, vec![apples(3, 40)]));
            chunk.set_block(DESTINATION, BlockKind::Chest.default_state());
            chunk.set_block_entity(chest_entity(DESTINATION, Vec::new()));
            for (at, kind) in [
                (STATUE, BlockKind::CopperGolemStatue),
                (WAXED_STATUE, BlockKind::WaxedCopperGolemStatue),
            ] {
                chunk.set_block(at, kind.default_state());
                chunk.set_block_entity(BlockEntity {
                    pos: at,
                    kind: Identifier::minecraft("copper_golem_statue"),
                    data: Compound::new().with(
                        "components",
                        Compound::new().with("minecraft:custom_name", "Statuesque"),
                    ),
                });
            }
            chunk.set_block(COPPER_BASE, BlockKind::ExposedCopper.default_state());
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn yard_server() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Yard),
        StatusBoard::new(20),
    )
    .with_rng_seed(11);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    let mut s = scene_on(yard_server(), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, nbt: &str, at: Vec3) -> EntityId {
    let before = s.server.entities_of(Kind::CopperGolem).len();
    console(
        s,
        &format!(
            "summon minecraft:copper_golem {} {} {} {nbt}",
            at.x, at.y, at.z
        ),
    );
    let found = s.server.entities_of(Kind::CopperGolem);
    assert_eq!(found.len(), before + 1, "the copper golem was not summoned");
    found.last().unwrap().id
}

fn alive(s: &Scene, id: EntityId) -> bool {
    s.server
        .entities_of(Kind::CopperGolem)
        .iter()
        .any(|golem| golem.id == id)
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn lid_events(events: &[ClientEvent], pos: BlockPos) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockEvent {
                pos: at,
                action: 1,
                data,
                ..
            } if *at == pos => Some(*data),
            _ => None,
        })
        .collect()
}

fn items_of(s: &Scene, pos: BlockPos) -> Vec<(i64, String, i32)> {
    let data = s.server.block_entity_nbt(pos).expect("a block entity");
    data.get_list("Items")
        .unwrap_or_default()
        .iter()
        .filter_map(|tag| tag.as_compound())
        .map(|entry| {
            (
                entry.get_i64("Slot").unwrap_or(-1),
                entry.get_str("id").unwrap_or_default().to_owned(),
                entry.get_i32("count").unwrap_or(0),
            )
        })
        .collect()
}

fn hand_of(s: &Scene, id: EntityId) -> Option<(String, i32)> {
    let saved = s.server.saved_entity(id)?;
    let equipment = saved.get_compound("equipment")?;
    let hand = equipment.get_compound("mainhand")?;
    Some((
        hand.get_str("id")?.to_owned(),
        hand.get_i32("count").unwrap_or(1),
    ))
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(1);
}

fn use_on(s: &mut Scene, pos: BlockPos, face: Direction) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(2);
}

fn select_first_slot(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
}

fn level_events(events: &[ClientEvent]) -> Vec<i32> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, .. } => Some(*event),
            _ => None,
        })
        .collect()
}

fn ground_count(s: &Scene, item: rspigot_items::ItemKind) -> u32 {
    s.server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == item.id())
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

fn metadata_states(events: &[ClientEvent], entity: EntityId, index: u8) -> Vec<i32> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata {
                entity: id,
                entries,
            } if *id == entity => Some(entries.clone()),
            _ => None,
        })
        .flatten()
        .filter(|entry| entry.index == index)
        .filter_map(|entry| match entry.value {
            MetadataValue::State(_, state) => Some(state),
            _ => None,
        })
        .collect()
}

#[test]
fn a_summoned_copper_golem_is_persistent_with_a_brain_and_unaffected_copper() {
    let mut s = ready(Vec::new());
    let golem = summon(&mut s, "", GOLEM_AT);
    s.tick(1);
    let saved = s.server.saved_entity(golem).unwrap();
    assert!(saved.contains_key("Brain"));
    assert_eq!(saved.get_str("weather_state"), Some("unaffected"));
    assert!(saved.get_i64("next_weather_age").unwrap() > 0);
    assert_eq!(saved.get_f64("Health"), Some(12.0));
}

#[test]
fn a_golem_carries_sixteen_apples_from_the_copper_chest_to_the_chest() {
    let mut s = ready(Vec::new());
    let golem = summon(&mut s, "{}", GOLEM_AT);
    s.players[0].link.take();
    let mut heard = Vec::new();
    let mut lids = Vec::new();
    let mut done = false;
    for _ in 0..2_400 {
        s.tick(1);
        let events = s.players[0].link.take();
        heard.extend(sounds(&events));
        lids.extend(lid_events(&events, SOURCE));
        if items_of(&s, DESTINATION).iter().any(|item| item.2 == 16) {
            done = true;
            break;
        }
    }
    assert!(done, "the golem never delivered: {heard:?}");
    assert_eq!(
        items_of(&s, SOURCE),
        vec![(3, "minecraft:apple".to_owned(), 24)]
    );
    assert_eq!(
        items_of(&s, DESTINATION),
        vec![(0, "minecraft:apple".to_owned(), 16)]
    );
    assert!(hand_of(&s, golem).is_none());
    assert_eq!(lids, vec![1, 0]);
    for sound in [
        "minecraft:block.copper_chest.open",
        "minecraft:block.copper_chest.close",
        "minecraft:entity.copper_golem.no_item_get",
    ] {
        assert!(heard.contains(&sound), "{sound} in {heard:?}");
    }
}

#[test]
fn a_golem_holding_an_item_throws_it_at_a_player_with_an_empty_hand() {
    let mut s = ready(Vec::new());
    let golem = summon(
        &mut s,
        "{equipment:{mainhand:{id:\"minecraft:apple\",count:5}}}",
        Vec3::new(1.5, 64.0, 1.5),
    );
    assert_eq!(hand_of(&s, golem), Some(("minecraft:apple".to_owned(), 5)));
    interact(&mut s, golem);
    assert!(hand_of(&s, golem).is_none());
    assert_eq!(ground_count(&s, rspigot_items::ItemKind::Apple), 5);
}

#[test]
fn shears_take_the_antenna_flower_and_wear_the_tool() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice minecraft:shears");
    select_first_slot(&mut s);
    let golem = summon(
        &mut s,
        "{NoAI:1b,equipment:{saddle:{id:\"minecraft:poppy\",count:1}}}",
        Vec3::new(1.5, 64.0, 1.5),
    );
    s.players[0].link.take();
    interact(&mut s, golem);
    let events = s.players[0].link.take();
    assert!(sounds(&events).contains(&"minecraft:entity.copper_golem.shear"));
    assert_eq!(ground_count(&s, rspigot_items::ItemKind::Poppy), 1);
    let saved = s.server.saved_entity(golem).unwrap();
    assert!(
        saved
            .get_compound("equipment")
            .is_none_or(|equipment| !equipment.contains_key("saddle"))
    );
    interact(&mut s, golem);
    assert_eq!(ground_count(&s, rspigot_items::ItemKind::Poppy), 1);
}

#[test]
fn a_honeycomb_waxes_the_golem_once_and_an_axe_scrapes_the_wax_then_the_oxidation() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice minecraft:honeycomb 2");
    select_first_slot(&mut s);
    let golem = summon(
        &mut s,
        "{NoAI:1b,weather_state:\"weathered\"}",
        Vec3::new(1.5, 64.0, 1.5),
    );
    s.players[0].link.take();
    interact(&mut s, golem);
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events), vec![3003]);
    assert!(sounds(&events).contains(&"minecraft:item.honeycomb.wax_on"));
    assert_eq!(
        s.server
            .saved_entity(golem)
            .unwrap()
            .get_i64("next_weather_age"),
        Some(-2)
    );
    interact(&mut s, golem);
    assert!(level_events(&s.players[0].link.take()).is_empty());
    console(&mut s, "clear Alice");
    console(&mut s, "give Alice minecraft:iron_axe");
    select_first_slot(&mut s);
    interact(&mut s, golem);
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events), vec![3004]);
    assert!(sounds(&events).contains(&"minecraft:item.axe.scrape"));
    let saved = s.server.saved_entity(golem).unwrap();
    assert!(saved.get_i64("next_weather_age").unwrap() > 0);
    assert_eq!(saved.get_str("weather_state"), Some("weathered"));
    interact(&mut s, golem);
    assert_eq!(level_events(&s.players[0].link.take()), vec![3005]);
    assert_eq!(
        s.server
            .saved_entity(golem)
            .unwrap()
            .get_str("weather_state"),
        Some("exposed")
    );
    interact(&mut s, golem);
    assert_eq!(level_events(&s.players[0].link.take()), vec![3005]);
    interact(&mut s, golem);
    assert!(level_events(&s.players[0].link.take()).is_empty());
}

#[test]
fn a_golem_oxidizes_one_stage_when_its_age_is_due_and_a_waxed_one_never_does() {
    let mut s = ready(Vec::new());
    let due = summon(
        &mut s,
        "{NoAI:1b,next_weather_age:1L}",
        Vec3::new(1.5, 64.0, 1.5),
    );
    let waxed = summon(
        &mut s,
        "{NoAI:1b,next_weather_age:-2L}",
        Vec3::new(2.5, 64.0, 1.5),
    );
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    assert_eq!(metadata_states(&events, due, 16), vec![1]);
    assert!(metadata_states(&events, waxed, 16).is_empty());
    let saved = s.server.saved_entity(due).unwrap();
    assert_eq!(saved.get_str("weather_state"), Some("exposed"));
    assert!(saved.get_i64("next_weather_age").unwrap() > 400_000);
    let saved = s.server.saved_entity(waxed).unwrap();
    assert_eq!(saved.get_str("weather_state"), Some("unaffected"));
    assert_eq!(saved.get_i64("next_weather_age"), Some(-2));
}

#[test]
fn an_oxidized_golem_turns_into_an_oxidized_statue_and_drops_its_antenna() {
    let mut s = ready(Vec::new());
    let at = Vec3::new(5.5, 64.0, 12.5);
    let golem = summon(
        &mut s,
        "{NoAI:1b,weather_state:\"oxidized\",next_weather_age:0L,CustomName:\"Rusty\",\
         equipment:{saddle:{id:\"minecraft:poppy\",count:1}},drop_chances:{saddle:2.0f}}",
        at,
    );
    s.players[0].link.take();
    let mut heard = Vec::new();
    for _ in 0..4_000 {
        s.tick(1);
        heard.extend(sounds(&s.players[0].link.take()));
        if !alive(&s, golem) {
            break;
        }
    }
    assert!(!alive(&s, golem), "the golem never turned to a statue");
    let pos = at.block();
    let state = s.server.block_state_at(pos).expect("a block");
    assert_eq!(state.kind(), Some(BlockKind::OxidizedCopperGolemStatue));
    assert!(state.property("copper_golem_pose").is_some());
    assert!(state.property("facing").is_some());
    let data = s.server.block_entity_nbt(pos).expect("the statue entity");
    assert_eq!(
        data.get_compound("components")
            .and_then(|components| components.get_str("minecraft:custom_name")),
        Some("Rusty")
    );
    assert!(heard.contains(&"minecraft:entity.copper_golem_become_statue"));
    assert_eq!(ground_count(&s, rspigot_items::ItemKind::Poppy), 1);
}

#[test]
fn a_golem_standing_in_a_block_never_turns_into_a_statue() {
    let mut s = ready(Vec::new());
    let golem = summon(
        &mut s,
        "{NoAI:1b,weather_state:\"oxidized\",next_weather_age:0L}",
        Vec3::new(5.5, 62.0, 5.5),
    );
    s.tick(2_000);
    assert!(alive(&s, golem));
}

#[test]
fn using_a_statue_cycles_its_pose_with_the_become_statue_sound() {
    let mut s = ready(Vec::new());
    console(&mut s, "tp Alice 12.5 64 9.5");
    s.tick(2);
    let before = s
        .server
        .block_state_at(STATUE)
        .unwrap()
        .property("copper_golem_pose")
        .unwrap()
        .to_owned();
    assert_eq!(before, "standing");
    s.players[0].link.take();
    use_on(&mut s, STATUE, Direction::South);
    let events = s.players[0].link.take();
    assert!(sounds(&events).contains(&"minecraft:entity.copper_golem_become_statue"));
    let order = ["standing", "sitting", "running", "star", "standing"];
    for pose in &order[1..] {
        let now = s
            .server
            .block_state_at(STATUE)
            .unwrap()
            .property("copper_golem_pose")
            .unwrap()
            .to_owned();
        assert_eq!(&now, pose);
        use_on(&mut s, STATUE, Direction::South);
    }
    use_on(&mut s, WAXED_STATUE, Direction::South);
    assert_eq!(
        s.server
            .block_state_at(WAXED_STATUE)
            .unwrap()
            .property("copper_golem_pose"),
        Some("sitting")
    );
    let kept = s
        .server
        .block_entity_nbt(WAXED_STATUE)
        .expect("the statue entity");
    assert!(kept.get_compound("components").is_some());
}

#[test]
fn an_axe_on_an_unaffected_statue_brings_the_golem_back_and_wears_the_axe() {
    let mut s = ready(Vec::new());
    console(&mut s, "tp Alice 12.5 64 9.5");
    console(&mut s, "give Alice minecraft:iron_axe");
    select_first_slot(&mut s);
    s.tick(2);
    assert!(s.server.entities_of(Kind::CopperGolem).is_empty());
    use_on(&mut s, STATUE, Direction::South);
    let golems = s.server.entities_of(Kind::CopperGolem);
    assert_eq!(golems.len(), 1);
    assert_eq!(golems[0].position.x, 12.5);
    assert_eq!(golems[0].position.z, 12.5);
    assert_eq!(
        s.server.block_state_at(STATUE).and_then(BlockStateId::kind),
        Some(BlockKind::Air)
    );
    let saved = s.server.saved_entity(golems[0].id).unwrap();
    assert_eq!(saved.get_str("weather_state"), Some("unaffected"));
    assert_eq!(saved.get_bool("PersistenceRequired"), Some(true));
    assert_eq!(saved.get_str("CustomName"), Some("Statuesque"));
}

#[test]
fn an_axe_does_not_reanimate_a_waxed_statue() {
    let mut s = ready(Vec::new());
    console(&mut s, "tp Alice 12.5 64 9.5");
    console(&mut s, "give Alice minecraft:iron_axe");
    select_first_slot(&mut s);
    s.tick(2);
    use_on(&mut s, WAXED_STATUE, Direction::South);
    assert!(s.server.entities_of(Kind::CopperGolem).is_empty());
    assert_ne!(
        s.server
            .block_state_at(WAXED_STATUE)
            .and_then(BlockStateId::kind),
        Some(BlockKind::Air)
    );
}

#[test]
fn a_pumpkin_on_a_copper_block_builds_a_golem_and_turns_the_block_into_a_chest() {
    let mut s = ready(Vec::new());
    console(&mut s, "tp Alice 2.5 64 9.5");
    console(&mut s, "give Alice minecraft:carved_pumpkin");
    select_first_slot(&mut s);
    s.tick(2);
    s.players[0].link.take();
    use_on(&mut s, COPPER_BASE, Direction::Up);
    let golems = s.server.entities_of(Kind::CopperGolem);
    assert_eq!(golems.len(), 1, "no golem was built");
    assert!(golems[0].position.y > 64.8 && golems[0].position.y < 65.1);
    assert_eq!(golems[0].position.x, 2.5);
    assert_eq!(golems[0].position.z, 12.5);
    let saved = s.server.saved_entity(golems[0].id).unwrap();
    assert_eq!(saved.get_str("weather_state"), Some("exposed"));
    assert_eq!(saved.get_bool("PersistenceRequired"), Some(true));
    assert_eq!(
        s.server
            .block_state_at(COPPER_BASE)
            .and_then(BlockStateId::kind),
        Some(BlockKind::ExposedCopperChest)
    );
    assert_eq!(
        s.server
            .block_state_at(COPPER_BASE.offset(0, 1, 0))
            .and_then(BlockStateId::kind),
        Some(BlockKind::Air)
    );
    let events = s.players[0].link.take();
    assert!(level_events(&events).contains(&2001));
    assert!(sounds(&events).contains(&"minecraft:entity.copper_golem.spawn"));
}

#[test]
fn a_pumpkin_on_stone_builds_nothing() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice minecraft:carved_pumpkin");
    select_first_slot(&mut s);
    s.tick(2);
    use_on(&mut s, BlockPos::new(1, 63, 1), Direction::Up);
    assert!(s.server.entities_of(Kind::CopperGolem).is_empty());
}

#[test]
fn a_golem_spawned_by_command_makes_its_spawn_sound() {
    let mut s = ready(Vec::new());
    s.players[0].link.take();
    summon(&mut s, "", Vec3::new(1.5, 64.0, 1.5));
    s.tick(2);
    let events = s.players[0].link.take();
    assert!(sounds(&events).contains(&"minecraft:entity.copper_golem.spawn"));
    summon(&mut s, "{NoAI:1b}", Vec3::new(2.5, 64.0, 1.5));
    s.tick(2);
    assert!(!sounds(&s.players[0].link.take()).contains(&"minecraft:entity.copper_golem.spawn"));
}

#[test]
fn the_golem_entity_is_announced_with_its_two_synced_states() {
    let mut s = ready(Vec::new());
    s.players[0].link.take();
    let golem = summon(
        &mut s,
        "{NoAI:1b,weather_state:\"weathered\"}",
        Vec3::new(1.5, 64.0, 1.5),
    );
    s.tick(1);
    let events = s.players[0].link.take();
    let spawned = events.iter().any(|event| {
        matches!(event, ClientEvent::SpawnEntity { entity, kind: WireKind::Other(id), .. }
            if *entity == golem && *id == Kind::CopperGolem.id().0)
    });
    assert!(spawned);
    let announced: Vec<i32> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if *entity == golem => {
                Some(entries.clone())
            }
            _ => None,
        })
        .flatten()
        .filter(|entry| entry.index == 16)
        .filter_map(|entry| match entry.value {
            MetadataValue::State(_, state) => Some(state),
            _ => None,
        })
        .collect();
    assert!(!announced.is_empty());
    assert!(announced.iter().all(|state| *state == 2));
}

#[test]
fn a_plugin_that_cancels_the_target_block_event_keeps_the_golem_from_taking_anything() {
    let mut s = ready(vec![deciding(1, EventKind::EntityTargetBlock)]);
    let golem = summon(&mut s, "{}", GOLEM_AT);
    let mut asked = None;
    for _ in 0..300 {
        s.tick(1);
        let found = asked_for(&s.plugin, EventKind::EntityTargetBlock);
        if let Some(event) = found.last() {
            asked = Some(event.id);
            break;
        }
    }
    let asked = asked.expect("the golem never targeted a chest");
    rule_on(&mut s.server, 1, asked, true, None);
    s.tick(300);
    assert_eq!(
        items_of(&s, SOURCE),
        vec![(3, "minecraft:apple".to_owned(), 40)]
    );
    assert!(hand_of(&s, golem).is_none());
}

fn lid_and_hand_ticks(
    s: &mut Scene,
    golem: EntityId,
    pos: BlockPos,
    ticks: u32,
) -> (Vec<(u32, u8)>, Option<u32>) {
    let mut lids = Vec::new();
    let mut taken = None;
    for tick in 0..ticks {
        s.tick(1);
        for data in lid_events(&s.players[0].link.take(), pos) {
            lids.push((tick, data));
        }
        if taken.is_none() && hand_of(s, golem).is_some() {
            taken = Some(tick);
        }
        if lids.len() >= 2 && taken.is_some() {
            break;
        }
    }
    (lids, taken)
}

#[test]
fn a_single_chest_closes_in_the_tick_the_golem_finishes() {
    let mut s = ready(Vec::new());
    let golem = summon(&mut s, "", GOLEM_AT);
    let (lids, taken) = lid_and_hand_ticks(&mut s, golem, SOURCE, 1_200);
    assert_eq!(
        lids.iter()
            .map(|(_, data)| *data)
            .take(2)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert_eq!(Some(lids[1].0), taken);
}

#[test]
fn a_double_chest_stays_open_until_the_opener_recheck_notices_the_golem_left() {
    let mut s = ready(Vec::new());
    console(&mut s, "tp Alice -12.5 64 6.5");
    s.tick(40);
    let golem = summon(&mut s, "", Vec3::new(-12.5, 64.0, 5.5));
    s.players[0].link.take();
    let (lids, taken) = lid_and_hand_ticks(&mut s, golem, DOUBLE_LEFT, 1_200);
    assert_eq!(
        lids.iter()
            .map(|(_, data)| *data)
            .take(2)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    let taken = taken.expect("the golem took the stack");
    assert!(lids[1].0 > taken, "{lids:?} {taken}");
    assert!(lids[1].0 <= taken + 5, "{lids:?} {taken}");
    let remaining: i32 = [DOUBLE_LEFT, DOUBLE_RIGHT]
        .iter()
        .flat_map(|pos| items_of(&s, *pos))
        .map(|item| item.2)
        .sum();
    assert_eq!(remaining, 24);
}

#[test]
fn building_and_reanimating_are_reported_with_their_creature_spawn_reasons() {
    use rspigot_core::plugins::events::CreatureSpawnReason;
    use rspigot_core::plugins::{EventKind, EventPayload};
    let mut s = ready(vec![observe(1, EventKind::CreatureSpawn)]);
    console(&mut s, "tp Alice 12.5 64 9.5");
    console(&mut s, "give Alice minecraft:iron_axe");
    select_first_slot(&mut s);
    s.tick(2);
    use_on(&mut s, STATUE, Direction::South);
    let reasons: Vec<CreatureSpawnReason> = settled_for(&s.plugin, EventKind::CreatureSpawn)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) => Some(spawn.spawn_reason),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, vec![CreatureSpawnReason::Reanimate]);
}

#[test]
fn a_plugin_can_refuse_the_golem_turning_into_a_statue() {
    let mut s = ready(vec![deciding(1, EventKind::EntityChangeBlock)]);
    let at = Vec3::new(5.5, 64.0, 12.5);
    let golem = summon(
        &mut s,
        "{NoAI:1b,weather_state:\"oxidized\",next_weather_age:0L}",
        at,
    );
    let mut asked = None;
    for _ in 0..4_000 {
        s.tick(1);
        if let Some(event) = asked_for(&s.plugin, EventKind::EntityChangeBlock).last() {
            asked = Some(event.id);
            break;
        }
    }
    let asked = asked.expect("the statue change was never asked");
    rule_on(&mut s.server, 1, asked, true, None);
    s.tick(2);
    assert!(alive(&s, golem));
    assert_eq!(
        s.server
            .block_state_at(at.block())
            .and_then(BlockStateId::kind),
        Some(BlockKind::Air)
    );
}
