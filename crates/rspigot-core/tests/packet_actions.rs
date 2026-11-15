mod common;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_domain::world::Difficulty;

fn operators(names: &[&str]) -> impl FnOnce(&mut rspigot_core::config::GameConfig) {
    let entries: Vec<ListEntry> = names.iter().map(|name| ListEntry::named(name)).collect();
    move |config| {
        config.admin = AdminLists {
            operators: entries,
            ..AdminLists::default()
        }
    }
}

fn difficulties(player: &Player) -> Vec<(Difficulty, bool)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Difficulty { difficulty, locked } => Some((difficulty, locked)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_operator_changing_the_difficulty_tells_every_player() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    assert_eq!(difficulties(&alice), [(Difficulty::Hard, false)]);
    assert_eq!(difficulties(&bob), [(Difficulty::Hard, false)]);
}

#[test]
fn a_player_without_permission_cannot_change_or_lock_the_difficulty() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(
        &mut server,
        &bob,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    send_input(&mut server, &bob, PlayerInput::LockDifficulty(true));
    assert!(difficulties(&alice).is_empty());
    assert!(difficulties(&bob).is_empty());
}

#[test]
fn a_locked_difficulty_refuses_packet_changes_and_stays_locked_for_new_players() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    alice.link.take();
    send_input(&mut server, &alice, PlayerInput::LockDifficulty(true));
    assert_eq!(difficulties(&alice), [(Difficulty::Easy, true)]);
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Hard),
    );
    assert!(difficulties(&alice).is_empty());
    let late = arrive(&mut server, "Carol");
    assert_eq!(difficulties(&late), [(Difficulty::Easy, true)]);
    send_input(&mut server, &alice, PlayerInput::LockDifficulty(false));
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Normal),
    );
    assert_eq!(
        difficulties(&alice),
        [(Difficulty::Easy, false), (Difficulty::Normal, false)]
    );
}

#[test]
fn a_hardcore_world_keeps_the_hard_difficulty() {
    let mut server = configured(|config| {
        operators(&["Alice"])(config);
        config.hardcore = true;
    });
    let alice = arrive(&mut server, "Alice");
    alice.link.take();
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChangeDifficulty(Difficulty::Peaceful),
    );
    assert_eq!(difficulties(&alice), [(Difficulty::Hard, false)]);
}

fn tag_queries(player: &Player) -> Vec<(i32, Option<rspigot_nbt::Compound>)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::TagQuery { transaction, tag } => Some((transaction, tag)),
            _ => None,
        })
        .collect()
}

fn console(server: &mut rspigot_core::server::Server<Link>, line: &str) {
    server.handle(rspigot_core::events::Inbound::Console(line.to_owned()));
    server.deliver();
}

#[test]
fn an_operator_reads_the_saved_data_of_an_entity_without_its_id() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    console(&mut server, "summon minecraft:cow 1.5 64 0.5 {NoAI:1b}");
    let cow = server
        .entities_of(rspigot_entities::EntityKind::Cow)
        .last()
        .map(|entity| entity.id)
        .expect("the cow");
    alice.link.take();
    send_input(
        &mut server,
        &alice,
        PlayerInput::EntityTagQuery {
            transaction: 9,
            entity: cow,
        },
    );
    let answers = tag_queries(&alice);
    assert_eq!(answers.len(), 1);
    let (transaction, tag) = &answers[0];
    assert_eq!(*transaction, 9);
    let tag = tag.as_ref().expect("a compound");
    assert!(tag.get_list("Pos").is_some());
    assert!(tag.get_str("id").is_none());
    assert_eq!(tag.get_bool("NoAI"), Some(true));
}

#[test]
fn an_entity_query_for_nothing_or_by_a_player_without_permission_gets_no_answer() {
    let mut server = configured(operators(&["Alice"]));
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    console(&mut server, "summon minecraft:cow 1.5 64 0.5");
    let cow = server
        .entities_of(rspigot_entities::EntityKind::Cow)
        .last()
        .map(|entity| entity.id)
        .expect("the cow");
    alice.link.take();
    bob.link.take();
    let missing = rspigot_domain::EntityId(987_654);
    send_input(
        &mut server,
        &alice,
        PlayerInput::EntityTagQuery {
            transaction: 1,
            entity: missing,
        },
    );
    send_input(
        &mut server,
        &bob,
        PlayerInput::EntityTagQuery {
            transaction: 2,
            entity: cow,
        },
    );
    assert!(tag_queries(&alice).is_empty());
    assert!(tag_queries(&bob).is_empty());
}

fn written_sign(s: &mut Scene, sign: rspigot_domain::BlockPos) {
    use rspigot_core::events::{Direction, Hand};
    s.act(rspigot_core::plugins::Order::Block {
        pos: sign,
        order: rspigot_core::plugins::BlockOrder::SetWith {
            kind: "minecraft:oak_sign".to_owned(),
            properties: vec![("rotation".to_owned(), "0".to_owned())],
        },
    });
    let alice = s.players[0].clone();
    console(&mut s.server, "tp Alice 2.5 64 5.5");
    let teleport = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::Teleport { id, .. } => Some(id),
        _ => None,
    });
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::TeleportConfirm(teleport.expect("a teleport")),
    );
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: sign,
            face: Direction::South,
            cursor: rspigot_domain::Vec3::new(0.5, 0.5, 1.0),
            inside_block: false,
            sequence: 1,
        },
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::SignUpdate {
            pos: sign,
            front: true,
            lines: ["a", "b", "", ""].map(str::to_owned).to_vec(),
        },
    );
}

#[test]
fn a_block_query_answers_with_the_block_entity_data_or_nothing() {
    let sign = rspigot_domain::BlockPos::new(2, 64, 2);
    let mut s = scene_on(
        configured(operators(&["Alice"])),
        &["Alice", "Bob"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, sign);
    written_sign(&mut s, sign);
    let data = s.server.block_entity_nbt(sign).expect("a sign entity");
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    alice.link.take();
    bob.link.take();
    let ask = |s: &mut Scene, who: &Player, transaction: i32, pos| {
        send_input(
            &mut s.server,
            who,
            PlayerInput::BlockTagQuery { transaction, pos },
        );
    };
    ask(&mut s, &alice, 3, sign);
    ask(&mut s, &alice, 4, rspigot_domain::BlockPos::new(5, 64, 5));
    ask(&mut s, &bob, 5, sign);
    assert_eq!(tag_queries(&alice), [(3, Some(data)), (4, None)]);
    assert!(tag_queries(&bob).is_empty());
}
