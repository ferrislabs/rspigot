mod common;

use std::time::Duration;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::heartbeat::Heartbeat;

#[test]
fn the_run_loop_beats_every_tick_and_stops_the_heartbeat_on_shutdown() {
    let heartbeat = Heartbeat::new();
    let server = calm_server().with_heartbeat(heartbeat.clone());
    let (sender, inbox) = crossbeam_channel::unbounded::<Inbound<Link>>();
    let runner = std::thread::spawn(move || server.run(&inbox));
    std::thread::sleep(Duration::from_millis(300));
    assert!(heartbeat.since_beat().is_some());
    assert!(heartbeat.ticks() > 0);
    assert!(!heartbeat.stopped());
    sender.send(Inbound::Shutdown).unwrap();
    runner.join().unwrap();
    assert!(heartbeat.stopped());
}
