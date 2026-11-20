use rspigot_core::admin::{Ban, BanEntry, IpBanEntry, ListEntry};
use rspigot_core::permissions::GroupView;
use rspigot_core::plugins::{Order, ServerOrder, ServerState};
use rspigot_core::worlds::{GeneratorSpec, LayerSpec, WorldKind, WorldSpec};
use rspigot_domain::Identifier;
use rspigot_plugin_protocol::v1::{
    self, server_order::Order as Wire, world_generator as generator,
};

use super::Malformed;
use super::shared::{expiry, required, wire_time};

pub(super) fn order(order: v1::ServerOrder) -> Result<Order, Malformed> {
    let order = match required(order.order)? {
        Wire::Broadcast(text) => ServerOrder::Broadcast(text),
        Wire::DispatchCommand(line) => ServerOrder::DispatchCommand(line),
        Wire::Shutdown(_) => ServerOrder::Shutdown,
        Wire::Save(_) => ServerOrder::Save,
        Wire::SetWhitelistEnabled(enabled) => ServerOrder::SetWhitelistEnabled(enabled),
        Wire::WhitelistAdd(name) => ServerOrder::WhitelistAdd(name),
        Wire::WhitelistRemove(name) => ServerOrder::WhitelistRemove(name),
        Wire::OperatorAdd(name) => ServerOrder::OperatorAdd(name),
        Wire::OperatorRemove(name) => ServerOrder::OperatorRemove(name),
        Wire::BanAdd(ban) => ServerOrder::BanAdd {
            name: ban.target,
            reason: ban.reason,
            expires: expiry(ban.expires_ms)?,
        },
        Wire::BanRemove(name) => ServerOrder::BanRemove(name),
        Wire::IpBanAdd(ban) => ServerOrder::IpBanAdd {
            address: ban.target,
            reason: ban.reason,
            expires: expiry(ban.expires_ms)?,
        },
        Wire::IpBanRemove(address) => ServerOrder::IpBanRemove(address),
        Wire::CreateInventory(v1::CreateInventory {
            id,
            rows,
            title,
            locked,
        }) => ServerOrder::CreateInventory {
            id,
            rows,
            title,
            locked,
        },
        Wire::CreateWorld(create) => ServerOrder::CreateWorld(world_spec(create)?),
        Wire::AddRecipe(def) => ServerOrder::AddRecipe(Box::new(super::recipes::spec(def)?)),
        Wire::RemoveRecipe(id) => ServerOrder::RemoveRecipe(id),
    };
    Ok(Order::Server(order))
}

fn world_spec(create: v1::CreateWorld) -> Result<WorldSpec, Malformed> {
    let kind = match v1::WorldDimension::try_from(create.dimension) {
        Ok(v1::WorldDimension::Overworld) => WorldKind::Overworld,
        Ok(v1::WorldDimension::Nether) => WorldKind::Nether,
        Ok(v1::WorldDimension::End) => WorldKind::End,
        Ok(v1::WorldDimension::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "dimension",
                value: create.dimension,
            });
        }
    };
    let generator = match required(required(create.generator)?.kind)? {
        generator::Kind::Vanilla(_) => GeneratorSpec::Vanilla,
        generator::Kind::Flat(flat) => GeneratorSpec::Flat(
            flat.layers
                .into_iter()
                .map(|layer| LayerSpec {
                    block: layer.block,
                    height: layer.height,
                })
                .collect(),
        ),
        generator::Kind::Void(_) => GeneratorSpec::Void,
        generator::Kind::Datapack(id) => {
            GeneratorSpec::Datapack(Identifier::parse(&id).map_err(|_| Malformed::World(id))?)
        }
    };
    Ok(WorldSpec {
        name: create.name,
        kind,
        seed: create.seed,
        generator,
        structures: create.structures,
    })
}

pub(super) fn wire_state(state: ServerState) -> v1::ServerState {
    let [tps_1m, tps_5m, tps_15m] = state.tps;
    v1::ServerState {
        max_players: wire_count(state.max_players),
        online: wire_count(state.online),
        tps_1m,
        tps_5m,
        tps_15m,
        whitelist_enabled: state.whitelist_enabled,
    }
}

