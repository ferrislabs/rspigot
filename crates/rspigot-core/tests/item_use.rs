mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{
    Answer, BlockOrder, InventoryOrder, ItemSpec, Order, Question, TileEdit,
};
use rspigot_domain::{BlockPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

const WATER_AHEAD: BlockPos = BlockPos::new(0, 65, 2);
const FLOOR: BlockPos = BlockPos::new(2, 63, 2);
const ABOVE_FLOOR: BlockPos = BlockPos::new(2, 64, 2);

fn scene_of() -> Scene {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(9),
        &["Alice"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, FLOOR);
    s.tick(2);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn put(s: &mut Scene, slot: i32, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn slot(s: &mut Scene, slot: usize) -> Option<ItemSpec> {
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    inventory.slots[slot].clone()
}

fn use_item(s: &mut Scene, rotation: Rotation) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation,
        },
    );
    s.tick(2);
}

fn use_on(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 2,
        },
    );
    s.tick(2);
}

fn block_name(s: &Scene, pos: BlockPos) -> String {
    s.server
        .block_state_at(pos)
        .and_then(|state| state.kind())
        .map(|kind| kind.name().to_owned())
        .unwrap_or_default()
}

fn potion_of(spec: &ItemSpec) -> Option<String> {
    let mut bytes = spec.components.as_slice();
    let Ok(Some(Tag::Compound(components))) = rspigot_nbt::read_network(&mut bytes) else {
        return None;
    };
    components
        .get_compound("minecraft:potion_contents")
        .and_then(|contents| contents.get_str("potion"))
        .map(str::to_owned)
}

fn look_at_floor() -> Rotation {
    Rotation::new(-45.0, 31.0)
}

#[test]
fn a_glass_bottle_used_on_water_becomes_a_water_bottle() {
    let mut s = scene_of();
    console(&mut s, "setblock 0 65 2 minecraft:water");
    put(&mut s, 0, ItemSpec::of("minecraft:glass_bottle", 2));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    let first = slot(&mut s, 0).unwrap();
    assert_eq!(first.key, "minecraft:glass_bottle");
    assert_eq!(first.count, 1);
    let filled = (1..36)
        .filter_map(|index| slot(&mut s, index))
        .find(|spec| spec.key == "minecraft:potion")
        .expect("a potion in the inventory");
    assert_eq!(potion_of(&filled).as_deref(), Some("minecraft:water"));
    assert_ne!(block_name(&s, WATER_AHEAD), "minecraft:air");
}

#[test]
fn a_single_glass_bottle_is_replaced_by_the_water_bottle_in_hand() {
    let mut s = scene_of();
    console(&mut s, "setblock 0 65 2 minecraft:water");
    put(&mut s, 0, ItemSpec::of("minecraft:glass_bottle", 1));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    let held = slot(&mut s, 0).unwrap();
    assert_eq!(held.key, "minecraft:potion");
    assert_eq!(potion_of(&held).as_deref(), Some("minecraft:water"));
}

#[test]
fn a_glass_bottle_used_on_dry_ground_does_nothing() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:glass_bottle", 1));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:glass_bottle");
}

fn uuid_tag(uuid: uuid::Uuid) -> String {
    let bits = uuid.as_u128();
    let parts = [
        (bits >> 96) as u32 as i32,
        (bits >> 64) as u32 as i32,
        (bits >> 32) as u32 as i32,
        bits as u32 as i32,
    ];
    format!("[I;{},{},{},{}]", parts[0], parts[1], parts[2], parts[3])
}

#[test]
fn a_glass_bottle_next_to_a_dragon_breath_cloud_collects_dragon_breath() {
    let mut s = scene_of();
    console(&mut s, "summon ender_dragon 40.5 80 40.5 {NoAI:1b}");
    let dragon = s.server.entities_of(Kind::EnderDragon)[0].uuid;
    console(
        &mut s,
        &format!(
            "summon area_effect_cloud 0.5 64 0.5 {{Owner:{},Radius:3.0f,Duration:600}}",
            uuid_tag(dragon)
        ),
    );
    put(&mut s, 0, ItemSpec::of("minecraft:glass_bottle", 1));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:dragon_breath");
}

