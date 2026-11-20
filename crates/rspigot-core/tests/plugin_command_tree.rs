mod common;

use common::*;
use rspigot_core::commands::{ArgumentParser, CommandNode, CommandNodeKind, CommandTree};
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::permissions::Permissions;
use rspigot_core::plugins::{
    Answer, ArgumentKind, ArgumentValue, CommandArgument, CommandSpec, FromPlugin, PlayerOrder,
    PluginIntro, Question, ToPlugin,
};
use rspigot_core::server::Server;
use rspigot_domain::Vec3;

fn arg(name: &str, kind: ArgumentKind) -> CommandArgument {
    CommandArgument {
        name: name.to_owned(),
        kind,
        optional: false,
        suggested: false,
        choices: Vec::new(),
    }
}

fn optional(name: &str, kind: ArgumentKind) -> CommandArgument {
    CommandArgument {
        optional: true,
        ..arg(name, kind)
    }
}

fn choice(name: &str, choices: &[&str]) -> CommandArgument {
    CommandArgument {
        choices: choices.iter().map(|c| (*c).to_owned()).collect(),
        ..arg(name, ArgumentKind::Choice)
    }
}

fn spec(
    id: u32,
    name: &str,
    permission: Option<&str>,
    arguments: Vec<CommandArgument>,
) -> CommandSpec {
    CommandSpec {
        id,
        name: name.to_owned(),
        aliases: Vec::new(),
        permission: permission.map(str::to_owned),
        arguments,
    }
}

fn shop_commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            aliases: vec!["s".to_owned()],
            ..spec(1, "shop", None, Vec::new())
        },
        spec(
            2,
            "shop",
            Some("shop.buy"),
            vec![
                arg("buy", ArgumentKind::Literal),
                arg("item", ArgumentKind::Item),
                arg("amount", ArgumentKind::Integer),
            ],
        ),
        spec(3, "shop", None, vec![arg("list", ArgumentKind::Literal)]),
    ]
}

struct Market {
    server: Server<Link>,
    plugin: Mailbox,
}

fn market(commands: Vec<CommandSpec>) -> Market {
    let mut server = server(PluginSettings::default()).with_permissions(
        Permissions::default()
            .with_group("buyer", &["shop.buy"], &[])
            .with_player("Steve", &["buyer"]),
    );
    let plugin = Mailbox::default();
    server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            name: "market".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: Vec::new(),
            commands,
        },
        outbox: Box::new(plugin.clone()),
    });
    server.deliver();
    Market { server, plugin }
}

fn joined(market: &mut Market, name: &str) -> Player {
    let player = join(&mut market.server, name);
    market.server.deliver();
    order_to(
        market,
        FromPlugin::Order(to(&player, PlayerOrder::RefreshCommands)),
    );
    player
}

fn last_tree(player: &Player) -> CommandTree {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Commands(tree) => Some((*tree).clone()),
            _ => None,
        })
        .next_back()
        .expect("a command tree")
}

fn child<'a>(tree: &'a CommandTree, parent: &CommandNode, name: &str) -> Option<&'a CommandNode> {
    parent
        .children
        .iter()
        .map(|index| &tree.nodes[*index])
        .find(|node| match &node.kind {
            CommandNodeKind::Literal(literal) => literal == name,
            CommandNodeKind::Argument { name: argument, .. } => argument == name,
            CommandNodeKind::Root => false,
        })
}

fn root<'a>(tree: &'a CommandTree, name: &str) -> Option<&'a CommandNode> {
    child(tree, &tree.nodes[tree.root], name)
}

fn parser_of(node: &CommandNode) -> ArgumentParser {
    match &node.kind {
        CommandNodeKind::Argument { parser, .. } => parser.clone(),
        other => panic!("not an argument: {other:?}"),
    }
}

fn run(market: &mut Market, player: &Player, line: &str) {
    send_input(
        &mut market.server,
        player,
        PlayerInput::Command(line.to_owned()),
    );
}

fn invoked(plugin: &Mailbox) -> Vec<(u32, Vec<ArgumentValue>)> {
    plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Invocation {
                command, arguments, ..
            } => Some((command, arguments)),
            _ => None,
        })
        .collect()
}

