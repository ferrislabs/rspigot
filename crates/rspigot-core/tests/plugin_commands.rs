mod common;

use common::*;
use rspigot_core::commands::{ArgumentParser, CommandNodeKind, CommandTree};
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::permissions::Permissions;
use rspigot_core::plugins::events::{
    EntityDamageCause, PlayerSpawnChangeCause, PlayerTeleportCause, TabCompletePatch,
};
use rspigot_core::plugins::{
    ArgumentKind, ArgumentValue, CommandArgument, CommandSpec, EventKind, EventPatch, EventPayload,
    HandlerSpec, PlayerOrder, PluginIntro, ToPlugin,
};
use rspigot_core::server::Server;
use rspigot_domain::Vec3;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn physics_blocks(s: &Scene) -> Vec<(i32, i32, i32)> {
    settled_for(&s.plugin, EventKind::BlockPhysics)
        .into_iter()
        .filter_map(|fired| match fired.payload {
            EventPayload::BlockPhysics(event) => {
                Some((event.block.x, event.block.y, event.block.z))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn setblock_asks_physics_at_the_changed_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "setblock 2 64 0 minecraft:stone");
    assert!(physics_blocks(&s).contains(&(2, 64, 0)));
}

#[test]
fn fill_asks_physics_once_per_changed_block() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "fill 2 64 0 3 64 0 minecraft:stone");
    let heard = physics_blocks(&s);
    assert_eq!(heard.iter().filter(|b| **b == (2, 64, 0)).count(), 1);
    assert_eq!(heard.iter().filter(|b| **b == (3, 64, 0)).count(), 1);
}

#[test]
fn clone_asks_physics_at_the_destination() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BlockPhysics)]);
    console(&mut s, "setblock 2 64 0 minecraft:stone");
    console(&mut s, "clone 2 64 0 2 64 0 6 64 0");
    assert!(physics_blocks(&s).contains(&(6, 64, 0)));
}

#[test]
fn spawnpoint_fires_a_command_spawn_change_with_the_new_spawn() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let events = settled_for(&s.plugin, EventKind::PlayerSpawnChange);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerSpawnChange(event) = &events[0].payload else {
        panic!("expected a spawn change");
    };
    assert_eq!(event.cause, PlayerSpawnChangeCause::Command);
    assert!(event.forced);
    let spawn = event.new_spawn.as_ref().expect("a new spawn");
    assert_eq!(spawn.position, Vec3::new(5.0, 64.0, 7.0));
}

fn respawn(s: &mut Scene) -> Vec3 {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kill));
    s.tick(2);
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    s.server.deliver();
    alice
        .link
        .take()
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::Teleport { position, .. } => Some(*position),
            _ => None,
        })
        .expect("a respawn teleport")
}

#[test]
fn a_cancelled_spawn_change_keeps_the_world_spawn_for_the_respawn() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let asked = last_asked(&s.plugin, EventKind::PlayerSpawnChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(0.5, 64.0, 0.5)) < 1.0e-6);
}

#[test]
fn an_allowed_spawnpoint_sets_the_respawn_position() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerSpawnChange)]);
    console(&mut s, "spawnpoint Alice 5 64 7");
    let spot = respawn(&mut s);
    assert!(spot.distance_squared(Vec3::new(5.5, 64.1, 7.5)) < 1.0e-6);
}

#[test]
fn damage_hurts_a_player_through_the_damage_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityDamage)]);
    console(&mut s, "damage Alice 3");
    let heard = settled_for(&s.plugin, EventKind::EntityDamage);
    assert_eq!(heard.len(), 1);
    let EventPayload::EntityDamage(event) = &heard[0].payload else {
        panic!("expected a damage event");
    };
    assert_eq!(event.cause, EntityDamageCause::Custom);
    assert_eq!(event.damage, 3.0);
}

fn ride_scene() -> Scene {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityMount),
            observe(1, EventKind::EntityDismount),
        ],
    );
    console(&mut s, "summon minecraft:cow 5 70 5");
    console(&mut s, "summon minecraft:pig 6 70 5");
    s
}

