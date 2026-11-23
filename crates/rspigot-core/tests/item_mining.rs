mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{DigAction, Direction, PlayerInput};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, InventoryOrder, ItemSpec, Order, PlayerOrder, Question,
};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_nbt::{Compound, Tag};

const STONE: BlockPos = BlockPos::new(2, 63, 2);
const EYES: BlockPos = BlockPos::new(0, 65, 0);

fn scene_of() -> Scene {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(5),
        &["Alice"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, STONE);
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(1);
    s
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

fn tool_stick(speed: f32) -> ItemSpec {
    let rule = Compound::new()
        .with("blocks", "#minecraft:mineable/pickaxe")
        .with("speed", speed)
        .with("correct_for_drops", true);
    let components = Compound::new().with(
        "minecraft:tool",
        Compound::new().with("rules", Tag::List(vec![Tag::Compound(rule)])),
    );
    let mut spec = ItemSpec::of("minecraft:stick", 1);
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

fn dig(s: &mut Scene, action: DigAction) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action,
            pos: STONE,
            face: Direction::Up,
            sequence: 9,
        },
    );
}

fn mined_after(s: &mut Scene, ticks: u32) -> bool {
    dig(s, DigAction::Start);
    s.tick(ticks);
    dig(s, DigAction::Finish);
    s.tick(1);
    s.server.block_state_at(STONE) == Some(BlockStateId::AIR)
}

fn alice_entity(s: &mut Scene) -> EntityId {
    let Answer::Player(details) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    EntityId(details.player.entity_id)
}

fn attribute(s: &mut Scene, name: &str, value: f64) {
    let entity = alice_entity(s);
    s.act(Order::Entity {
        entity,
        order: EntityOrder::SetAttribute {
            attribute: name.to_owned(),
            value,
        },
    });
}

fn flood_eyes(s: &mut Scene) {
    s.act(Order::Block {
        pos: EYES,
        order: BlockOrder::Set("minecraft:water".to_owned()),
    });
    s.tick(1);
}

#[test]
fn efficiency_speeds_up_the_server_side_mining_like_the_client() {
    let mut plain = scene_of();
    put(&mut plain, 0, ItemSpec::of("minecraft:iron_pickaxe", 1));
    assert!(!mined_after(&mut plain, 2));
    let mut fast = scene_of();
    let mut pickaxe = ItemSpec::of("minecraft:iron_pickaxe", 1);
    pickaxe.enchantments = vec![("minecraft:efficiency".to_owned(), 3)];
    put(&mut fast, 0, pickaxe);
    assert!(mined_after(&mut fast, 2));
}

#[test]
fn mining_fatigue_scales_by_point_three_to_the_amplifier_plus_one() {
    let mut s = scene_of();
    put(&mut s, 0, tool_stick(1000.0));
    s.act(to(
        &s.players[0].clone(),
        PlayerOrder::AddEffect(rspigot_core::plugins::EffectSpec {
            key: "minecraft:mining_fatigue".into(),
            duration: 600,
            amplifier: 2,
            ambient: false,
            particles: true,
            icon: true,
        }),
    ));
    assert!(mined_after(&mut s, 1));
}

#[test]
fn the_submerged_factor_is_the_attribute_and_aqua_affinity_lifts_it() {
    let mut plain = scene_of();
    flood_eyes(&mut plain);
    put(&mut plain, 0, tool_stick(30.0));
    assert!(!mined_after(&mut plain, 2));
    let mut aqua = scene_of();
    flood_eyes(&mut aqua);
    put(&mut aqua, 0, tool_stick(30.0));
    let mut helmet = ItemSpec::of("minecraft:iron_helmet", 1);
    helmet.enchantments = vec![("minecraft:aqua_affinity".to_owned(), 1)];
    put(&mut aqua, 39, helmet);
    assert!(mined_after(&mut aqua, 2));
}

#[test]
fn the_block_break_speed_attribute_multiplies_the_mining_speed() {
    let mut s = scene_of();
    put(&mut s, 0, tool_stick(1000.0));
    attribute(&mut s, "minecraft:block_break_speed", 0.01);
    dig(&mut s, DigAction::Start);
    s.tick(1);
    assert_ne!(s.server.block_state_at(STONE), Some(BlockStateId::AIR));
}

#[test]
fn the_mining_efficiency_attribute_adds_to_a_fast_tool_speed() {
    let mut slow = scene_of();
    put(&mut slow, 0, tool_stick(6.0));
    assert!(!mined_after(&mut slow, 2));
    let mut boosted = scene_of();
    put(&mut boosted, 0, tool_stick(6.0));
    attribute(&mut boosted, "minecraft:mining_efficiency", 10.0);
    assert!(mined_after(&mut boosted, 2));
}
