mod common;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::events::{ClientEvent, Inbound};
use std::sync::Arc;

use rspigot_domain::Identifier;
use rspigot_domain::text::{ClickEvent, Component, Content};
use rspigot_domain::world::Dimension;

fn operator_scene() -> (Scene, Player) {
    let mut s = scene(&["Alice", "Bob"]);
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    let alice = s.players[0].clone();
    alice.link.take();
    (s, alice)
}

fn run(s: &mut Scene, alice: &Player, line: &str) -> (CommandOutcome, Vec<String>) {
    let outcome = execute(&mut s.server, CommandSender::Player(alice.id), line);
    s.server.deliver();
    let says = alice
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect();
    (outcome, says)
}

fn components(s: &mut Scene, alice: &Player, line: &str) -> (CommandOutcome, Vec<Component>) {
    let outcome = execute(&mut s.server, CommandSender::Player(alice.id), line);
    s.server.deliver();
    let shown = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message),
            _ => None,
        })
        .collect();
    (outcome, shown)
}

#[test]
fn w27_4_tellraw_resolves_score_selector_and_nbt_on_the_server() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "scoreboard objectives add kills dummy");
    run(&mut s, &alice, "scoreboard players set Alice kills 7");
    run(
        &mut s,
        &alice,
        "data merge storage t:s {list:[1,2,3],name:'Zed'}",
    );
    let (outcome, shown) = components(
        &mut s,
        &alice,
        r#"tellraw Alice {score:{name:"Alice",objective:"kills"}}"#,
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(shown[0].plain_text(), "7");
    let (_, shown) = components(
        &mut s,
        &alice,
        r#"tellraw Alice {score:{name:"*",objective:"kills"}}"#,
    );
    assert_eq!(shown[0].plain_text(), "7");
    let (_, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {selector:'@a',separator:' & '}",
    );
    assert_eq!(shown[0].plain_text(), "Alice & Bob");
    let (_, shown) = components(&mut s, &alice, "tellraw Alice {selector:'@a'}");
    assert_eq!(shown[0].plain_text(), "Alice, Bob");
    let (_, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {nbt:'name',storage:'t:s',color:'red'}",
    );
    assert_eq!(shown[0].plain_text(), "\"Zed\"");
    let (_, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {nbt:'name',storage:'t:s',interpret:true}",
    );
    assert_eq!(shown[0].plain_text(), "Zed");
    let (_, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {nbt:'list[]',storage:'t:s',plain:true,separator:'|'}",
    );
    assert_eq!(shown[0].plain_text(), "1|2|3");
    let (_, shown) = components(&mut s, &alice, "tellraw Alice {keybind:'key.jump'}");
    assert_eq!(shown[0].content, Content::Keybind("key.jump".to_owned()));
    let (_, shown) = components(&mut s, &alice, "tellraw Alice {sprite:'block/stone'}");
    assert!(matches!(shown[0].content, Content::Object(_)));
}

