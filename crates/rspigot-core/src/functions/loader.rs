use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::ports::{DatapackError, DatapackSource, PackContents};

#[derive(Debug)]
pub struct DatapackLoader {
    requests: Sender<()>,
    results: Receiver<Result<Vec<PackContents>, DatapackError>>,
}

impl DatapackLoader {
    pub fn spawn<S: DatapackSource>(source: Arc<S>) -> Self {
        let (requests, wanted) = crossbeam_channel::unbounded::<()>();
        let (sender, results) = crossbeam_channel::unbounded();
        let spawned = std::thread::Builder::new()
            .name("datapack-reader".to_owned())
            .spawn(move || {
                while wanted.recv().is_ok() {
                    if sender.send(source.read_packs()).is_err() {
                        break;
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::error!("Could not start the datapack reader: {error}");
        }
        Self { requests, results }
    }

    pub fn read(&self) -> Result<Vec<PackContents>, DatapackError> {
        let gone = || {
            DatapackError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the datapack reader stopped",
            ))
        };
        self.requests.send(()).map_err(|_| gone())?;
        self.results.recv().map_err(|_| gone())?
    }
}
