mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::events::{BlockRedstonePatch, CrafterCraftPatch, Instrument};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, EventKind, EventPatch, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, Vec3};
use screen::use_block;
use seeded::{Seed, world};

const AT: BlockPos = BlockPos::new(3, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

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

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn power_on(s: &mut Scene) {
    put(s, POWER, "minecraft:redstone_block", &[]);
}

fn sounds(s: &Scene) -> Vec<(&'static str, f32)> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, pitch, .. } => Some((sound, pitch)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_powered_note_block_plays_its_note_and_reports_it() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::NotePlay)]);
    put(&mut s, AT, "minecraft:note_block", &[("note", "24")]);
    sounds(&s);
    power_on(&mut s);
    let heard = settled_for(&s.plugin, EventKind::NotePlay);
    let EventPayload::NotePlay(event) = &heard.last().expect("a note").payload else {
        panic!("a note event");
    };
    assert_eq!(
        (event.block, event.instrument, event.note),
        (AT, Instrument::Piano, 24)
    );
    let played = sounds(&s);
    assert!(played.contains(&("minecraft:block.note_block.harp", 2.0)));
    assert_eq!(prop(&mut s, AT, "powered"), "true");
}

#[test]
fn a_cancelled_note_stays_silent() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::NotePlay)]);
    put(&mut s, AT, "minecraft:note_block", &[]);
    sounds(&s);
    power_on(&mut s);
    let asked = last_asked(&s.plugin, EventKind::NotePlay);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(sounds(&s).is_empty());
}

#[test]
fn clicking_a_note_block_raises_its_note_and_plays_it() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::NotePlay)]);
    put(&mut s, AT, "minecraft:note_block", &[("note", "12")]);
    let alice = s.players[0].clone();
    sounds(&s);
    use_block(&mut s, &alice, AT);
    assert_eq!(prop(&mut s, AT, "note"), "13");
    assert_eq!(settled_for(&s.plugin, EventKind::NotePlay).len(), 1);
}

#[test]
fn the_instrument_follows_the_block_under_the_note_block() {
    let mut s = scene(&["Alice"]);
    put(&mut s, AT, "minecraft:note_block", &[]);
    put(&mut s, AT.offset(0, -1, 0), "minecraft:gold_block", &[]);
    assert_eq!(prop(&mut s, AT, "instrument"), "bell");
    put(&mut s, AT.offset(0, -1, 0), "minecraft:hay_block", &[]);
    assert_eq!(prop(&mut s, AT, "instrument"), "banjo");
}

#[test]
fn a_powered_bell_rings_and_tells_the_plugins() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BellRing)]);
    put(
        &mut s,
        AT,
        "minecraft:bell",
        &[("facing", "north"), ("attachment", "floor")],
    );
    let alice = s.players[0].clone();
    alice.link.take();
    power_on(&mut s);
    let heard = settled_for(&s.plugin, EventKind::BellRing);
    let EventPayload::BellRing(event) = &heard.last().expect("a ring").payload else {
        panic!("a ring event");
    };
    assert_eq!(event.block, AT);
    assert!(event.entity.is_none());
    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::BlockEvent { pos, action: 1, .. } if *pos == AT
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::Sound {
            sound: "minecraft:block.bell.use",
            ..
        }
    )));
}

#[test]
fn a_cancelled_ring_makes_no_sound() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BellRing)]);
    put(&mut s, AT, "minecraft:bell", &[("attachment", "floor")]);
    let alice = s.players[0].clone();
    power_on(&mut s);
    let asked = last_asked(&s.plugin, EventKind::BellRing);
    alice.link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Sound { .. }))
    );
}

#[test]
fn a_bell_ringing_near_a_raider_resonates_and_names_it() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BellResonate)]);
    put(&mut s, AT, "minecraft:bell", &[("attachment", "floor")]);
    let Answer::Spawned(raider) = s.ask(Question::Spawn {
        kind: "minecraft:pillager".to_owned(),
        position: Vec3::new(8.5, 64.0, 8.5),
    }) else {
        panic!("a spawned pillager");
    };
    power_on(&mut s);
    s.tick(60);
    let heard = settled_for(&s.plugin, EventKind::BellResonate);
    let EventPayload::BellResonate(event) = &heard.last().expect("a resonance").payload else {
        panic!("a resonance event");
    };
    assert_eq!(event.block, AT);
    assert!(
        event
            .resonated_entities
            .iter()
            .any(|entity| entity.id == raider.id)
    );
}