#[test]
fn sub_commands_share_one_root_with_their_own_literals_and_arguments() {
    let mut market = market(shop_commands());
    let steve = joined(&mut market, "Steve");
    let tree = last_tree(&steve);
    let shop = root(&tree, "shop").expect("the shop command");
    assert!(shop.executable);
    let buy = child(&tree, shop, "buy").expect("the buy literal");
    assert!(matches!(&buy.kind, CommandNodeKind::Literal(_)));
    assert!(!buy.executable);
    let item = child(&tree, buy, "item").expect("the item argument");
    assert_eq!(parser_of(item), ArgumentParser::ItemStack);
    let amount = child(&tree, item, "amount").expect("the amount argument");
    assert!(amount.executable);
    let list = child(&tree, shop, "list").expect("the list literal");
    assert!(list.executable);
}

#[test]
fn a_sub_command_without_the_right_is_hidden_and_refused() {
    let mut market = market(shop_commands());
    let alex = joined(&mut market, "Alex");
    let tree = last_tree(&alex);
    let shop = root(&tree, "shop").expect("the shop command");
    assert!(child(&tree, shop, "buy").is_none());
    assert!(child(&tree, shop, "list").is_some());
    run(&mut market, &alex, "shop buy diamond 2");
    assert!(invoked(&market.plugin).is_empty());
    run(&mut market, &alex, "shop list");
    assert_eq!(invoked(&market.plugin), vec![(3, Vec::new())]);
}

#[test]
fn a_line_runs_the_sub_command_it_matches_without_the_literals_as_values() {
    let mut market = market(shop_commands());
    let steve = joined(&mut market, "Steve");
    run(&mut market, &steve, "shop buy diamond 2");
    run(&mut market, &steve, "shop");
    run(&mut market, &steve, "shop buy diamond");
    run(&mut market, &steve, "shop sell diamond 1");
    assert_eq!(
        invoked(&market.plugin),
        vec![
            (
                2,
                vec![
                    ArgumentValue::Item("minecraft:diamond".to_owned()),
                    ArgumentValue::Integer(2)
                ]
            ),
            (1, Vec::new()),
        ]
    );
}

#[test]
fn an_alias_is_a_redirect_to_the_command_and_runs_it() {
    let mut market = market(shop_commands());
    let steve = joined(&mut market, "Steve");
    let tree = last_tree(&steve);
    let alias = root(&tree, "s").expect("the alias");
    let target = tree
        .nodes
        .iter()
        .position(|node| std::ptr::eq(node, root(&tree, "shop").unwrap()))
        .unwrap();
    assert_eq!(alias.redirect, Some(target));
    run(&mut market, &steve, "s list");
    run(&mut market, &steve, "S");
    assert_eq!(invoked(&market.plugin).len(), 2);
}

#[test]
fn an_alias_never_hides_a_vanilla_command() {
    let mut market = market(vec![CommandSpec {
        aliases: vec!["help".to_owned()],
        ..spec(1, "shop", None, Vec::new())
    }]);
    let steve = joined(&mut market, "Steve");
    let tree = last_tree(&steve);
    let helps = tree.nodes[tree.root]
        .children
        .iter()
        .filter(|c| matches!(&tree.nodes[**c].kind, CommandNodeKind::Literal(n) if n == "help"))
        .count();
    assert_eq!(helps, 1);
    assert!(root(&tree, "help").unwrap().redirect.is_none());
}

#[test]
fn new_argument_kinds_use_the_client_parsers() {
    let mut market = market(vec![
        spec(
            1,
            "probe",
            None,
            vec![arg("text", ArgumentKind::GreedyText)],
        ),
        spec(
            2,
            "probe",
            None,
            vec![
                arg("one", ArgumentKind::Double),
                arg("two", ArgumentKind::Float),
                arg("who", ArgumentKind::Entities),
                arg("where", ArgumentKind::Location),
                choice("mode", &["fast", "slow"]),
            ],
        ),
    ]);
    let steve = joined(&mut market, "Steve");
    let tree = last_tree(&steve);
    let probe = root(&tree, "probe").unwrap();
    let text = child(&tree, probe, "text").unwrap();
    assert_eq!(parser_of(text), ArgumentParser::GreedyString);
    let one = child(&tree, probe, "one").unwrap();
    assert_eq!(
        parser_of(one),
        ArgumentParser::Double {
            min: None,
            max: None
        }
    );
    let two = child(&tree, one, "two").unwrap();
    assert_eq!(
        parser_of(two),
        ArgumentParser::Float {
            min: None,
            max: None
        }
    );
    let who = child(&tree, two, "who").unwrap();
    assert_eq!(
        parser_of(who),
        ArgumentParser::Entity {
            single: false,
            players_only: false
        }
    );
    let place = child(&tree, who, "where").unwrap();
    assert_eq!(parser_of(place), ArgumentParser::Vec3);
    let mode = child(&tree, place, "mode").unwrap();
    assert_eq!(parser_of(mode), ArgumentParser::Word);
    assert!(mode.executable);
    assert!(matches!(
        &mode.kind,
        CommandNodeKind::Argument {
            suggests: Some(_),
            ..
        }
    ));
}

