use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::ports::{DatapackError, DatapackSource, PackContents, PackExtras};

pub type Read = (Vec<PackContents>, Vec<PackExtras>);

enum Request {
    Read,
    Create {
        name: String,
        meta: String,
        reply: Sender<Result<bool, DatapackError>>,
    },
}

#[derive(Debug)]
pub struct DatapackLoader {
    requests: Sender<Request>,
    results: Receiver<Result<Read, DatapackError>>,
}

impl DatapackLoader {
    pub fn spawn<S: DatapackSource>(source: Arc<S>) -> Self {
        let (requests, wanted) = crossbeam_channel::unbounded::<Request>();
        let (sender, results) = crossbeam_channel::unbounded();
        let spawned = std::thread::Builder::new()
            .name("datapack-reader".to_owned())
            .spawn(move || {
                while let Ok(request) = wanted.recv() {
                    match request {
                        Request::Read => {
                            let read = source
                                .read_packs()
                                .and_then(|packs| Ok((packs, source.read_extras()?)));
                            if sender.send(read).is_err() {
                                break;
                            }
                        }
                        Request::Create { name, meta, reply } => {
                            let _ = reply.send(source.create_pack(&name, &meta));
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::error!("Could not start the datapack reader: {error}");
        }
        Self { requests, results }
    }

    pub fn create(&self, name: &str, meta: &str) -> Result<bool, DatapackError> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        let gone = || {
            DatapackError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the datapack reader stopped",
            ))
        };
        self.requests
            .send(Request::Create {
                name: name.to_owned(),
                meta: meta.to_owned(),
                reply,
            })
            .map_err(|_| gone())?;
        answer.recv().map_err(|_| gone())?
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
        self.requests.send(Request::Read).map_err(|_| gone())?;
        self.results.recv().map_err(|_| gone())?
    }
}