pub(super) fn wire_groups(groups: Vec<GroupView>) -> v1::PermissionGroups {
    v1::PermissionGroups {
        groups: groups
            .into_iter()
            .map(|group| v1::PermissionGroup {
                name: group.name,
                permissions: group.permissions,
                inherits: group.inherits,
            })
            .collect(),
    }
}

pub(super) fn wire_names(names: Vec<ListEntry>) -> v1::ListedPlayers {
    v1::ListedPlayers {
        players: names
            .into_iter()
            .map(|entry| v1::ListedPlayer {
                name: entry.name,
                uuid: entry.uuid.map(|uuid| uuid.hyphenated().to_string()),
            })
            .collect(),
    }
}

pub(super) fn wire_bans(bans: Vec<BanEntry>) -> v1::BanList {
    wire_ban_list(bans, |entry| {
        (
            entry.name,
            entry.uuid.map(|uuid| uuid.hyphenated().to_string()),
        )
    })
}

pub(super) fn wire_ip_bans(bans: Vec<IpBanEntry>) -> v1::BanList {
    wire_ban_list(bans, |address| (address.to_string(), None))
}

fn wire_ban_list<T>(
    bans: Vec<Ban<T>>,
    target: impl Fn(T) -> (String, Option<String>),
) -> v1::BanList {
    v1::BanList {
        entries: bans
            .into_iter()
            .map(|ban| {
                let (name, uuid) = target(ban.target);
                v1::BanEntry {
                    target: name,
                    uuid,
                    created_ms: wire_time(ban.created),
                    source: ban.source,
                    expires_ms: ban.expires.map(wire_time),
                    reason: ban.reason,
                }
            })
            .collect(),
    }
}

