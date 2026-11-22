use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::Sender;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{PluginIntro, ToPlugin};
use rspigot_core::ports::ClientLink;
use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::plugin_host_server::PluginHost;
use rspigot_plugin_protocol::v1::{self, from_plugin};
use tokio::sync::{Mutex, mpsc, watch};
use tokio::time::Instant;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tonic_health::ServingStatus;
use tonic_health::server::HealthReporter;

use crate::convert;
use crate::limits::{
    Limiter, MAX_DEPENDENCIES, MAX_HANDLERS, MAX_NAME_LENGTH, MESSAGE_BURST, MESSAGES_PER_SECOND,
    OUTBOX_CAPACITY,
};
use crate::outbox::{Outbox, Overflow};
use crate::roster::{Dependencies, Roster};

const REPLY_CAPACITY: usize = 4096;

type Reply = Result<v1::ToPlugin, Status>;
type Ending = ControlFlow<Option<Status>>;

pub(crate) async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}

#[derive(Clone)]
pub(crate) struct Health {
    reporter: HealthReporter,
    holders: Arc<Mutex<HashMap<String, u64>>>,
}

impl Health {
    pub(crate) fn new(reporter: HealthReporter) -> Self {
        Self {
            reporter,
            holders: Arc::default(),
        }
    }

    async fn join(&self, name: &str, connection: u64, announce: impl FnOnce() -> bool) -> bool {
        let mut holders = self.holders.lock().await;
        if !announce() {
            return false;
        }
        holders.insert(name.to_owned(), connection);
        self.reporter
            .set_service_status(name, ServingStatus::Serving)
            .await;
        true
    }

    async fn leave(&self, name: &str, connection: u64) {
        let mut holders = self.holders.lock().await;
        if holders.get(name) == Some(&connection) {
            holders.remove(name);
            self.reporter
                .set_service_status(name, ServingStatus::NotServing)
                .await;
        }
    }
}

pub(crate) struct Timing {
    pub(crate) introduction: Duration,
    pub(crate) soft_wait: Duration,
}

pub(crate) struct Host<L> {
    token: Option<String>,
    inbound: Sender<Inbound<L>>,
    health: Health,
    stop: watch::Receiver<bool>,
    timing: Timing,
    roster: Roster,
    next_connection: Arc<AtomicU64>,
}

