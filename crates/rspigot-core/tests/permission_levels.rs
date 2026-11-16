mod common;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};

fn levels(player: &Player) -> Vec<u8> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::OperatorLevel { level, .. } => Some(level),
            _ => None,
        })
        .collect()
}

fn say(
    server: &mut rspigot_core::server::Server<Link>,
    player: &Player,
    text: &str,
) -> Vec<String> {
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Command(text.to_owned()),
    });
    server.deliver();
    texts(player).into_iter().map(|(text, _)| text).collect()
}

fn with_levels() -> rspigot_core::server::Server<Link> {
    configured(|config| {
        config.admin = AdminLists {
            operators: vec![
                ListEntry::named("Mod").operator(2),
                ListEntry::named("Owner"),
            ],
            ..AdminLists::default()
        };
    })
}

#[test]
fn the_client_receives_the_level_stored_for_the_operator() {
    let mut server = with_levels();
    let moderator = arrive(&mut server, "Mod");
    let owner = arrive(&mut server, "Owner");
    let plain = arrive(&mut server, "Plain");
    assert_eq!(levels(&moderator).last(), Some(&2));
    assert_eq!(levels(&owner).last(), Some(&4));
    assert_eq!(levels(&plain).last(), Some(&0));
}

#[test]
fn a_gamemaster_cannot_use_admin_commands() {
    let mut server = with_levels();
    let moderator = arrive(&mut server, "Mod");
    moderator.link.take();
    let refused = say(&mut server, &moderator, "ban Plain");
    assert_eq!(refused[0], "command.unknown.command");
    let allowed = say(&mut server, &moderator, "say hello");
    assert!(allowed.iter().all(|line| line != "command.unknown.command"));
}

#[test]
fn op_uses_the_configured_level() {
    let mut server = configured(|config| config.op_permission_level = 3);
    let target = arrive(&mut server, "Plain");
    target.link.take();
    rspigot_core::commands::execute(
        &mut server,
        rspigot_core::commands::CommandSender::Console,
        "op Plain",
    );
    server.deliver();
    assert_eq!(levels(&target).last(), Some(&3));
}
