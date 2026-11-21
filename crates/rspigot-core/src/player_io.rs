use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use rspigot_domain::player::PlayerId;
use rspigot_nbt::Compound;

use crate::ports::{PlayerStore, ProgressFile, StoreError};

enum Job {
    Load {
        player: PlayerId,
        reply: Sender<Result<Option<Compound>, StoreError>>,
    },
    Save {
        player: PlayerId,
        data: Compound,
    },
    LoadProgress {
        player: PlayerId,
        file: ProgressFile,
        reply: Sender<Result<Option<String>, StoreError>>,
    },
    SaveProgress {
        player: PlayerId,
        file: ProgressFile,
        json: String,
    },
    LoadData {
        name: String,
        reply: Sender<Result<Option<Compound>, StoreError>>,
    },
    SaveData {
        name: String,
        data: Compound,
    },
}

pub struct PlayerIo {
    jobs: Sender<Job>,
    handle: Option<JoinHandle<()>>,
}

impl PlayerIo {
    pub fn spawn<S: PlayerStore>(store: Arc<S>) -> Self {
        let (jobs, inbox) = unbounded();
        let handle = std::thread::Builder::new()
            .name("player-io".to_owned())
            .spawn(move || run(&*store, &inbox))
            .ok();
        Self { jobs, handle }
    }

    pub fn load(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
        let (reply, answer) = bounded(1);
        let closed = || std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        self.jobs
            .send(Job::Load { player, reply })
            .map_err(|_| closed())?;
        answer.recv().map_err(|_| closed())?
    }

    pub fn save(&self, player: PlayerId, data: Compound) {
        if self.jobs.send(Job::Save { player, data }).is_err() {
            tracing::warn!("Player data writer is gone, {player} was not saved");
        }
    }

    pub fn load_progress(
        &self,
        player: PlayerId,
        file: ProgressFile,
    ) -> Result<Option<String>, StoreError> {
        let (reply, answer) = bounded(1);
        let closed = || std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        self.jobs
            .send(Job::LoadProgress {
                player,
                file,
                reply,
            })
            .map_err(|_| closed())?;
        answer.recv().map_err(|_| closed())?
    }

    pub fn save_progress(&self, player: PlayerId, file: ProgressFile, json: String) {
        if self
            .jobs
            .send(Job::SaveProgress { player, file, json })
            .is_err()
        {
            tracing::warn!("Player data writer is gone, {player} progress was not saved");
        }
    }

    pub fn load_saved_data(&self, name: &str) -> Result<Option<Compound>, StoreError> {
        let (reply, answer) = bounded(1);
        let closed = || std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        self.jobs
            .send(Job::LoadData {
                name: name.to_owned(),
                reply,
            })
            .map_err(|_| closed())?;
        answer.recv().map_err(|_| closed())?
    }

    pub fn save_saved_data(&self, name: &str, data: Compound) {
        let job = Job::SaveData {
            name: name.to_owned(),
            data,
        };
        if self.jobs.send(job).is_err() {
            tracing::warn!("Player data writer is gone, {name} was not saved");
        }
    }

    pub fn shutdown(self) {
        drop(self.jobs);
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }
}

fn run<S: PlayerStore>(store: &S, inbox: &Receiver<Job>) {
    while let Ok(job) = inbox.recv() {
        match job {
            Job::Load { player, reply } => {
                let _ = reply.send(store.load_player(player));
            }
            Job::Save { player, data } => {
                if let Err(error) = store.save_player(player, &data) {
                    tracing::warn!("Failed to save player data for {player}: {error}");
                }
            }
            Job::LoadProgress {
                player,
                file,
                reply,
            } => {
                let _ = reply.send(store.load_progress(player, file));
            }
            Job::SaveProgress { player, file, json } => {
                if let Err(error) = store.save_progress(player, file, &json) {
                    tracing::warn!("Failed to save {file:?} for {player}: {error}");
                }
            }
            Job::LoadData { name, reply } => {
                let _ = reply.send(store.load_saved_data(&name));
            }
            Job::SaveData { name, data } => {
                if let Err(error) = store.save_saved_data(&name, &data) {
                    tracing::warn!("Failed to save {name}: {error}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rustc_hash::FxHashMap;

    use super::*;

    #[derive(Default)]
    struct Memory {
        files: Mutex<FxHashMap<PlayerId, Compound>>,
    }

    impl PlayerStore for Memory {
        fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
            Ok(self.files.lock().ok().and_then(|f| f.get(&player).cloned()))
        }

        fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError> {
            if let Ok(mut files) = self.files.lock() {
                files.insert(player, data.clone());
            }
            Ok(())
        }
    }

    #[test]
    fn a_load_after_a_save_sees_the_saved_data() {
        let store = Arc::new(Memory::default());
        let io = PlayerIo::spawn(Arc::clone(&store));
        let id = PlayerId::offline("Alice");
        assert_eq!(io.load(id).expect("loads"), None);
        io.save(id, Compound::new().with("XpLevel", 4));
        let loaded = io.load(id).expect("loads").expect("present");
        assert_eq!(loaded.get_i32("XpLevel"), Some(4));
        io.shutdown();
    }

    #[test]
    fn shutdown_waits_for_pending_saves() {
        let store = Arc::new(Memory::default());
        let io = PlayerIo::spawn(Arc::clone(&store));
        let id = PlayerId::offline("Bob");
        io.save(id, Compound::new().with("XpLevel", 9));
        io.shutdown();
        let saved = store.load_player(id).expect("loads").expect("present");
        assert_eq!(saved.get_i32("XpLevel"), Some(9));
    }
}
