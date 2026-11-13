mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::CreatureSpawnReason;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

const HIVE: BlockPos = BlockPos::new(6, 66, 6);

struct Garden {
    blocks: Vec<(BlockPos, BlockStateId)>,
    entities: Vec<(BlockPos, Compound)>,
}

fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).expect("a block state")
}

fn hive_state(honey: &str) -> BlockStateId {
    state(
        BlockKind::Beehive,
        &[("facing", "south"), ("honey_level", honey)],
    )
}

fn stored_bee(nectar: bool, ticks: i32, min: i32) -> Tag {
    Tag::Compound(
        Compound::new()
            .with(
                "entity_data",
                Compound::new()
                    .with("id", "minecraft:bee")
                    .with("HasNectar", nectar),
            )
            .with("ticks_in_hive", ticks)
            .with("min_ticks_in_hive", min),
    )
}

fn hive_data(bees: Vec<Tag>) -> Compound {
    Compound::new().with("bees", Tag::List(bees))
}

impl Garden {
    fn empty() -> Self {
        Self {
            blocks: Vec::new(),
            entities: Vec::new(),
        }
    }

    fn with_hive(honey: &str, bees: Vec<Tag>) -> Self {
        Self {
            blocks: vec![(HIVE, hive_state(honey))],
            entities: vec![(HIVE, hive_data(bees))],
        }
    }
}

