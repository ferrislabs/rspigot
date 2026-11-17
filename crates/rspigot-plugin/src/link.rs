use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};
use tokio::sync::{mpsc, oneshot};

use crate::{ActionError, Error};

pub(crate) const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

tokio::task_local! {
    static HANDLING: u64;
}

pub(crate) async fn handling<F: Future>(event: u64, handler: F) -> F::Output {
    HANDLING.scope(event, handler).await
}

pub(crate) type Outbound = mpsc::UnboundedSender<v1::FromPlugin>;

type Waiting = oneshot::Sender<Option<answer::Body>>;

#[derive(Clone, Default)]
pub(crate) struct Link(Arc<Shared>);

#[derive(Default)]
struct Shared {
    current: Mutex<Option<Connection>>,
    generation: AtomicU64,
    last_request: AtomicU64,
    name: Mutex<String>,
}

struct Connection {
    outbound: Outbound,
    waiting: HashMap<u64, Waiting>,
}

pub(crate) struct Attached(Link);

impl Drop for Attached {
    fn drop(&mut self) {
        let gone = self.0.lock().take();
        if let Some(Connection { outbound, waiting }) = gone {
            drop(outbound);
            drop(waiting);
        }
    }
}

impl Link {
    pub(crate) fn attach(&self, outbound: Outbound) -> Attached {
        let mut current = self.lock();
        self.0.generation.fetch_add(1, Ordering::Relaxed);
        *current = Some(Connection {
            outbound,
            waiting: HashMap::new(),
        });
        drop(current);
        Attached(self.clone())
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::Relaxed)
    }

    pub(crate) fn name_as(&self, name: &str) {
        let mut own = self.0.name.lock().unwrap_or_else(PoisonError::into_inner);
        name.clone_into(&mut own);
    }

    pub(crate) fn name(&self) -> String {
        self.0
            .name
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn order(&self, body: from_plugin::Body) {
        send(self.lock().as_ref(), body);
    }

    pub(crate) fn order_from(&self, generation: u64, body: from_plugin::Body) {
        let current = self.lock();
        if self.generation() == generation {
            send(current.as_ref(), body);
        } else {
            tracing::warn!("Dropped an order on an inventory made before rspigot reconnected");
        }
    }

    pub(crate) async fn ask<T>(
        &self,
        question: question::Question,
        expected: &str,
        pick: impl FnOnce(answer::Body) -> Option<T>,
    ) -> Result<T, Error> {
        let posted = self.post(|request| {
            message(from_plugin::Body::Question(v1::Question {
                request,
                question: Some(question),
            }))
        });
        match self.exchange(posted).await? {
            answer::Body::NotFound(_) => Err(Error::NotFound),
            answer::Body::Refused(refused) => Err(Error::Rejected(refused.reason)),
            body => pick(body)
                .ok_or_else(|| Error::Malformed(format!("an answer that is not {expected}"))),
        }
    }

    pub(crate) async fn act(&self, body: from_plugin::Body) -> Result<(), Error> {
        let posted = self.post(|request| v1::FromPlugin {
            body: Some(body),
            origin: HANDLING.try_with(|event| *event).ok(),
            request: Some(request),
        });
        match self.exchange(posted).await? {
            answer::Body::Done(_) => Ok(()),
            _ => Err(Error::Malformed(
                "an answer that is not a result".to_owned(),
            )),
        }
    }

    pub(crate) async fn act_from(
        &self,
        generation: u64,
        body: from_plugin::Body,
    ) -> Result<(), Error> {
        if self.generation() != generation {
            return Err(Error::Disconnected);
        }
        self.act(body).await
    }

    async fn exchange(
        &self,
        posted: Result<(u64, oneshot::Receiver<Option<answer::Body>>), Error>,
    ) -> Result<answer::Body, Error> {
        let (request, answered) = posted?;
        let body = match tokio::time::timeout(ANSWER_TIMEOUT, answered).await {
            Ok(Ok(Some(body))) => body,
            Ok(Ok(None)) => return Err(Error::Malformed("an empty answer".to_owned())),
            Ok(Err(_)) => return Err(Error::Disconnected),
            Err(_) => {
                self.forget(request);
                return Err(Error::Timeout);
            }
        };
        match body {
            answer::Body::Failed(failed) => Err(Error::Action(ActionError::from_wire(failed))),
            body => Ok(body),
        }
    }

    fn post(
        &self,
        build: impl FnOnce(u64) -> v1::FromPlugin,
    ) -> Result<(u64, oneshot::Receiver<Option<answer::Body>>), Error> {
        let mut current = self.lock();
        let connection = current.as_mut().ok_or(Error::Disconnected)?;
        let request = self.0.last_request.fetch_add(1, Ordering::Relaxed) + 1;
        connection
            .outbound
            .send(build(request))
            .map_err(|_| Error::Disconnected)?;
        let (waiting, answered) = oneshot::channel();
        connection.waiting.insert(request, waiting);
        Ok((request, answered))
    }

    fn forget(&self, request: u64) {
        if let Some(connection) = self.lock().as_mut() {
            connection.waiting.remove(&request);
        }
    }

    pub(crate) fn answer(&self, answer: v1::Answer) {
        let waiting = self
            .lock()
            .as_mut()
            .and_then(|connection| connection.waiting.remove(&answer.request));
        if let Some(waiting) = waiting {
            let _ = waiting.send(answer.body);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Connection>> {
        self.0
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("connected", &self.lock().is_some())
            .finish()
    }
}

fn send(connection: Option<&Connection>, body: from_plugin::Body) {
    if let Some(connection) = connection {
        let origin = HANDLING.try_with(|event| *event).ok();
        let _ = connection.outbound.send(v1::FromPlugin {
            body: Some(body),
            origin,
            request: None,
        });
    }
}

pub(crate) const fn message(body: from_plugin::Body) -> v1::FromPlugin {
    v1::FromPlugin {
        body: Some(body),
        origin: None,
        request: None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_question_gives_up_after_ten_seconds() {
        assert_eq!(ANSWER_TIMEOUT, Duration::from_secs(10));
        let link = Link::default();
        let (outbound, mut sent) = mpsc::unbounded_channel();
        let _attached = link.attach(outbound);
        let started = Instant::now();

        let asked = link
            .ask(
                question::Question::Players(v1::Empty {}),
                "a player list",
                Some,
            )
            .await;

        assert!(matches!(asked, Err(Error::Timeout)), "got {asked:?}");
        assert!(started.elapsed() >= ANSWER_TIMEOUT);
        assert!(sent.try_recv().is_ok());
        assert!(link.lock().as_ref().unwrap().waiting.is_empty());
    }
}
