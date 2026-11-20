use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_nbt::{Compound, Tag};

use super::{DATA_VERSION, DataKind, upgrade_checked};
use crate::chunk::from_nbt;
use crate::error::AnvilError;
use crate::level::{read_level_data, write_gzip_nbt, write_level_data};
use crate::testutil::TempDir;

const POS: ChunkPos = ChunkPos::new(0, 0);

fn single_block_section(y: i8, name: &str, properties: &[(&str, &str)]) -> Tag {
    let mut state = Compound::new().with("Name", name);
    if !properties.is_empty() {
        let mut props = Compound::new();
        for (key, value) in properties {
            props.insert(*key, *value);
        }
        state.insert("Properties", props);
    }
    Tag::Compound(
        Compound::new()
            .with("Y", y)
            .with(
                "block_states",
                Compound::new().with("palette", Tag::List(vec![Tag::Compound(state)])),
            )
            .with(
                "biomes",
                Compound::new().with(
                    "palette",
                    Tag::List(vec![Tag::String("minecraft:plains".into())]),
                ),
            )
            .with("SkyLight", Tag::ByteArray(vec![0xff; 2048])),
    )
}

fn chunk(version: i32, sections: Vec<Tag>, block_entities: Vec<Compound>) -> Compound {
    Compound::new()
        .with("DataVersion", version)
        .with("xPos", 0)
        .with("zPos", 0)
        .with("yPos", -4)
        .with("Status", "minecraft:full")
        .with("isLightOn", 1i8)
        .with("InhabitedTime", 0i64)
        .with("sections", Tag::List(sections))
        .with(
            "block_entities",
            Tag::List(block_entities.into_iter().map(Tag::Compound).collect()),
        )
}

fn load(root: Compound) -> ChunkColumn {
    from_nbt(root, POS, Some(WorldHeight::OVERWORLD))
        .unwrap()
        .unwrap()
}

fn block_at_section(chunk: &ChunkColumn, y: i32) -> rspigot_domain::BlockStateId {
    chunk.block(BlockPos::new(0, y * 16, 0)).unwrap()
}

fn entity_data<'a>(chunk: &'a ChunkColumn, id: &str) -> &'a Compound {
    &chunk
        .block_entities
        .iter()
        .find(|entity| entity.kind.to_string() == id)
        .unwrap_or_else(|| panic!("no {id} block entity"))
        .data
}

fn at(id: &str, y: i32) -> Compound {
    Compound::new()
        .with("id", id)
        .with("x", 1)
        .with("y", y)
        .with("z", 1)
}

fn chain_state() -> rspigot_domain::BlockStateId {
    BlockKind::IronChain
        .state_with(&[("axis", "y"), ("waterlogged", "false")])
        .unwrap()
}

fn sword_item_1_20() -> Compound {
    Compound::new()
        .with("Slot", 0i8)
        .with("id", "minecraft:diamond_sword")
        .with("Count", 1i8)
        .with(
            "tag",
            Compound::new()
                .with("Damage", 5)
                .with(
                    "display",
                    Compound::new().with("Name", "{\"text\":\"Blade\"}"),
                )
                .with(
                    "Enchantments",
                    Tag::List(vec![Tag::Compound(
                        Compound::new()
                            .with("id", "minecraft:sharpness")
                            .with("lvl", 5i16),
                    )]),
                ),
        )
}