#[test]
fn ride_mount_and_dismount_fire_the_mount_events() {
    let mut s = ride_scene();
    console(&mut s, "ride @e[type=cow] mount @e[type=pig]");
    assert_eq!(settled_for(&s.plugin, EventKind::EntityMount).len(), 1);
    assert!(settled_for(&s.plugin, EventKind::EntityDismount).is_empty());
    console(&mut s, "ride @e[type=cow] dismount");
    assert_eq!(settled_for(&s.plugin, EventKind::EntityDismount).len(), 1);
}

#[test]
fn a_cancelled_mount_keeps_the_rider_off() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityMount)]);
    console(&mut s, "summon minecraft:cow 5 70 5");
    console(&mut s, "summon minecraft:pig 6 70 5");
    console(&mut s, "ride @e[type=cow] mount @e[type=pig]");
    let asked = last_asked(&s.plugin, EventKind::EntityMount);
    rule_on(&mut s.server, 1, asked.id, true, None);
    console(&mut s, "ride @e[type=cow] dismount");
    assert!(settled_for(&s.plugin, EventKind::EntityDismount).is_empty());
}

#[test]
fn a_joining_player_receives_the_world_border() {
    let mut s = scene(&[]);
    let alice = arrive(&mut s.server, "Alice");
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Border(_)))
    );
}

#[test]
fn spreadplayers_teleports_a_player_with_the_command_cause() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerTeleport)]);
    console(&mut s, "spreadplayers 0 0 1 8 under 70 false Alice");
    let heard = settled_for(&s.plugin, EventKind::PlayerTeleport);
    assert_eq!(heard.len(), 1);
    let EventPayload::PlayerTeleport(event) = &heard[0].payload else {
        panic!("expected a teleport");
    };
    assert_eq!(event.cause, PlayerTeleportCause::Command);
}

fn idle_scene(minutes: u32) -> Scene {
    scene_on(
        configured(|config| config.player_idle_timeout = minutes),
        &["Alice"],
        vec![observe(1, EventKind::PlayerKick)],
    )
}

#[test]
fn an_idle_player_is_kicked_after_the_timeout_through_the_kick_event() {
    let mut s = idle_scene(1);
    let alice = s.players[0].clone();
    s.tick(1_195);
    assert!(alice.link.disconnected_with().is_none());
    s.tick(10);
    let reason = alice.link.disconnected_with().expect("kicked");
    assert_eq!(reason.plain_text(), "multiplayer.disconnect.idling");
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerKick).len(), 1);
}

#[test]
fn player_activity_pushes_the_idle_kick_back() {
    let mut s = idle_scene(1);
    let alice = s.players[0].clone();
    s.tick(1_000);
    send_input(&mut s.server, &alice, PlayerInput::Swing(Hand::Main));
    s.tick(1_000);
    assert!(alice.link.disconnected_with().is_none());
    s.tick(300);
    assert!(alice.link.disconnected_with().is_some());
}

#[test]
fn no_idle_timeout_never_kicks() {
    let mut s = idle_scene(0);
    let alice = s.players[0].clone();
    s.tick(2_500);
    assert!(alice.link.disconnected_with().is_none());
}

fn argument(name: &str, kind: ArgumentKind, optional: bool, suggested: bool) -> CommandArgument {
    CommandArgument {
        name: name.to_owned(),
        kind,
        optional,
        suggested,
        choices: Vec::new(),
    }
}

fn kit() -> CommandSpec {
    CommandSpec {
        id: 1,
        name: "kit".to_owned(),
        aliases: Vec::new(),
        permission: Some("kit.use".to_owned()),
        arguments: vec![
            argument("target", ArgumentKind::Player, false, false),
            argument("item", ArgumentKind::Item, false, false),
            argument("count", ArgumentKind::Integer, true, false),
        ],
    }
}

fn warp() -> CommandSpec {
    CommandSpec {
        id: 2,
        name: "warp".to_owned(),
        aliases: Vec::new(),
        permission: None,
        arguments: vec![argument("place", ArgumentKind::Text, false, true)],
    }
}

fn joined(server: &mut Server<Link>, name: &str) -> Player {
    let player = join(server, name);
    server.deliver();
    player
}

fn shop() -> Permissions {
    Permissions::default()
        .with_group("vip", &["kit.use"], &[])
        .with_player("Steve", &["vip"])
}

fn shop_server() -> Server<Link> {
    server(PluginSettings::default()).with_permissions(shop())
}