fn wire_count(count: u32) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::time::{Duration, UNIX_EPOCH};

    use rspigot_core::admin::{Ban, ListEntry};
    use rspigot_core::plugins::{
        Answer, FromPlugin, Order, Question, ServerOrder, ServerState, ToPlugin,
    };
    use rspigot_plugin_protocol::v1::{self, answer, server_order};
    use uuid::Uuid;

    use super::*;
    use crate::convert::{from_plugin, question, to_plugin};

    fn convert(order: Option<server_order::Order>) -> Result<FromPlugin, Malformed> {
        from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Server(v1::ServerOrder { order })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
    }

    fn server(order: ServerOrder) -> FromPlugin {
        FromPlugin::Order(Order::Server(order))
    }

    fn ask(asked: question::Question) -> Question {
        let converted = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 3,
                    question: Some(asked),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        );
        let Ok(FromPlugin::Question {
            request: 3,
            question,
        }) = converted
        else {
            panic!("expected a question");
        };
        question
    }

    fn answered(answer: Answer) -> answer::Body {
        let Some(to_plugin::Body::Answer(v1::Answer {
            request: 3,
            body: Some(body),
        })) = to_plugin(ToPlugin::Answer { request: 3, answer }).body
        else {
            panic!("expected an answer");
        };
        body
    }

    #[test]
    fn server_orders_and_answers_convert() {
        assert_eq!(
            convert(Some(server_order::Order::Broadcast("§eHi".into()))).unwrap(),
            server(ServerOrder::Broadcast("§eHi".into()))
        );
        assert_eq!(
            convert(Some(server_order::Order::WhitelistAdd("Carol".into()))).unwrap(),
            server(ServerOrder::WhitelistAdd("Carol".into()))
        );
        assert_eq!(
            convert(Some(server_order::Order::Shutdown(v1::Empty {}))).unwrap(),
            server(ServerOrder::Shutdown)
        );
        assert!(matches!(convert(None), Err(Malformed::Empty)));

        assert_eq!(
            ask(question::Question::Server(v1::Empty {})),
            Question::Server
        );
        assert_eq!(
            ask(question::Question::Whitelist(v1::Empty {})),
            Question::Whitelist
        );
        assert_eq!(
            ask(question::Question::Operators(v1::Empty {})),
            Question::Operators
        );
        assert_eq!(
            ask(question::Question::PlayerNamed("alice".into())),
            Question::PlayerNamed("alice".into())
        );

        assert_eq!(
            answered(Answer::Server(ServerState {
                max_players: 20,
                online: 2,
                tps: [19.5, 19.8, 20.0],
                whitelist_enabled: true,
            })),
            answer::Body::Server(v1::ServerState {
                max_players: 20,
                online: 2,
                tps_1m: 19.5,
                tps_5m: 19.8,
                tps_15m: 20.0,
                whitelist_enabled: true,
            })
        );
        let alice = ListEntry {
            name: "Alice".into(),
            uuid: Some(Uuid::from_u128(1)),
            level: None,
            bypasses_player_limit: false,
        };
        assert_eq!(
            answered(Answer::Names(vec![ListEntry::named("Carol"), alice])),
            answer::Body::Listed(v1::ListedPlayers {
                players: vec![
                    v1::ListedPlayer {
                        name: "Carol".into(),
                        uuid: None,
                    },
                    v1::ListedPlayer {
                        name: "Alice".into(),
                        uuid: Some("00000000-0000-0000-0000-000000000001".into()),
                    },
                ],
            })
        );
    }

    #[test]
    fn ban_orders_and_lists_convert() {
        assert_eq!(
            convert(Some(server_order::Order::BanAdd(v1::Ban {
                target: "Steve".into(),
                reason: Some("griefing".into()),
                expires_ms: Some(1_000_000),
            })))
            .unwrap(),
            server(ServerOrder::BanAdd {
                name: "Steve".into(),
                reason: Some("griefing".into()),
                expires: Some(UNIX_EPOCH + Duration::from_secs(1000)),
            })
        );
        assert_eq!(
            convert(Some(server_order::Order::IpBanAdd(v1::Ban {
                target: "203.0.113.7".into(),
                reason: None,
                expires_ms: None,
            })))
            .unwrap(),
            server(ServerOrder::IpBanAdd {
                address: "203.0.113.7".into(),
                reason: None,
                expires: None,
            })
        );
        assert_eq!(
            convert(Some(server_order::Order::BanRemove("Steve".into()))).unwrap(),
            server(ServerOrder::BanRemove("Steve".into()))
        );
        assert_eq!(
            convert(Some(server_order::Order::IpBanRemove("203.0.113.7".into()))).unwrap(),
            server(ServerOrder::IpBanRemove("203.0.113.7".into()))
        );
        assert_eq!(ask(question::Question::Bans(v1::Empty {})), Question::Bans);
        assert_eq!(
            ask(question::Question::IpBans(v1::Empty {})),
            Question::IpBans
        );

        let created = UNIX_EPOCH + Duration::from_millis(1_793_534_400_000);
        assert_eq!(
            answered(Answer::Bans(vec![Ban {
                target: ListEntry {
                    name: "Steve".into(),
                    uuid: Some(Uuid::from_u128(1)),
                    level: None,
                    bypasses_player_limit: false,
                },
                created,
                source: "Alice".into(),
                expires: Some(UNIX_EPOCH + Duration::from_secs(1000)),
                reason: Some("griefing".into()),
            }])),
            answer::Body::Bans(v1::BanList {
                entries: vec![v1::BanEntry {
                    target: "Steve".into(),
                    uuid: Some("00000000-0000-0000-0000-000000000001".into()),
                    created_ms: 1_793_534_400_000,
                    source: "Alice".into(),
                    expires_ms: Some(1_000_000),
                    reason: Some("griefing".into()),
                }],
            })
        );
        assert_eq!(
            answered(Answer::IpBans(vec![Ban {
                target: IpAddr::from([203, 0, 113, 7]),
                created,
                source: "tester".into(),
                expires: None,
                reason: None,
            }])),
            answer::Body::Bans(v1::BanList {
                entries: vec![v1::BanEntry {
                    target: "203.0.113.7".into(),
                    uuid: None,
                    created_ms: 1_793_534_400_000,
                    source: "tester".into(),
                    expires_ms: None,
                    reason: None,
                }],
            })
        );
    }
}