#[test]
fn w27_4_components_and_styles_parse_as_snbt_and_keep_their_full_style() {
    let (mut s, alice) = operator_scene();
    let (outcome, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {text:'hi',color:'gold',insertion:'ins',font:'minecraft:uniform',shadow_color:-1,click_event:{action:'open_url',url:'https://x.y'},hover_event:{action:'show_text',value:'tip'}}",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let style = &shown[0].style;
    assert_eq!(style.insertion(), Some("ins"));
    assert_eq!(style.font(), Some("minecraft:uniform"));
    assert_eq!(style.shadow_color(), Some(u32::MAX));
    assert_eq!(
        style.click.as_deref(),
        Some(&ClickEvent::OpenUrl("https://x.y".to_owned()))
    );
    assert_eq!(
        style.hover.as_ref().map(|h| h.plain_text()).as_deref(),
        Some("tip")
    );
    let (_, shown) = components(
        &mut s,
        &alice,
        "tellraw Alice {translate:'a.b',fallback:'Hello %s',with:['x']}",
    );
    assert!(matches!(
        &shown[0].content,
        Content::Translatable { fallback: Some(f), .. } if f.as_str() == "Hello %s"
    ));
    let (outcome, _) = components(
        &mut s,
        &alice,
        r#"tellraw Alice {"text":"json still works"}"#,
    );
    assert_eq!(outcome, CommandOutcome::Success);
}

#[test]
fn w27_5_data_and_store_write_non_player_entities() {
    let (mut s, alice) = operator_scene();
    let (outcome, _) = run(&mut s, &alice, "summon pig ~ ~ ~");
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(
        &mut s,
        &alice,
        "data merge entity @e[type=pig,limit=1] {Health:7f}",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, says) = run(
        &mut s,
        &alice,
        "data get entity @e[type=pig,limit=1] Health",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].contains('7'), "{says:?}");
    run(&mut s, &alice, "scoreboard objectives add x dummy");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute store result entity @e[type=pig,limit=1] Health float 1 run scoreboard players set Q x 3",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let (_, says) = run(
        &mut s,
        &alice,
        "data get entity @e[type=pig,limit=1] Health",
    );
    assert!(says[0].contains('3'), "{says:?}");
    let (outcome, says) = run(&mut s, &alice, "data merge entity Alice {Health:1f}");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(says[0], "commands.data.entity.invalid");
}

fn total(s: &mut Scene, alice: &Player) -> i64 {
    let (outcome, says) = run(s, alice, "time query time");
    assert_eq!(outcome, CommandOutcome::Success);
    says[0]
        .rsplit(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(-1)
}

#[test]
fn w27_1_time_sets_markers_adds_negative_and_queries_timelines() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "gamerule advance_time false");
    let (outcome, says) = run(&mut s, &alice, "time set minecraft:noon");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        says[0].starts_with("commands.time.set.time_marker"),
        "{says:?}"
    );
    assert_eq!(total(&mut s, &alice), 6000);
    let (outcome, says) = run(&mut s, &alice, "time set noon");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says[0].starts_with("commands.time.set.already_at_time_marker"));
    let (outcome, _) = run(&mut s, &alice, "time set day");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(total(&mut s, &alice), 24_000 + 1000);
    let (outcome, _) = run(&mut s, &alice, "time add -500");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(total(&mut s, &alice), 24_500);
    let (outcome, says) = run(&mut s, &alice, "time query minecraft:day");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].ends_with("500"), "{says:?}");
    let (_, says) = run(&mut s, &alice, "time query day repetition");
    assert!(says[0].starts_with("commands.time.query.timeline.repetitions"));
    assert!(says[0].ends_with(" 1"), "{says:?}");
    let (outcome, says) = run(&mut s, &alice, "time set minecraft:nothing");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says[0].starts_with("commands.time.no_time_marker_found"));
    let (outcome, _) = run(&mut s, &alice, "time of minecraft:overworld query gametime");
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn w27_1_time_pause_resume_and_rate_drive_the_clock() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "time set 100");
    let (outcome, says) = run(&mut s, &alice, "time pause");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].starts_with("commands.time.pause"));
    let before = total(&mut s, &alice);
    s.tick(5);
    assert_eq!(total(&mut s, &alice), before);
    let (outcome, says) = run(&mut s, &alice, "time pause");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says[0].starts_with("commands.time.pause.already_paused"));
    run(&mut s, &alice, "time resume");
    run(&mut s, &alice, "time of minecraft:overworld rate 2");
    let before = total(&mut s, &alice);
    s.tick(5);
    assert_eq!(total(&mut s, &alice), before + 10);
    let (outcome, says) = run(&mut s, &alice, "time rate 2");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says[0].starts_with("commands.time.rate.already_same"));
    let (outcome, _) = run(&mut s, &alice, "time rate 0");
    assert_eq!(outcome, CommandOutcome::Failure);
}

fn inventory(s: &mut Scene, alice: &Player, who: &str) -> String {
    let (_, says) = run(s, alice, &format!("data get entity {who} Inventory"));
    says.join("")
}

