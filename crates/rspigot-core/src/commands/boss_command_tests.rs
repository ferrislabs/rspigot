use rspigot_domain::Identifier;
use rspigot_domain::boss_event::{BossColor, BossOverlay, BossUpdate};
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
    link.take();
    Rig { server, id, link }
}

fn key() -> Identifier {
    Identifier::parse("test:bar").unwrap()
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn ok(&mut self, line: &str) {
        assert_eq!(self.run(line), CommandOutcome::Success, "{line}");
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

    fn updates(&self) -> Vec<BossUpdate> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::BossEvent { update, .. } => Some(update),
                _ => None,
            })
            .collect()
    }

    fn bar(&self) -> rspigot_domain::custom_boss::CustomBossBar {
        self.server.boss_bar_view(&key()).expect("bar")
    }
}

#[test]
fn add_creates_a_bar_and_refuses_duplicates() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    assert_eq!(rig.bar().event().name().plain_text(), "Boss");
    assert_eq!(rig.texts().len(), 1);
    assert_eq!(
        rig.run("bossbar add test:bar \"Other\""),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.bossbar.create.failed test:bar"]);
}

#[test]
fn unknown_bars_are_reported_by_get_set_and_remove() {
    let mut rig = rig();
    for line in [
        "bossbar get test:bar value",
        "bossbar set test:bar value 3",
        "bossbar remove test:bar",
    ] {
        assert_eq!(rig.run(line), CommandOutcome::Failure, "{line}");
        assert_eq!(rig.texts(), ["commands.bossbar.unknown test:bar"]);
    }
}

#[test]
fn list_reports_none_then_the_bars() {
    let mut rig = rig();
    rig.ok("bossbar list");
    assert_eq!(rig.texts(), ["commands.bossbar.list.bars.none"]);
    rig.ok("bossbar add test:bar \"Boss\"");
    rig.texts();
    rig.ok("bossbar list");
    let shown = rig.texts();
    assert!(shown[0].starts_with("commands.bossbar.list.bars.some 1"));
}

#[test]
fn set_changes_the_bar_and_rejects_no_op_changes() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    rig.ok("bossbar set test:bar color red");
    rig.ok("bossbar set test:bar style notched_10");
    rig.ok("bossbar set test:bar max 20");
    rig.ok("bossbar set test:bar value 5");
    rig.ok("bossbar set test:bar visible false");
    rig.ok("bossbar set test:bar name \"Renamed\"");
    let bar = rig.bar();
    assert_eq!(bar.event().color(), BossColor::Red);
    assert_eq!(bar.event().overlay(), BossOverlay::Notched10);
    assert_eq!((bar.value(), bar.max()), (5, 20));
    assert_eq!(bar.event().progress(), 0.25);
    assert!(!bar.event().is_visible());
    assert_eq!(bar.event().name().plain_text(), "Renamed");
    rig.texts();
    for (line, key) in [
        ("bossbar set test:bar color red", "color"),
        ("bossbar set test:bar style notched_10", "style"),
        ("bossbar set test:bar max 20", "max"),
        ("bossbar set test:bar value 5", "value"),
        ("bossbar set test:bar name \"Renamed\"", "name"),
    ] {
        assert_eq!(rig.run(line), CommandOutcome::Failure, "{line}");
        assert_eq!(
            rig.texts(),
            [format!("commands.bossbar.set.{key}.unchanged")]
        );
    }
    assert_eq!(
        rig.run("bossbar set test:bar visible false"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.bossbar.set.visibility.unchanged.hidden"]
    );
}

#[test]
fn value_and_max_have_vanilla_lower_bounds() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    assert_eq!(
        rig.run("bossbar set test:bar value -1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("bossbar set test:bar max 0"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.bar().max(), 100);
}

#[test]
fn players_sets_the_audience_and_sends_the_packets() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    rig.ok("bossbar set test:bar players @a");
    assert!(matches!(rig.updates()[0], BossUpdate::Add { .. }));
    assert!(rig.bar().event().has_player(rig.id));
    assert_eq!(
        rig.run("bossbar set test:bar players @a"),
        CommandOutcome::Failure
    );
    rig.texts();
    rig.ok("bossbar set test:bar value 50");
    assert_eq!(rig.updates(), vec![BossUpdate::Progress(0.5)]);
    rig.ok("bossbar set test:bar players");
    assert_eq!(rig.updates(), vec![BossUpdate::Remove]);
    assert_eq!(rig.bar().event().players().count(), 0);
}

#[test]
fn get_reports_the_state_and_remove_hides_the_bar() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    rig.ok("bossbar set test:bar players @a");
    rig.ok("bossbar set test:bar max 40");
    rig.texts();
    rig.updates();
    rig.ok("bossbar get test:bar max");
    assert!(rig.texts()[0].ends_with(" 40"));
    rig.ok("bossbar get test:bar visible");
    assert!(rig.texts()[0].starts_with("commands.bossbar.get.visible.visible"));
    rig.ok("bossbar get test:bar players");
    assert!(rig.texts()[0].starts_with("commands.bossbar.get.players.some"));
    rig.ok("bossbar remove test:bar");
    assert_eq!(rig.updates(), vec![BossUpdate::Remove]);
    assert!(rig.server.boss_bar_view(&key()).is_none());
}

#[test]
fn execute_store_writes_the_value_and_the_max() {
    let mut rig = rig();
    rig.ok("bossbar add test:bar \"Boss\"");
    rig.ok("execute store result bossbar test:bar value run time set 7");
    assert_eq!(rig.bar().value(), 7);
    rig.ok("execute store result bossbar test:bar max run time set 70");
    assert_eq!(rig.bar().max(), 70);
}
