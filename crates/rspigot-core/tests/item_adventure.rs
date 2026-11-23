mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_nbt::{Compound, Tag};

const FLOOR: BlockPos = BlockPos::new(2, 63, 2);
const ABOVE: BlockPos = BlockPos::new(2, 64, 2);

fn scene_of() -> Scene {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(5),
        &["Alice"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, FLOOR);
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

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn adventure(s: &mut Scene) {
    console(s, "gamemode adventure Alice");
}

fn with(mut spec: ItemSpec, components: Compound) -> ItemSpec {
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

fn hold(s: &mut Scene, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(item),
        },
    });
}

fn predicate(blocks: &str) -> Tag {
    Tag::List(vec![Tag::Compound(Compound::new().with("blocks", blocks))])
}

fn fast_tool() -> Compound {
    let rule = Compound::new()
        .with("blocks", "#minecraft:mineable/pickaxe")
        .with("speed", 1000.0f32)
        .with("correct_for_drops", true);
    Compound::new().with("rules", Tag::List(vec![Tag::Compound(rule)]))
}

fn dig(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos,
            face: Direction::Up,
            sequence: 2,
        },
    );
    s.tick(2);
}

fn place(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FLOOR,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(2);
}

fn air(s: &Scene, pos: BlockPos) -> bool {
    s.server.block_state_at(pos) == Some(BlockStateId::AIR)
}

#[test]
fn an_adventure_player_cannot_break_without_a_can_break_component() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(
        &mut s,
        with(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new().with("minecraft:tool", fast_tool()),
        ),
    );
    dig(&mut s, FLOOR);
    assert!(!air(&s, FLOOR));
}

#[test]
fn an_adventure_player_breaks_the_blocks_listed_in_can_break() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(
        &mut s,
        with(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new()
                .with("minecraft:tool", fast_tool())
                .with("minecraft:can_break", predicate("minecraft:stone")),
        ),
    );
    dig(&mut s, FLOOR);
    assert!(air(&s, FLOOR));
}

#[test]
fn can_break_does_not_cover_other_blocks() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(
        &mut s,
        with(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new()
                .with("minecraft:tool", fast_tool())
                .with("minecraft:can_break", predicate("minecraft:dirt")),
        ),
    );
    dig(&mut s, FLOOR);
    assert!(!air(&s, FLOOR));
}

#[test]
fn an_adventure_player_cannot_place_without_can_place_on() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(&mut s, ItemSpec::of("minecraft:stone", 4));
    place(&mut s);
    assert!(air(&s, ABOVE));
}

#[test]
fn an_adventure_player_places_on_the_blocks_listed_in_can_place_on() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(
        &mut s,
        with(
            ItemSpec::of("minecraft:stone", 4),
            Compound::new().with("minecraft:can_place_on", predicate("minecraft:stone")),
        ),
    );
    place(&mut s);
    assert_eq!(
        s.server.block_state_at(ABOVE),
        Some(BlockKind::from_name("stone").unwrap().default_state())
    );
}

#[test]
fn can_place_on_refuses_other_supports() {
    let mut s = scene_of();
    adventure(&mut s);
    hold(
        &mut s,
        with(
            ItemSpec::of("minecraft:stone", 4),
            Compound::new().with("minecraft:can_place_on", predicate("minecraft:dirt")),
        ),
    );
    place(&mut s);
    assert!(air(&s, ABOVE));
}

#[test]
fn a_creative_player_without_operator_rights_cannot_break_a_command_block() {
    let mut s = scene_of();
    console(&mut s, "setblock 2 64 2 minecraft:command_block");
    console(&mut s, "gamemode creative Alice");
    dig(&mut s, ABOVE);
    assert!(!air(&s, ABOVE));
}

#[test]
fn an_operator_in_creative_breaks_a_command_block() {
    let mut s = scene_of();
    console(&mut s, "op Alice");
    console(&mut s, "setblock 2 64 2 minecraft:command_block");
    console(&mut s, "gamemode creative Alice");
    dig(&mut s, ABOVE);
    assert!(air(&s, ABOVE));
}

#[test]
fn a_creative_player_still_breaks_ordinary_blocks() {
    let mut s = scene_of();
    console(&mut s, "gamemode creative Alice");
    dig(&mut s, FLOOR);
    assert!(air(&s, FLOOR));
}

#[test]
fn a_can_break_state_predicate_only_matches_that_block_state() {
    let breaks = |kind: &str| {
        let mut s = scene_of();
        console(&mut s, &format!("setblock 2 64 2 {kind}"));
        adventure(&mut s);
        console(
            &mut s,
            "give Alice stick[tool={rules:[{blocks:\"#minecraft:mineable/axe\",speed:1000.0f,correct_for_drops:true}]},can_break=[{blocks:\"minecraft:oak_slab\",state:{type:\"top\"}}]]",
        );
        dig(&mut s, ABOVE);
        air(&s, ABOVE)
    };
    assert!(breaks("minecraft:oak_slab[type=top]"));
    assert!(!breaks("minecraft:oak_slab[type=bottom]"));
}
