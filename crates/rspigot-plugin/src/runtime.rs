use std::future::Future;
use std::time::Duration;

use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::plugin_host_client::PluginHostClient;
use rspigot_plugin_protocol::v1::{self, event, from_plugin, to_plugin};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::transport::Endpoint;
use tonic::{Code, Status, Streaming};

use crate::Error;
use crate::commands::Invocation;
use crate::commands::{ArgumentKind, CommandSpec};
use crate::events::EventKind;
use crate::link::{ANSWER_TIMEOUT, Link, Outbound, handling, message};
use crate::listener::{HandlerSpec, Listeners, Outcome, Plugin, Verdict};
use crate::server::Server;
use crate::tasks::{TASK_RESOLUTION, TaskClock};

const DEFAULT_URL: &str = "http://127.0.0.1:25580";
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HELD_TIMEOUT: Duration = Duration::from_secs(600);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(20);

#[allow(clippy::large_enum_variant)]
enum Queued {
    Event {
        event: Box<v1::Event>,
        reply: mpsc::WeakUnboundedSender<v1::FromPlugin>,
    },
    Invocation(Invocation),
    Enable(Link),
    Disable,
}

type Queue = mpsc::UnboundedSender<Queued>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Meta {
    pub(crate) id: u64,
    pub(crate) cancelled: bool,
    pub(crate) rolled_back: bool,
}

pub(crate) struct Received {
    pub(crate) payload: event::Payload,
    pub(crate) meta: Meta,
    pub(crate) blocks: std::sync::Arc<Vec<v1::BlockMeta>>,
}

impl Received {
    fn from_wire(event: v1::Event) -> Result<Self, Error> {
        let meta = Meta {
            id: event.id,
            cancelled: event.cancelled,
            rolled_back: event.phase() == v1::Phase::RolledBack,
        };
        let payload = event
            .payload
            .ok_or_else(|| Error::Malformed(format!("event {} without a payload", meta.id)))?;
        Ok(Self {
            payload,
            meta,
            blocks: std::sync::Arc::new(event.block_meta),
        })
    }
}

enum Ended {
    Refused(Status),
    Replaced,
    Lost(String),
}