impl<L: ClientLink> Host<L> {
    pub(crate) fn new(
        token: Option<String>,
        inbound: Sender<Inbound<L>>,
        health: Health,
        stop: watch::Receiver<bool>,
        timing: Timing,
        roster: Roster,
    ) -> Self {
        Self {
            token,
            inbound,
            health,
            stop,
            timing,
            roster,
            next_connection: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn open(
        &self,
        mut incoming: Streaming<v1::FromPlugin>,
    ) -> Result<ReceiverStream<Reply>, Status> {
        let hello = self.introduction(&mut incoming).await?;
        let (intro, dependencies) = self.admit(hello)?;
        let begun = self
            .roster
            .begin(&intro.name, &dependencies)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let (outbox, from_core) = mpsc::channel(OUTBOX_CAPACITY);
        let (replies, stream) = mpsc::channel(REPLY_CAPACITY + 1);
        let overflow = Overflow::default();
        let session = Session {
            connection: 0,
            name: intro.name.clone(),
            incoming,
            from_core,
            replies,
            inbound: self.inbound.clone(),
            health: self.health.clone(),
            stop: self.stop.clone(),
            roster: self.roster.clone(),
            dependencies,
            ticket: begun.ticket,
            soft_wait: self.timing.soft_wait,
            next_connection: Arc::clone(&self.next_connection),
            limiter: Limiter::new(
                MESSAGES_PER_SECOND,
                MESSAGE_BURST,
                std::time::Instant::now(),
            ),
            overflow: overflow.clone(),
        };
        for other in &begun.already_running {
            session.warn_late(other);
        }
        tokio::spawn(session.run(intro, Outbox::new(outbox, overflow)));
        Ok(ReceiverStream::new(stream))
    }

    async fn introduction(
        &self,
        incoming: &mut Streaming<v1::FromPlugin>,
    ) -> Result<v1::Hello, Status> {
        let mut stop = self.stop.clone();
        let first = tokio::select! {
            first = tokio::time::timeout(self.timing.introduction, incoming.message()) => {
                first.map_err(|_| Status::deadline_exceeded("no introduction"))??
            }
            () = stopped(&mut stop) => return Err(Status::unavailable("the server is stopping")),
        };
        match first.and_then(|message| message.body) {
            Some(from_plugin::Body::Hello(hello)) => Ok(hello),
            _ => Err(Status::invalid_argument(
                "the first message must be a Hello",
            )),
        }
    }

    fn admit(&self, hello: v1::Hello) -> Result<(PluginIntro, Dependencies), Status> {
        if let Some(token) = &self.token
            && !same_token(token, &hello.token)
        {
            return Err(Status::unauthenticated("wrong plugin token"));
        }
        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(Status::failed_precondition(format!(
                "the server speaks plugin protocol {PROTOCOL_VERSION}, the plugin speaks {}",
                hello.protocol_version
            )));
        }
        if hello.handlers.len() > MAX_HANDLERS {
            return Err(Status::invalid_argument(format!(
                "{} handlers declared, the server accepts at most {MAX_HANDLERS}",
                hello.handlers.len()
            )));
        }
        let dependencies = Dependencies {
            depends: hello.depends.clone(),
            soft_depends: hello.soft_depends.clone(),
            load_before: hello.load_before.clone(),
        };
        check_dependencies(&hello.name, &dependencies)?;
        let intro =
            convert::intro(hello).map_err(|error| Status::invalid_argument(error.to_string()))?;
        intro
            .validate()
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.roster
            .attempt(&intro.name, std::time::Instant::now())
            .map_err(|wait| {
                Status::resource_exhausted(format!(
                    "plugin {} reconnects too fast, retry in {} s",
                    intro.name,
                    wait.as_secs() + 1
                ))
            })?;
        Ok((intro, dependencies))
    }
}

fn check_dependencies(name: &str, dependencies: &Dependencies) -> Result<(), Status> {
    let lists = [
        ("depends", &dependencies.depends),
        ("soft_depends", &dependencies.soft_depends),
        ("load_before", &dependencies.load_before),
    ];
    for (field, list) in lists {
        if list.len() > MAX_DEPENDENCIES {
            return Err(Status::invalid_argument(format!(
                "{} entries in {field}, the server accepts at most {MAX_DEPENDENCIES}",
                list.len()
            )));
        }
        for other in list {
            if other.is_empty() || other.len() > MAX_NAME_LENGTH {
                return Err(Status::invalid_argument(format!(
                    "{field} holds an empty or overlong plugin name"
                )));
            }
            if other == name {
                return Err(Status::invalid_argument(format!(
                    "plugin {name} lists itself in {field}"
                )));
            }
        }
    }
    Ok(())
}

fn same_token(expected: &str, given: &str) -> bool {
    expected.len() == given.len()
        && expected
            .bytes()
            .zip(given.bytes())
            .fold(0, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[tonic::async_trait]
impl<L: ClientLink> PluginHost for Host<L> {
    type ConnectStream = ReceiverStream<Reply>;

    async fn connect(
        &self,
        request: Request<Streaming<v1::FromPlugin>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let opened = self.open(request.into_inner()).await;
        if let Err(status) = &opened {
            tracing::warn!("Refused a plugin: {}", status.message());
        }
        opened.map(Response::new)
    }
}

struct Session<L> {
    connection: u64,
    name: String,
    incoming: Streaming<v1::FromPlugin>,
    from_core: mpsc::Receiver<ToPlugin>,
    replies: mpsc::Sender<Reply>,
    inbound: Sender<Inbound<L>>,
    health: Health,
    stop: watch::Receiver<bool>,
    roster: Roster,
    dependencies: Dependencies,
    ticket: u64,
    soft_wait: Duration,
    next_connection: Arc<AtomicU64>,
    limiter: Limiter,
    overflow: Overflow,
}

impl<L: ClientLink> Session<L> {
    fn warn_late(&self, other: &str) {
        let _ = self
            .replies
            .try_send(Ok(convert::to_plugin(ToPlugin::Warning(format!(
                "load_before {other} is too late: {other} is already running"
            )))));
    }

    async fn hold(&mut self) -> Result<(), Option<Status>> {
        let mut changes = self.roster.subscribe();
        let mut told = None;
        let mut soft_deadline: Option<Instant> = None;
        loop {
            if !self.roster.is_current(&self.name, self.ticket) {
                return Err(Some(Status::already_exists(
                    "replaced by a newer connection",
                )));
            }
            let blockers = self.roster.blockers(&self.name, &self.dependencies);
            let mut wake = Instant::now() + Duration::from_secs(86_400);
            if blockers.hard.is_empty() {
                if blockers.soft.is_empty() {
                    return Ok(());
                }
                let deadline =
                    *soft_deadline.get_or_insert_with(|| Instant::now() + self.soft_wait);
                if Instant::now() >= deadline {
                    return Ok(());
                }
                wake = deadline;
            }
            if told.as_ref() != Some(&blockers) {
                let waiting: Vec<&str> = blockers
                    .hard
                    .iter()
                    .chain(&blockers.soft)
                    .map(String::as_str)
                    .collect();
                tracing::info!(
                    "Plugin {} is held until {} connect",
                    self.name,
                    waiting.join(", ")
                );
                let notice = ToPlugin::Warning(format!(
                    "{} is held until {} connect",
                    self.name,
                    waiting.join(", ")
                ));
                if let ControlFlow::Break(ending) = self.reply(notice) {
                    return Err(ending);
                }
                told = Some(blockers);
            }
            tokio::select! {
                changed = changes.changed() => if changed.is_err() { return Err(None) },
                () = tokio::time::sleep_until(wake) => {}
                message = self.incoming.message() => match message {
                    Ok(Some(_)) => {}
                    _ => return Err(None),
                },
                () = self.replies.closed() => return Err(None),
                () = stopped(&mut self.stop) => {
                    return Err(Some(Status::unavailable("the server is stopping")));
                }
            }
        }
    }

    async fn run(mut self, intro: PluginIntro, outbox: Outbox) {
        if let Err(ending) = self.hold().await {
            if let Some(status) = ending {
                let _ = self.replies.try_send(Err(status));
            }
            self.roster.withdraw(&self.name, self.ticket);
            tracing::info!("Plugin {} left before it was released", self.name);
            return;
        }
        self.connection = self.next_connection.fetch_add(1, Ordering::Relaxed);
        let version = intro.version.clone();
        let connected = Inbound::PluginConnected {
            connection: self.connection,
            intro,
            outbox: Box::new(outbox),
        };
        let announce = || self.inbound.send(connected).is_ok();
        if !self
            .health
            .join(&self.name, self.connection, announce)
            .await
        {
            let _ = self
                .replies
                .try_send(Err(Status::unavailable("the server is stopping")));
            self.roster.withdraw(&self.name, self.ticket);
            return;
        }
        self.roster.connected(&self.name, self.connection);
        tracing::info!("Plugin {} {version} connected", self.name);
        let ending = loop {
            if let ControlFlow::Break(ending) = self.step().await {
                break ending;
            }
        };
        if let Some(status) = ending {
            let _ = self.replies.try_send(Err(status));
        }
        self.health.leave(&self.name, self.connection).await;
        let _ = self.inbound.send(Inbound::PluginDisconnected {
            connection: self.connection,
        });
        self.roster.left(&self.name, self.connection);
        self.roster.withdraw(&self.name, self.ticket);
        tracing::info!("Plugin {} disconnected", self.name);
    }

    async fn step(&mut self) -> Ending {
        if self.overflow.happened() {
            tracing::warn!(
                "Plugin {} cannot keep up with the server, dropping it",
                self.name
            );
            return ControlFlow::Break(Some(Status::resource_exhausted(
                "the plugin is too slow to read what the server sends",
            )));
        }
        let throttle = self.limiter.wait(std::time::Instant::now());
        tokio::select! {
            received = self.incoming.message(), if throttle.is_zero() => match received {
                Ok(Some(message)) => {
                    self.limiter.take();
                    self.forward(message)
                }
                Ok(None) => ControlFlow::Break(None),
                Err(status) => {
                    tracing::debug!(plugin = %self.name, %status, "plugin stream failed");
                    ControlFlow::Break(None)
                }
            },
            () = tokio::time::sleep(throttle), if !throttle.is_zero() => ControlFlow::Continue(()),
            sent = self.from_core.recv() => match sent {
                Some(message) => self.reply(message),
                None => ControlFlow::Break(None),
            },
            () = self.replies.closed() => ControlFlow::Break(None),
            () = stopped(&mut self.stop) => self.flush(),
        }
    }

    fn flush(&mut self) -> Ending {
        while let Ok(message) = self.from_core.try_recv() {
            self.reply(message)?;
        }
        ControlFlow::Break(None)
    }

    fn forward(&self, message: v1::FromPlugin) -> Ending {
        let mut warnings = Vec::new();
        match convert::from_plugin(message, &mut warnings) {
            Ok(message) => {
                let _ = self.inbound.send(Inbound::PluginMessage {
                    connection: self.connection,
                    message: Box::new(message),
                });
            }
            Err(error) => tracing::warn!("Dropped a message from plugin {}: {error}", self.name),
        }
        for warning in warnings {
            self.reply(ToPlugin::Warning(warning))?;
        }
        ControlFlow::Continue(())
    }

    fn reply(&self, message: ToPlugin) -> Ending {
        if self.replies.capacity() <= 1 {
            tracing::warn!(
                "Plugin {} is not reading its stream, dropping it",
                self.name
            );
            return ControlFlow::Break(Some(Status::resource_exhausted(
                "the plugin is not reading its stream",
            )));
        }
        let replaced = matches!(message, ToPlugin::Replaced);
        if self
            .replies
            .try_send(Ok(convert::to_plugin(message)))
            .is_err()
        {
            return ControlFlow::Break(None);
        }
        if replaced {
            return ControlFlow::Break(Some(Status::already_exists(
                "replaced by a newer connection",
            )));
        }
        ControlFlow::Continue(())
    }
}