#[test]
fn a_1_21_chunk_gets_renamed_blocks_nbt_text_and_no_stale_light() {
    let sign = at("minecraft:sign", 64).with(
        "front_text",
        Compound::new()
            .with(
                "messages",
                Tag::List(vec![
                    Tag::String("{\"text\":\"hi\"}".into()),
                    Tag::String("{\"text\":\"\"}".into()),
                    Tag::String("{\"text\":\"\"}".into()),
                    Tag::String("{\"text\":\"\"}".into()),
                ]),
            )
            .with("color", "black")
            .with("has_glowing_text", false),
    );
    let chest = at("minecraft:chest", 65).with(
        "Items",
        Tag::List(vec![Tag::Compound(
            Compound::new()
                .with("Slot", 0i8)
                .with("id", "minecraft:chain")
                .with("count", 2)
                .with(
                    "components",
                    Compound::new().with("minecraft:custom_name", "{\"text\":\"Wand\"}"),
                ),
        )]),
    );
    let loaded = load(chunk(
        4189,
        vec![single_block_section(
            0,
            "minecraft:chain",
            &[("axis", "y"), ("waterlogged", "false")],
        )],
        vec![sign, chest],
    ));
    assert_eq!(block_at_section(&loaded, 0), chain_state());
    assert!(loaded.is_dirty());
    let front = entity_data(&loaded, "minecraft:sign")
        .get_compound("front_text")
        .unwrap();
    assert_eq!(
        front.get_list("messages").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_str("text"),
        Some("hi")
    );
    let item = entity_data(&loaded, "minecraft:chest")
        .get_list("Items")
        .unwrap()[0]
        .as_compound()
        .unwrap();
    assert_eq!(item.get_str("id"), Some("minecraft:iron_chain"));
    assert_eq!(
        item.get_compound("components")
            .unwrap()
            .get_compound("minecraft:custom_name")
            .unwrap()
            .get_str("text"),
        Some("Wand")
    );
    assert!(
        item.get_compound("components")
            .unwrap()
            .get("minecraft:custom_name")
            .is_some()
    );
}

#[test]
fn a_1_20_chunk_gets_short_grass_components_and_double_sided_signs() {
    let chest =
        at("minecraft:chest", 65).with("Items", Tag::List(vec![Tag::Compound(sword_item_1_20())]));
    let sign = at("minecraft:sign", 64).with(
        "front_text",
        Compound::new()
            .with(
                "messages",
                Tag::List(vec![Tag::String("{\"text\":\"old\"}".into())]),
            )
            .with("color", "red"),
    );
    let furnace = at("minecraft:furnace", 66)
        .with("CookTime", 7i16)
        .with("BurnTime", 40i16);
    let loaded = load(chunk(
        3465,
        vec![single_block_section(1, "minecraft:grass", &[])],
        vec![chest, sign, furnace],
    ));
    assert_eq!(
        block_at_section(&loaded, 1),
        BlockKind::ShortGrass.default_state()
    );
    let stack = entity_data(&loaded, "minecraft:chest")
        .get_list("Items")
        .unwrap()[0]
        .as_compound()
        .unwrap();
    assert_eq!(stack.get("count"), Some(&Tag::Int(1)));
    assert!(!stack.contains_key("Count") && !stack.contains_key("tag"));
    let components = stack.get_compound("components").unwrap();
    assert_eq!(components.get("minecraft:damage"), Some(&Tag::Int(5)));
    assert_eq!(
        components
            .get_compound("minecraft:custom_name")
            .unwrap()
            .get_str("text"),
        Some("Blade")
    );
    assert_eq!(
        components
            .get_compound("minecraft:enchantments")
            .unwrap()
            .get_i32("minecraft:sharpness"),
        Some(5)
    );
    let front = entity_data(&loaded, "minecraft:sign")
        .get_compound("front_text")
        .unwrap();
    assert_eq!(front.get_str("color"), Some("red"));
    assert_eq!(
        front.get_list("messages").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_str("text"),
        Some("old")
    );
    let furnace = entity_data(&loaded, "minecraft:furnace");
    assert_eq!(furnace.get_i32("cooking_time_spent"), Some(7));
    assert_eq!(furnace.get_i32("lit_time_remaining"), Some(40));
}

#[test]
fn a_1_18_chunk_loads_with_the_same_fixes() {
    let loaded = load(chunk(
        2975,
        vec![
            single_block_section(0, "minecraft:grass", &[]),
            single_block_section(
                1,
                "minecraft:chain",
                &[("axis", "y"), ("waterlogged", "false")],
            ),
        ],
        vec![
            at("minecraft:chest", 65)
                .with("Items", Tag::List(vec![Tag::Compound(sword_item_1_20())])),
            at("minecraft:sign", 64)
                .with("Text1", "{\"text\":\"legacy\"}")
                .with("GlowingText", 1i8),
        ],
    ));
    let front = entity_data(&loaded, "minecraft:sign")
        .get_compound("front_text")
        .unwrap();
    assert_eq!(front.get_bool("has_glowing_text"), Some(true));
    assert_eq!(
        front.get_list("messages").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_str("text"),
        Some("legacy")
    );
    assert_eq!(
        block_at_section(&loaded, 0),
        BlockKind::ShortGrass.default_state()
    );
    assert_eq!(block_at_section(&loaded, 1), chain_state());
    assert!(
        entity_data(&loaded, "minecraft:chest")
            .get_list("Items")
            .unwrap()[0]
            .as_compound()
            .unwrap()
            .get_compound("components")
            .is_some()
    );
}

