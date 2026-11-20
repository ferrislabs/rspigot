mod common;

use common::*;
use rspigot_core::commands::{CommandNodeKind, CommandTree};
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::permissions::Permissions;
use rspigot_core::plugins::{
    Answer, CommandSpec, FromPlugin, PlayerOrder, PluginIntro, Question, ToPlugin,
};
use rspigot_core::server::Server;

fn shop() -> CommandSpec {
    CommandSpec {
        id: 1,
        name: "shop".to_owned(),
        aliases: Vec::new(),
        permission: Some("shop.use".to_owned()),
        arguments: Vec::new(),
    }
}

fn permissions() -> Permissions {
    Permissions::default()
        .with_group("default", &[], &[])
        .with_group("vip", &["shop.use"], &[])
        .with_group("staff", &["shop.use", "shop.admin"], &["vip"])
}

struct Market {
    server: Server<Link>,
    plugin: Mailbox,
    steve: Player,
}

fn market() -> Market {
    let mut server = server(PluginSettings::default()).with_permissions(permissions());
    let plugin = Mailbox::default();
    server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            name: "market".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: Vec::new(),
            commands: vec![shop()],
        },
        outbox: Box::new(plugin.clone()),
    });
    let steve = join(&mut server, "Steve");
    server.deliver();
    steve.link.take();
    Market {
        server,
        plugin,
        steve,
    }
}

fn act(market: &mut Market, connection: u64, order: PlayerOrder) {
    let steve = market.steve.clone();
    order_message(
        &mut market.server,
        connection,
        FromPlugin::Order(to(&steve, order)),
    );
}

fn order_message(server: &mut Server<Link>, connection: u64, message: FromPlugin) {
    common::order(server, connection, message);
}

fn grant(node: &str, value: bool) -> PlayerOrder {
    PlayerOrder::SetPermission {
        node: node.to_owned(),
        value,
    }
}

fn invocations(plugin: &Mailbox) -> usize {
    plugin
        .received()
        .iter()
        .filter(|message| matches!(message, ToPlugin::Invocation { .. }))
        .count()
}

fn run_shop(market: &mut Market) {
    send_input(
        &mut market.server,
        &market.steve,
        PlayerInput::Command("shop".to_owned()),
    );
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

fn has_shop(tree: &CommandTree) -> bool {
    tree.nodes[tree.root].children.iter().any(
        |child| matches!(&tree.nodes[*child].kind, CommandNodeKind::Literal(name) if name == "shop"),
    )
}

fn rights(market: &mut Market) -> Vec<String> {
    let plugin = market.plugin.clone();
    let Answer::Player(state) = ask(
        &mut market.server,
        1,
        &plugin,
        Question::Player(market.steve.id),
    ) else {
        panic!("a player state");
    };
    state.player.permissions
}

fn ask(server: &mut Server<Link>, connection: u64, plugin: &Mailbox, question: Question) -> Answer {
    let request = 7_000 + connection;
    order_message(
        server,
        connection,
        FromPlugin::Question { request, question },
    );
    plugin
        .received()
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
        .expect("an answer")
}

#[test]
fn a_plugin_granted_node_lets_a_player_run_a_guarded_command() {
    let mut market = market();
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 0);
    act(&mut market, 1, grant("shop.use", true));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
}

#[test]
fn unsetting_a_node_takes_it_back() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", true));
    act(
        &mut market,
        1,
        PlayerOrder::UnsetPermission("shop.use".to_owned()),
    );
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 0);
}

#[test]
fn a_wildcard_grant_covers_the_nodes_below_it() {
    let mut market = market();
    act(&mut market, 1, grant("shop.*", true));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
}

#[test]
fn a_negation_beats_the_group_right() {
    let mut market = market();
    act(&mut market, 1, PlayerOrder::AddGroup("vip".to_owned()));
    act(&mut market, 1, grant("shop.use", false));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 0);
}

#[test]
fn setting_a_node_again_replaces_the_earlier_value() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", false));
    act(&mut market, 1, grant("shop.use", true));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
}

