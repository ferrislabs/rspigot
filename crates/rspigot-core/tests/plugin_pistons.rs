mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::Face;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::{BlockPos, EntityId, Vec3};

const PISTON: BlockPos = BlockPos::new(3, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn at(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 3)
}

fn put(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
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

fn state(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    state(s, pos).0
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    state(s, pos)
        .1
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_default()
}

fn piston(s: &mut Scene, kind: &str) {
    put(
        s,
        PISTON,
        kind,
        &[("facing", "east"), ("extended", "false")],
    );
}

fn power_on(s: &mut Scene) {
    put(s, POWER, "minecraft:redstone_block", &[]);
}

fn power_off(s: &mut Scene) {
    put(s, POWER, "minecraft:air", &[]);
}

#[test]
fn a_powered_piston_pushes_the_blocks_in_front_of_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    put(&mut s, at(5), "minecraft:dirt", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(1);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:moving_piston");
    s.tick(3);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:stone");
    assert_eq!(kind_at(&mut s, at(6)), "minecraft:dirt");
}

#[test]
fn an_unpowered_piston_pulls_its_head_back() {
    let mut s = scene(&["Alice"]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
    power_off(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:piston");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:air");
}

#[test]
fn a_sticky_piston_pulls_the_block_it_pushed() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:stone");
    power_off(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
    assert_eq!(kind_at(&mut s, at(5)), "minecraft:air");
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:sticky_piston");
}

#[test]
fn a_piston_cannot_push_more_than_twelve_blocks() {
    let mut s = scene(&["Alice"]);
    for x in 4..=16 {
        put(&mut s, at(x), "minecraft:stone", &[]);
    }
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
}

#[test]
fn twelve_blocks_move_together() {
    let mut s = scene(&["Alice"]);
    for x in 4..=15 {
        put(&mut s, at(x), "minecraft:stone", &[]);
    }
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(16)), "minecraft:stone");
}

#[test]
fn obsidian_stops_a_piston() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:obsidian", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
}

#[test]
fn a_torch_in_the_way_is_broken_by_the_piston() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:torch", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
}

#[test]
fn slime_drags_a_block_resting_on_it() {
    let mut s = scene(&["Alice"]);
    let high = |x: i32, y: i32| BlockPos::new(x, y, 3);
    put(&mut s, high(4, 70), "minecraft:slime_block", &[]);
    put(&mut s, high(4, 71), "minecraft:stone", &[]);
    put(
        &mut s,
        high(3, 70),
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(
        &mut s,
        BlockPos::new(3, 70, 2),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(4);
    assert_eq!(kind_at(&mut s, high(5, 70)), "minecraft:slime_block");
    assert_eq!(kind_at(&mut s, high(5, 71)), "minecraft:stone");
}

#[test]
fn an_extend_event_lists_the_blocks_it_moves_and_breaks() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonExtend)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    put(&mut s, at(5), "minecraft:torch", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(2);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonExtend);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonExtend(event) = &heard[0].payload else {
        panic!("an extend event");
    };
    assert_eq!(event.block, PISTON);
    assert_eq!(event.blocks, vec![at(4), at(5)]);
    assert_eq!(event.direction, Face::East);
    assert!(!event.sticky);
}

#[test]
fn a_cancelled_extension_leaves_everything_in_place() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockPistonExtend)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BlockPistonExtend);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "false");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:stone");
}

#[test]
fn a_retract_event_comes_for_a_plain_piston_with_no_blocks() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonRetract)]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(4);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonRetract);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonRetract(event) = &heard[0].payload else {
        panic!("a retract event");
    };
    assert_eq!(event.block, PISTON);
    assert!(event.blocks.is_empty());
    assert_eq!(event.direction, Face::East);
}

#[test]
fn a_sticky_retract_event_lists_the_pulled_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPistonRetract)]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(4);
    let heard = settled_for(&s.plugin, EventKind::BlockPistonRetract);
    assert_eq!(heard.len(), 1);
    let EventPayload::BlockPistonRetract(event) = &heard[0].payload else {
        panic!("a retract event");
    };
    assert_eq!(event.blocks, vec![at(5)]);
    assert!(event.sticky);
}

