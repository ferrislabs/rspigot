mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::config::PluginSettings;
use rspigot_core::ports::{PlayerStore, StoreError};
use rspigot_domain::player::PlayerId;
use rspigot_nbt::Compound;

#[derive(Default)]
struct Data(Mutex<HashMap<String, Compound>>);

impl Data {
    fn wait_for(&self, name: &str) -> Compound {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(found) = self.0.lock().unwrap().get(name).cloned() {
                return found;
            }
            assert!(Instant::now() < deadline, "{name} was never saved");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl PlayerStore for Data {
    fn load_player(&self, _: PlayerId) -> Result<Option<Compound>, StoreError> {
        Ok(None)
    }

    fn save_player(&self, _: PlayerId, _: &Compound) -> Result<(), StoreError> {
        Ok(())
    }

    fn load_saved_data(&self, name: &str) -> Result<Option<Compound>, StoreError> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }

    fn save_saved_data(&self, name: &str, data: &Compound) -> Result<(), StoreError> {
        self.0.lock().unwrap().insert(name.to_owned(), data.clone());
        Ok(())
    }

    fn saved_data_namespaces(&self, leaf: &str) -> Result<Vec<String>, StoreError> {
        let suffix = format!(":{leaf}");
        Ok(self
            .0
            .lock()
            .unwrap()
            .keys()
            .filter_map(|name| name.strip_suffix(&suffix).map(str::to_owned))
            .collect())
    }
}

fn run(s: &mut Scene, line: &str) -> CommandOutcome {
    let outcome = execute(&mut s.server, CommandSender::Console, line);
    s.server.deliver();
    outcome
}

fn scene_over(data: &Arc<Data>) -> Scene {
    let server = server(PluginSettings::default()).with_player_store(Arc::clone(data));
    scene_on(server, &[], Vec::new())
}

#[test]
fn command_storage_survives_a_restart() {
    let data = Arc::new(Data::default());
    let mut first = scene_over(&data);
    assert_eq!(
        run(&mut first, "data merge storage t:s {v:4}"),
        CommandOutcome::Success
    );
    run(&mut first, "data merge storage other:deep/path {w:1}");
    run(&mut first, "save-all");
    let saved = data.wait_for("t:command_storage");
    let contents = saved.get_compound("contents").expect("contents");
    assert_eq!(
        contents.get_compound("s").and_then(|s| s.get_i32("v")),
        Some(4)
    );
    data.wait_for("other:command_storage");
    let mut second = scene_over(&data);
    assert_eq!(
        run(&mut second, "execute if data storage t:s v"),
        CommandOutcome::Success
    );
    assert_eq!(
        run(&mut second, "execute if data storage other:deep/path w"),
        CommandOutcome::Success
    );
    assert_eq!(
        run(&mut second, "execute if data storage t:missing v"),
        CommandOutcome::Failure
    );
}
