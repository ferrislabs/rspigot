mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::CauldronLevelChangeReason as Reason;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, HandlerSpec, InventoryOrder, ItemSpec, Order,
    Question, WorldOrder,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

const CAULDRON: BlockPos = BlockPos::new(2, 64, 2);

struct Ground(&'static str);

impl ChunkGenerator for Ground {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(self.0).unwrap();
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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn wet_server(biome: &'static str, speed: i32) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        rules: rspigot_core::config::GameRules {
            random_tick_speed: speed,
            ..Default::default()
        },
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Ground(biome)),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, key: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: key.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn block(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn level_of(s: &mut Scene, pos: BlockPos) -> (String, Option<String>) {
    let (kind, properties) = block(s, pos);
    let level = properties
        .into_iter()
        .find(|(name, _)| name == "level")
        .map(|(_, value)| value);
    (kind, level)
}

fn held(s: &mut Scene, player: &Player) -> Option<ItemSpec> {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory.slots.into_iter().next().flatten(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn give(s: &mut Scene, player: &Player, item: ItemSpec) {
    s.act(Order::Inventory {
        player: player.id,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(item),
        },
    });
}

fn with_components(key: &str, components: Compound) -> ItemSpec {
    let mut spec = ItemSpec::of(key, 1);
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

fn use_on(s: &mut Scene, player: &Player, pos: BlockPos) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(1);
}

fn watching() -> Scene {
    scene_with(&["Alice"], vec![observe(1, EventKind::CauldronLevelChange)])
}

fn change_of(s: &Scene) -> rspigot_core::plugins::events::CauldronLevelChange {
    let heard = settled_for(&s.plugin, EventKind::CauldronLevelChange);
    match &heard.last().expect("a cauldron event").payload {
        EventPayload::CauldronLevelChange(event) => event.clone(),
        other => panic!("expected a cauldron event, got {other:?}"),
    }
}

fn water_potion() -> ItemSpec {
    with_components(
        "minecraft:potion",
        Compound::new().with(
            "minecraft:potion_contents",
            Compound::new().with("potion", "minecraft:water"),
        ),
    )
}

#[test]
fn a_water_bucket_fills_an_empty_cauldron_and_fires_a_level_change() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(&mut s, CAULDRON, "minecraft:cauldron", &[]);
    give(&mut s, &alice, ItemSpec::of("minecraft:water_bucket", 1));
    use_on(&mut s, &alice, CAULDRON);
    let event = change_of(&s);
    assert_eq!(event.block, CAULDRON);
    assert_eq!(event.reason, Reason::BucketEmpty);
    assert_eq!(event.entity.as_ref().unwrap().kind, "minecraft:player");
    assert_eq!(event.new_state.state.kind, "minecraft:water_cauldron");
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("3".to_owned()))
    );
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:bucket");
}

#[test]
fn a_cancelled_level_change_keeps_the_cauldron_and_the_bucket() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::CauldronLevelChange)],
    );
    let alice = s.players[0].clone();
    set(&mut s, CAULDRON, "minecraft:cauldron", &[]);
    give(&mut s, &alice, ItemSpec::of("minecraft:water_bucket", 1));
    use_on(&mut s, &alice, CAULDRON);
    let asked = last_asked(&s.plugin, EventKind::CauldronLevelChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(block(&mut s, CAULDRON).0, "minecraft:cauldron");
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:water_bucket");
}

#[test]
fn a_lava_bucket_makes_a_lava_cauldron() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(&mut s, CAULDRON, "minecraft:cauldron", &[]);
    give(&mut s, &alice, ItemSpec::of("minecraft:lava_bucket", 1));
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BucketEmpty);
    assert_eq!(block(&mut s, CAULDRON).0, "minecraft:lava_cauldron");
}

#[test]
fn an_empty_bucket_drains_only_a_full_cauldron() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "2")],
    );
    give(&mut s, &alice, ItemSpec::of("minecraft:bucket", 1));
    use_on(&mut s, &alice, CAULDRON);
    assert!(settled_for(&s.plugin, EventKind::CauldronLevelChange).is_empty());
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "3")],
    );
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BucketFill);
    assert_eq!(block(&mut s, CAULDRON).0, "minecraft:cauldron");
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:water_bucket");
}

#[test]
fn a_glass_bottle_takes_a_level_and_a_water_bottle_gives_it_back() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "2")],
    );
    give(&mut s, &alice, ItemSpec::of("minecraft:glass_bottle", 1));
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BottleFill);
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("1".to_owned()))
    );
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:potion");
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BottleEmpty);
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("2".to_owned()))
    );
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:glass_bottle");
}

#[test]
fn a_water_bottle_fills_an_empty_cauldron_to_one_level() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(&mut s, CAULDRON, "minecraft:cauldron", &[]);
    give(&mut s, &alice, water_potion());
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BottleEmpty);
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("1".to_owned()))
    );
}

#[test]
fn washing_takes_the_dye_off_armor_and_a_layer_off_a_banner() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "3")],
    );
    give(
        &mut s,
        &alice,
        with_components(
            "minecraft:leather_boots",
            Compound::new().with("minecraft:dyed_color", 0xB0_2E_26_i32),
        ),
    );
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::ArmorWash);
    assert!(held(&mut s, &alice).unwrap().components.is_empty());
    let layer = Compound::new()
        .with("pattern", "minecraft:stripe_bottom")
        .with("color", "red");
    give(
        &mut s,
        &alice,
        with_components(
            "minecraft:white_banner",
            Compound::new().with(
                "minecraft:banner_patterns",
                Tag::List(vec![Tag::Compound(layer)]),
            ),
        ),
    );
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::BannerWash);
    assert!(held(&mut s, &alice).unwrap().components.is_empty());
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("1".to_owned()))
    );
}