#[test]
fn a_powered_crafter_crafts_and_drops_the_result() {
    let seeds = vec![Seed::block(AT, BlockKind::Crafter).holding("minecraft:oak_log", 2)];
    let mut s = world(seeds, vec![observe(1, EventKind::CrafterCraft)]);
    power_on(&mut s);
    s.tick(6);
    let ground = s.server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(
        ground[0].1.item.kind().map(|kind| kind.name()),
        Some("minecraft:oak_planks")
    );
    assert_eq!(ground[0].1.count.get(), 4);
    let heard = settled_for(&s.plugin, EventKind::CrafterCraft);
    let EventPayload::CrafterCraft(event) = &heard.last().expect("a craft").payload else {
        panic!("a craft event");
    };
    assert_eq!(event.block, AT);
    assert_eq!(
        (event.result.key.as_str(), event.result.count),
        ("minecraft:oak_planks", 4)
    );
    assert_eq!(prop(&mut s, AT, "crafting"), "true");
    s.tick(8);
    assert_eq!(prop(&mut s, AT, "crafting"), "false");
}

#[test]
fn a_cancelled_craft_keeps_the_ingredients() {
    let seeds = vec![Seed::block(AT, BlockKind::Crafter).holding("minecraft:oak_log", 2)];
    let mut s = world(seeds, vec![deciding(1, EventKind::CrafterCraft)]);
    power_on(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::CrafterCraft);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_patched_result_replaces_the_crafted_item() {
    let seeds = vec![Seed::block(AT, BlockKind::Crafter).holding("minecraft:oak_log", 2)];
    let mut s = world(seeds, vec![deciding(1, EventKind::CrafterCraft)]);
    power_on(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::CrafterCraft);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::CrafterCraft(CrafterCraftPatch {
            result: Some(ItemSpec::of("minecraft:diamond", 2)),
        })),
    );
    let ground = s.server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(
        ground[0].1.item.kind().map(|kind| kind.name()),
        Some("minecraft:diamond")
    );
}

fn rail_line(s: &mut Scene, kind: &str, length: i32) {
    for x in 0..length {
        put(
            s,
            BlockPos::new(3 + x, 64, 5),
            kind,
            &[("shape", "east_west")],
        );
    }
}

#[test]
fn a_powered_rail_passes_its_power_down_the_line() {
    let mut s = scene(&["Alice"]);
    rail_line(&mut s, "minecraft:powered_rail", 6);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:redstone_block",
        &[],
    );
    for x in 0..6 {
        assert_eq!(prop(&mut s, BlockPos::new(3 + x, 64, 5), "powered"), "true");
    }
    put(&mut s, BlockPos::new(2, 64, 5), "minecraft:air", &[]);
    for x in 0..6 {
        assert_eq!(
            prop(&mut s, BlockPos::new(3 + x, 64, 5), "powered"),
            "false"
        );
    }
}

#[test]
fn power_stops_after_eight_rails() {
    let mut s = scene(&["Alice"]);
    rail_line(&mut s, "minecraft:powered_rail", 12);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:redstone_block",
        &[],
    );
    assert_eq!(prop(&mut s, BlockPos::new(9, 64, 5), "powered"), "true");
    assert_eq!(prop(&mut s, BlockPos::new(13, 64, 5), "powered"), "false");
}

#[test]
fn an_activator_rail_takes_power_like_a_powered_rail() {
    let mut s = scene(&["Alice"]);
    rail_line(&mut s, "minecraft:activator_rail", 2);
    put(
        &mut s,
        BlockPos::new(2, 64, 5),
        "minecraft:redstone_block",
        &[],
    );
    assert_eq!(prop(&mut s, BlockPos::new(4, 64, 5), "powered"), "true");
}

#[test]
fn a_detector_rail_is_powered_while_a_minecart_rides_over_it() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockRedstone)]);
    let rail = BlockPos::new(4, 64, 5);
    rail_line(&mut s, "minecraft:detector_rail", 3);
    let Answer::Spawned(cart) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(4.5, 64.1, 5.5),
    }) else {
        panic!("a spawned minecart");
    };
    s.tick(2);
    assert_eq!(prop(&mut s, rail, "powered"), "true");
    let heard = settled_for(&s.plugin, EventKind::BlockRedstone);
    assert!(heard.iter().any(|fired| matches!(
        &fired.payload,
        EventPayload::BlockRedstone(event)
            if event.block == rail && event.old_current == 0 && event.new_current == 15
    )));
    s.act(Order::Entity {
        entity: cart.id,
        order: EntityOrder::Remove,
    });
    s.tick(25);
    assert_eq!(prop(&mut s, rail, "powered"), "false");
}

fn speed_of(s: &mut Scene, id: rspigot_domain::EntityId) -> f64 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity.x.abs() + details.velocity.z.abs(),
        other => panic!("expected an entity, got {other:?}"),
    }
}