#[test]
fn a_cancelled_retraction_keeps_the_piston_out() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockPistonRetract)]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(4);
    power_off(&mut s);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BlockPistonRetract);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(4);
    assert_eq!(prop(&mut s, PISTON, "extended"), "true");
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:piston_head");
}

#[test]
fn a_moving_block_carries_the_player_standing_in_its_way() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].id;
    let before = match s.ask(Question::Player(alice)) {
        Answer::Player(state) => state.player.position.x,
        other => panic!("expected a player, got {other:?}"),
    };
    put(
        &mut s,
        BlockPos::new(-1, 64, 0),
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(&mut s, BlockPos::new(0, 64, 0), "minecraft:stone", &[]);
    put(
        &mut s,
        BlockPos::new(-1, 64, -1),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(5);
    let after = match s.ask(Question::Player(alice)) {
        Answer::Player(state) => state.player.position.x,
        other => panic!("expected a player, got {other:?}"),
    };
    assert!(after > before + 0.9, "{before} -> {after}");
}

fn block_events(s: &Scene) -> Vec<(BlockPos, u8, u8, u16)> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::BlockEvent {
                pos,
                action,
                data,
                block,
            } => Some((pos, action, data, block)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_extension_tells_nearby_players_with_a_block_event() {
    let mut s = scene(&["Alice"]);
    piston(&mut s, "minecraft:piston");
    s.players[0].link.take();
    power_on(&mut s);
    s.tick(4);
    assert_eq!(
        block_events(&s),
        vec![(PISTON, 0, 5, BlockKind::Piston.id().0)]
    );
}

#[test]
fn a_retraction_tells_nearby_players_with_a_block_event() {
    let mut s = scene(&["Alice"]);
    piston(&mut s, "minecraft:sticky_piston");
    power_on(&mut s);
    s.tick(4);
    s.players[0].link.take();
    power_off(&mut s);
    s.tick(4);
    assert_eq!(
        block_events(&s),
        vec![(PISTON, 1, 5, BlockKind::StickyPiston.id().0)]
    );
}

#[test]
fn a_blocked_extension_sends_no_block_event() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:obsidian", &[]);
    piston(&mut s, "minecraft:piston");
    s.players[0].link.take();
    power_on(&mut s);
    s.tick(4);
    assert!(block_events(&s).is_empty());
}

#[test]
fn moving_blocks_are_not_sent_as_block_updates() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:piston");
    s.players[0].link.take();
    power_on(&mut s);
    s.tick(1);
    let sent: Vec<_> = s.players[0].link.take();
    assert!(!sent.iter().any(|event| matches!(
        event,
        ClientEvent::BlockUpdate { state, .. } if state.kind() == Some(BlockKind::MovingPiston)
    )));
}

fn extended_piston(s: &mut Scene) {
    piston(s, "minecraft:piston");
    power_on(s);
    s.tick(4);
    assert_eq!(kind_at(s, at(4)), "minecraft:piston_head");
}

#[test]
fn losing_the_head_breaks_the_piston_and_drops_it() {
    let mut s = scene(&["Alice"]);
    extended_piston(&mut s);
    put(&mut s, at(4), "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:air");
    assert_eq!(s.server.items_on_ground(), 1);
}

#[test]
fn a_creative_player_breaking_the_head_leaves_no_piston_item() {
    let mut s = scene(&["Alice"]);
    extended_piston(&mut s);
    s.server
        .handle(Inbound::Console("gamemode creative Alice".to_owned()));
    s.server.deliver();
    let alice = s.players[0].clone();
    common::send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: at(4),
            face: Direction::Up,
            sequence: 1,
        },
    );
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:air");
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:air");
    assert_eq!(s.server.items_on_ground(), 0);
}