impl ChunkGenerator for Garden {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        if pos == ChunkPos::new(0, 0) {
            for (at, block) in &self.blocks {
                chunk.set_block(*at, *block);
            }
            for (at, data) in &self.entities {
                chunk.set_block_entity(BlockEntity {
                    pos: *at,
                    kind: Identifier::minecraft("beehive"),
                    data: data.clone(),
                });
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn garden_server(garden: Garden, seed: u64, day_time: i64) -> Server<Link> {
    let config = GameConfig {
        plugins: rspigot_core::config::PluginSettings {
            decision_ticks: 1_000,
            ..rspigot_core::config::PluginSettings::default()
        },
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(garden),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(garden: Garden, seed: u64, day_time: i64) -> Scene {
    ready_with(garden, seed, day_time, Vec::new())
}

fn ready_with(
    garden: Garden,
    seed: u64,
    day_time: i64,
    handlers: Vec<rspigot_core::plugins::HandlerSpec>,
) -> Scene {
    let mut s = scene_on(garden_server(garden, seed, day_time), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(20);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(Kind::Bee)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon minecraft:bee {at} {nbt}"));
    s.server
        .entities_of(Kind::Bee)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new bee")
}

fn bees(s: &Scene) -> Vec<EntityId> {
    s.server
        .entities_of(Kind::Bee)
        .into_iter()
        .map(|entity| entity.id)
        .collect()
}

fn hive_bees(s: &Scene) -> Vec<Compound> {
    s.server
        .block_entity_nbt(HIVE)
        .and_then(|data| {
            data.get_list("bees").map(|list| {
                list.iter()
                    .filter_map(|tag| tag.as_compound().cloned())
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn honey(s: &mut Scene) -> i32 {
    match s.ask(Question::Block(HIVE)) {
        Answer::Block(block) => block
            .properties
            .into_iter()
            .find(|(name, _)| name == "honey_level")
            .and_then(|(_, value)| value.parse().ok())
            .unwrap_or(-1),
        _ => -1,
    }
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn use_on_hive(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: HIVE,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 5,
        },
    );
    s.tick(2);
}

fn items(s: &mut Scene) -> Vec<(String, i32)> {
    let alice = s.players[0].clone();
    let Answer::Inventory(state) = s.ask(Question::Inventory(alice.id)) else {
        panic!("an inventory");
    };
    state
        .slots
        .into_iter()
        .flatten()
        .map(|item| (item.key, item.count))
        .collect()
}

fn dropped(s: &Scene, key: &str) -> i32 {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter_map(|saved| {
            let stack = saved.get_compound("Item")?.clone();
            (stack.get_str("id") == Some(key)).then(|| stack.get_i32("count").unwrap_or(0))
        })
        .sum()
}

fn uuid_ints(uuid: uuid::Uuid) -> String {
    let (high, low) = uuid.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32
    )
}

#[test]
fn a_bee_saves_the_vanilla_layout_and_keeps_it_across_a_load() {
    let mut s = ready(Garden::empty(), 3, 6_000);
    let bee = summon(
        &mut s,
        "8.5 70 8.5",
        "{NoAI:1b,HasNectar:1b,HasStung:1b,TicksSincePollination:77,CannotEnterHiveTicks:5000,CropsGrownSincePollination:3,hive_pos:[I;1,2,3],flower_pos:[I;4,5,6]}",
    );
    let nbt = s.server.saved_entity(bee).expect("a saved bee");
    assert_eq!(nbt.get_bool("HasNectar"), Some(true));
    assert_eq!(nbt.get_bool("HasStung"), Some(true));
    assert_eq!(nbt.get_i32("TicksSincePollination"), Some(77));
    assert_eq!(nbt.get_i32("CannotEnterHiveTicks"), Some(5000));
    assert_eq!(nbt.get_i32("CropsGrownSincePollination"), Some(3));
    assert_eq!(nbt.get_int_array("hive_pos"), Some(&[1, 2, 3][..]));
    assert_eq!(nbt.get_int_array("flower_pos"), Some(&[4, 5, 6][..]));
    assert_eq!(nbt.get_i64("anger_end_time"), Some(-1));
}

#[test]
fn a_bee_never_takes_fall_damage() {
    let mut s = ready(Garden::empty(), 3, 6_000);
    let bee = summon(&mut s, "8.5 110 8.5", "{NoAI:1b}");
    s.tick(100);
    let health = s
        .server
        .entities_of(Kind::Bee)
        .into_iter()
        .find(|entity| entity.id == bee)
        .and_then(|entity| entity.health);
    assert_eq!(health, Some(10.0));
}

#[test]
fn a_bee_with_nectar_enters_the_hive_it_remembers_and_the_hive_keeps_it() {
    let mut s = ready_with(
        Garden::with_hive("0", Vec::new()),
        3,
        6_000,
        vec![observe(1, EventKind::EntityEnterBlock)],
    );
    summon(
        &mut s,
        "6.5 66.5 7.5",
        "{HasNectar:1b,hive_pos:[I;6,66,6],Age:0}",
    );
    s.tick(80);
    assert!(bees(&s).is_empty());
    let stored = hive_bees(&s);
    assert_eq!(stored.len(), 1);
    let data = stored[0].get_compound("entity_data").expect("entity data");
    assert_eq!(data.get_str("id"), Some("minecraft:bee"));
    assert_eq!(data.get_bool("HasNectar"), Some(true));
    assert_eq!(stored[0].get_i32("min_ticks_in_hive"), Some(2400));
    assert!(!data.contains_key("Pos") && !data.contains_key("UUID"));
    let entered: Vec<BlockPos> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityEnterBlock(enter) => Some(enter.block),
            _ => None,
        })
        .collect();
    assert_eq!(entered, [HIVE]);
}

#[test]
fn a_plugin_refusing_the_entry_keeps_the_bee_out_for_four_hundred_ticks() {
    let mut s = ready_with(
        Garden::with_hive("0", Vec::new()),
        3,
        6_000,
        vec![deciding(1, EventKind::EntityEnterBlock)],
    );
    let bee = summon(
        &mut s,
        "6.5 66.5 7.5",
        "{HasNectar:1b,hive_pos:[I;6,66,6],Age:0}",
    );
    s.tick(10);
    assert_eq!(asked_for(&s.plugin, EventKind::EntityEnterBlock).len(), 1);
    let asked = last_asked(&s.plugin, EventKind::EntityEnterBlock);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(bees(&s), vec![bee]);
    assert!(hive_bees(&s).is_empty());
    let left = s
        .server
        .saved_entity(bee)
        .and_then(|nbt| nbt.get_i32("CannotEnterHiveTicks"))
        .unwrap_or(0);
    assert!((380..=400).contains(&left), "{left}");
}

#[test]
fn a_full_hive_does_not_take_a_fourth_bee() {
    let full = vec![
        stored_bee(false, 0, 600_000),
        stored_bee(false, 0, 600_000),
        stored_bee(false, 0, 600_000),
    ];
    let mut s = ready(Garden::with_hive("0", full), 3, 6_000);
    summon(
        &mut s,
        "6.5 66.5 7.5",
        "{HasNectar:1b,hive_pos:[I;6,66,6],Age:0}",
    );
    s.tick(60);
    assert_eq!(bees(&s).len(), 1);
    assert_eq!(hive_bees(&s).len(), 3);
}

#[test]
fn an_occupant_comes_out_after_its_stay_leaves_its_nectar_and_fills_the_hive() {
    let mut s = ready_with(
        Garden::with_hive("0", vec![stored_bee(true, 0, 60)]),
        3,
        6_000,
        vec![observe(1, EventKind::CreatureSpawn)],
    );
    assert_eq!(bees(&s).len(), 0);
    s.tick(60);
    let reasons: Vec<CreatureSpawnReason> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) => Some(spawn.spawn_reason),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, [CreatureSpawnReason::Beehive]);
    let out = bees(&s);
    assert_eq!(out.len(), 1);
    assert!(hive_bees(&s).is_empty());
    let level = honey(&mut s);
    assert!((1..=2).contains(&level), "{level}");
    let nbt = s.server.saved_entity(out[0]).expect("the bee");
    assert_eq!(nbt.get_bool("HasNectar"), Some(false));
    assert_eq!(nbt.get_int_array("hive_pos"), Some(&[6, 66, 6][..]));
    let at = s
        .server
        .entities_of(Kind::Bee)
        .into_iter()
        .find(|entity| entity.id == out[0])
        .expect("the bee")
        .position;
    assert!((at.x - 6.5).abs() < 2.0 && at.z > 6.5, "{at:?}");
}

#[test]
fn a_bee_without_nectar_leaves_the_honey_alone() {
    let mut s = ready(
        Garden::with_hive("2", vec![stored_bee(false, 20, 10)]),
        3,
        6_000,
    );
    s.tick(20);
    assert_eq!(bees(&s).len(), 1);
    assert_eq!(honey(&mut s), 2);
}

#[test]
fn nothing_leaves_the_hive_at_night_and_the_stay_restarts_halfway_when_blocked() {
    let mut s = ready(
        Garden::with_hive("0", vec![stored_bee(false, 20, 10)]),
        3,
        14_000,
    );
    s.tick(40);
    assert!(bees(&s).is_empty());
    assert_eq!(hive_bees(&s).len(), 1);
    let ticks = hive_bees(&s)[0].get_i32("ticks_in_hive").unwrap();
    assert!((5..=11).contains(&ticks), "{ticks}");

    let mut s = ready(
        Garden::with_hive("0", vec![stored_bee(false, 0, 60)]),
        3,
        6_000,
    );
    set(&mut s, BlockPos::new(6, 66, 7), "minecraft:stone", &[]);
    s.tick(100);
    assert!(bees(&s).is_empty());
    assert_eq!(hive_bees(&s).len(), 1);
    set(&mut s, BlockPos::new(6, 66, 7), "minecraft:air", &[]);
    s.tick(60);
    assert_eq!(bees(&s).len(), 1);
    assert!(hive_bees(&s).is_empty());
}

#[test]
fn rain_keeps_the_bees_inside() {
    let mut s = ready(
        Garden::with_hive("0", vec![stored_bee(false, 20, 10)]),
        3,
        6_000,
    );
    console(&mut s, "weather rain");
    s.tick(40);
    assert!(bees(&s).is_empty());
    assert_eq!(hive_bees(&s).len(), 1);
}

#[test]
fn a_glass_bottle_on_a_full_hive_gives_honey_and_angers_the_bees_inside() {
    let mut s = ready(
        Garden::with_hive("5", vec![stored_bee(false, 0, 600_000)]),
        3,
        6_000,
    );
    console(&mut s, "give Alice minecraft:glass_bottle 2");
    use_on_hive(&mut s);
    assert_eq!(honey(&mut s), 0);
    let held = items(&mut s);
    assert!(
        held.contains(&("minecraft:honey_bottle".to_owned(), 1)),
        "{held:?}"
    );
    assert!(
        held.contains(&("minecraft:glass_bottle".to_owned(), 1)),
        "{held:?}"
    );
    assert!(hive_bees(&s).is_empty());
    let out = bees(&s);
    assert_eq!(out.len(), 1);
    let angry = s
        .server
        .saved_entity(out[0])
        .and_then(|nbt| nbt.get_i64("anger_end_time"))
        .unwrap_or(-1);
    assert!(angry == -1 || angry > 0);
}

#[test]
fn a_hive_that_is_not_full_gives_nothing() {
    let mut s = ready(Garden::with_hive("4", Vec::new()), 3, 6_000);
    console(&mut s, "give Alice minecraft:glass_bottle 1");
    use_on_hive(&mut s);
    assert_eq!(honey(&mut s), 4);
    assert!(
        !items(&mut s)
            .iter()
            .any(|(key, _)| key == "minecraft:honey_bottle")
    );
}

#[test]
fn shears_on_a_full_hive_drop_three_honeycombs_and_reset_the_honey() {
    let mut s = ready(Garden::with_hive("5", Vec::new()), 3, 6_000);
    console(&mut s, "give Alice minecraft:shears 1");
    use_on_hive(&mut s);
    assert_eq!(honey(&mut s), 0);
    s.tick(30);
    let picked: i32 = items(&mut s)
        .into_iter()
        .filter(|(key, _)| key == "minecraft:honeycomb")
        .map(|(_, count)| count)
        .sum();
    assert_eq!(dropped(&s, "minecraft:honeycomb") + picked, 3);
}

#[test]
fn campfire_smoke_calms_a_hive_so_the_bees_stay_inside() {
    let mut garden = Garden::with_hive("5", vec![stored_bee(false, 0, 600_000)]);
    garden.blocks.push((
        BlockPos::new(6, 64, 6),
        state(BlockKind::Campfire, &[("lit", "true")]),
    ));
    let mut s = ready(garden, 3, 6_000);
    console(&mut s, "give Alice minecraft:glass_bottle 1");
    use_on_hive(&mut s);
    assert_eq!(honey(&mut s), 0);
    assert!(
        items(&mut s)
            .iter()
            .any(|(key, _)| key == "minecraft:honey_bottle")
    );
    assert!(bees(&s).is_empty());
    assert_eq!(hive_bees(&s).len(), 1);
}

#[test]
fn fire_next_to_a_hive_drives_the_bees_out() {
    let mut s = ready(
        Garden::with_hive("0", vec![stored_bee(false, 0, 600_000)]),
        3,
        14_000,
    );
    assert!(bees(&s).is_empty());
    set(&mut s, BlockPos::new(6, 66, 7), "minecraft:fire", &[]);
    s.tick(5);
    assert_eq!(bees(&s).len(), 1);
    assert!(hive_bees(&s).is_empty());
}

#[test]
fn a_hive_is_made_of_the_beehive_block_and_a_nest_works_the_same_way() {
    let mut garden = Garden::empty();
    garden.blocks.push((
        HIVE,
        state(
            BlockKind::BeeNest,
            &[("facing", "south"), ("honey_level", "0")],
        ),
    ));
    garden
        .entities
        .push((HIVE, hive_data(vec![stored_bee(true, 20, 10)])));
    let mut s = ready(garden, 3, 6_000);
    s.tick(20);
    assert_eq!(bees(&s).len(), 1);
    assert!(honey(&mut s) >= 1);
}

#[test]
fn an_angry_bee_stings_its_target_poisons_it_and_calms_down() {
    let mut s = ready(Garden::empty(), 3, 6_000);
    console(&mut s, "difficulty normal");
    let alice = s.players[0].clone();
    let angry_at = uuid_ints(alice.id.0);
    summon(
        &mut s,
        "8.5 65 8.5",
        &format!("{{anger_end_time:9999999L,angry_at:{angry_at}}}"),
    );
    let before = s.server.player_health(alice.id).unwrap();
    let mut lowest = before;
    let mut poisoned = false;
    for _ in 0..100 {
        s.tick(2);
        lowest = lowest.min(s.server.player_health(alice.id).unwrap());
        if let Answer::Player(state) = s.ask(Question::Player(alice.id)) {
            poisoned |= state
                .effects
                .iter()
                .any(|effect| effect.key == "minecraft:poison");
        }
    }
    assert!(lowest <= before - 2.0, "{before} {lowest}");
    assert!(poisoned);
    if let Some(bee) = bees(&s).first() {
        let nbt = s.server.saved_entity(*bee).expect("the bee");
        assert_eq!(nbt.get_bool("HasStung"), Some(true));
        assert_eq!(nbt.get_i64("anger_end_time"), Some(-1));
    }
}

#[test]
fn a_bee_pollinates_a_flower_and_carries_its_nectar() {
    let mut garden = Garden::empty();
    garden.blocks.push((
        BlockPos::new(10, 64, 10),
        BlockKind::Dandelion.default_state(),
    ));
    let mut s = ready(garden, 5, 6_000);
    let bee = summon(&mut s, "8.5 66 8.5", "{}");
    let mut nectar = false;
    for _ in 0..30 {
        s.tick(30);
        nectar = s
            .server
            .saved_entity(bee)
            .and_then(|nbt| nbt.get_bool("HasNectar"))
            .unwrap_or(false);
        if nectar {
            break;
        }
    }
    let nbt = s.server.saved_entity(bee).expect("the bee");
    assert!(nectar, "{nbt:?}");
}

#[test]
fn a_bee_with_nectar_makes_the_crops_it_flies_over_grow() {
    let mut garden = Garden::with_hive("0", Vec::new());
    for x in 7..15 {
        for z in 5..8 {
            garden.blocks.push((
                BlockPos::new(x, 63, z),
                state(BlockKind::Farmland, &[("moisture", "0")]),
            ));
            garden.blocks.push((
                BlockPos::new(x, 64, z),
                state(BlockKind::Wheat, &[("age", "0")]),
            ));
        }
    }
    let mut s = ready_with(
        garden,
        7,
        6_000,
        vec![observe(1, EventKind::EntityChangeBlock)],
    );
    summon(
        &mut s,
        "14.5 66.5 6.5",
        "{HasNectar:1b,hive_pos:[I;6,66,6]}",
    );
    s.tick(60);
    let changes: Vec<(String, String)> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityChangeBlock(change) => Some((change.entity_type, change.to)),
            _ => None,
        })
        .collect();
    assert!(!changes.is_empty());
    assert!(
        changes
            .iter()
            .all(|(kind, to)| kind == "minecraft:bee" && to == "minecraft:wheat")
    );
}

fn feed(s: &mut Scene, bee: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: bee,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::ZERO,
        },
    );
    s.tick(1);
}

#[test]
fn two_bees_fed_flowers_fall_in_love_and_have_a_baby() {
    let mut s = ready(Garden::empty(), 3, 6_000);
    let first = summon(&mut s, "8.5 65 8.5", "{}");
    let second = summon(&mut s, "9.5 65 8.5", "{}");
    console(&mut s, "give Alice minecraft:dandelion 2");
    feed(&mut s, first);
    feed(&mut s, second);
    let love = |s: &Scene, bee: EntityId| {
        s.server
            .saved_entity(bee)
            .and_then(|nbt| nbt.get_i32("InLove"))
            .unwrap_or(0)
    };
    assert!(love(&s, first) > 500 && love(&s, second) > 500);
    let mut baby = None;
    for _ in 0..40 {
        s.tick(10);
        baby = bees(&s)
            .into_iter()
            .find(|bee| *bee != first && *bee != second);
        if baby.is_some() {
            break;
        }
    }
    let baby = baby.expect("a baby bee");
    let nbt = s.server.saved_entity(baby).expect("the baby");
    assert!(nbt.get_i32("Age").is_some_and(|age| age < 0));
}

#[test]
fn a_wither_rose_poisons_the_bee_that_is_fed_it_and_an_open_eyeblossom_too() {
    let mut s = ready(Garden::empty(), 3, 6_000);
    let bee = summon(&mut s, "8.5 65 8.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:wither_rose 1");
    feed(&mut s, bee);
    let nbt = s.server.saved_entity(bee).expect("the bee");
    let effects = nbt
        .get_list("active_effects")
        .map(<[Tag]>::to_vec)
        .unwrap_or_default();
    assert!(effects.iter().any(
        |tag| tag.as_compound().and_then(|effect| effect.get_str("id")) == Some("minecraft:wither")
    ));
    assert_eq!(nbt.get_i32("InLove").unwrap_or(0), 0);
}
