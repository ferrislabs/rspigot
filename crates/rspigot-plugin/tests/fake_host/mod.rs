#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use rspigot_plugin::{
    ClickKind, Error, InventoryClick, InventoryClose, InventoryRef, InventoryTypeSlotType,
    ItemStack, Player, PlayerJoin, Plugin, Server, listeners, run_with,
};
use rspigot_plugin_protocol::v1::plugin_host_server::{PluginHost, PluginHostServer};
use rspigot_plugin_protocol::v1::{self, answer, event, from_plugin, question, to_plugin};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::transport::server::TcpIncoming;
use tonic::{Request, Response, Status, Streaming};

pub const STEVE: &str = "00000000-0000-0000-0000-000000000001";
pub const ALEX: &str = "00000000-0000-0000-0000-000000000002";

pub type Reply = Result<v1::ToPlugin, Status>;
pub type Replies = UnboundedReceiverStream<Reply>;
type Opened = Pin<Box<dyn Future<Output = Result<Response<Replies>, Status>> + Send>>;

pub struct Knock {
    pub hello: v1::Hello,
    pub incoming: Streaming<v1::FromPlugin>,
    pub answer: oneshot::Sender<Result<Replies, Status>>,
}

impl Knock {
    pub fn accept(self) -> Session {
        let (says, said) = mpsc::unbounded_channel();
        let session = Session {
            says,
            hears: self.incoming,
        };
        session.say(to_plugin::Body::Accepted(v1::Accepted {}));
        let _ = self.answer.send(Ok(UnboundedReceiverStream::new(said)));
        session
    }

    pub fn refuse(self, status: Status) {
        let _ = self.answer.send(Err(status));
    }
}

pub struct Session {
    pub says: mpsc::UnboundedSender<Reply>,
    pub hears: Streaming<v1::FromPlugin>,
}

impl Session {
    pub fn say(&self, body: to_plugin::Body) {
        self.says
            .send(Ok(v1::ToPlugin { body: Some(body) }))
            .unwrap();
    }

    pub fn end(self, status: Status) {
        self.says.send(Err(status)).unwrap();
    }

    pub async fn hear(&mut self) -> from_plugin::Body {
        self.hear_message().await.body.unwrap()
    }

    pub async fn hear_message(&mut self) -> v1::FromPlugin {
        within(self.hears.message()).await.unwrap().unwrap()
    }

    pub async fn acted(&mut self) -> (u64, from_plugin::Body) {
        let message = self.hear_message().await;
        let request = message.request.expect("an order that expects a result");
        (request, message.body.unwrap())
    }

    pub async fn performed(&mut self, expected: from_plugin::Body) {
        let (request, body) = self.acted().await;
        assert_eq!(body, expected);
        self.say(answer(request, answer::Body::Done(v1::Empty {})));
    }

    pub async fn asked(&mut self) -> (u64, question::Question) {
        match self.hear().await {
            from_plugin::Body::Question(v1::Question {
                request,
                question: Some(question),
            }) => (request, question),
            body => panic!("expected a question, heard {body:?}"),
        }
    }
}

pub struct Stub {
    pub knocks: mpsc::UnboundedSender<Knock>,
}

impl Stub {
    fn open(&self, request: Request<Streaming<v1::FromPlugin>>) -> Opened {
        let knocks = self.knocks.clone();
        Box::pin(async move {
            let mut incoming = request.into_inner();
            let hello = match incoming.message().await? {
                Some(v1::FromPlugin {
                    body: Some(from_plugin::Body::Hello(hello)),
                    ..
                }) => hello,
                _ => return Err(Status::invalid_argument("expected a Hello")),
            };
            let (answer, answered) = oneshot::channel();
            let gone = || Status::unavailable("the stub host is gone");
            knocks
                .send(Knock {
                    hello,
                    incoming,
                    answer,
                })
                .map_err(|_| gone())?;
            answered.await.map_err(|_| gone())?.map(Response::new)
        })
    }
}

impl PluginHost for Stub {
    type ConnectStream = Replies;

    fn connect<'host, 'call>(
        &'host self,
        request: Request<Streaming<v1::FromPlugin>>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Replies>, Status>> + Send + 'call>>
    where
        'host: 'call,
        Self: 'call,
    {
        self.open(request)
    }
}

pub struct Host {
    pub url: String,
    pub knocks: mpsc::UnboundedReceiver<Knock>,
}

impl Host {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (knocks, knocked) = mpsc::unbounded_channel();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(PluginHostServer::new(Stub { knocks }))
                .serve_with_incoming(TcpIncoming::from(listener)),
        );
        Self {
            url,
            knocks: knocked,
        }
    }

    pub async fn knock(&mut self) -> Knock {
        within(self.knocks.recv()).await.unwrap()
    }
}

pub async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("timed out")
}

pub fn snapshot(uuid: &str, name: &str, entity_id: i32) -> v1::PlayerSnapshot {
    v1::PlayerSnapshot {
        uuid: uuid.to_owned(),
        name: name.to_owned(),
        entity_id,
        world: "minecraft:overworld".to_owned(),
        position: Some(v1::Vec3 {
            x: 1.0,
            y: 64.0,
            z: -3.5,
        }),
        game_mode: v1::GameMode::Creative as i32,
        rotation: Some(v1::Rotation {
            yaw: 90.0,
            pitch: -30.0,
        }),
    }
}