#[test]
fn washing_a_shulker_box_turns_it_white_and_the_last_level_empties_the_cauldron() {
    let mut s = watching();
    let alice = s.players[0].clone();
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "1")],
    );
    give(&mut s, &alice, ItemSpec::of("minecraft:red_shulker_box", 1));
    use_on(&mut s, &alice, CAULDRON);
    assert_eq!(change_of(&s).reason, Reason::ShulkerWash);
    assert_eq!(held(&mut s, &alice).unwrap().key, "minecraft:shulker_box");
    assert_eq!(block(&mut s, CAULDRON).0, "minecraft:cauldron");
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let before: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .iter()
        .map(|entity| entity.id)
        .find(|id| !before.contains(id))
        .expect("a new entity")
}

#[test]
fn a_burning_mob_in_a_water_cauldron_puts_itself_out_and_lowers_the_level() {
    let mut s = watching();
    set(
        &mut s,
        CAULDRON,
        "minecraft:water_cauldron",
        &[("level", "3")],
    );
    let cow = summon(&mut s, Kind::Cow, "2.5 64.3 2.5", "{Fire:400s}");
    for _ in 0..40 {
        s.tick(1);
        if !settled_for(&s.plugin, EventKind::CauldronLevelChange).is_empty() {
            break;
        }
    }
    let event = change_of(&s);
    assert_eq!(event.reason, Reason::Extinguish);
    assert_eq!(event.entity.as_ref().unwrap().id, cow);
    assert_eq!(
        level_of(&mut s, CAULDRON),
        ("minecraft:water_cauldron".to_owned(), Some("2".to_owned()))
    );
    assert!(
        s.server
            .entity_fire_ticks(cow)
            .is_some_and(|ticks| ticks <= 0)
    );
}

fn flood_with(s: &mut Scene, key: &str, properties: &[(&str, &str)]) {
    for x in 0..16 {
        for z in 0..16 {
            set(s, BlockPos::new(x, 64, z), key, properties);
        }
    }
}

fn rain(s: &mut Scene) {
    s.act(Order::World(WorldOrder::SetWeather(
        rspigot_core::commands::Weather::Rain,
    )));
}

fn raining(biome: &'static str, handlers: Vec<HandlerSpec>) -> Scene {
    scene_on(wet_server(biome, 100), &["Alice"], handlers)
}

#[test]
fn rain_tops_up_a_water_cauldron_with_a_natural_fill() {
    let mut s = raining(
        "minecraft:plains",
        vec![observe(1, EventKind::CauldronLevelChange)],
    );
    flood_with(&mut s, "minecraft:water_cauldron", &[("level", "1")]);
    rain(&mut s);
    for _ in 0..600 {
        s.tick(1);
        if !settled_for(&s.plugin, EventKind::CauldronLevelChange).is_empty() {
            break;
        }
    }
    let event = change_of(&s);
    assert_eq!(event.reason, Reason::NaturalFill);
    assert!(event.entity.is_none());
    let (kind, level) = level_of(&mut s, event.block);
    assert_eq!(kind, "minecraft:water_cauldron");
    assert_eq!(level.as_deref(), Some("2"));
}

#[test]
fn a_cancelled_natural_fill_leaves_the_level_alone() {
    let mut s = raining(
        "minecraft:plains",
        vec![deciding(1, EventKind::CauldronLevelChange)],
    );
    flood_with(&mut s, "minecraft:water_cauldron", &[("level", "1")]);
    rain(&mut s);
    for _ in 0..600 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::CauldronLevelChange).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::CauldronLevelChange);
    let EventPayload::CauldronLevelChange(event) = &asked.payload else {
        panic!("a cauldron event");
    };
    let at = event.block;
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        level_of(&mut s, at),
        ("minecraft:water_cauldron".to_owned(), Some("1".to_owned()))
    );
}

#[test]
fn rain_fills_an_empty_cauldron_without_an_event() {
    let mut s = raining(
        "minecraft:plains",
        vec![observe(1, EventKind::CauldronLevelChange)],
    );
    flood_with(&mut s, "minecraft:cauldron", &[]);
    rain(&mut s);
    for _ in 0..600 {
        s.tick(1);
        for x in 0..16 {
            for z in 0..16 {
                if block(&mut s, BlockPos::new(x, 64, z)).0 == "minecraft:water_cauldron" {
                    assert!(settled_for(&s.plugin, EventKind::CauldronLevelChange).is_empty());
                    return;
                }
            }
        }
    }
    panic!("the rain never filled a cauldron");
}

#[test]
fn snow_fills_a_powder_snow_cauldron_with_a_natural_fill() {
    let mut s = raining(
        "minecraft:snowy_plains",
        vec![observe(1, EventKind::CauldronLevelChange)],
    );
    flood_with(&mut s, "minecraft:powder_snow_cauldron", &[("level", "1")]);
    rain(&mut s);
    for _ in 0..600 {
        s.tick(1);
        if !settled_for(&s.plugin, EventKind::CauldronLevelChange).is_empty() {
            break;
        }
    }
    let event = change_of(&s);
    assert_eq!(event.reason, Reason::NaturalFill);
    assert_eq!(event.new_state.state.kind, "minecraft:powder_snow_cauldron");
}