pub async fn run<P: Plugin>(plugin: P) -> Result<(), Error> {
    let url = std::env::var("RSPIGOT_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_owned());
    let token = std::env::var("RSPIGOT_TOKEN").ok();
    run_with(plugin, url, token).await
}

pub async fn run_with<P: Plugin>(
    plugin: P,
    url: String,
    token: Option<String>,
) -> Result<(), Error> {
    let endpoint = endpoint(&url)?;
    let commands = P::commands();
    check_commands::<P>(&commands)?;
    let link = Link::default();
    link.name_as(P::NAME);
    let (queue, queued) = mpsc::unbounded_channel();
    let specs = P::handlers();
    let kinds: Vec<(u32, EventKind)> = specs.iter().map(|spec| (spec.id, spec.event)).collect();
    let hello = v1::Hello {
        protocol_version: PROTOCOL_VERSION,
        name: P::NAME.to_owned(),
        version: P::VERSION.to_owned(),
        token: token.unwrap_or_default(),
        handlers: specs.into_iter().map(HandlerSpec::to_wire).collect(),
        commands: commands.iter().map(CommandSpec::to_wire).collect(),
        depends: names(P::DEPENDS),
        soft_depends: names(P::SOFT_DEPENDS),
        load_before: names(P::LOAD_BEFORE),
    };
    let (error, ()) = tokio::join!(
        keep_connected(endpoint, url, hello, &link, queue),
        handle_events(&plugin, &link, &kinds, queued),
    );
    Err(error)
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

fn command_path(spec: &CommandSpec) -> Vec<&'static str> {
    std::iter::once(spec.name)
        .chain(
            spec.arguments
                .iter()
                .take_while(|argument| argument.kind == ArgumentKind::Literal)
                .map(|argument| argument.name),
        )
        .collect()
}

fn check_commands<P: Listeners>(commands: &[CommandSpec]) -> Result<(), Error> {
    let mut modules = Vec::new();
    P::command_modules(0, &mut modules);
    let owner = |spec: &CommandSpec| modules.iter().find(|module| module.contains(spec.id));
    for (index, spec) in commands.iter().enumerate() {
        let path = command_path(spec);
        let first = owner(spec);
        let clash = commands[index + 1..].iter().find(|other| {
            command_path(other) == path && owner(other).map(|m| &m.name) != first.map(|m| &m.name)
        });
        if let Some(other) = clash {
            let name = |spec: &CommandSpec| {
                owner(spec).map_or_else(|| "?".to_owned(), |module| module.name.clone())
            };
            return Err(Error::Modules(format!(
                "`/{}` is declared by both `{}` and `{}`",
                path.join(" "),
                name(spec),
                name(other)
            )));
        }
    }
    Ok(())
}

fn endpoint(url: &str) -> Result<Endpoint, Error> {
    let endpoint = Endpoint::from_shared(url.to_owned()).map_err(|_| Error::Url(url.to_owned()))?;
    if !matches!(endpoint.uri().scheme_str(), Some("http" | "https")) {
        return Err(Error::Url(url.to_owned()));
    }
    Ok(endpoint
        .connect_timeout(CONNECT_TIMEOUT)
        .http2_keep_alive_interval(KEEP_ALIVE_INTERVAL)
        .keep_alive_timeout(KEEP_ALIVE_TIMEOUT)
        .keep_alive_while_idle(true))
}

async fn keep_connected(
    endpoint: Endpoint,
    url: String,
    hello: v1::Hello,
    link: &Link,
    queue: Queue,
) -> Error {
    let mut backoff = Backoff::default();
    loop {
        let reason = match connect(&endpoint, &hello, link, &queue, &mut backoff).await {
            Ended::Refused(status) => return Error::Refused(status),
            Ended::Replaced => return Error::Replaced,
            Ended::Lost(reason) => reason,
        };
        let delay = backoff.delay();
        tracing::warn!(
            "Not connected to rspigot at {url} ({reason}), retrying in {} s",
            delay.as_secs()
        );
        tokio::time::sleep(delay).await;
    }
}

async fn connect(
    endpoint: &Endpoint,
    hello: &v1::Hello,
    link: &Link,
    queue: &Queue,
    backoff: &mut Backoff,
) -> Ended {
    let channel = match endpoint.connect().await {
        Ok(channel) => channel,
        Err(error) => return Ended::Lost(causes(&error)),
    };
    let (outbound, said) = mpsc::unbounded_channel();
    let _ = outbound.send(message(from_plugin::Body::Hello(hello.clone())));
    let mut hears = match PluginHostClient::new(channel)
        .connect(UnboundedReceiverStream::new(said))
        .await
    {
        Ok(response) => response.into_inner(),
        Err(status) => return ended(status),
    };
    if let Err(ended) = accepted(&mut hears).await {
        return ended;
    }
    backoff.reset();
    tracing::info!("Connected to rspigot as {} {}", hello.name, hello.version);
    let mut reading = JoinSet::new();
    reading.spawn(read(hears, link.clone(), outbound, queue.clone()));
    match reading.join_next().await {
        Some(Ok(ended)) => ended,
        Some(Err(error)) => Ended::Lost(error.to_string()),
        None => Ended::Lost("the stream reader vanished".to_owned()),
    }
}

trait Hear {
    fn hear(&mut self) -> impl Future<Output = Result<Option<v1::ToPlugin>, Status>>;
}

impl Hear for Streaming<v1::ToPlugin> {
    fn hear(&mut self) -> impl Future<Output = Result<Option<v1::ToPlugin>, Status>> {
        self.message()
    }
}

async fn accepted(hears: &mut impl Hear) -> Result<(), Ended> {
    let mut wait = ANSWER_TIMEOUT;
    loop {
        match tokio::time::timeout(wait, hears.hear()).await {
            Ok(Ok(Some(v1::ToPlugin {
                body: Some(to_plugin::Body::Accepted(_)),
            }))) => return Ok(()),
            Ok(Ok(Some(v1::ToPlugin {
                body: Some(to_plugin::Body::Warning(warning)),
            }))) => {
                tracing::warn!("rspigot: {}", warning.message);
                wait = HELD_TIMEOUT;
            }
            Ok(Ok(Some(_))) => {
                return Err(Ended::Lost(
                    "rspigot spoke before accepting the plugin".to_owned(),
                ));
            }
            Ok(Ok(None)) => return Err(Ended::Lost("rspigot closed the stream".to_owned())),
            Ok(Err(status)) => return Err(ended(status)),
            Err(_) => {
                return Err(Ended::Lost(
                    "rspigot did not accept the plugin in time".to_owned(),
                ));
            }
        }
    }
}

async fn read(
    hears: Streaming<v1::ToPlugin>,
    link: Link,
    outbound: Outbound,
    queue: Queue,
) -> Ended {
    let reply = outbound.downgrade();
    let _attached = link.attach(outbound);
    if queue.send(Queued::Enable(link.clone())).is_err() {
        return Ended::Lost("the plugin stopped handling events".to_owned());
    }
    let ended = listen(hears, &link, &reply, &queue).await;
    let _ = queue.send(Queued::Disable);
    ended
}

async fn listen(
    mut hears: Streaming<v1::ToPlugin>,
    link: &Link,
    reply: &mpsc::WeakUnboundedSender<v1::FromPlugin>,
    queue: &Queue,
) -> Ended {
    loop {
        let body = match hears.message().await {
            Ok(Some(message)) => message.body,
            Ok(None) => return Ended::Lost("rspigot closed the stream".to_owned()),
            Err(status) => return ended(status),
        };
        match body {
            Some(to_plugin::Body::Event(event)) => {
                let queued = Queued::Event {
                    event: Box::new(event),
                    reply: reply.clone(),
                };
                if queue.send(queued).is_err() {
                    return Ended::Lost("the plugin stopped handling events".to_owned());
                }
            }
            Some(to_plugin::Body::Invocation(invocation)) => {
                let queued = Queued::Invocation(Invocation::from_wire(invocation, link));
                if queue.send(queued).is_err() {
                    return Ended::Lost("the plugin stopped handling events".to_owned());
                }
            }
            Some(to_plugin::Body::Answer(answer)) => link.answer(answer),
            Some(to_plugin::Body::Warning(warning)) => {
                tracing::warn!("rspigot: {}", warning.message);
            }
            Some(to_plugin::Body::TaskDue(due)) => link.task_due(due.id),
            Some(to_plugin::Body::Replaced(_)) => return Ended::Replaced,
            Some(to_plugin::Body::Accepted(_)) | None => {}
        }
    }
}

fn causes(error: &tonic::transport::Error) -> String {
    let mut causes: Vec<String> =
        std::iter::successors(std::error::Error::source(error), |cause| cause.source())
            .map(ToString::to_string)
            .collect();
    causes.dedup();
    if causes.is_empty() {
        error.to_string()
    } else {
        causes.join(": ")
    }
}

fn ended(status: Status) -> Ended {
    match status.code() {
        Code::Unauthenticated | Code::InvalidArgument | Code::FailedPrecondition => {
            Ended::Refused(status)
        }
        Code::AlreadyExists => Ended::Replaced,
        code => Ended::Lost(format!("{code:?}: {}", status.message())),
    }
}

async fn handle_events<P: Listeners>(
    plugin: &P,
    link: &Link,
    kinds: &[(u32, EventKind)],
    mut queued: mpsc::UnboundedReceiver<Queued>,
) {
    let mut tasks: Option<(Server, TaskClock)> = None;
    let mut poll = tokio::time::interval(TASK_RESOLUTION);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let next = tokio::select! {
            next = queued.recv() => match next {
                Some(next) => next,
                None => return,
            },
            _ = poll.tick(), if tasks.is_some() => {
                if let Some((server, clock)) = tasks.as_mut() {
                    clock.advance(std::time::Instant::now());
                    plugin.run_tasks(server, clock).await;
                }
                continue;
            }
        };
        match next {
            Queued::Event { event, reply } => handle(plugin, link, kinds, *event, reply).await,
            Queued::Invocation(invocation) => plugin.invoke(invocation).await,
            Queued::Enable(connected) => {
                let server = Server::new(connected);
                plugin.enable(&server).await;
                tasks = Some((server, TaskClock::new(std::time::Instant::now())));
            }
            Queued::Disable => {
                tasks = None;
                link.cancel_all_tasks();
                plugin.disable().await;
            }
        }
    }
}