fn declare(
    server: &mut Server<Link>,
    commands: Vec<CommandSpec>,
    handlers: Vec<HandlerSpec>,
) -> Mailbox {
    let mailbox = Mailbox::default();
    server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            name: "shop".to_owned(),
            version: "1.0.0".to_owned(),
            handlers,
            commands,
        },
        outbox: Box::new(mailbox.clone()),
    });
    server.deliver();
    mailbox
}

fn trees(player: &Player) -> Vec<CommandTree> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Commands(tree) => Some((*tree).clone()),
            _ => None,
        })
        .collect()
}

fn branch<'a>(
    tree: &'a CommandTree,
    name: &str,
) -> Option<&'a rspigot_core::commands::CommandNode> {
    tree.nodes[tree.root]
        .children
        .iter()
        .map(|child| &tree.nodes[*child])
        .find(|node| matches!(&node.kind, CommandNodeKind::Literal(literal) if literal == name))
}

fn chain(tree: &CommandTree, name: &str) -> Vec<(String, ArgumentParser, bool)> {
    let mut found = Vec::new();
    let mut node = branch(tree, name);
    while let Some(current) = node {
        let next = current.children.first().map(|child| &tree.nodes[*child]);
        if let Some(next) = next
            && let CommandNodeKind::Argument { name, parser, .. } = &next.kind
        {
            found.push((name.clone(), parser.clone(), next.executable));
        }
        node = next;
    }
    found
}

#[test]
fn a_plugin_command_appears_in_the_tree_with_typed_arguments() {
    let mut server = shop_server();
    let steve = joined(&mut server, "Steve");
    declare(&mut server, vec![kit()], Vec::new());
    let tree = trees(&steve).pop().expect("a command tree");
    let root = branch(&tree, "kit").expect("the kit command");
    assert!(!root.executable);
    assert_eq!(
        chain(&tree, "kit"),
        vec![
            (
                "target".to_owned(),
                ArgumentParser::Entity {
                    single: true,
                    players_only: true
                },
                false
            ),
            ("item".to_owned(), ArgumentParser::ItemStack, true),
            (
                "count".to_owned(),
                ArgumentParser::Integer {
                    min: None,
                    max: None
                },
                true
            ),
        ]
    );
}

#[test]
fn a_connecting_plugin_reaches_the_players_online_and_leaving_takes_its_commands_back() {
    let mut server = shop_server();
    let steve = joined(&mut server, "Steve");
    assert!(
        trees(&steve)
            .iter()
            .all(|tree| branch(tree, "kit").is_none())
    );
    declare(&mut server, vec![kit()], Vec::new());
    let tree = trees(&steve).pop().expect("a tree after the connection");
    assert!(branch(&tree, "kit").is_some());
    server.handle(Inbound::PluginDisconnected { connection: 1 });
    server.deliver();
    let tree = trees(&steve).pop().expect("a tree after the departure");
    assert!(branch(&tree, "kit").is_none());
}

#[test]
fn a_command_without_the_right_is_hidden_and_refused() {
    let mut server = shop_server();
    let steve = joined(&mut server, "Steve");
    let alex = joined(&mut server, "Alex");
    let plugin = declare(&mut server, vec![kit()], Vec::new());
    assert!(branch(&trees(&alex).pop().expect("a tree"), "kit").is_none());
    send_input(
        &mut server,
        &alex,
        PlayerInput::Command("kit Steve diamond".to_owned()),
    );
    let said = system_texts(&alex);
    assert_eq!(
        said[0],
        "Unknown or incomplete command, see below for error"
    );
    assert!(
        !plugin
            .received()
            .iter()
            .any(|message| matches!(message, ToPlugin::Invocation { .. }))
    );

    send_input(
        &mut server,
        &steve,
        PlayerInput::Command("kit Alex diamond 3".to_owned()),
    );
    let invoked: Vec<_> = plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Invocation {
                command,
                sender,
                arguments,
            } => Some((command, sender.name, arguments)),
            _ => None,
        })
        .collect();
    assert_eq!(invoked.len(), 1);
    let (command, sender, arguments) = &invoked[0];
    assert_eq!((*command, sender.as_str()), (1, "Steve"));
    assert!(matches!(&arguments[0], ArgumentValue::Player(target) if target.name == "Alex"));
    assert_eq!(
        arguments[1],
        ArgumentValue::Item("minecraft:diamond".to_owned())
    );
    assert_eq!(arguments[2], ArgumentValue::Integer(3));
}