#[test]
fn a_1_16_chunk_is_refused_with_the_open_it_with_vanilla_message() {
    let root = Compound::new()
        .with("DataVersion", 2586)
        .with("Level", Compound::new().with("xPos", 0).with("zPos", 0));
    let error = from_nbt(root, POS, Some(WorldHeight::OVERWORLD)).unwrap_err();
    assert!(matches!(
        error,
        AnvilError::TooOldDataVersion {
            found: 2586,
            oldest: 2860
        }
    ));
    assert_eq!(
        error.to_string(),
        "world data version 2586 is older than the oldest supported version 2860; open it with a vanilla server first"
    );
}

fn legacy_level(version: i32) -> Compound {
    Compound::new().with(
        "Data",
        Compound::new()
            .with("DataVersion", version)
            .with("LevelName", "Old World")
            .with("SpawnX", 10)
            .with("SpawnY", 70)
            .with("SpawnZ", -20)
            .with("SpawnAngle", 90.0f32)
            .with("Time", 5000i64)
            .with("DayTime", 13_000i64)
            .with("raining", 1i8)
            .with("rainTime", 400)
            .with("thundering", 0i8)
            .with("Difficulty", 3i8)
            .with(
                "GameRules",
                Compound::new()
                    .with("doDaylightCycle", "false")
                    .with("keepInventory", "true")
                    .with("randomTickSpeed", "7"),
            )
            .with(
                "WorldGenSettings",
                Compound::new()
                    .with("seed", 99i64)
                    .with("generate_features", 1i8),
            ),
    )
}

fn assert_legacy_level_loads(version: i32) {
    let dir = TempDir::new("era-level");
    write_gzip_nbt(&dir.path().join("level.dat"), &legacy_level(version), None).unwrap();
    let level = read_level_data(dir.path()).unwrap().unwrap();
    assert_eq!(level.name, "Old World");
    assert_eq!(level.seed, 99);
    assert_eq!(level.spawn, Some(BlockPos::new(10, 70, -20)));
    assert_eq!(level.spawn_angle, 90.0);
    assert_eq!(level.game_time, 5000);
    assert_eq!(level.day_time, 13_000);
    assert!(!level.daylight_cycle);
    assert!(level.raining);
    assert_eq!(level.rain_time, 400);
    let rules = level.rules.as_ref().unwrap().values();
    assert!(rules.iter().any(|(name, value)| *name == "keep_inventory"
        && matches!(value, rspigot_core::config::RuleValue::Flag(true))));
    assert!(
        rules
            .iter()
            .any(|(name, value)| *name == "random_tick_speed"
                && matches!(value, rspigot_core::config::RuleValue::Number(7)))
    );
}

#[test]
fn a_1_21_level_dat_loads_and_is_upgraded_on_save() {
    assert_legacy_level_loads(4189);
    let dir = TempDir::new("era-level-save");
    write_gzip_nbt(&dir.path().join("level.dat"), &legacy_level(4189), None).unwrap();
    let level = read_level_data(dir.path()).unwrap().unwrap();
    write_level_data(dir.path(), &level).unwrap();
    let saved = crate::level::read_gzip_nbt(&dir.path().join("level.dat"))
        .unwrap()
        .unwrap();
    let data = saved.get_compound("Data").unwrap();
    assert_eq!(data.get_i32("DataVersion"), Some(DATA_VERSION));
    assert_eq!(
        data.get_compound("difficulty_settings")
            .unwrap()
            .get_str("difficulty"),
        Some("hard")
    );
    assert!(!data.contains_key("GameRules") && !data.contains_key("WorldGenSettings"));
    let reread = read_level_data(dir.path()).unwrap().unwrap();
    assert_eq!(reread.seed, 99);
    assert_eq!(reread.day_time, 13_000);
    assert!(!reread.daylight_cycle);
}

#[test]
fn a_1_20_level_dat_loads() {
    assert_legacy_level_loads(3465);
}