async fn handle<P: Listeners>(
    plugin: &P,
    link: &Link,
    kinds: &[(u32, EventKind)],
    mut event: v1::Event,
    reply: mpsc::WeakUnboundedSender<v1::FromPlugin>,
) {
    let id = event.id;
    let handlers = std::mem::take(&mut event.handlers);
    if event.phase() != v1::Phase::Decide {
        match Received::from_wire(event) {
            Ok(received) => {
                for (handler, call) in calls(&received, &handlers, kinds, link) {
                    match call {
                        Some(call) => {
                            handling(id, plugin.dispatch_at(handler, call)).await;
                        }
                        None => tracing::warn!("Skipped event {id} for handler {handler}"),
                    }
                }
            }
            Err(error) => tracing::warn!("Skipped event {id}: {error}"),
        }
        return;
    }
    let Some(reply) = reply.upgrade() else {
        tracing::debug!("Dropped event {id}: its connection is gone");
        return;
    };
    let mut verdicts = Vec::with_capacity(handlers.len());
    match Received::from_wire(event) {
        Ok(received) => {
            for (handler, call) in calls(&received, &handlers, kinds, link) {
                let Some(call) = call else {
                    tracing::warn!("Let event {id} through for handler {handler}");
                    continue;
                };
                let verdict = match handling(id, plugin.dispatch_at(handler, call)).await {
                    Outcome::Decided(verdict) => verdict,
                    Outcome::Observed => Verdict::default(),
                };
                let cancelled = verdict.cancel;
                verdicts.push(verdict.into_wire(handler));
                if cancelled && !received.writes_when_cancelled() {
                    break;
                }
            }
        }
        Err(error) => tracing::warn!("Let event {id} through: {error}"),
    }
    let _ = reply.send(message(from_plugin::Body::Decision(v1::Decision {
        event: id,
        verdicts,
    })));
}