#[test]
fn w27_2_give_and_clear_take_selectors_components_and_counts() {
    let (mut s, alice) = operator_scene();
    let (outcome, says) = run(&mut s, &alice, "give @a minecraft:diamond 3");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        says[0].starts_with("commands.give.success.multiple"),
        "{says:?}"
    );
    for who in ["Alice", "Bob"] {
        assert!(inventory(&mut s, &alice, who).contains("minecraft:diamond"));
    }
    let (outcome, says) = run(&mut s, &alice, "give Alice minecraft:diamond 6401");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(
        says[0].starts_with("commands.give.failed.toomanyitems"),
        "{says:?}"
    );
    let (outcome, _) = run(
        &mut s,
        &alice,
        "give Alice minecraft:stick[minecraft:custom_name='\"Wand\"'] 2",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(inventory(&mut s, &alice, "Alice").contains("Wand"));

    let (outcome, says) = run(&mut s, &alice, "clear Alice minecraft:diamond 0");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        says[0].starts_with("commands.clear.test.single"),
        "{says:?}"
    );
    assert!(says[0].contains(" 3 "), "{says:?}");
    assert!(inventory(&mut s, &alice, "Alice").contains("minecraft:diamond"));
    let (outcome, says) = run(&mut s, &alice, "clear Alice minecraft:diamond 2");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].starts_with("commands.clear.success.single"));
    assert!(says[0].contains(" 2 "), "{says:?}");
    let (outcome, _) = run(&mut s, &alice, "clear Alice minecraft:diamond 5");
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, says) = run(&mut s, &alice, "clear Alice minecraft:diamond");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says[0].starts_with("commands.clear.failed.single"));
    let (outcome, _) = run(&mut s, &alice, "clear @a");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(!inventory(&mut s, &alice, "Alice").contains("Wand"));
}

#[test]
fn w27_2_gamemode_and_kick_take_selectors() {
    let (mut s, alice) = operator_scene();
    let (outcome, says) = run(&mut s, &alice, "gamemode creative @a");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(
        says.iter()
            .filter(|line| line.starts_with("commands.gamemode.success"))
            .count(),
        2,
        "{says:?}"
    );
    let (outcome, _) = run(&mut s, &alice, "kick @a[name=Bob] bye");
    assert_eq!(outcome, CommandOutcome::Success);
    let bob = s.players[1].clone();
    assert!(bob.link.disconnected_with().is_some());
    assert!(alice.link.disconnected_with().is_none());
}

fn kind_at(s: &Scene, x: i32, y: i32, z: i32) -> Option<String> {
    s.server
        .block_at(rspigot_domain::BlockPos::new(x, y, z))
        .and_then(|state| state.kind())
        .map(|kind| kind.name().to_owned())
}

