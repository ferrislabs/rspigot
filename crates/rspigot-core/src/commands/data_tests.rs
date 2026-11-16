use rspigot_domain::player::PlayerId;

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn merge_and_get_round_trip_through_storage() {
    let mut rig = rig();
    assert_eq!(
        rig.run("data merge storage test:s {a:1,b:{c:2}}"),
        CommandOutcome::Success
    );
    assert_eq!(rig.texts(), ["commands.data.storage.modified test:s"]);
    assert_eq!(
        rig.run("data get storage test:s b.c"),
        CommandOutcome::Success
    );
    assert_eq!(rig.texts(), ["commands.data.storage.query test:s 2"]);
    assert_eq!(
        rig.run("data get storage test:s b.c 10"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.data.storage.get b.c test:s 10.00 20"]
    );
    assert_eq!(
        rig.run("data merge storage test:s {a:1}"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.data.merge.failed"]);
    assert_eq!(
        rig.run("data get storage test:s zzz"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.data.get.unknown zzz"]);
}

#[test]
fn modify_covers_every_operation() {
    let mut rig = rig();
    rig.run("data merge storage test:s {l:[1,2],o:{a:1}}");
    assert_eq!(
        rig.run("data modify storage test:s l append value 3"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data modify storage test:s l prepend value 0"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data modify storage test:s l insert 2 value 9"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data modify storage test:s o merge value {b:2}"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data modify storage test:s n set value 5"),
        CommandOutcome::Success
    );
    let stored =
        rig.server.command_storage[&rspigot_domain::Identifier::parse("test:s").unwrap()].clone();
    assert_eq!(
        rspigot_nbt::Tag::Compound(stored).to_string(),
        "{l:[0,1,9,2,3],o:{a:1,b:2},n:5}"
    );
    rig.texts();
    assert_eq!(
        rig.run("data modify storage test:s n append value 1"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.data.modify.expected_list"]);
    assert_eq!(
        rig.run("data modify storage test:s l insert 99 value 1"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.data.modify.invalid_index"]);
}

#[test]
fn modify_reads_from_other_targets_and_strings() {
    let mut rig = rig();
    rig.run("data merge storage test:a {name:\"abcdef\",n:7}");
    assert_eq!(
        rig.run("data modify storage test:b copy set from storage test:a n"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data modify storage test:b part set string storage test:a name 1 3"),
        CommandOutcome::Success
    );
    let stored =
        rig.server.command_storage[&rspigot_domain::Identifier::parse("test:b").unwrap()].clone();
    assert_eq!(
        rspigot_nbt::Tag::Compound(stored).to_string(),
        "{copy:7,part:\"bc\"}"
    );
}

#[test]
fn remove_deletes_and_reports() {
    let mut rig = rig();
    rig.run("data merge storage test:s {a:1,b:2}");
    assert_eq!(
        rig.run("data remove storage test:s a"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data remove storage test:s a"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("data get storage test:s a"),
        CommandOutcome::Failure
    );
}

#[test]
fn entity_data_can_be_read_but_not_rewritten() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(
        rig.run("data get entity @s playerGameType"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("data merge entity @s {playerGameType:1}"),
        CommandOutcome::Failure
    );
    let shown = rig.texts();
    assert!(shown[0].starts_with("commands.data.entity.query"));
    assert_eq!(shown[1], "commands.data.entity.invalid");
}

#[test]
fn block_data_follows_the_block_entity() {
    let mut rig = rig();
    assert_eq!(rig.run("data get block ~ ~ ~"), CommandOutcome::Failure);
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("commands.data.block.invalid")
    );
}

#[test]
fn data_inside_execute_store_and_if() {
    let mut rig = rig();
    rig.run("data merge storage test:s {v:3}");
    assert_eq!(
        rig.run(
            "execute if data storage test:s {v:3} run data modify storage test:s w set value 1b"
        ),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("execute if data storage test:s w"),
        CommandOutcome::Success
    );
}
