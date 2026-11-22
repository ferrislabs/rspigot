mod common;

use common::*;
use rspigot_core::commands::{CommandSender, execute};
use rspigot_core::events::{ClientEvent, Inbound};

fn says(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect()
}

#[test]
fn perf_stops_by_itself_after_ten_seconds_and_writes_a_report() {
    let directory = std::env::temp_dir().join(format!("rspigot-perf-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::env::set_current_dir(&directory).unwrap();
    let mut s = scene(&["Alice"]);
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    let alice = s.players[0].clone();
    alice.link.take();
    execute(&mut s.server, CommandSender::Player(alice.id), "perf start");
    s.server.deliver();
    assert_eq!(says(&alice), vec!["commands.perf.started"]);
    s.tick(199);
    assert!(says(&alice).is_empty());
    s.tick(1);
    let heard = says(&alice);
    assert!(heard[0].starts_with("commands.perf.stopped"), "{heard:?}");
    assert!(
        heard[1].starts_with("commands.perf.reportSaved"),
        "{heard:?}"
    );
    let reports = std::fs::read_dir(directory.join("debug/profiling"))
        .unwrap()
        .count();
    assert_eq!(reports, 1);
    std::fs::remove_dir_all(&directory).ok();
}