#[test]
fn w27_3_fill_filters_combine_with_hollow_and_strict_skips_updates() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "fill 0 64 0 2 66 2 minecraft:stone");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "fill 0 64 0 2 66 2 minecraft:glass replace minecraft:stone hollow",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(kind_at(&s, 0, 65, 0).as_deref(), Some("minecraft:glass"));
    assert_eq!(kind_at(&s, 1, 65, 1).as_deref(), Some("minecraft:air"));
    let (outcome, _) = run(
        &mut s,
        &alice,
        "fill 0 64 0 2 66 2 minecraft:dirt replace minecraft:stone outline",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    let (outcome, _) = run(&mut s, &alice, "fill 0 64 0 2 64 2 minecraft:dirt strict");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(kind_at(&s, 1, 64, 1).as_deref(), Some("minecraft:dirt"));
    let (outcome, _) = run(
        &mut s,
        &alice,
        "fill 0 64 0 2 64 2 minecraft:stone replace minecraft:dirt strict",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(kind_at(&s, 1, 64, 1).as_deref(), Some("minecraft:stone"));

    let (outcome, _) = run(&mut s, &alice, "setblock 10 90 10 minecraft:sand strict");
    assert_eq!(outcome, CommandOutcome::Success);
    s.tick(20);
    assert_eq!(kind_at(&s, 10, 90, 10).as_deref(), Some("minecraft:sand"));
    let (outcome, _) = run(&mut s, &alice, "setblock 12 90 10 minecraft:sand");
    assert_eq!(outcome, CommandOutcome::Success);
    s.tick(20);
    assert_ne!(kind_at(&s, 12, 90, 10).as_deref(), Some("minecraft:sand"));
}

#[test]
fn w27_3_clone_accepts_strict_and_dimension_clauses() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "fill 0 64 0 1 64 1 minecraft:gold_block");
    let (outcome, says) = run(&mut s, &alice, "clone 0 64 0 1 64 1 4 64 4 strict");
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    assert_eq!(
        kind_at(&s, 5, 64, 5).as_deref(),
        Some("minecraft:gold_block")
    );
    let (outcome, says) = run(
        &mut s,
        &alice,
        "clone from minecraft:overworld 0 64 0 1 64 1 to minecraft:overworld 8 64 8",
    );
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    assert_eq!(
        kind_at(&s, 9, 64, 9).as_deref(),
        Some("minecraft:gold_block")
    );
    let (outcome, _) = run(
        &mut s,
        &alice,
        "clone from minecraft:nowhere 0 64 0 1 64 1 4 64 4",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn w27_3_tp_sets_rotation_and_copies_the_destination_rotation() {
    let (mut s, alice) = operator_scene();
    let bob = s.players[1].clone();
    bob.link.take();
    let (outcome, _) = run(&mut s, &alice, "tp Bob 5 70 5 90 -20");
    assert_eq!(outcome, CommandOutcome::Success);
    let looks: Vec<(f32, f32)> = bob
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerRotation { yaw, pitch } => Some((yaw, pitch)),
            _ => None,
        })
        .collect();
    assert_eq!(looks.last(), Some(&(90.0, -20.0)));
    let (outcome, _) = run(&mut s, &alice, "tp Bob 5 70 5 ~10 ~5");
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(&mut s, &alice, "tp Bob 5 70 5 90");
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn w27_5_execute_if_items_and_stopwatch() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "give Alice minecraft:diamond 3");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute if items entity Alice container.* minecraft:diamond",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute if items entity Alice container.* minecraft:emerald",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute unless items entity Alice container.* minecraft:emerald run say none",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    run(&mut s, &alice, "stopwatch create t:w");
    let (outcome, _) = run(&mut s, &alice, "execute if stopwatch t:w 0..");
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(&mut s, &alice, "execute if stopwatch t:w 100000..");
    assert_eq!(outcome, CommandOutcome::Failure);
    let (outcome, says) = run(&mut s, &alice, "execute if stopwatch t:missing 0..");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(
        says[0].starts_with("commands.stopwatch.does_not_exist"),
        "{says:?}"
    );
}

#[test]
fn w27_5_say_resolves_selectors_and_team_formats_names_in_chat_and_deaths() {
    let (mut s, alice) = operator_scene();
    let bob = s.players[1].clone();
    bob.link.take();
    run(&mut s, &alice, "team add red");
    run(&mut s, &alice, "team modify red prefix {text:'[R] '}");
    run(&mut s, &alice, "team modify red color red");
    run(&mut s, &alice, "team join red Alice");
    bob.link.take();
    let (outcome, _) = run(&mut s, &alice, "say hello @a[name=Bob]");
    assert_eq!(outcome, CommandOutcome::Success);
    let heard: Vec<(String, String)> = bob
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::ChatMessage {
                sender_name,
                message,
                ..
            } => Some((sender_name.plain_text(), message.plain_text())),
            _ => None,
        })
        .collect();
    assert_eq!(
        heard,
        vec![("[R] Alice".to_owned(), "hello Bob".to_owned())]
    );
    let (outcome, _) = run(&mut s, &alice, "kill Alice");
    assert_eq!(outcome, CommandOutcome::Success);
    let deaths: Vec<String> = bob
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect();
    assert!(
        deaths.iter().any(|line| line.contains("[R] Alice")),
        "{deaths:?}"
    );
}