#[test]
fn a_lily_pad_is_placed_on_the_water_surface_by_use_item() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 63 2 minecraft:water");
    put(&mut s, 0, ItemSpec::of("minecraft:lily_pad", 3));
    use_item(&mut s, look_at_floor());
    assert_eq!(block_name(&s, ABOVE_FLOOR), "minecraft:lily_pad");
    assert_eq!(slot(&mut s, 0).unwrap().count, 2);
}

#[test]
fn frogspawn_is_placed_on_the_water_surface_by_use_item() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 63 2 minecraft:water");
    put(&mut s, 0, ItemSpec::of("minecraft:frogspawn", 1));
    use_item(&mut s, look_at_floor());
    assert_eq!(block_name(&s, ABOVE_FLOOR), "minecraft:frogspawn");
}

#[test]
fn honeycomb_waxes_a_copper_block() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:copper_block");
    put(&mut s, 0, ItemSpec::of("minecraft:honeycomb", 2));
    use_on(&mut s, ABOVE_FLOOR);
    assert_eq!(block_name(&s, ABOVE_FLOOR), "minecraft:waxed_copper_block");
    assert_eq!(slot(&mut s, 0).unwrap().count, 1);
}

#[test]
fn honeycomb_waxes_a_weathered_copper_block_keeping_its_stage() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:exposed_copper");
    put(&mut s, 0, ItemSpec::of("minecraft:honeycomb", 1));
    use_on(&mut s, ABOVE_FLOOR);
    assert_eq!(
        block_name(&s, ABOVE_FLOOR),
        "minecraft:waxed_exposed_copper"
    );
}

fn sign_scene() -> Scene {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:oak_sign[rotation=8]");
    for front in [true, false] {
        s.act(Order::Block {
            pos: ABOVE_FLOOR,
            order: BlockOrder::Tile(TileEdit::SignLines {
                front,
                lines: vec!["hi".to_owned(), String::new(), String::new(), String::new()],
            }),
        });
    }
    s
}

fn sign_front(s: &Scene) -> Compound {
    s.server
        .block_entity_nbt(ABOVE_FLOOR)
        .and_then(|data| data.get_compound("front_text").cloned())
        .unwrap_or_default()
}

fn sign_data(s: &Scene) -> Compound {
    s.server.block_entity_nbt(ABOVE_FLOOR).unwrap_or_default()
}

fn use_on_sign_from_front(s: &mut Scene) {
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        rspigot_core::plugins::PlayerOrder::Teleport(Vec3::new(2.5, 64.0, 4.5)),
    ));
    s.tick(2);
    use_on(s, ABOVE_FLOOR);
}

#[test]
fn a_dye_colours_the_sign_text() {
    let mut s = sign_scene();
    put(&mut s, 0, ItemSpec::of("minecraft:red_dye", 2));
    use_on_sign_from_front(&mut s);
    let front = sign_front(&s);
    let back = sign_data(&s)
        .get_compound("back_text")
        .cloned()
        .unwrap_or_default();
    let coloured = [front.get_str("color"), back.get_str("color")];
    assert!(coloured.contains(&Some("red")), "{coloured:?}");
    assert_eq!(slot(&mut s, 0).unwrap().count, 1);
}

#[test]
fn a_glow_ink_sac_makes_the_text_glow_and_an_ink_sac_removes_it() {
    let mut s = sign_scene();
    put(&mut s, 0, ItemSpec::of("minecraft:glow_ink_sac", 1));
    use_on_sign_from_front(&mut s);
    let data = sign_data(&s);
    let glowing = |key: &str| {
        data.get_compound(key)
            .and_then(|text| text.get_bool("has_glowing_text"))
            .unwrap_or(false)
    };
    assert!(glowing("front_text") || glowing("back_text"));
    put(&mut s, 0, ItemSpec::of("minecraft:ink_sac", 1));
    use_on(&mut s, ABOVE_FLOOR);
    let data = sign_data(&s);
    let glowing = |key: &str| {
        data.get_compound(key)
            .and_then(|text| text.get_bool("has_glowing_text"))
            .unwrap_or(false)
    };
    assert!(!glowing("front_text") && !glowing("back_text"));
}

#[test]
fn honeycomb_waxes_a_sign() {
    let mut s = sign_scene();
    put(&mut s, 0, ItemSpec::of("minecraft:honeycomb", 1));
    use_on_sign_from_front(&mut s);
    assert_eq!(sign_data(&s).get_bool("is_waxed"), Some(true));
}