#[test]
fn greedy_text_takes_the_rest_of_the_line() {
    let mut market = market(vec![spec(
        1,
        "say2",
        None,
        vec![
            arg("who", ArgumentKind::Text),
            arg("text", ArgumentKind::GreedyText),
        ],
    )]);
    let steve = joined(&mut market, "Steve");
    run(&mut market, &steve, "say2 bob hello  big world ");
    run(&mut market, &steve, "say2 bob");
    assert_eq!(
        invoked(&market.plugin),
        vec![(
            1,
            vec![
                ArgumentValue::Text("bob".to_owned()),
                ArgumentValue::Text("hello  big world".to_owned())
            ]
        )]
    );
}

#[test]
fn decimals_and_choices_are_checked_before_the_plugin_hears_of_them() {
    let mut market = market(vec![spec(
        1,
        "probe",
        None,
        vec![
            arg("one", ArgumentKind::Double),
            arg("two", ArgumentKind::Float),
            choice("mode", &["fast", "slow"]),
        ],
    )]);
    let steve = joined(&mut market, "Steve");
    run(&mut market, &steve, "probe 1.5 -2 fast");
    run(&mut market, &steve, "probe x 1 fast");
    run(&mut market, &steve, "probe 1 1 medium");
    run(&mut market, &steve, "probe 1 NaN fast");
    assert_eq!(
        invoked(&market.plugin),
        vec![(
            1,
            vec![
                ArgumentValue::Decimal(1.5),
                ArgumentValue::Decimal(-2.0),
                ArgumentValue::Text("fast".to_owned())
            ]
        )]
    );
}

#[test]
fn a_location_resolves_tildes_against_the_sender() {
    let mut market = market(vec![spec(
        1,
        "go",
        None,
        vec![
            arg("where", ArgumentKind::Location),
            optional("after", ArgumentKind::Integer),
        ],
    )]);
    let steve = joined(&mut market, "Steve");
    market.server.deliver();
    steve.link.take();
    run(&mut market, &steve, "go 1 2.5 -3");
    run(&mut market, &steve, "go ~ ~2 ~-1 7");
    run(&mut market, &steve, "go 1 2");
    run(&mut market, &steve, "go ~ a 3");
    let seen = invoked(&market.plugin);
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].1,
        vec![ArgumentValue::Location(Vec3::new(1.5, 2.5, -2.5))]
    );
    let ArgumentValue::Location(spot) = seen[1].1[0] else {
        panic!("a location");
    };
    let here = standing(&mut market, &steve);
    assert_eq!(spot, Vec3::new(here.x, here.y + 2.0, here.z - 1.0));
    assert_eq!(seen[1].1[1], ArgumentValue::Integer(7));
}

