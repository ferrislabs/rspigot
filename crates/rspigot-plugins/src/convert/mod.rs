mod block;
mod boss_bar;
mod command;
mod contract;
mod custom_inventory;
mod decision;
mod edits;
mod entity;
mod events;
mod inventory;
mod meta;
mod npc;
mod player;
mod scoreboard;
mod server;
mod shared;
mod tiles;
mod world;

use rspigot_core::plugins::{
    Access, ActionErrorKind, Answer, FromPlugin, HandlerSpec, Phase, PluginIntro, Priority,
    Question, ToPlugin,
};
use rspigot_domain::{EntityId, Identifier};
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question, to_plugin};

use player::{wire_player, wire_state};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Malformed {
    #[error("handler {handler} has an unknown {field} value {value}")]
    Unknown {
        handler: u32,
        field: &'static str,
        value: i32,
    },
    #[error("`{0}` is not a player UUID")]
    Player(String),
    #[error("`{0}` is not an entity UUID")]
    Entity(String),
    #[error("`{0}` is not a UUID")]
    Uuid(String),
    #[error("the message carries nothing")]
    Empty,
    #[error("the plugin already introduced itself")]
    SecondHello,
    #[error("unknown {field} value {value}")]
    Enum { field: &'static str, value: i32 },
    #[error("{0} ms is not a time the server can hold")]
    Time(i64),
    #[error("`{0}` is not a world")]
    World(String),
    #[error("{value} is out of range for {field}")]
    OutOfRange { field: &'static str, value: i64 },
}

pub(crate) fn intro(hello: v1::Hello) -> Result<PluginIntro, Malformed> {
    Ok(PluginIntro {
        name: hello.name,
        version: hello.version,
        handlers: hello
            .handlers
            .into_iter()
            .map(handler)
            .collect::<Result<_, _>>()?,
        commands: hello
            .commands
            .into_iter()
            .map(command::spec)
            .collect::<Result<_, _>>()?,
    })
}

fn handler(handler: v1::Handler) -> Result<HandlerSpec, Malformed> {
    let unknown = |field, value| Malformed::Unknown {
        handler: handler.id,
        field,
        value,
    };
    let event = events::event_kind(handler.event).ok_or_else(|| unknown("event", handler.event))?;
    let access = match v1::Access::try_from(handler.access) {
        Ok(v1::Access::Observe) => Access::Observe,
        Ok(v1::Access::Decide) => Access::Decide,
        Ok(v1::Access::Unspecified) | Err(_) => return Err(unknown("access", handler.access)),
    };
    let priority = match v1::Priority::try_from(handler.priority) {
        Ok(v1::Priority::Lowest) => Priority::Lowest,
        Ok(v1::Priority::Low) => Priority::Low,
        Ok(v1::Priority::Unspecified | v1::Priority::Normal) => Priority::Normal,
        Ok(v1::Priority::High) => Priority::High,
        Ok(v1::Priority::Highest) => Priority::Highest,
        Err(_) => return Err(unknown("priority", handler.priority)),
    };
    Ok(HandlerSpec {
        id: handler.id,
        event,
        access,
        priority,
        ignore_cancelled: handler.ignore_cancelled,
    })
}

pub(crate) fn from_plugin(
    message: v1::FromPlugin,
    warnings: &mut Vec<String>,
) -> Result<FromPlugin, Malformed> {
    let origin = message.origin;
    let request = message.request;
    let ordered = |order| match (request, origin) {
        (Some(request), _) => FromPlugin::Action { request, order },
        (None, Some(event)) => FromPlugin::Reaction { event, order },
        (None, None) => FromPlugin::Order(order),
    };
    let world = message.in_world;
    let converted = match message.body.ok_or(Malformed::Empty)? {
        from_plugin::Body::Hello(_) => return Err(Malformed::SecondHello),
        from_plugin::Body::Decision(decided) => decision::decision(decided, warnings),
        from_plugin::Body::Player(order) => ordered(player::order(order)?),
        from_plugin::Body::Server(order) => ordered(server::order(order)?),
        from_plugin::Body::Inventory(order) => ordered(inventory::order(order)?),
        from_plugin::Body::World(order) => ordered(world::order(order)?),
        from_plugin::Body::Block(order) => ordered(block::order(order)?),
        from_plugin::Body::Entity(order) => ordered(entity::order(order)?),
        from_plugin::Body::CustomInventory(order) => ordered(custom_inventory::order(order)?),
        from_plugin::Body::Scoreboard(order) => ordered(scoreboard::order(order)?),
        from_plugin::Body::BossBar(order) => ordered(boss_bar::order(order)?),
        from_plugin::Body::Npc(order) => ordered(npc::order(order)?),
        from_plugin::Body::Question(asked) => question(asked)?,
        from_plugin::Body::RegisterCommand(spec) => {
            FromPlugin::RegisterCommand(command::spec(spec)?)
        }
        from_plugin::Body::UnregisterCommand(name) => FromPlugin::UnregisterCommand(name),
    };
    scoped(world, converted)
}

fn scoped(world: Option<String>, message: FromPlugin) -> Result<FromPlugin, Malformed> {
    let Some(world) = world.filter(|world| !world.is_empty()) else {
        return Ok(message);
    };
    let world = Identifier::parse(&world).map_err(|_| Malformed::World(world))?;
    let wrap = |order: rspigot_core::plugins::Order| match order {
        rspigot_core::plugins::Order::World(_)
        | rspigot_core::plugins::Order::Block { .. }
        | rspigot_core::plugins::Order::Entity { .. }
        | rspigot_core::plugins::Order::Npc { .. } => rspigot_core::plugins::Order::InWorld {
            world: world.clone(),
            order: Box::new(order),
        },
        other => other,
    };
    Ok(match message {
        FromPlugin::Order(order) => FromPlugin::Order(wrap(order)),
        FromPlugin::Reaction { event, order } => FromPlugin::Reaction {
            event,
            order: wrap(order),
        },
        FromPlugin::Action { request, order } => FromPlugin::Action {
            request,
            order: wrap(order),
        },
        FromPlugin::Question { request, question } => FromPlugin::Question {
            request,
            question: match question {
                Question::World
                | Question::Block(_)
                | Question::Tile(_)
                | Question::BlockInfo(_)
                | Question::Entities { .. }
                | Question::Entity(_)
                | Question::AttributeValue { .. }
                | Question::Spawn { .. }
                | Question::SpawnWith { .. }
                | Question::DropItem { .. }
                | Question::CreateNpc(_)
                | Question::NpcByKey(_)
                | Question::Npcs
                | Question::Npc(_)
                | Question::Meta { .. }
                | Question::FindBlocks { .. }
                | Question::EntitiesWithMeta { .. } => Question::InWorld {
                    world,
                    question: Box::new(question),
                },
                other => other,
            },
        },
        other => other,
    })
}

fn question(asked: v1::Question) -> Result<FromPlugin, Malformed> {
    let question = match asked.question.ok_or(Malformed::Empty)? {
        question::Question::Player(player) => Question::Player(player::id(player)?),
        question::Question::Players(_) => Question::Players,
        question::Question::PlayerNamed(name) => Question::PlayerNamed(name),
        question::Question::Inventory(player) => Question::Inventory(player::id(player)?),
        question::Question::TargetBlock(query) => player::target_block(query)?,
        question::Question::TargetEntity(query) => player::target_entity(query)?,
        question::Question::Server(_) => Question::Server,
        question::Question::Whitelist(_) => Question::Whitelist,
        question::Question::Operators(_) => Question::Operators,
        question::Question::Bans(_) => Question::Bans,
        question::Question::IpBans(_) => Question::IpBans,
        question::Question::World(_) => Question::World,
        question::Question::Block(pos) => Question::Block(shared::block_pos(pos)),
        question::Question::Tile(pos) => Question::Tile(shared::block_pos(pos)),
        question::Question::BlockInfo(pos) => Question::BlockInfo(shared::block_pos(pos)),
        question::Question::Entities(query) => entity::query(query)?,
        question::Question::Entity(id) => Question::Entity(EntityId(id)),
        question::Question::Spawn(spawn) => entity::spawn(spawn)?,
        question::Question::DropItem(drop) => entity::drop_item(drop)?,
        question::Question::CustomInventory(id) => Question::CustomInventory(id),
        question::Question::Scoreboard(board) => Question::Scoreboard(board),
        question::Question::NewScoreboard(_) => Question::NewScoreboard,
        question::Question::CreateBossBar(create) => {
            Question::CreateBossBar(boss_bar::spec(create))
        }
        question::Question::BossBar(bar) => Question::BossBar(bar),
        question::Question::BossBars(_) => Question::BossBars,
        question::Question::PermissionGroups(_) => Question::PermissionGroups,
        question::Question::AdvancementProgress(query) => Question::AdvancementProgress {
            player: player::id(query.player)?,
            advancement: query.advancement,
        },
        question::Question::DoneAdvancements(player) => {
            Question::DoneAdvancements(player::id(player)?)
        }
        question::Question::AttributeValue(query) => entity::attribute_query(query)?,
        question::Question::CreateNpc(create) => Question::CreateNpc(npc::spec(create)?),
        question::Question::NpcByKey(key) => Question::NpcByKey(key),
        question::Question::Npcs(_) => Question::Npcs,
        question::Question::Npc(id) => Question::Npc(EntityId(id)),
        question::Question::Meta(query) => meta::query(query)?,
        question::Question::FindBlocks(find) => meta::find_blocks(find)?,
        question::Question::EntitiesWithMeta(query) => Question::EntitiesWithMeta {
            key: query.key,
            volatile: query.volatile,
        },
        question::Question::PlayersWithMeta(query) => Question::PlayersWithMeta {
            key: query.key,
            volatile: query.volatile,
        },
    };
    Ok(FromPlugin::Question {
        request: asked.request,
        question,
    })
}

pub(crate) fn to_plugin(message: ToPlugin) -> v1::ToPlugin {
    let body = match message {
        ToPlugin::Accepted => to_plugin::Body::Accepted(v1::Accepted {}),
        ToPlugin::Event {
            id,
            handlers,
            phase,
            cancelled,
            payload,
            blocks,
        } => to_plugin::Body::Event(v1::Event {
            id,
            handlers,
            phase: wire_phase(phase) as i32,
            cancelled,
            block_meta: blocks.into_iter().map(meta::wire_block).collect(),
            payload: Some(events::wire_payload(payload)),
        }),
        ToPlugin::Answer { request, answer } => to_plugin::Body::Answer(v1::Answer {
            request,
            body: Some(wire_answer(answer)),
        }),
        ToPlugin::Warning(message) => to_plugin::Body::Warning(v1::Warning { message }),
        ToPlugin::Replaced => to_plugin::Body::Replaced(v1::Replaced {}),
        ToPlugin::Invocation {
            command,
            sender,
            arguments,
        } => to_plugin::Body::Invocation(command::invocation(command, sender, arguments)),
    };
    v1::ToPlugin { body: Some(body) }
}

const fn wire_phase(phase: Phase) -> v1::Phase {
    match phase {
        Phase::Decide => v1::Phase::Decide,
        Phase::Settled => v1::Phase::Settled,
        Phase::RolledBack => v1::Phase::RolledBack,
    }
}

fn wire_answer(answer: Answer) -> answer::Body {
    match answer {
        Answer::Player(state) => answer::Body::Player(wire_state(state)),
        Answer::Players(players) => answer::Body::Players(v1::PlayerList {
            players: players.into_iter().map(wire_player).collect(),
        }),
        Answer::Inventory(state) => answer::Body::Inventory(inventory::wire_state(state)),
        Answer::Server(state) => answer::Body::Server(server::wire_state(state)),
        Answer::Names(names) => answer::Body::Listed(server::wire_names(names)),
        Answer::Bans(bans) => answer::Body::Bans(server::wire_bans(bans)),
        Answer::IpBans(bans) => answer::Body::Bans(server::wire_ip_bans(bans)),
        Answer::World(state) => answer::Body::World(world::wire_state(state)),
        Answer::Block(state) => answer::Body::Block(block::wire_state(state)),
        Answer::Tile(view) => answer::Body::Tile(tiles::wire_tile(view)),
        Answer::BlockInfo(info) => answer::Body::BlockInfo(tiles::wire_info(info)),
        Answer::Entities(views) => answer::Body::Entities(v1::EntityList {
            entities: views.into_iter().map(entity::wire_view).collect(),
        }),
        Answer::Entity(details) => answer::Body::Entity(edits::wire_details(details)),
        Answer::AttributeValue(value) => answer::Body::AttributeValue(value),
        Answer::Spawned(view) => answer::Body::Spawned(entity::wire_view(view)),
        Answer::Target(hit) => answer::Body::Target(player::wire_target(hit)),
        Answer::CustomInventory(state) => {
            answer::Body::CustomInventory(custom_inventory::wire_state(state))
        }
        Answer::Scoreboard(view) => answer::Body::Scoreboard(scoreboard::wire_state(view)),
        Answer::ScoreboardCreated(board) => answer::Body::ScoreboardCreated(board),
        Answer::BossBarCreated(bar) => answer::Body::BossBarCreated(bar),
        Answer::BossBar(view) => answer::Body::BossBar(boss_bar::wire_state(view)),
        Answer::BossBars(views) => answer::Body::BossBars(v1::BossBarList {
            bars: views.into_iter().map(boss_bar::wire_state).collect(),
        }),
        Answer::AdvancementProgress(view) => {
            answer::Body::AdvancementProgress(v1::AdvancementProgress {
                done: view.done,
                criteria_done: view.criteria_done,
                criteria_remaining: view.criteria_remaining,
            })
        }
        Answer::Advancements(ids) => answer::Body::Advancements(v1::AdvancementList { ids }),
        Answer::PermissionGroups(groups) => {
            answer::Body::PermissionGroups(server::wire_groups(groups))
        }
        Answer::Npc(view) => answer::Body::Npc(npc::wire_state(view)),
        Answer::MetaEntries(entries) => answer::Body::MetaEntries(meta::wire_entries(entries)),
        Answer::BlockPositions(positions) => {
            answer::Body::BlockPositions(meta::wire_positions(positions))
        }
        Answer::Npcs(views) => answer::Body::Npcs(v1::NpcList {
            npcs: views.into_iter().map(npc::wire_state).collect(),
        }),
        Answer::Refused(reason) => answer::Body::Refused(v1::Refused { reason }),
        Answer::WorldCreated(key) => answer::Body::WorldCreated(key),
        Answer::Done => answer::Body::Done(v1::Empty {}),
        Answer::Failed(error) => answer::Body::Failed(v1::ActionError {
            kind: wire_error_kind(error.kind) as i32,
            reason: error.reason,
        }),
        Answer::NotFound => answer::Body::NotFound(v1::NotFound {}),
    }
}

const fn wire_error_kind(kind: ActionErrorKind) -> v1::ActionErrorKind {
    match kind {
        ActionErrorKind::InvalidArgument => v1::ActionErrorKind::InvalidArgument,
        ActionErrorKind::TargetGone => v1::ActionErrorKind::TargetGone,
        ActionErrorKind::IncompatibleState => v1::ActionErrorKind::IncompatibleState,
        ActionErrorKind::Cancelled => v1::ActionErrorKind::Cancelled,
        ActionErrorKind::NotFound => v1::ActionErrorKind::NotFound,
        ActionErrorKind::LimitReached => v1::ActionErrorKind::LimitReached,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{Order, PlayerOrder, PlayerSnapshot};
    use rspigot_domain::player::{GameMode, PlayerId};
    use rspigot_domain::{Rotation, Vec3};
    use rspigot_plugin_protocol::v1::player_order;
    use uuid::Uuid;

    use super::*;

    const STEVE: &str = "00000000-0000-0000-0000-000000000001";

    fn body(body: from_plugin::Body) -> v1::FromPlugin {
        v1::FromPlugin {
            body: Some(body),
            origin: None,
            request: None,
            in_world: None,
        }
    }

    #[test]
    fn a_world_id_wraps_the_orders_and_reads_that_depend_on_it() {
        let arena = Identifier::parse("rspigot:arena").unwrap();
        let mut set_pvp = body(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::SetPvp(true)),
        }));
        set_pvp.in_world = Some("rspigot:arena".to_owned());
        assert_eq!(
            from_plugin(set_pvp, &mut Vec::new()).unwrap(),
            FromPlugin::Order(Order::InWorld {
                world: arena.clone(),
                order: Box::new(Order::World(rspigot_core::plugins::WorldOrder::SetPvp(
                    true
                ))),
            })
        );
        let mut asked = body(from_plugin::Body::Question(v1::Question {
            request: 3,
            question: Some(question::Question::World(v1::Empty {})),
        }));
        asked.in_world = Some("rspigot:arena".to_owned());
        assert_eq!(
            from_plugin(asked, &mut Vec::new()).unwrap(),
            FromPlugin::Question {
                request: 3,
                question: Question::InWorld {
                    world: arena,
                    question: Box::new(Question::World),
                },
            }
        );
        let mut players = body(from_plugin::Body::Question(v1::Question {
            request: 4,
            question: Some(question::Question::Players(v1::Empty {})),
        }));
        players.in_world = Some("rspigot:arena".to_owned());
        assert_eq!(
            from_plugin(players, &mut Vec::new()).unwrap(),
            FromPlugin::Question {
                request: 4,
                question: Question::Players,
            }
        );
        let mut nowhere = body(from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::SetPvp(true)),
        }));
        nowhere.in_world = Some("not a world".to_owned());
        assert!(from_plugin(nowhere, &mut Vec::new()).is_err());
    }

    #[test]
    fn a_player_order_and_the_two_questions_keep_their_shape() {
        let steve = PlayerId(Uuid::from_u128(1));
        let said = body(from_plugin::Body::Player(v1::PlayerOrder {
            player: STEVE.into(),
            order: Some(player_order::Order::SendMessage("hi".into())),
        }));
        assert_eq!(
            from_plugin(said, &mut Vec::new()).unwrap(),
            FromPlugin::Order(Order::Player {
                player: steve,
                order: PlayerOrder::SendMessage("hi".into())
            })
        );
        let asked = body(from_plugin::Body::Question(v1::Question {
            request: 7,
            question: Some(question::Question::Player(STEVE.into())),
        }));
        assert_eq!(
            from_plugin(asked, &mut Vec::new()).unwrap(),
            FromPlugin::Question {
                request: 7,
                question: Question::Player(steve)
            }
        );
        let all = body(from_plugin::Body::Question(v1::Question {
            request: 8,
            question: Some(question::Question::Players(v1::Empty {})),
        }));
        assert_eq!(
            from_plugin(all, &mut Vec::new()).unwrap(),
            FromPlugin::Question {
                request: 8,
                question: Question::Players
            }
        );
    }

    #[test]
    fn an_order_with_a_request_becomes_an_action_and_a_refusal_keeps_its_kind() {
        let teleport = v1::FromPlugin {
            body: Some(from_plugin::Body::Player(v1::PlayerOrder {
                player: STEVE.into(),
                order: Some(player_order::Order::Teleport(v1::Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                })),
            })),
            origin: Some(4),
            request: Some(9),
            in_world: None,
        };
        assert_eq!(
            from_plugin(teleport, &mut Vec::new()).unwrap(),
            FromPlugin::Action {
                request: 9,
                order: Order::Player {
                    player: PlayerId(Uuid::from_u128(1)),
                    order: PlayerOrder::Teleport(Vec3::new(1.0, 2.0, 3.0)),
                },
            }
        );
        let failed = to_plugin(ToPlugin::Answer {
            request: 9,
            answer: Answer::Failed(rspigot_core::plugins::ActionError::new(
                ActionErrorKind::TargetGone,
                "gone",
            )),
        });
        assert_eq!(
            failed.body,
            Some(to_plugin::Body::Answer(v1::Answer {
                request: 9,
                body: Some(answer::Body::Failed(v1::ActionError {
                    kind: v1::ActionErrorKind::TargetGone as i32,
                    reason: "gone".into(),
                })),
            }))
        );
    }

    #[test]
    fn an_order_sent_while_a_handler_runs_becomes_a_reaction_to_its_event() {
        let broadcast = |origin| v1::FromPlugin {
            body: Some(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(v1::server_order::Order::Broadcast("hi".into())),
            })),
            origin,
            request: None,
            in_world: None,
        };
        let order = Order::Server(rspigot_core::plugins::ServerOrder::Broadcast("hi".into()));
        assert_eq!(
            from_plugin(broadcast(Some(9)), &mut Vec::new()).unwrap(),
            FromPlugin::Reaction {
                event: 9,
                order: order.clone()
            }
        );
        assert_eq!(
            from_plugin(broadcast(None), &mut Vec::new()).unwrap(),
            FromPlugin::Order(order)
        );
        let asked = v1::FromPlugin {
            body: Some(from_plugin::Body::Question(v1::Question {
                request: 3,
                question: Some(question::Question::Players(v1::Empty {})),
            })),
            origin: Some(9),
            request: None,
            in_world: None,
        };
        assert_eq!(
            from_plugin(asked, &mut Vec::new()).unwrap(),
            FromPlugin::Question {
                request: 3,
                question: Question::Players
            }
        );
    }

    #[test]
    fn a_dynamic_command_registration_and_removal_convert() {
        let registered = v1::FromPlugin {
            body: Some(from_plugin::Body::RegisterCommand(v1::CommandSpec {
                id: 9,
                name: "late".to_owned(),
                permission: None,
                arguments: Vec::new(),
                aliases: vec!["l".to_owned()],
            })),
            origin: None,
            request: None,
            in_world: None,
        };
        let Ok(FromPlugin::RegisterCommand(spec)) = from_plugin(registered, &mut Vec::new()) else {
            panic!("a registration");
        };
        assert_eq!(
            (spec.id, spec.name.as_str(), spec.aliases.as_slice()),
            (9, "late", ["l".to_owned()].as_slice())
        );
        let removed = v1::FromPlugin {
            body: Some(from_plugin::Body::UnregisterCommand("late".to_owned())),
            origin: Some(3),
            request: None,
            in_world: None,
        };
        assert_eq!(
            from_plugin(removed, &mut Vec::new()).unwrap(),
            FromPlugin::UnregisterCommand("late".to_owned())
        );
    }

    #[test]
    fn a_snapshot_carries_its_rights() {
        let snapshot = PlayerSnapshot {
            meta: Default::default(),
            permissions: vec!["kit.use".to_owned(), "-kit.ban".to_owned()],
            id: PlayerId(Uuid::from_u128(1)),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.0, 64.0, -3.5),
            rotation: Rotation::new(0.0, 0.0),
            game_mode: GameMode::Creative,
        };
        let wire = wire_player(snapshot.clone());
        assert_eq!(wire.permissions, ["kit.use", "-kit.ban"]);
        assert_eq!(player::snapshot(wire).unwrap(), snapshot);
    }

    #[test]
    fn a_snapshot_carries_its_rotation() {
        let snapshot = PlayerSnapshot {
            meta: Default::default(),
            permissions: Vec::new(),
            id: PlayerId(Uuid::from_u128(1)),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.0, 64.0, -3.5),
            rotation: Rotation::new(90.0, -30.0),
            game_mode: GameMode::Creative,
        };
        assert_eq!(
            wire_player(snapshot).rotation,
            Some(v1::Rotation {
                yaw: 90.0,
                pitch: -30.0
            })
        );
    }
}
