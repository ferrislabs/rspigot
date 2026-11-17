use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::management::{
    ListAction, ManagementCall, ManagementNotice, ManagementReply, ManagementRequest, PlayerRef,
    Setting, SettingValue,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

#[derive(Clone, Default)]
struct Link {
    events: Arc<Mutex<Vec<ClientEvent>>>,
    closed: Arc<AtomicBool>,
}

impl ClientLink for Link {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    fn disconnect(&self, _: Component) {
        self.closed.store(true, Ordering::Release);
    }

    fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }
}

struct NoStore;

impl ChunkStore for NoStore {
    fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
        Ok(None)
    }

    fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
        Ok(())
    }

    fn flush(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl EntityStore for NoStore {
    fn load_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(&self, _: &Identifier, _: ChunkPos, _: &[Compound]) -> Result<(), StoreError> {
        Ok(())
    }
}

struct Floor;

impl ChunkGenerator for Floor {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn profile(name: &str) -> GameProfile {
    GameProfile {
        id: PlayerId::offline(name),
        name: name.to_owned(),
        properties: Vec::new(),
    }
}

fn server() -> (Server<Link>, crossbeam_channel::Receiver<ManagementNotice>) {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        chunks: ChunkManagerConfig {
            autosave_interval_ticks: 0,
            ..ChunkManagerConfig::default()
        },
        ..GameConfig::default()
    };
    let (notices, heard) = crossbeam_channel::unbounded();
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    )
    .with_management_sink(notices);
    server.wait_for_spawn(Duration::from_secs(10));
    (server, heard)
}

fn call(server: &mut Server<Link>, request: ManagementRequest) -> ManagementReply {
    let (reply, answer) = crossbeam_channel::bounded(1);
    server.handle(Inbound::Management(ManagementCall { request, reply }));
    answer.recv().expect("answered")
}