#[test]
fn a_selector_resolves_to_the_entities_it_names() {
    let mut market = market(vec![spec(
        1,
        "zap",
        None,
        vec![arg("who", ArgumentKind::Entities)],
    )]);
    let steve = joined(&mut market, "Steve");
    joined(&mut market, "Alex");
    run(&mut market, &steve, "zap @a");
    run(&mut market, &steve, "zap Alex");
    run(&mut market, &steve, "zap @s");
    run(&mut market, &steve, "zap @e[type=minecraft:cow]");
    let seen = invoked(&market.plugin);
    let names = |index: usize| -> Vec<String> {
        let ArgumentValue::Entities(views) = &seen[index].1[0] else {
            panic!("entities");
        };
        let mut names: Vec<String> = views
            .iter()
            .filter_map(|view| view.player.as_ref().map(|p| p.name.clone()))
            .collect();
        names.sort();
        names
    };
    assert_eq!(seen.len(), 3);
    assert_eq!(names(0), ["Alex", "Steve"]);
    assert_eq!(names(1), ["Alex"]);
    assert_eq!(names(2), ["Steve"]);
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

#[test]
fn a_choice_argument_suggests_its_values_filtered_by_the_typed_prefix() {
    let mut market = market(vec![spec(
        1,
        "probe",
        None,
        vec![
            arg("n", ArgumentKind::Integer),
            choice("mode", &["fast", "slow", "steady"]),
        ],
    )]);
    let steve = joined(&mut market, "Steve");
    steve.link.take();
    send_input(
        &mut market.server,
        &steve,
        PlayerInput::CommandSuggestion {
            id: 4,
            text: "/probe 3 s".to_owned(),
        },
    );
    assert_eq!(
        suggestions(&steve),
        vec![vec!["slow".to_owned(), "steady".to_owned()]]
    );
}

#[test]
fn a_registered_command_reaches_every_player_and_runs() {
    let mut market = market(Vec::new());
    let steve = joined(&mut market, "Steve");
    steve.link.take();
    order_to(
        &mut market,
        FromPlugin::RegisterCommand(spec(
            9,
            "late",
            None,
            vec![optional("n", ArgumentKind::Integer)],
        )),
    );
    assert!(root(&last_tree(&steve), "late").is_some());
    run(&mut market, &steve, "late 4");
    assert_eq!(
        invoked(&market.plugin),
        vec![(9, vec![ArgumentValue::Integer(4)])]
    );
    order_to(
        &mut market,
        FromPlugin::UnregisterCommand("late".to_owned()),
    );
    assert!(root(&last_tree(&steve), "late").is_none());
    run(&mut market, &steve, "late 5");
    assert_eq!(invoked(&market.plugin).len(), 1);
}

fn standing(market: &mut Market, player: &Player) -> Vec3 {
    let request = 8_000;
    order_to(
        market,
        FromPlugin::Question {
            request,
            question: Question::Player(player.id),
        },
    );
    market
        .plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer: Answer::Player(state),
            } if answered == request => Some(state.player.position),
            _ => None,
        })
        .expect("a player state")
}

fn order_to(market: &mut Market, message: FromPlugin) {
    common::order(&mut market.server, 1, message);
}

#[test]
fn a_registered_command_with_a_known_name_or_a_bad_shape_is_refused_with_a_warning() {
    let mut market = market(shop_commands());
    for bad in [
        spec(9, "help", None, Vec::new()),
        spec(2, "other", None, Vec::new()),
        spec(10, "", None, Vec::new()),
        spec(
            11,
            "greedy",
            None,
            vec![
                arg("t", ArgumentKind::GreedyText),
                arg("u", ArgumentKind::Text),
            ],
        ),
        spec(12, "empty", None, vec![choice("c", &[])]),
    ] {
        order_to(&mut market, FromPlugin::RegisterCommand(bad));
    }
    let warnings = market
        .plugin
        .received()
        .into_iter()
        .filter(|m| matches!(m, ToPlugin::Warning(_)))
        .count();
    assert_eq!(warnings, 5);
}

#[test]
fn a_departing_plugin_takes_its_registered_commands_back() {
    let mut market = market(Vec::new());
    let steve = joined(&mut market, "Steve");
    order_to(
        &mut market,
        FromPlugin::RegisterCommand(spec(9, "late", None, Vec::new())),
    );
    steve.link.take();
    market
        .server
        .handle(Inbound::PluginDisconnected { connection: 1 });
    market.server.deliver();
    assert!(root(&last_tree(&steve), "late").is_none());
}

#[test]
fn a_second_plugin_cannot_extend_a_command_of_the_first() {
    let mut market = market(shop_commands());
    let other = Mailbox::default();
    market.server.handle(Inbound::PluginConnected {
        connection: 2,
        intro: PluginIntro {
            name: "rival".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: Vec::new(),
            commands: vec![spec(
                1,
                "shop",
                None,
                vec![arg("steal", ArgumentKind::Literal)],
            )],
        },
        outbox: Box::new(other.clone()),
    });
    market.server.deliver();
    let steve = joined(&mut market, "Steve");
    let tree = last_tree(&steve);
    assert!(child(&tree, root(&tree, "shop").unwrap(), "steal").is_none());
    assert!(
        other
            .received()
            .iter()
            .any(|m| matches!(m, ToPlugin::Warning(text) if text.contains("shop")))
    );
}