#[test]
fn a_wind_charge_used_by_the_player_launches_a_charge_and_starts_the_cooldown() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    put(&mut s, 0, ItemSpec::of("minecraft:wind_charge", 4));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert_eq!(s.server.entities_of(Kind::WindCharge).len(), 1);
    assert_eq!(slot(&mut s, 0).unwrap().count, 3);
    assert!(
        s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::WindCharge.id())
    );
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert_eq!(s.server.entities_of(Kind::WindCharge).len(), 1);
}

#[test]
fn a_goat_horn_plays_its_sound_and_starts_a_seven_second_cooldown() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    put(&mut s, 0, ItemSpec::of("minecraft:goat_horn", 1));
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert!(
        s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::GoatHorn.id())
    );
    s.tick(137);
    assert!(
        s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::GoatHorn.id())
    );
    s.tick(5);
    assert!(
        !s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::GoatHorn.id())
    );
}

#[test]
fn a_thrown_blue_egg_hatches_a_cold_chicken() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:blue_egg", 16));
    for _ in 0..24 {
        use_item(&mut s, Rotation::new(0.0, -80.0));
        s.tick(30);
    }
    let chickens = s.server.entities_of(Kind::Chicken);
    assert!(!chickens.is_empty());
    let cold = chickens.iter().all(|chicken| {
        s.server
            .saved_entity(chicken.id)
            .and_then(|data| data.get_str("variant").map(str::to_owned))
            .is_some_and(|name| name == "minecraft:cold")
    });
    assert!(cold);
}

fn components_of(mut spec: ItemSpec, components: Compound) -> ItemSpec {
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

#[test]
fn a_water_bottle_turns_dirt_into_mud_and_returns_a_glass_bottle() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:dirt");
    put(
        &mut s,
        0,
        components_of(
            ItemSpec::of("minecraft:potion", 1),
            Compound::new().with(
                "minecraft:potion_contents",
                Compound::new().with("potion", "minecraft:water"),
            ),
        ),
    );
    use_on(&mut s, ABOVE_FLOOR);
    assert_eq!(block_name(&s, ABOVE_FLOOR), "minecraft:mud");
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:glass_bottle");
}

#[test]
fn shears_snip_a_kelp_head_to_its_maximum_age() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:kelp[age=3]");
    put(&mut s, 0, ItemSpec::of("minecraft:shears", 1));
    use_on(&mut s, ABOVE_FLOOR);
    let age = s
        .server
        .block_state_at(ABOVE_FLOOR)
        .and_then(|state| state.property("age").map(str::to_owned));
    assert_eq!(age.as_deref(), Some("25"));
}

#[test]
fn a_knowledge_book_unlocks_its_recipes_and_is_consumed() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    put(
        &mut s,
        0,
        components_of(
            ItemSpec::of("minecraft:knowledge_book", 1),
            Compound::new().with(
                "minecraft:recipes",
                Tag::List(vec![Tag::String("minecraft:crafting_table".to_owned())]),
            ),
        ),
    );
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert!(slot(&mut s, 0).is_none());
    assert!(
        s.server
            .known_recipes(alice)
            .contains(&"minecraft:crafting_table")
    );
}

#[test]
fn a_spawn_egg_used_on_water_spawns_the_mob_on_the_surface() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 63 2 minecraft:water");
    put(&mut s, 0, ItemSpec::of("minecraft:cod_spawn_egg", 1));
    use_item(&mut s, look_at_floor());
    assert_eq!(s.server.entities_of(Kind::Cod).len(), 1);
    assert!(slot(&mut s, 0).is_none());
}

fn using_flags(s: &Scene) -> Vec<i8> {
    s.players[0]
        .link
        .take_raw()
        .into_iter()
        .filter_map(|event| match event {
            rspigot_core::events::ClientEvent::EntityMetadata { entries, .. } => Some(entries),
            _ => None,
        })
        .flatten()
        .filter_map(|entry| match entry.value {
            rspigot_core::events::MetadataValue::Byte(flags) => Some(flags),
            _ => None,
        })
        .collect()
}

#[test]
fn a_goat_horn_shows_the_use_animation_for_its_duration() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:goat_horn", 1));
    let _ = using_flags(&s);
    use_item(&mut s, Rotation::new(0.0, 0.0));
    assert!(using_flags(&s).iter().any(|flags| flags & 1 == 1));
    s.tick(140);
    assert!(using_flags(&s).iter().any(|flags| flags & 1 == 0));
}