#[test]
fn a_1_16_level_dat_is_refused() {
    let dir = TempDir::new("era-level-old");
    write_gzip_nbt(&dir.path().join("level.dat"), &legacy_level(2586), None).unwrap();
    let error = read_level_data(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains("world data version 2586 is older than the oldest supported version 2860; open it with a vanilla server first"),
        "{error}"
    );
}

#[test]
fn a_1_21_entity_and_player_use_the_current_schema() {
    let zombie = Compound::new()
        .with("id", "minecraft:zombie")
        .with(
            "HandItems",
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:iron_sword")
                        .with("count", 1),
                ),
                Tag::Compound(Compound::new()),
            ]),
        )
        .with(
            "ArmorItems",
            Tag::List(vec![Tag::Compound(Compound::new()); 4]),
        )
        .with("CustomName", "{\"text\":\"Bob\"}")
        .with(
            "attributes",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:generic.max_health")
                    .with("base", 30.0),
            )]),
        );
    let root = Compound::new()
        .with("DataVersion", 3953)
        .with("Entities", Tag::List(vec![Tag::Compound(zombie)]))
        .with("Position", Tag::IntArray(vec![0, 0]));
    let upgraded = upgrade_checked(DataKind::Entity, root).unwrap().compound;
    let zombie = upgraded.get_list("Entities").unwrap()[0]
        .as_compound()
        .unwrap();
    assert_eq!(
        zombie
            .get_compound("equipment")
            .unwrap()
            .get_compound("mainhand")
            .unwrap()
            .get_str("id"),
        Some("minecraft:iron_sword")
    );
    assert!(!zombie.contains_key("HandItems"));
    assert_eq!(
        zombie.get_compound("CustomName").unwrap().get_str("text"),
        Some("Bob")
    );
    assert_eq!(
        zombie.get_list("attributes").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_str("id"),
        Some("minecraft:max_health")
    );

    let player = Compound::new().with("DataVersion", 3465).with(
        "Inventory",
        Tag::List(vec![
            Tag::Compound(sword_item_1_20()),
            Tag::Compound(
                Compound::new()
                    .with("Slot", 103i8)
                    .with("id", "minecraft:iron_helmet")
                    .with("Count", 1i8),
            ),
        ]),
    );
    let upgraded = upgrade_checked(DataKind::Player, player).unwrap().compound;
    assert_eq!(upgraded.get_list("Inventory").unwrap().len(), 1);
    assert_eq!(
        upgraded
            .get_compound("equipment")
            .unwrap()
            .get_compound("head")
            .unwrap()
            .get_str("id"),
        Some("minecraft:iron_helmet")
    );
    assert!(
        upgraded.get_list("Inventory").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_compound("components")
            .is_some()
    );
}

#[test]
fn an_old_entity_file_gets_id_and_properties_on_its_block_states() {
    let falling = Compound::new().with("id", "minecraft:falling_block").with(
        "BlockState",
        Compound::new()
            .with("Name", "minecraft:anvil")
            .with("Properties", Compound::new().with("facing", "east")),
    );
    let enderman = Compound::new().with("id", "minecraft:enderman").with(
        "carriedBlockState",
        Compound::new().with("Name", "minecraft:stone"),
    );
    let root = Compound::new()
        .with("DataVersion", 4189)
        .with(
            "Entities",
            Tag::List(vec![Tag::Compound(falling), Tag::Compound(enderman)]),
        )
        .with("Position", Tag::IntArray(vec![0, 0]));
    let upgraded = upgrade_checked(DataKind::Entity, root).unwrap().compound;
    let entities = upgraded.get_list("Entities").unwrap();
    let state = entities[0]
        .as_compound()
        .unwrap()
        .get_compound("BlockState")
        .unwrap();
    assert_eq!(state.get_str("id"), Some("minecraft:anvil"));
    assert_eq!(
        state.get_compound("properties").unwrap().get_str("facing"),
        Some("east")
    );
    assert!(!state.contains_key("Name") && !state.contains_key("Properties"));
    let carried = entities[1]
        .as_compound()
        .unwrap()
        .get_compound("carriedBlockState")
        .unwrap();
    assert_eq!(carried.get_str("id"), Some("minecraft:stone"));
}