fn join(server: &mut Server<Link>, name: &str) -> Link {
    let link = Link::default();
    server.handle(Inbound::Join {
        profile: profile(name),
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    link
}

fn drained(heard: &crossbeam_channel::Receiver<ManagementNotice>) -> Vec<ManagementNotice> {
    heard.try_iter().collect()
}

#[test]
fn allowlist_changes_are_returned_and_announced() {
    let (mut server, heard) = server();
    let reply = call(
        &mut server,
        ManagementRequest::Allowlist(ListAction::Add(vec![PlayerRef::named("Bob")])),
    );
    assert_eq!(
        reply,
        ManagementReply::Allowlist(vec![PlayerRef::named("Bob")])
    );
    assert_eq!(
        drained(&heard),
        vec![ManagementNotice::AllowlistAdded(PlayerRef::named("Bob"))]
    );
    let reply = call(&mut server, ManagementRequest::Allowlist(ListAction::Clear));
    assert_eq!(reply, ManagementReply::Allowlist(Vec::new()));
    assert_eq!(
        drained(&heard),
        vec![ManagementNotice::AllowlistRemoved(PlayerRef::named("Bob"))]
    );
}

#[test]
fn operators_carry_their_level() {
    let (mut server, _heard) = server();
    let request = rspigot_core::management::OperatorRequest {
        player: PlayerRef::named("Carol"),
        level: Some(2),
        bypasses_player_limit: Some(true),
    };
    let ManagementReply::Operators(list) = call(
        &mut server,
        ManagementRequest::Operators(ListAction::Add(vec![request])),
    ) else {
        panic!("operators");
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].level, 2);
    assert!(list[0].bypasses_player_limit);
}

#[test]
fn players_and_kick_follow_the_online_list() {
    let (mut server, heard) = server();
    let link = join(&mut server, "Dave");
    let ManagementReply::Players(online) = call(&mut server, ManagementRequest::Players) else {
        panic!("players");
    };
    assert_eq!(online.len(), 1);
    assert!(
        drained(&heard)
            .iter()
            .any(|n| matches!(n, ManagementNotice::PlayerJoined(_)))
    );
    let reply = call(
        &mut server,
        ManagementRequest::Kick(vec![(PlayerRef::named("Dave"), Some("bye".to_owned()))]),
    );
    assert_eq!(
        reply,
        ManagementReply::Players(vec![PlayerRef {
            id: Some(PlayerId::offline("Dave").0),
            name: Some("Dave".to_owned()),
        }])
    );
    assert!(!link.is_connected());
}

#[test]
fn settings_and_game_rules_round_trip() {
    let (mut server, heard) = server();
    let reply = call(
        &mut server,
        ManagementRequest::Setting {
            setting: Setting::MaxPlayers,
            value: Some(SettingValue::Int(7)),
        },
    );
    assert_eq!(reply, ManagementReply::Setting(SettingValue::Int(7)));
    let reply = call(
        &mut server,
        ManagementRequest::Setting {
            setting: Setting::Difficulty,
            value: Some(SettingValue::Bool(true)),
        },
    );
    assert!(matches!(reply, ManagementReply::Fault(_)));
    let reply = call(
        &mut server,
        ManagementRequest::GameRule {
            key: "minecraft:keep_inventory".to_owned(),
            value: "true".to_owned(),
        },
    );
    assert_eq!(
        reply,
        ManagementReply::GameRule("keep_inventory".to_owned(), "true".to_owned())
    );
    assert_eq!(
        drained(&heard),
        vec![ManagementNotice::GameRuleUpdated {
            key: "keep_inventory".to_owned(),
            value: "true".to_owned()
        }]
    );
    let reply = call(
        &mut server,
        ManagementRequest::GameRule {
            key: "nope".to_owned(),
            value: "1".to_owned(),
        },
    );
    assert!(matches!(reply, ManagementReply::Fault(_)));
}

#[test]
fn stop_ends_the_run_and_status_reports_the_version() {
    let (mut server, _heard) = server();
    let ManagementReply::Status(status) = call(&mut server, ManagementRequest::Status) else {
        panic!("status");
    };
    assert!(status.started);
    assert_eq!(status.protocol, rspigot_data::PROTOCOL_VERSION);
    assert_eq!(
        call(&mut server, ManagementRequest::Stop),
        ManagementReply::Done(true)
    );
    assert!(!server.is_running());
}

#[test]
fn reload_asks_for_the_files_and_applies_what_comes_back() {
    use rspigot_core::admin::{AdminLists, ListEntry};
    use rspigot_core::reload::{ReloadData, ReloadRequest};
    let (server, _heard) = server();
    let (requests, asked) = crossbeam_channel::unbounded();
    let mut server = server.with_reload_sink(requests);
    server.handle(Inbound::Console("reload".to_owned()));
    assert_eq!(
        asked.try_recv().expect("a request"),
        ReloadRequest {
            whitelist_only: false
        }
    );
    let config = GameConfig {
        max_players: 3,
        ..GameConfig::default()
    };
    let admin = AdminLists {
        whitelist: vec![ListEntry::named("Zed")],
        whitelist_enabled: true,
        ..AdminLists::default()
    };
    server.handle(Inbound::Reload(Box::new(ReloadData {
        admin,
        config: Some(config),
        permissions: None,
    })));
    assert_eq!(
        call(&mut server, ManagementRequest::Allowlist(ListAction::Get)),
        ManagementReply::Allowlist(vec![PlayerRef::named("Zed")])
    );
    assert_eq!(
        call(
            &mut server,
            ManagementRequest::Setting {
                setting: Setting::MaxPlayers,
                value: None
            }
        ),
        ManagementReply::Setting(SettingValue::Int(3))
    );
}

#[test]
fn a_whitelist_reload_leaves_the_rest_alone() {
    use rspigot_core::admin::{AdminLists, ListEntry};
    use rspigot_core::reload::{ReloadData, ReloadRequest};
    let (server, _heard) = server();
    let (requests, asked) = crossbeam_channel::unbounded();
    let mut server = server.with_reload_sink(requests);
    server.handle(Inbound::Console("whitelist reload".to_owned()));
    assert_eq!(
        asked.try_recv().expect("a request"),
        ReloadRequest {
            whitelist_only: true
        }
    );
    server.handle(Inbound::Reload(Box::new(ReloadData {
        admin: AdminLists {
            whitelist: vec![ListEntry::named("Zed")],
            operators: vec![ListEntry::named("Boss")],
            ..AdminLists::default()
        },
        config: None,
        permissions: None,
    })));
    assert_eq!(
        call(&mut server, ManagementRequest::Operators(ListAction::Get)),
        ManagementReply::Operators(Vec::new())
    );
    assert_eq!(
        call(&mut server, ManagementRequest::Allowlist(ListAction::Get)),
        ManagementReply::Allowlist(vec![PlayerRef::named("Zed")])
    );
}
