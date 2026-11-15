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

mod spectating {
    use super::*;
    use rspigot_core::plugins::events::{PlayerTeleport, PlayerTeleportCause};
    use rspigot_core::plugins::{EventKind, EventPayload};
    use rspigot_domain::Vec3;
    use rspigot_domain::player::PlayerId;

    fn cow_uuid(s: &Scene) -> uuid::Uuid {
        s.server
            .entities_of(rspigot_entities::EntityKind::Cow)
            .last()
            .map(|cow| cow.uuid)
            .expect("the cow")
    }

    fn setup(mode: &str) -> Scene {
        let mut s = scene_with(
            &["Alice", "Bob"],
            vec![deciding(1, EventKind::PlayerTeleport)],
        );
        console(&mut s.server, "summon minecraft:cow 10.5 64 3.5 {NoAI:1b}");
        console(&mut s.server, &format!("gamemode {mode} Alice"));
        s.players[0].link.take();
        s
    }

    fn teleport_to(s: &mut Scene, target: uuid::Uuid) {
        let alice = s.players[0].clone();
        send_input(&mut s.server, &alice, PlayerInput::TeleportToEntity(target));
    }

    #[test]
    fn a_spectator_goes_to_the_entity_with_the_spectate_cause() {
        let mut s = setup("spectator");
        let cow = cow_uuid(&s);
        teleport_to(&mut s, cow);
        let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
        let EventPayload::PlayerTeleport(PlayerTeleport { cause, to, .. }) = &asked.payload else {
            panic!("expected a teleport");
        };
        assert_eq!(*cause, PlayerTeleportCause::Spectate);
        assert_eq!(
            to.as_ref().map(|to| to.position),
            Some(Vec3::new(10.5, 64.0, 3.5))
        );
        rule_on(&mut s.server, 1, asked.id, false, None);
        assert_eq!(
            s.server.player_location(s.players[0].id),
            Some(Vec3::new(10.5, 64.0, 3.5))
        );
    }

    #[test]
    fn a_spectator_can_go_to_another_player() {
        let mut s = setup("spectator");
        let bob = s.players[1].id;
        let target = bob_uuid(bob);
        teleport_to(&mut s, target);
        let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
        rule_on(&mut s.server, 1, asked.id, false, None);
        assert_eq!(
            s.server.player_location(s.players[0].id),
            s.server.player_location(bob)
        );
    }

    fn bob_uuid(id: PlayerId) -> uuid::Uuid {
        id.0
    }

    #[test]
    fn a_player_who_is_not_a_spectator_stays_where_they_are() {
        let mut s = setup("survival");
        let cow = cow_uuid(&s);
        teleport_to(&mut s, cow);
        assert!(asked_for(&s.plugin, EventKind::PlayerTeleport).is_empty());
        assert_ne!(
            s.server.player_location(s.players[0].id),
            Some(Vec3::new(10.5, 64.0, 3.5))
        );
    }

    #[test]
    fn an_unknown_target_changes_nothing() {
        let mut s = setup("spectator");
        teleport_to(&mut s, uuid::Uuid::from_u128(42));
        assert!(asked_for(&s.plugin, EventKind::PlayerTeleport).is_empty());
    }
}

mod game_rules {
    use super::*;
    use rspigot_core::plugins::{Answer, GameRule, Question};

    fn set(s: &mut Scene, who: usize, entries: &[(&str, &str)]) {
        let player = s.players[who].clone();
        let entries = entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        send_input(&mut s.server, &player, PlayerInput::SetGameRules(entries));
    }

    fn flags(s: &mut Scene) -> Vec<(GameRule, bool)> {
        let Answer::World(state) = s.ask(Question::World) else {
            panic!("expected the world");
        };
        state.rules
    }

    fn scene() -> Scene {
        scene_on(
            configured(operators(&["Alice"])),
            &["Alice", "Bob"],
            Vec::new(),
        )
    }

    #[test]
    fn an_operator_sets_a_flag_rule_by_its_key() {
        let mut s = scene();
        assert!(flags(&mut s).contains(&(GameRule::KeepInventory, false)));
        set(&mut s, 0, &[("minecraft:keep_inventory", "true")]);
        assert!(flags(&mut s).contains(&(GameRule::KeepInventory, true)));
    }

    #[test]
    fn plain_and_number_rules_are_stored_when_in_range() {
        let mut s = scene();
        set(
            &mut s,
            0,
            &[
                ("minecraft:tnt_explodes", "false"),
                ("minecraft:random_tick_speed", "7"),
                ("minecraft:max_snow_accumulation_height", "8"),
                ("minecraft:fire_spread_radius_around_player", "-1"),
            ],
        );
        let rules = s.server.game_rules();
        assert!(!rules.tnt_explodes);
        assert_eq!(rules.random_tick_speed, 7);
        assert_eq!(rules.max_snow_accumulation_height, 8);
        assert_eq!(rules.fire_spread_radius, -1);
    }

    #[test]
    fn invalid_values_unknown_keys_and_non_operators_change_nothing() {
        let mut s = scene();
        let before = s.server.game_rules();
        set(
            &mut s,
            0,
            &[
                ("minecraft:keep_inventory", "yes"),
                ("minecraft:random_tick_speed", "-1"),
                ("minecraft:max_snow_accumulation_height", "9"),
                ("minecraft:no_such_rule", "true"),
                ("keep_inventory", "true"),
            ],
        );
        set(&mut s, 1, &[("minecraft:random_tick_speed", "50")]);
        assert_eq!(s.server.game_rules(), before);
        assert!(flags(&mut s).contains(&(GameRule::KeepInventory, false)));
    }
}