fn cart_on_rail(s: &mut Scene, powered: bool) -> rspigot_domain::EntityId {
    rail_line(s, "minecraft:powered_rail", 6);
    if powered {
        put(s, BlockPos::new(2, 64, 5), "minecraft:redstone_block", &[]);
    }
    let Answer::Spawned(cart) = s.ask(Question::Spawn {
        kind: "minecraft:minecart".to_owned(),
        position: Vec3::new(4.5, 64.1, 5.5),
    }) else {
        panic!("a spawned minecart");
    };
    s.act(Order::Entity {
        entity: cart.id,
        order: EntityOrder::SetVelocity(Vec3::new(0.2, 0.0, 0.0)),
    });
    cart.id
}

#[test]
fn a_minecart_speeds_up_on_a_powered_rail() {
    let mut s = scene(&["Alice"]);
    let cart = cart_on_rail(&mut s, true);
    s.tick(3);
    assert!(speed_of(&mut s, cart) > 0.2);
}

#[test]
fn a_minecart_is_held_back_by_an_unpowered_rail() {
    let mut s = scene(&["Alice"]);
    let cart = cart_on_rail(&mut s, false);
    s.tick(3);
    assert!(speed_of(&mut s, cart) < 0.1);
}

#[test]
fn a_powered_door_opens_and_reports_the_current() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockRedstone)]);
    put(&mut s, AT, "minecraft:oak_door", &[("half", "lower")]);
    put(
        &mut s,
        AT.offset(0, 1, 0),
        "minecraft:oak_door",
        &[("half", "upper")],
    );
    power_on(&mut s);
    assert_eq!(prop(&mut s, AT, "open"), "true");
    assert_eq!(prop(&mut s, AT, "powered"), "true");
    assert_eq!(prop(&mut s, AT.offset(0, 1, 0), "open"), "true");
    let heard = settled_for(&s.plugin, EventKind::BlockRedstone);
    assert!(heard.iter().any(|fired| matches!(
        &fired.payload,
        EventPayload::BlockRedstone(event)
            if event.block == AT && event.old_current == 0 && event.new_current == 15
    )));
    put(&mut s, POWER, "minecraft:air", &[]);
    assert_eq!(prop(&mut s, AT, "open"), "false");
}

#[test]
fn a_patched_current_of_zero_keeps_the_door_shut() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockRedstone)]);
    put(&mut s, AT, "minecraft:oak_door", &[("half", "lower")]);
    put(
        &mut s,
        AT.offset(0, 1, 0),
        "minecraft:oak_door",
        &[("half", "upper")],
    );
    power_on(&mut s);
    let asked = asked_for(&s.plugin, EventKind::BlockRedstone)
        .into_iter()
        .find(|fired| matches!(&fired.payload, EventPayload::BlockRedstone(event) if event.block == AT))
        .expect("the door was asked");
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BlockRedstone(BlockRedstonePatch {
            new_current: Some(0),
        })),
    );
    assert_eq!(prop(&mut s, AT, "open"), "false");
}

#[test]
fn trapdoors_and_fence_gates_follow_the_power_too() {
    let mut s = scene(&["Alice"]);
    put(&mut s, AT, "minecraft:oak_trapdoor", &[]);
    put(&mut s, AT.offset(2, 0, 0), "minecraft:oak_fence_gate", &[]);
    power_on(&mut s);
    put(&mut s, AT.offset(2, 0, -1), "minecraft:redstone_block", &[]);
    assert_eq!(prop(&mut s, AT, "open"), "true");
    assert_eq!(prop(&mut s, AT.offset(2, 0, 0), "open"), "true");
    put(&mut s, POWER, "minecraft:air", &[]);
    assert_eq!(prop(&mut s, AT, "open"), "false");
}

#[test]
fn a_lever_with_no_block_under_it_falls_off() {
    let mut s = scene(&["Alice"]);
    put(&mut s, AT, "minecraft:stone", &[]);
    put(
        &mut s,
        AT.offset(0, 1, 0),
        "minecraft:lever",
        &[("face", "floor"), ("facing", "north")],
    );
    put(&mut s, AT, "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, AT.offset(0, 1, 0)), "minecraft:air");
}

#[test]
fn a_button_on_a_wall_falls_off_with_its_support() {
    let mut s = scene(&["Alice"]);
    put(&mut s, AT, "minecraft:stone", &[]);
    put(
        &mut s,
        AT.offset(0, 0, 1),
        "minecraft:stone_button",
        &[("face", "wall"), ("facing", "south")],
    );
    assert_eq!(
        kind_at(&mut s, AT.offset(0, 0, 1)),
        "minecraft:stone_button"
    );
    put(&mut s, AT, "minecraft:air", &[]);
    assert_eq!(kind_at(&mut s, AT.offset(0, 0, 1)), "minecraft:air");
}