#[test]
fn a_head_with_no_matching_base_does_not_break_the_block_behind_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, PISTON, "minecraft:stone", &[]);
    put(
        &mut s,
        at(4),
        "minecraft:piston_head",
        &[("facing", "east"), ("type", "normal"), ("short", "false")],
    );
    put(&mut s, at(4), "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, PISTON), "minecraft:stone");
}

#[test]
fn using_a_moving_block_that_nothing_animates_removes_it() {
    let mut s = scene(&["Alice"]);
    put(
        &mut s,
        at(4),
        "minecraft:moving_piston",
        &[("facing", "east"), ("type", "normal")],
    );
    s.tick(3);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:moving_piston");
    let alice = s.players[0].clone();
    common::send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: at(4),
            face: Direction::West,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(1);
    assert_eq!(kind_at(&mut s, at(4)), "minecraft:air");
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityId {
    match s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) {
        Answer::Spawned(view) => view.id,
        other => panic!("expected the spawned {kind}, got {other:?}"),
    }
}

fn details(s: &mut Scene, id: EntityId) -> (Vec3, Vec3) {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => (details.entity.position, details.velocity),
        other => panic!("expected the entity, got {other:?}"),
    }
}

#[test]
fn an_entity_in_the_way_is_pushed_out_by_the_extending_head() {
    let mut s = scene(&["Alice"]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(4.5, 64.0, 3.5));
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, cart);
    assert!(position.x > 5.4, "{position:?}");
}

#[test]
fn an_entity_beside_the_path_is_not_moved() {
    let mut s = scene(&["Alice"]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(4.5, 64.0, 7.5));
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, cart);
    assert!((position.x - 4.5).abs() < 0.01, "{position:?}");
}

#[test]
fn an_entity_that_ignores_pistons_stays_where_it_is() {
    let mut s = scene(&["Alice"]);
    let marker = spawn(
        &mut s,
        "minecraft:area_effect_cloud",
        Vec3::new(4.5, 64.0, 3.5),
    );
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, marker);
    assert!((position.x - 4.5).abs() < 0.01, "{position:?}");
}

fn raised(x: i32) -> BlockPos {
    BlockPos::new(x, 70, 3)
}

fn raised_piston(s: &mut Scene) {
    put(
        s,
        raised(3),
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(s, BlockPos::new(3, 70, 2), "minecraft:redstone_block", &[]);
}

#[test]
fn slime_launches_the_entity_it_pushes_along_the_axis() {
    let mut s = scene(&["Alice"]);
    put(&mut s, raised(4), "minecraft:slime_block", &[]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(5.5, 70.0, 3.5));
    raised_piston(&mut s);
    s.tick(2);
    let (_, velocity) = details(&mut s, cart);
    assert!(velocity.x > 0.3, "{velocity:?}");
}

#[test]
fn honey_carries_the_entity_standing_on_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, raised(4), "minecraft:honey_block", &[]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(4.5, 71.0, 3.5));
    s.tick(2);
    raised_piston(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, cart);
    assert!(position.x > 5.3, "{position:?}");
}

#[test]
fn a_plain_block_does_not_carry_the_entity_standing_on_it() {
    let mut s = scene(&["Alice"]);
    put(&mut s, raised(4), "minecraft:stone", &[]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(4.5, 71.0, 3.5));
    s.tick(2);
    raised_piston(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, cart);
    assert!(position.x < 5.2, "{position:?}");
}

#[test]
fn an_entity_is_pushed_no_further_than_a_wall_allows() {
    let mut s = scene(&["Alice"]);
    put(&mut s, at(6), "minecraft:stone", &[]);
    let cart = spawn(&mut s, "minecraft:minecart", Vec3::new(5.4, 64.0, 3.5));
    put(&mut s, at(4), "minecraft:stone", &[]);
    piston(&mut s, "minecraft:piston");
    power_on(&mut s);
    s.tick(5);
    let (position, _) = details(&mut s, cart);
    assert!(position.x < 5.52, "{position:?}");
}
