use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::ports::{DatapackError, DatapackSource, PackContents, PackExtras};

pub type Read = (Vec<PackContents>, Vec<PackExtras>);

#[derive(Debug)]
pub struct DatapackLoader {
    requests: Sender<()>,
    results: Receiver<Result<Read, DatapackError>>,
}

impl DatapackLoader {
    pub fn spawn<S: DatapackSource>(source: Arc<S>) -> Self {
        let (requests, wanted) = crossbeam_channel::unbounded::<()>();
        let (sender, results) = crossbeam_channel::unbounded();
        let spawned = std::thread::Builder::new()
            .name("datapack-reader".to_owned())
            .spawn(move || {
                while wanted.recv().is_ok() {
                    let read = source
                        .read_packs()
                        .and_then(|packs| Ok((packs, source.read_extras()?)));
                    if sender.send(read).is_err() {
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
        self.read_all().map(|(packs, _)| packs)
    }

    pub fn read_all(&self) -> Result<Read, DatapackError> {
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