fn calls(
    received: &Received,
    handlers: &[u32],
    kinds: &[(u32, EventKind)],
    link: &Link,
) -> Vec<(u32, Option<crate::Call>)> {
    handlers
        .iter()
        .map(|&handler| {
            let call = kinds
                .iter()
                .find(|(id, _)| *id == handler)
                .and_then(|&(_, kind)| {
                    crate::meta::scoped(&received.blocks, || received.call(handler, kind, link))
                })
                .and_then(|call| {
                    call.map_err(|error| tracing::warn!("Event {}: {error}", received.meta.id))
                        .ok()
                });
            (handler, call)
        })
        .collect()
}

struct Backoff(Duration);

impl Default for Backoff {
    fn default() -> Self {
        Self(FIRST_RETRY)
    }
}

impl Backoff {
    fn delay(&mut self) -> Duration {
        let delay = self.0;
        self.0 = (delay * 2).min(LAST_RETRY);
        delay
    }

    const fn reset(&mut self) {
        self.0 = FIRST_RETRY;
    }
}

#[cfg(test)]
mod tests {
    use rspigot_plugin_protocol::v1::patch;
    use tokio::time::Instant;

    use super::*;
    use crate::Call;

    fn snapshot() -> v1::PlayerSnapshot {
        v1::PlayerSnapshot {
            permissions: Vec::new(),
            uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 64.0,
                z: -3.5,
            }),
            game_mode: v1::GameMode::Survival as i32,
            rotation: Some(v1::Rotation {
                yaw: 0.0,
                pitch: 0.0,
            }),
            ..Default::default()
        }
    }

    fn zombie() -> v1::EntitySnapshot {
        v1::EntitySnapshot {
            entity_id: 40,
            uuid: "00000000-0000-0000-0000-000000000028".to_owned(),
            kind: "minecraft:zombie".to_owned(),
            position: Some(v1::Vec3 {
                x: 2.0,
                y: 64.0,
                z: -3.5,
            }),
            player: None,
            ..Default::default()
        }
    }

    fn received(payload: event::Payload) -> Received {
        Received::from_wire(v1::Event {
            id: 9,
            handlers: vec![1],
            phase: v1::Phase::Decide as i32,
            cancelled: false,
            payload: Some(payload),
            block_meta: Vec::new(),
        })
        .expect("an event with a payload")
    }

    #[test]
    fn a_child_event_reaches_a_parent_handler_as_the_parent_type() {
        let link = Link::default();
        let hit = received(event::Payload::EntityDamageByEntity(
            v1::EntityDamageByEntity {
                cause: v1::EntityDamageCause::EntityAttack as i32,
                damage: 4.0,
                damage_source: Some(v1::DamageSource {
                    kind: "minecraft:mob_attack".to_owned(),
                    causing: Some(zombie()),
                    direct: Some(zombie()),
                    location: None,
                }),
                damager: Some(zombie()),
                entity: Some(zombie()),
                entity_type: "minecraft:zombie".to_owned(),
                final_damage: 4.0,
            },
        ));
        let Some(Ok(Call::EntityDamage { handler, mut event })) =
            hit.call(1, EventKind::EntityDamage, &link)
        else {
            panic!("an EntityDamage handler hears the hit");
        };
        assert_eq!((handler, event.id(), event.damage()), (1, 9, 4.0));
        event.set_damage(1.5);
        assert_eq!(
            Verdict::from(event).patch.map(|patch| *patch),
            Some(v1::Patch {
                change: Some(patch::Change::EntityDamage(v1::EntityDamagePatch {
                    damage: Some(1.5)
                })),
            })
        );
        assert!(matches!(
            hit.call(2, EventKind::EntityDamageByEntity, &link),
            Some(Ok(Call::EntityDamageByEntity { handler: 2, .. }))
        ));
        assert!(hit.call(3, EventKind::EntityDamageByBlock, &link).is_none());
        assert!(hit.call(4, EventKind::PlayerMove, &link).is_none());
    }

    #[test]
    fn a_damager_that_is_a_player_gives_the_player_and_a_mob_gives_none() {
        let link = Link::default();
        let hit = |damager: v1::EntitySnapshot| {
            received(event::Payload::EntityDamageByEntity(
                v1::EntityDamageByEntity {
                    cause: v1::EntityDamageCause::EntityAttack as i32,
                    damage_source: Some(v1::DamageSource::default()),
                    damager: Some(damager),
                    entity: Some(zombie()),
                    entity_type: "minecraft:zombie".to_owned(),
                    ..v1::EntityDamageByEntity::default()
                },
            ))
        };
        let by_player = v1::EntitySnapshot {
            entity_id: 12,
            uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
            kind: "minecraft:player".to_owned(),
            position: snapshot().position,
            player: Some(snapshot()),
            ..Default::default()
        };
        let Some(Ok(Call::EntityDamageByEntity { event, .. })) =
            hit(by_player).call(1, EventKind::EntityDamageByEntity, &link)
        else {
            panic!("an EntityDamageByEntity handler hears the hit");
        };
        assert_eq!(
            event.player().map(|player| player.name.as_str()),
            Some("Steve")
        );
        let Some(Ok(Call::EntityDamageByEntity { event, .. })) =
            hit(zombie()).call(1, EventKind::EntityDamageByEntity, &link)
        else {
            panic!("an EntityDamageByEntity handler hears the hit");
        };
        assert!(event.player().is_none());
    }

    #[test]
    fn a_player_death_reaches_an_entity_death_handler_with_the_player_as_an_entity() {
        let death = received(event::Payload::PlayerDeath(v1::PlayerDeath {
            damage_source: Some(v1::DamageSource {
                kind: "minecraft:fall".to_owned(),
                ..v1::DamageSource::default()
            }),
            death_message: Some("Steve fell".to_owned()),
            entity: Some(snapshot()),
            entity_type: "minecraft:player".to_owned(),
            ..v1::PlayerDeath::default()
        }));
        let Some(Ok(Call::EntityDeath { event, .. })) =
            death.call(1, EventKind::EntityDeath, &Link::default())
        else {
            panic!("an EntityDeath handler hears the death");
        };
        assert_eq!(event.entity().entity_id, 12);
        assert_eq!(event.entity().kind, crate::EntityType::Player);
    }

    #[test]
    fn cancelling_a_player_interaction_denies_both_results() {
        let click = received(event::Payload::PlayerInteract(v1::PlayerInteract {
            action: v1::Action::RightClickBlock as i32,
            block: true,
            block_face: v1::Face::Up as i32,
            material: "minecraft:stone".to_owned(),
            player: Some(snapshot()),
            use_interacted_block: v1::EventResult::Default as i32,
            use_item_in_hand: v1::EventResult::Default as i32,
            ..v1::PlayerInteract::default()
        }));
        let Some(Ok(Call::PlayerInteract { mut event, .. })) =
            click.call(1, EventKind::PlayerInteract, &Link::default())
        else {
            panic!("a PlayerInteract handler hears the click");
        };
        assert!(!event.is_cancelled());
        event.cancel();
        assert!(event.is_cancelled());
        let verdict = Verdict::from(event);
        assert!(!verdict.cancel);
        assert_eq!(
            verdict.patch.map(|patch| *patch),
            Some(v1::Patch {
                change: Some(patch::Change::PlayerInteract(v1::PlayerInteractPatch {
                    use_interacted_block: Some(v1::EventResult::Deny as i32),
                    use_item_in_hand: Some(v1::EventResult::Deny as i32),
                })),
            })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn no_acceptance_within_ten_seconds_is_a_lost_connection() {
        let started = Instant::now();

        let accepted = accepted(&mut Silent).await;

        assert!(matches!(accepted, Err(Ended::Lost(_))));
        assert!(started.elapsed() >= ANSWER_TIMEOUT);
    }

    struct Silent;

    impl Hear for Silent {
        fn hear(&mut self) -> impl Future<Output = Result<Option<v1::ToPlugin>, Status>> {
            std::future::pending()
        }
    }

    struct Told(std::collections::VecDeque<v1::ToPlugin>);

    impl Hear for Told {
        fn hear(&mut self) -> impl Future<Output = Result<Option<v1::ToPlugin>, Status>> {
            let next = self.0.pop_front();
            async move {
                match next {
                    Some(next) => Ok(Some(next)),
                    None => std::future::pending().await,
                }
            }
        }
    }

    fn told(body: to_plugin::Body) -> v1::ToPlugin {
        v1::ToPlugin { body: Some(body) }
    }

    #[tokio::test(start_paused = true)]
    async fn a_warning_before_the_acceptance_holds_the_plugin_instead_of_failing() {
        let mut hears = Told(
            [
                told(to_plugin::Body::Warning(v1::Warning {
                    message: "shop is held until base connect".to_owned(),
                })),
                told(to_plugin::Body::Accepted(v1::Accepted {})),
            ]
            .into(),
        );
        assert!(accepted(&mut hears).await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_plugin_waits_far_longer_than_the_usual_acceptance_timeout() {
        let started = Instant::now();
        let mut hears = Told(
            [told(to_plugin::Body::Warning(v1::Warning {
                message: "held".to_owned(),
            }))]
            .into(),
        );
        let ended = accepted(&mut hears).await;
        assert!(matches!(ended, Err(Ended::Lost(_))));
        assert!(started.elapsed() >= HELD_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn any_other_message_before_the_acceptance_is_a_lost_connection() {
        let mut hears = Told([told(to_plugin::Body::Replaced(v1::Replaced {}))].into());
        assert!(matches!(accepted(&mut hears).await, Err(Ended::Lost(_))));
    }

    #[tokio::test]
    async fn a_failed_connection_says_why() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let endpoint = Endpoint::from_shared(format!("http://{address}")).unwrap();
        let (queue, _queued) = mpsc::unbounded_channel();

        let ended = connect(
            &endpoint,
            &v1::Hello::default(),
            &Link::default(),
            &queue,
            &mut Backoff::default(),
        )
        .await;

        let Ended::Lost(reason) = ended else {
            panic!("expected a lost connection");
        };
        assert!(reason.contains("refused"), "{reason}");
    }

    #[test]
    fn the_retry_delay_doubles_up_to_thirty_seconds_and_starts_over_once_accepted() {
        let mut backoff = Backoff::default();
        let delays: Vec<u64> = (0..7).map(|_| backoff.delay().as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30]);
        backoff.reset();
        assert_eq!(backoff.delay(), FIRST_RETRY);
    }
}
