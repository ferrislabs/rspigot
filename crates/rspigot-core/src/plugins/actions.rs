use std::fmt;

use rspigot_domain::player::PlayerId;

use crate::plugins::{
    Answer, BlockOrder, EntityOrder, Order, PlayerOrder, ServerOrder, ToPlugin, WorldOrder,
};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionErrorKind {
    InvalidArgument,
    TargetGone,
    IncompatibleState,
    Cancelled,
    NotFound,
    LimitReached,
}

impl ActionErrorKind {
    const fn label(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid argument",
            Self::TargetGone => "target gone",
            Self::IncompatibleState => "incompatible state",
            Self::Cancelled => "cancelled",
            Self::NotFound => "not found",
            Self::LimitReached => "limit reached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionError {
    pub kind: ActionErrorKind,
    pub reason: String,
}

impl ActionError {
    pub fn new(kind: ActionErrorKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
        }
    }

    pub(crate) fn player_gone() -> Self {
        Self::new(ActionErrorKind::TargetGone, "the player is not online")
    }
}

impl From<String> for ActionError {
    fn from(reason: String) -> Self {
        Self::new(ActionErrorKind::InvalidArgument, reason)
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.label(), self.reason)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) connection: u64,
    pub(crate) request: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Done,
    Pending,
}

fn no_world(world: &rspigot_domain::Identifier) -> ActionError {
    ActionError::new(
        ActionErrorKind::NotFound,
        format!("there is no world {world}"),
    )
}

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_action(&mut self, connection: u64, request: u64, order: Order) {
        let reply = Reply {
            connection,
            request,
        };
        let outcome = match order {
            Order::Player { player, order } => self.act_on_player(connection, reply, player, order),
            Order::Server(ServerOrder::CreateInventory {
                id,
                rows,
                title,
                locked,
            }) => self
                .create_plugin_inventory(connection, id, rows, &title, locked)
                .map(|()| Outcome::Done),
            Order::Server(ServerOrder::CreateWorld(spec)) => match self.create_world(spec) {
                Ok(key) => {
                    self.plugins.send(
                        connection,
                        ToPlugin::Answer {
                            request,
                            answer: Answer::WorldCreated(key.to_string()),
                        },
                    );
                    Ok(Outcome::Pending)
                }
                Err(error) => Err(error),
            },
            Order::Server(ServerOrder::AddRecipe(spec)) => self
                .add_plugin_recipe(connection, &spec)
                .map(|()| Outcome::Done),
            Order::World(WorldOrder::Delete { files }) => {
                let key = self.world.dimension.key.clone();
                self.ask_world_unload(key, files, reply)
                    .map(|()| Outcome::Pending)
            }
            Order::InWorld { world, order } => match *order {
                Order::World(WorldOrder::Delete { files }) => self
                    .ask_world_unload(world, files, reply)
                    .map(|()| Outcome::Pending),
                Order::Entity {
                    entity,
                    order: EntityOrder::Meta(order),
                } => self
                    .within_world(&world, |server| server.entity_meta_change(entity, order))
                    .unwrap_or_else(|| Err(no_world(&world)))
                    .map(|()| Outcome::Done),
                Order::Block {
                    pos,
                    order: BlockOrder::Meta(order),
                } => self
                    .within_world(&world, |server| server.block_meta_change(pos, order))
                    .unwrap_or_else(|| Err(no_world(&world)))
                    .map(|()| Outcome::Done),
                other => Err(ActionError::new(
                    ActionErrorKind::InvalidArgument,
                    format!("{} does not report a result", other.name()),
                )),
            },
            Order::Entity {
                entity,
                order: EntityOrder::Meta(order),
            } => self
                .entity_meta_change(entity, order)
                .map(|()| Outcome::Done),
            Order::Block {
                pos,
                order: BlockOrder::Meta(order),
            } => self.block_meta_change(pos, order).map(|()| Outcome::Done),
            other => Err(ActionError::new(
                ActionErrorKind::InvalidArgument,
                format!("{} does not report a result", other.name()),
            )),
        };
        match outcome {
            Ok(Outcome::Pending) => {}
            Ok(Outcome::Done) => self.answer_action(reply, Ok(())),
            Err(error) => self.answer_action(reply, Err(error)),
        }
    }

    fn act_on_player(
        &mut self,
        connection: u64,
        reply: Reply,
        player: PlayerId,
        order: PlayerOrder,
    ) -> Result<Outcome, ActionError> {
        match order {
            PlayerOrder::Teleport(_)
            | PlayerOrder::TeleportFacing(..)
            | PlayerOrder::TeleportTo { .. } => {
                self.begin_teleport(player, order, Some(reply))?;
            }
            PlayerOrder::Meta(order) => {
                self.player_meta_change(player, order)?;
                return Ok(Outcome::Done);
            }
            PlayerOrder::Transfer { host, port } => {
                self.transfer_for_plugin(player, &host, port)?;
                return Ok(Outcome::Done);
            }
            PlayerOrder::OpenInventory(inventory) => {
                self.begin_plugin_open(connection, player, inventory, Some(reply))?;
            }
            other => {
                return Err(ActionError::new(
                    ActionErrorKind::InvalidArgument,
                    format!("{} does not report a result", other.name()),
                ));
            }
        }
        Ok(Outcome::Pending)
    }

    pub(crate) fn answer_action(&self, reply: Reply, result: Result<(), ActionError>) {
        let answer = match result {
            Ok(()) => Answer::Done,
            Err(error) => Answer::Failed(error),
        };
        self.plugins.send(
            reply.connection,
            ToPlugin::Answer {
                request: reply.request,
                answer,
            },
        );
    }
}