#[test]
fn the_overlay_is_in_the_snapshot_the_plugin_reads() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", true));
    act(&mut market, 1, grant("shop.ban", false));
    let nodes = rights(&mut market);
    assert!(nodes.contains(&"shop.use".to_owned()));
    assert!(nodes.contains(&"-shop.ban".to_owned()));
}

#[test]
fn a_departing_plugin_takes_its_grants_back_and_the_tree_is_resent() {
    let mut market = market();
    let other = connect(&mut market.server, 2, "other");
    act(&mut market, 1, grant("shop.use", true));
    market.steve.link.take();
    market
        .server
        .handle(Inbound::PluginDisconnected { connection: 1 });
    market.server.deliver();
    let tree = trees(&market.steve).pop().expect("a tree");
    assert!(!has_shop(&tree));
    let Answer::Player(state) = ask(
        &mut market.server,
        2,
        &other,
        Question::Player(market.steve.id),
    ) else {
        panic!("a player state");
    };
    assert!(!state.player.permissions.contains(&"shop.use".to_owned()));
}

#[test]
fn a_grant_belongs_to_the_plugin_that_made_it() {
    let mut market = market();
    connect(&mut market.server, 2, "other");
    act(&mut market, 1, grant("shop.use", true));
    act(
        &mut market,
        2,
        PlayerOrder::UnsetPermission("shop.use".to_owned()),
    );
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
}

#[test]
fn a_player_who_leaves_loses_the_grants() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", true));
    market.server.handle(leave_of(&market.steve));
    market.server.deliver();
    let steve = join(&mut market.server, "Steve");
    market.server.deliver();
    market.steve = steve;
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 0);
}

#[test]
fn a_group_added_by_a_plugin_gives_its_rights_and_removing_it_takes_them_back() {
    let mut market = market();
    act(&mut market, 1, PlayerOrder::AddGroup("staff".to_owned()));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
    let nodes = rights(&mut market);
    assert!(nodes.contains(&"shop.admin".to_owned()));
    act(&mut market, 1, PlayerOrder::RemoveGroup("staff".to_owned()));
    run_shop(&mut market);
    assert_eq!(invocations(&market.plugin), 1);
}

#[test]
fn an_unknown_group_is_a_warning() {
    let mut market = market();
    act(&mut market, 1, PlayerOrder::AddGroup("ghost".to_owned()));
    assert!(
        market
            .plugin
            .received()
            .iter()
            .any(|message| matches!(message, ToPlugin::Warning(text) if text.contains("ghost")))
    );
}

#[test]
fn a_change_resends_the_tree_and_refresh_resends_it_again() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", true));
    let sent = trees(&market.steve);
    assert!(sent.last().is_some_and(has_shop));
    act(&mut market, 1, PlayerOrder::RefreshCommands);
    assert_eq!(trees(&market.steve).len(), 1);
}

#[test]
fn the_groups_of_permissions_yml_are_listed() {
    let mut market = market();
    let plugin = market.plugin.clone();
    let Answer::PermissionGroups(groups) =
        ask(&mut market.server, 1, &plugin, Question::PermissionGroups)
    else {
        panic!("a list of groups");
    };
    let names: Vec<&str> = groups.iter().map(|group| group.name.as_str()).collect();
    assert_eq!(names, ["default", "staff", "vip"]);
    let staff = groups.iter().find(|group| group.name == "staff").unwrap();
    assert_eq!(staff.permissions, ["shop.use", "shop.admin"]);
    assert_eq!(staff.inherits, ["vip"]);
}

#[test]
fn a_plugin_that_reconnects_under_its_name_starts_without_the_old_grants() {
    let mut market = market();
    act(&mut market, 1, grant("shop.use", true));
    let fresh = Mailbox::default();
    market.server.handle(Inbound::PluginConnected {
        connection: 2,
        intro: PluginIntro {
            name: "market".to_owned(),
            version: "1.0.1".to_owned(),
            handlers: Vec::new(),
            commands: vec![shop()],
        },
        outbox: Box::new(fresh.clone()),
    });
    market.server.deliver();
    send_input(
        &mut market.server,
        &market.steve,
        PlayerInput::Command("shop".to_owned()),
    );
    assert_eq!(invocations(&fresh), 0);
}