#[test]
fn w27_5_command_feedback_reaches_the_other_operators() {
    let (mut s, alice) = operator_scene();
    s.server.handle(Inbound::Console("op Bob".to_owned()));
    s.server.deliver();
    let bob = s.players[1].clone();
    bob.link.take();
    alice.link.take();
    let (outcome, says) = run(&mut s, &alice, "time set 500");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].starts_with("commands.time.set.absolute"));
    let relayed: Vec<String> = bob
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect();
    assert!(
        relayed
            .iter()
            .any(|line| line.starts_with("chat.type.admin Alice")),
        "{relayed:?}"
    );
    assert!(says.iter().all(|line| !line.starts_with("chat.type.admin")));
}

fn dimensions_scene() -> (Scene, Player) {
    let mut server = configured(|_| {});
    for (key, dimension) in [
        (Identifier::minecraft("the_nether"), Dimension::nether()),
        (Identifier::minecraft("the_end"), Dimension::end()),
    ] {
        server.load_dimension(key, dimension, Arc::new(Floor), Arc::new(NoStore));
    }
    let mut s = scene_on(server, &["Alice", "Bob"], Vec::new());
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    let alice = s.players[0].clone();
    alice.link.take();
    (s, alice)
}

#[test]
fn w27_3_tp_moves_players_between_dimensions() {
    let (mut s, alice) = dimensions_scene();
    let (outcome, says) = run(
        &mut s,
        &alice,
        "execute in minecraft:the_nether run tp Alice 10 80 10",
    );
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    s.tick(5);
    let (_, says) = run(&mut s, &alice, "data get entity Alice Dimension");
    assert!(says.join("").contains("the_nether"), "{says:?}");
    let (outcome, _) = run(&mut s, &alice, "tp Alice Bob");
    assert_eq!(outcome, CommandOutcome::Success);
    s.tick(5);
    let (_, says) = run(&mut s, &alice, "data get entity Alice Dimension");
    assert!(says.join("").contains("overworld"), "{says:?}");
}

#[test]
fn w27_1_time_of_a_clock_runs_in_its_own_dimension() {
    let (mut s, alice) = dimensions_scene();
    let (outcome, says) = run(&mut s, &alice, "time of minecraft:the_end set 777");
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    let (outcome, says) = run(&mut s, &alice, "time of minecraft:the_end query time");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].ends_with(" 777"), "{says:?}");
    let (_, says) = run(&mut s, &alice, "time of minecraft:overworld query time");
    assert!(!says[0].ends_with(" 777"), "{says:?}");
    let (outcome, _) = run(&mut s, &alice, "time of minecraft:the_end pause");
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(&mut s, &alice, "time of minecraft:overworld pause");
    assert_eq!(outcome, CommandOutcome::Success);
}