#[test]
fn a_bad_argument_is_refused_before_the_plugin_hears_of_it() {
    let mut server = shop_server();
    let plugin = declare(&mut server, vec![kit()], Vec::new());
    let steve = joined(&mut server, "Steve");
    joined(&mut server, "Alex");
    for line in [
        "kit",
        "kit Alex",
        "kit Alex diamond many",
        "kit Alex diamond 1 2",
    ] {
        send_input(&mut server, &steve, PlayerInput::Command(line.to_owned()));
    }
    assert!(
        !plugin
            .received()
            .iter()
            .any(|message| matches!(message, ToPlugin::Invocation { .. }))
    );
}

#[test]
fn a_plugin_command_never_hides_a_vanilla_one() {
    let mut server = shop_server();
    let steve = joined(&mut server, "Steve");
    let plugin = declare(
        &mut server,
        vec![CommandSpec {
            name: "help".to_owned(),
            ..warp()
        }],
        Vec::new(),
    );
    let tree = trees(&steve).pop().expect("a tree");
    let seeds = tree.nodes[tree.root]
        .children
        .iter()
        .filter(|child| matches!(&tree.nodes[**child].kind, CommandNodeKind::Literal(name) if name == "help"))
        .count();
    assert_eq!(seeds, 1);
    assert!(
        plugin
            .received()
            .iter()
            .any(|message| matches!(message, ToPlugin::Warning(text) if text.contains("help")))
    );
}

fn suggestions(player: &Player) -> Vec<Vec<String>> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::CommandSuggestions { matches, .. } => Some(matches),
            _ => None,
        })
        .collect()
}

fn ask_suggestions(server: &mut Server<Link>, player: &Player, text: &str) {
    send_input(
        server,
        player,
        PlayerInput::CommandSuggestion {
            id: 9,
            text: text.to_owned(),
        },
    );
}

#[test]
fn a_silent_plugin_gives_no_suggestions_without_blocking() {
    let mut server = shop_server();
    let plugin = declare(
        &mut server,
        vec![warp()],
        vec![deciding(1, EventKind::TabComplete)],
    );
    let steve = joined(&mut server, "Steve");
    trees(&steve);
    ask_suggestions(&mut server, &steve, "/warp ho");
    assert_eq!(asked_for(&plugin, EventKind::TabComplete).len(), 1);
    for _ in 0..3 {
        server.tick();
        server.deliver();
    }
    assert!(suggestions(&steve).iter().all(Vec::is_empty));
}

#[test]
fn a_plugin_suggestion_is_filtered_by_the_typed_prefix() {
    let mut server = shop_server();
    let plugin = declare(
        &mut server,
        vec![warp()],
        vec![deciding(1, EventKind::TabComplete)],
    );
    let steve = joined(&mut server, "Steve");
    trees(&steve);
    ask_suggestions(&mut server, &steve, "/warp ho");
    let asked = last_asked(&plugin, EventKind::TabComplete);
    let patch = EventPatch::TabComplete(TabCompletePatch {
        completions: replacing(
            &[],
            vec![
                "spawn".to_owned(),
                "home".to_owned(),
                "hub".to_owned(),
                "Hotel".to_owned(),
            ],
        ),
    });
    rule_on(&mut server, 1, asked.id, false, Some(patch));
    assert_eq!(
        suggestions(&steve),
        vec![vec!["Hotel".to_owned(), "home".to_owned()]]
    );
}

#[test]
fn a_denied_player_gets_no_suggestions_from_a_command_it_cannot_use() {
    let mut server = shop_server();
    let guarded = CommandSpec {
        permission: Some("kit.use".to_owned()),
        ..warp()
    };
    let plugin = declare(
        &mut server,
        vec![guarded],
        vec![deciding(1, EventKind::TabComplete)],
    );
    let alex = joined(&mut server, "Alex");
    trees(&alex);
    ask_suggestions(&mut server, &alex, "/warp ho");
    assert!(asked_for(&plugin, EventKind::TabComplete).is_empty());
}