pub fn steve() -> v1::PlayerSnapshot {
    snapshot(STEVE, "Steve", 12)
}

pub fn alex() -> v1::PlayerSnapshot {
    snapshot(ALEX, "Alex", 13)
}

pub fn event(
    id: u64,
    handlers: &[u32],
    phase: v1::Phase,
    cancelled: bool,
    payload: event::Payload,
) -> to_plugin::Body {
    to_plugin::Body::Event(v1::Event {
        id,
        handlers: handlers.to_vec(),
        phase: phase as i32,
        cancelled,
        payload: Some(payload),
    })
}

pub fn answer(request: u64, body: answer::Body) -> to_plugin::Body {
    to_plugin::Body::Answer(v1::Answer {
        request,
        body: Some(body),
    })
}

pub fn join(id: u64) -> to_plugin::Body {
    event(
        id,
        &[1],
        v1::Phase::Settled,
        false,
        event::Payload::PlayerJoin(v1::PlayerJoin {
            player: Some(steve()),
            join_message: None,
        }),
    )
}

pub type Script = fn(Player) -> Pin<Box<dyn Future<Output = ()> + Send>>;

pub type Enable = fn(Server) -> Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Debug)]
pub struct Clicked {
    pub id: u64,
    pub who: String,
    pub inventory: InventoryRef,
    pub clicked: Option<InventoryRef>,
    pub slot: i32,
    pub raw_slot: i32,
    pub slot_type: InventoryTypeSlotType,
    pub click: ClickKind,
    pub hotbar_button: i32,
    pub item: Option<ItemStack>,
    pub title: String,
    pub viewers: usize,
    pub cancelled: bool,
}

#[derive(Debug)]
pub enum Mark {
    Enabled,
    Finished,
    Clicked(Clicked),
    Closed(InventoryClose),
    Disabled,
}

#[derive(Plugin)]
#[plugin(name = "caller", version = "1.0.0")]
pub struct Caller {
    pub script: Script,
    pub enable: Enable,
    pub marks: mpsc::UnboundedSender<Mark>,
}

#[listeners]
impl Caller {
    async fn on_player_join(&self, event: &PlayerJoin) {
        (self.script)(event.player().clone()).await;
        let _ = self.marks.send(Mark::Finished);
    }

    async fn on_inventory_click(&self, event: &InventoryClick) {
        let _ = self.marks.send(Mark::Clicked(Clicked {
            id: event.id(),
            who: event.player().name.clone(),
            inventory: event.inventory().clone(),
            clicked: event.clicked_inventory().cloned(),
            slot: event.slot(),
            raw_slot: event.raw_slot(),
            slot_type: event.slot_type(),
            click: event.click(),
            hotbar_button: event.hotbar_button(),
            item: event.current_item().cloned(),
            title: event.view().title.clone(),
            viewers: event.viewers().len(),
            cancelled: event.is_cancelled(),
        }));
    }

    async fn on_inventory_close(&self, event: &InventoryClose) {
        let _ = self.marks.send(Mark::Closed(event.clone()));
    }

    async fn on_enable(&self, server: &Server) {
        (self.enable)(server.clone()).await;
        let _ = self.marks.send(Mark::Enabled);
    }

    async fn on_disable(&self) {
        tokio::task::yield_now().await;
        let _ = self.marks.send(Mark::Disabled);
    }
}

pub fn idle(_: Server) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async {})
}

pub struct Marks(mpsc::UnboundedReceiver<Mark>);

impl Marks {
    pub async fn next(&mut self) -> Mark {
        within(self.0.recv()).await.expect("the plugin stopped")
    }

    pub async fn expect(&mut self, expected: fn(&Mark) -> bool) {
        let mark = self.next().await;
        assert!(expected(&mark), "unexpected {mark:?}");
    }

    pub fn left(&mut self) -> Vec<Mark> {
        std::iter::from_fn(|| self.0.try_recv().ok()).collect()
    }
}

pub fn start_caller(
    host: &Host,
    script: Script,
    enable: Enable,
) -> (Marks, JoinHandle<Result<(), Error>>) {
    let (marks, marked) = mpsc::unbounded_channel();
    let ended = tokio::spawn(run_with(
        Caller {
            script,
            enable,
            marks,
        },
        host.url.clone(),
        None,
    ));
    (Marks(marked), ended)
}

pub struct Called {
    pub session: Session,
    pub marks: Marks,
}

impl Called {
    pub async fn finished(&mut self) {
        let mark = self.marks.next().await;
        assert!(
            matches!(mark, Mark::Finished),
            "the script did not finish: {mark:?}"
        );
    }
}

pub async fn calling(script: Script) -> Called {
    let mut host = Host::start().await;
    let (mut marks, _) = start_caller(&host, script, idle);
    let session = host.knock().await.accept();
    marks.expect(|mark| matches!(mark, Mark::Enabled)).await;
    session.say(join(1));
    Called { session, marks }
}