#[test]
fn w27_medium_random_is_open_to_players_except_reset_and_sequences() {
    let (mut s, _alice) = operator_scene();
    let bob = s.players[1].clone();
    bob.link.take();
    let outcome = execute(
        &mut s.server,
        CommandSender::Player(bob.id),
        "random value 1..10",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let outcome = execute(
        &mut s.server,
        CommandSender::Player(bob.id),
        "random value 1..10 t:seq",
    );
    assert_ne!(outcome, CommandOutcome::Success);
    let outcome = execute(
        &mut s.server,
        CommandSender::Player(bob.id),
        "random reset *",
    );
    assert_ne!(outcome, CommandOutcome::Success);
}

#[test]
fn w27_medium_spawn_commands_take_a_rotation_and_list_shows_uuids() {
    let (mut s, alice) = operator_scene();
    let (outcome, says) = run(&mut s, &alice, "setworldspawn 4 64 4 45 10");
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    assert!(says[0].ends_with(" 45"), "{says:?}");
    let (outcome, says) = run(&mut s, &alice, "spawnpoint Alice 1 64 1 90 5");
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    assert!(says[0].contains(" 90 "), "{says:?}");
    let (outcome, says) = run(&mut s, &alice, "list uuids");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        says[0].contains("commands.list.nameAndId Alice"),
        "{says:?}"
    );
    let (outcome, says) = run(&mut s, &alice, "help time");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        says.iter()
            .any(|line| line.contains("/time of <clock> pause")),
        "{says:?}"
    );
    let (outcome, _) = run(&mut s, &alice, "help nothing");
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn w27_5_clock_state_reaches_clients_and_the_end_clock_is_kept() {
    let (mut s, alice) = dimensions_scene();
    alice.link.take();
    for line in ["time rate 2", "time pause"] {
        let outcome = execute(&mut s.server, CommandSender::Player(alice.id), line);
        assert_eq!(outcome, CommandOutcome::Success);
        s.server.deliver();
    }
    let rates: Vec<f32> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Time { rate, .. } => Some(rate),
            _ => None,
        })
        .collect();
    assert!(rates.contains(&2.0), "{rates:?}");
    assert_eq!(rates.last(), Some(&0.0));
    run(&mut s, &alice, "time of minecraft:the_end set 4242");
    run(&mut s, &alice, "time of minecraft:the_end rate 3");
    let level = s.server.level_snapshot();
    let (ticks, runtime) = level.end_clock.expect("the end clock is saved");
    assert_eq!(ticks, 4242);
    assert_eq!(runtime.rate, 3.0);
    assert_eq!(level.clock.rate, 2.0);
    assert!(level.clock.paused);
}

#[test]
fn w27_1_unsigned_chat_uses_the_team_formatted_name() {
    let (mut s, alice) = operator_scene();
    let bob = s.players[1].clone();
    run(&mut s, &alice, "team add red");
    run(&mut s, &alice, "team modify red prefix {text:'[R] '}");
    run(&mut s, &alice, "team join red Alice");
    bob.link.take();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Chat("hi".to_owned()),
    );
    let names: Vec<String> = bob
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerChat { sender_name, .. } => Some(sender_name.plain_text()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["[R] Alice".to_owned()]);
}

#[test]
fn w27_5_data_modify_compute() {
    let (mut s, alice) = operator_scene();
    let (outcome, says) = run(
        &mut s,
        &alice,
        "data modify storage t:c n set compute default integer 7",
    );
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    let (_, says) = run(&mut s, &alice, "data get storage t:c n");
    assert!(says[0].contains('7'), "{says:?}");
}

#[test]
fn w27_5_execute_if_slots_and_on_relations() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "give Alice minecraft:diamond 2");
    let (outcome, says) = run(&mut s, &alice, "execute if slots entity Alice container.*");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says[0].contains("36"), "{says:?}");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute if slots entity @e[type=pig,limit=1] container.*",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    run(&mut s, &alice, "summon pig ~ ~ ~");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute as @e[type=pig] on target run say targeted",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute as @e[type=pig] on attacker run say hit",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    let bits = alice.id.0.as_u128();
    let owner = format!(
        "[I;{},{},{},{}]",
        (bits >> 96) as u32 as i32,
        (bits >> 64) as u32 as i32,
        (bits >> 32) as u32 as i32,
        bits as u32 as i32
    );
    let (outcome, says) = run(
        &mut s,
        &alice,
        &format!("summon minecraft:arrow ~ ~3 ~ {{Owner:{owner}}}"),
    );
    assert_eq!(outcome, CommandOutcome::Success, "{says:?}");
    let (outcome, _) = run(
        &mut s,
        &alice,
        "execute as @e[type=arrow,limit=1] on origin run tag @s add owner_seen",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let (outcome, _) = run(&mut s, &alice, "execute if entity @a[tag=owner_seen]");
    assert_eq!(outcome, CommandOutcome::Success);
}
