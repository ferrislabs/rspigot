mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, ItemSpec, Question};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn inventory(s: &mut Scene, player: &Player) -> Vec<Option<ItemSpec>> {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory.slots,
        other => panic!("expected the inventory, got {other:?}"),
    }
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

fn click_entity(s: &mut Scene, player: &Player, target: EntityId) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.1, 0.0),
        },
    );
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

fn first_slot(s: &mut Scene, player: &Player) -> Option<String> {
    inventory(s, player)
        .into_iter()
        .flatten()
        .next()
        .map(|spec| spec.key)
}

#[test]
fn a_water_bucket_scoops_up_a_fish_and_leaves_a_fish_bucket_holding_its_state() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    let cod = summon(
        &mut s,
        Kind::Cod,
        "1.5 64 0.5",
        "{NoAI:1b,Silent:1b,Health:2f}",
    );
    click_entity(&mut s, &alice, cod);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:cod_bucket")
    );
    assert!(s.server.entities_of(Kind::Cod).is_empty());
}

#[test]
fn another_item_or_a_dead_fish_is_not_scooped() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice bucket 1");
    let cod = summon(&mut s, Kind::Cod, "1.5 64 0.5", "{NoAI:1b}");
    click_entity(&mut s, &alice, cod);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
    assert_eq!(s.server.entities_of(Kind::Cod).len(), 1);
}

#[test]
fn a_cancelled_bucket_event_keeps_the_water_bucket_and_the_fish() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    let cod = summon(&mut s, Kind::Cod, "1.5 64 0.5", "{NoAI:1b}");
    click_entity(&mut s, &alice, cod);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketEntity);
    let EventPayload::PlayerBucketEntity(event) = &asked.payload else {
        panic!("expected a bucket event");
    };
    assert_eq!(event.entity.id, cod);
    assert_eq!(event.entity_bucket.key, "minecraft:cod_bucket");
    assert_eq!(event.original_bucket.key, "minecraft:water_bucket");
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:water_bucket")
    );
    assert_eq!(s.server.entities_of(Kind::Cod).len(), 1);
}

#[test]
fn an_allowed_bucket_event_swaps_the_bucket_and_removes_the_fish() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    let cod = summon(&mut s, Kind::Cod, "1.5 64 0.5", "{NoAI:1b}");
    click_entity(&mut s, &alice, cod);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketEntity);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:cod_bucket")
    );
    assert!(s.server.entities_of(Kind::Cod).is_empty());
}

#[test]
fn emptying_a_fish_bucket_on_the_floor_places_water_and_a_bucketed_fish() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice cod_bucket 1");
    use_down(&mut s, &alice);
    s.tick(2);
    let cods = s.server.entities_of(Kind::Cod);
    assert_eq!(cods.len(), 1);
    let nbt = s.server.saved_entity(cods[0].id).unwrap();
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:bucket")
    );
    let water = (-2..=2).any(|x| {
        (-2..=2).any(|z| {
            matches!(
                s.ask(Question::Block(BlockPos::new(x, 64, z))),
                Answer::Block(state) if state.kind == "minecraft:water"
            )
        })
    });
    assert!(water);
}

#[test]
fn a_fish_goes_into_a_bucket_and_comes_out_with_its_size_name_and_health() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice water_bucket 1");
    let salmon = summon(
        &mut s,
        Kind::Salmon,
        "1.5 64 0.5",
        "{NoAI:1b,type:\"large\",Health:2f,CustomName:'\"Sam\"',Silent:1b}",
    );
    click_entity(&mut s, &alice, salmon);
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:salmon_bucket")
    );
    use_down(&mut s, &alice);
    s.tick(2);
    let salmons = s.server.entities_of(Kind::Salmon);
    assert_eq!(salmons.len(), 1);
    let nbt = s.server.saved_entity(salmons[0].id).unwrap();
    assert_eq!(nbt.get_str("type"), Some("large"));
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    assert_eq!(nbt.get_bool("NoAI"), Some(true));
    assert_eq!(nbt.get_bool("Silent"), Some(true));
    assert!(nbt.get_f64("Health").is_some_and(|health| health <= 2.0));
    assert!(nbt.get_str("CustomName").is_some());
}

#[test]
fn a_cancelled_empty_event_keeps_the_bucket_and_spawns_nothing() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerBucketEmpty)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice cod_bucket 1");
    use_down(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketEmpty);
    let EventPayload::PlayerBucketEmpty(event) = &asked.payload else {
        panic!("expected a bucket event");
    };
    assert_eq!(event.bucket, "minecraft:cod_bucket");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(s.server.entities_of(Kind::Cod).is_empty());
    assert_eq!(
        first_slot(&mut s, &alice).as_deref(),
        Some("minecraft:cod_bucket")
    );
}
