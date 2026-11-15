mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::PlayerBucketFillPatch;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, Rotation};

const FLOOR: BlockPos = BlockPos::new(0, 63, 0);
const ABOVE: BlockPos = BlockPos::new(0, 64, 0);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn use_down(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 90.0),
        },
    );
}

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn first_slot(s: &mut Scene, player: &Player) -> Option<String> {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .into_iter()
            .flatten()
            .next()
            .map(|spec| spec.key),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn spec(key: &str) -> ItemSpec {
    ItemSpec {
        key: key.to_owned(),
        count: 1,
        name: None,
        lore: Vec::new(),
        enchantments: Vec::new(),
        custom_model_data: None,
        components: Vec::new(),
    }
}

#[test]
fn an_empty_bucket_scoops_a_water_source_and_fires_a_fill_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerBucketFill)]);
    let alice = s.players[0].clone();
    set(&mut s, FLOOR, "minecraft:water");
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    let heard = settled_for(&s.plugin, EventKind::PlayerBucketFill);
    let EventPayload::PlayerBucketFill(event) = &heard[0].payload else {
        panic!("expected a bucket fill");
    };
    assert_eq!(event.block, FLOOR);
    assert_eq!(event.block_clicked, FLOOR);
    assert_eq!(event.bucket, "minecraft:bucket");
    assert_eq!(
        event.item_stack.as_ref().unwrap().key,
        "minecraft:water_bucket"
    );
    assert_eq!(kind_at(&mut s, FLOOR), "minecraft:air");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:water_bucket")
    );
}

#[test]
fn a_lava_source_fills_a_lava_bucket_without_any_handler() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    set(&mut s, FLOOR, "minecraft:lava");
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    assert_eq!(kind_at(&mut s, FLOOR), "minecraft:air");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:lava_bucket")
    );
}

#[test]
fn flowing_water_is_not_scooped() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(Order::Block {
        pos: FLOOR,
        order: BlockOrder::SetWith {
            kind: "minecraft:water".to_owned(),
            properties: vec![("level".to_owned(), "3".to_owned())],
        },
    });
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
}

#[test]
fn a_cancelled_fill_keeps_the_water_and_the_empty_bucket() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketFill)]);
    let alice = s.players[0].clone();
    set(&mut s, FLOOR, "minecraft:water");
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketFill);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(kind_at(&mut s, FLOOR), "minecraft:water");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
}

#[test]
fn a_patched_fill_hands_over_the_patched_item() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketFill)]);
    let alice = s.players[0].clone();
    set(&mut s, FLOOR, "minecraft:water");
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketFill);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerBucketFill(PlayerBucketFillPatch {
            item_stack: Some(Some(spec("minecraft:milk_bucket"))),
        })),
    );
    assert_eq!(kind_at(&mut s, FLOOR), "minecraft:air");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:milk_bucket")
    );
}

#[test]
fn a_creative_player_keeps_the_empty_bucket_and_gains_the_full_one() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode creative Alice");
    set(&mut s, FLOOR, "minecraft:water");
    console(&mut s, "give Alice bucket 1");
    use_down(&mut s, &alice);
    assert_eq!(kind_at(&mut s, FLOOR), "minecraft:air");
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(alice.id)) else {
        panic!("an inventory");
    };
    let keys: Vec<String> = inventory
        .slots
        .into_iter()
        .flatten()
        .map(|s| s.key)
        .collect();
    assert!(keys.contains(&"minecraft:bucket".to_owned()));
    assert!(keys.contains(&"minecraft:water_bucket".to_owned()));
}

#[test]
fn a_water_bucket_places_a_source_and_fires_an_empty_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerBucketEmpty)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    use_down(&mut s, &alice);
    let heard = settled_for(&s.plugin, EventKind::PlayerBucketEmpty);
    let EventPayload::PlayerBucketEmpty(event) = &heard[0].payload else {
        panic!("expected a bucket empty");
    };
    assert_eq!(event.block, ABOVE);
    assert_eq!(event.block_clicked, FLOOR);
    assert_eq!(event.bucket, "minecraft:water_bucket");
    assert_eq!(event.item_stack.as_ref().unwrap().key, "minecraft:bucket");
    assert_eq!(kind_at(&mut s, ABOVE), "minecraft:water");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
}

#[test]
fn a_lava_bucket_places_lava() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice lava_bucket 1");
    use_down(&mut s, &alice);
    assert_eq!(kind_at(&mut s, ABOVE), "minecraft:lava");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
}

#[test]
fn a_cancelled_empty_keeps_the_water_bucket() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketEmpty)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    use_down(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketEmpty);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(kind_at(&mut s, ABOVE), "minecraft:air");
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:water_bucket")
    );
}
