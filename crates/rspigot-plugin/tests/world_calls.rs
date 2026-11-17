mod fake_host;

use std::fmt::Debug;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fake_host::*;
use rspigot_plugin::{
    Ban, BanEntry, BlockPos, BlockState, BlockType, Difficulty, Effect, EffectType, EntityType,
    Error, Explosion, Face, GameRule, GameRuleSetting, IpBanEntry, ItemStack, ListedPlayer,
    Material, Particle, ParticleBurst, Position, ServerState, Sound, SoundCue, Target, Uuid,
    Vector, Weather, WorldState,
};
use rspigot_plugin_protocol::v1::{
    self, answer, block_order, entity_order, from_plugin, question, server_order, target,
    world_order,
};

const ZOMBIE: &str = "00000000-0000-0000-0000-000000000007";

const RULES: [GameRule; 11] = [
    GameRule::DaylightCycle,
    GameRule::NaturalRegeneration,
    GameRule::KeepInventory,
    GameRule::ShowDeathMessages,
    GameRule::ImmediateRespawn,
    GameRule::FallDamage,
    GameRule::MobDrops,
    GameRule::MobGriefing,
    GameRule::MobExplosionDropDecay,
    GameRule::BlockDrops,
    GameRule::UniversalAnger,
];

const WIRE_RULES: [v1::GameRule; 11] = [
    v1::GameRule::DaylightCycle,
    v1::GameRule::NaturalRegeneration,
    v1::GameRule::KeepInventory,
    v1::GameRule::ShowDeathMessages,
    v1::GameRule::ImmediateRespawn,
    v1::GameRule::FallDamage,
    v1::GameRule::MobDrops,
    v1::GameRule::MobGriefing,
    v1::GameRule::MobExplosionDropDecay,
    v1::GameRule::BlockDrops,
    v1::GameRule::UniversalAnger,
];

const AT: BlockPos = BlockPos { x: 2, y: 64, z: -3 };
const WIRE_AT: v1::BlockPos = v1::BlockPos { x: 2, y: 64, z: -3 };
const HERE: Position = Position {
    x: 4.5,
    y: 64.0,
    z: -2.5,
};

const fn vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
    v1::Vec3 { x, y, z }
}

fn served(order: server_order::Order) -> from_plugin::Body {
    from_plugin::Body::Server(v1::ServerOrder { order: Some(order) })
}

fn worldly(order: world_order::Order) -> from_plugin::Body {
    from_plugin::Body::World(v1::WorldOrder { order: Some(order) })
}

fn placed(pos: v1::BlockPos, order: block_order::Order) -> from_plugin::Body {
    from_plugin::Body::Block(v1::BlockOrder {
        pos: Some(pos),
        order: Some(order),
    })
}

fn acted(order: entity_order::Order) -> from_plugin::Body {
    from_plugin::Body::Entity(v1::EntityOrder {
        entity: 12,
        order: Some(order),
    })
}

fn zombie(kind: &str) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: 12,
        uuid: ZOMBIE.to_owned(),
        kind: kind.to_owned(),
        position: Some(vec3(4.5, 64.0, -2.5)),
        player: None,
    }
}

fn steve_uuid() -> Uuid {
    Uuid::parse_str(STEVE).unwrap()
}

fn ban_created() -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(1_793_534_400_000)
}

fn world_state() -> v1::WorldState {
    v1::WorldState {
        name: "world".to_owned(),
        uuid: Some("00000000-0000-0000-0000-000000000009".to_owned()),
        seed: -42,
        day_time: 6000,
        game_time: 120_000,
        raining: true,
        thundering: false,
        difficulty: v1::Difficulty::Hard as i32,
        spawn: Some(v1::BlockPos { x: 8, y: 64, z: -8 }),
        pvp: true,
        rules: WIRE_RULES
            .iter()
            .enumerate()
            .map(|(index, rule)| v1::GameRuleValue {
                rule: *rule as i32,
                value: index % 2 == 0,
            })
            .collect(),
    }
}

fn block_hit(face: i32, block: &str) -> answer::Body {
    answer::Body::Target(v1::Target {
        hit: Some(target::Hit::Block(v1::BlockHit {
            pos: Some(WIRE_AT),
            face,
            block: block.to_owned(),
        })),
    })
}

fn refused(reason: &str) -> answer::Body {
    answer::Body::Refused(v1::Refused {
        reason: reason.to_owned(),
    })
}

fn not_found() -> answer::Body {
    answer::Body::NotFound(v1::NotFound {})
}

fn malformed<T: Debug>(result: Result<T, Error>) {
    assert!(matches!(result, Err(Error::Malformed(_))), "{result:?}");
}

#[tokio::test]
async fn server_orders_and_collections() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            server.message().broadcast("hi");
            server.command().dispatch("time set 0");
            server.lifecycle().shutdown();
            server.lifecycle().save();
            server.whitelist().set_enabled(true);
            server.whitelist().add("Carol");
            server.whitelist().remove("Dave");
            server.operators().add(String::from("Alice"));
            server.operators().remove("Bob");
            server.bans().add(
                Ban::new("Steve")
                    .reason("grief")
                    .expires(UNIX_EPOCH + Duration::from_secs(1000)),
            );
            server.bans().remove("Steve");
            server
                .ip_bans()
                .add(Ban::new("203.0.113.7".parse::<IpAddr>().unwrap()));
            server.ip_bans().add(
                Ban::new("2001:db8::1".parse::<IpAddr>().unwrap())
                    .reason("spam")
                    .expires(UNIX_EPOCH - Duration::from_millis(5)),
            );
            server
                .ip_bans()
                .remove("203.0.113.7".parse::<IpAddr>().unwrap().to_string());
        })
    })
    .await;

    for expected in [
        server_order::Order::Broadcast("hi".to_owned()),
        server_order::Order::DispatchCommand("time set 0".to_owned()),
        server_order::Order::Shutdown(v1::Empty {}),
        server_order::Order::Save(v1::Empty {}),
        server_order::Order::SetWhitelistEnabled(true),
        server_order::Order::WhitelistAdd("Carol".to_owned()),
        server_order::Order::WhitelistRemove("Dave".to_owned()),
        server_order::Order::OperatorAdd("Alice".to_owned()),
        server_order::Order::OperatorRemove("Bob".to_owned()),
        server_order::Order::BanAdd(v1::Ban {
            target: "Steve".to_owned(),
            reason: Some("grief".to_owned()),
            expires_ms: Some(1_000_000),
        }),
        server_order::Order::BanRemove("Steve".to_owned()),
        server_order::Order::IpBanAdd(v1::Ban {
            target: "203.0.113.7".to_owned(),
            reason: None,
            expires_ms: None,
        }),
        server_order::Order::IpBanAdd(v1::Ban {
            target: "2001:db8::1".to_owned(),
            reason: Some("spam".to_owned()),
            expires_ms: Some(-5),
        }),
        server_order::Order::IpBanRemove("203.0.113.7".to_owned()),
    ] {
        assert_eq!(called.session.hear().await, served(expected));
    }
    called.finished().await;
}

#[tokio::test]
async fn world_and_block_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = player.world();
            let spread = Vector {
                x: 0.5,
                y: 1.0,
                z: 1.5,
            };
            world.time().set(6000);
            for weather in [Weather::Clear, Weather::Rain, Weather::Thunder] {
                world.weather().set(weather);
            }
            for difficulty in [
                Difficulty::Peaceful,
                Difficulty::Easy,
                Difficulty::Normal,
                Difficulty::Hard,
            ] {
                world.difficulty().set(difficulty);
            }
            world.spawn_point().set(BlockPos { x: 8, y: 64, z: -8 });
            world.pvp().set(false);
            for rule in RULES {
                world.game_rule().set(GameRuleSetting::new(rule, true));
            }
            world.storage().save();
            world.sound().play(
                SoundCue::new(Sound::EntityGenericExplode)
                    .volume(4.0)
                    .pitch(0.5)
                    .at(HERE),
            );
            world.particle().spawn(
                ParticleBurst::new(Particle::Flame, HERE)
                    .count(8)
                    .spread(spread)
                    .speed(0.1),
            );
            world.explosion().create(Explosion::new(HERE, 4.0, true));
            let block = world.block(AT);
            assert_eq!(block.pos, AT);
            block.set(BlockType::Stone);
            block.set_with(BlockType::OakStairs, &[("facing", "east"), ("half", "top")]);
            block.break_naturally();
            player.server().world().pvp().set(true);
        })
    })
    .await;

    let mut expected = vec![worldly(world_order::Order::SetTime(6000))];
    expected.extend(
        [v1::Weather::Clear, v1::Weather::Rain, v1::Weather::Thunder]
            .map(|weather| worldly(world_order::Order::SetWeather(weather as i32))),
    );
    expected.extend(
        [
            v1::Difficulty::Peaceful,
            v1::Difficulty::Easy,
            v1::Difficulty::Normal,
            v1::Difficulty::Hard,
        ]
        .map(|difficulty| worldly(world_order::Order::SetDifficulty(difficulty as i32))),
    );
    expected.push(worldly(world_order::Order::SetSpawn(v1::BlockPos {
        x: 8,
        y: 64,
        z: -8,
    })));
    expected.push(worldly(world_order::Order::SetPvp(false)));
    expected.extend(WIRE_RULES.map(|rule| {
        worldly(world_order::Order::SetGameRule(v1::SetGameRule {
            rule: rule as i32,
            value: true,
        }))
    }));
    expected.extend([
        worldly(world_order::Order::Save(v1::Empty {})),
        worldly(world_order::Order::PlaySound(v1::Sound {
            key: "minecraft:entity.generic.explode".to_owned(),
            volume: 4.0,
            pitch: 0.5,
            position: Some(vec3(4.5, 64.0, -2.5)),
        })),
        worldly(world_order::Order::SpawnParticle(v1::Particle {
            key: "minecraft:flame".to_owned(),
            data: None,
            position: Some(vec3(4.5, 64.0, -2.5)),
            count: 8,
            spread: Some(vec3(0.5, 1.0, 1.5)),
            speed: 0.1,
        })),
        worldly(world_order::Order::Explode(v1::Explode {
            position: Some(vec3(4.5, 64.0, -2.5)),
            power: 4.0,
            breaks_blocks: true,
        })),
        placed(
            WIRE_AT,
            block_order::Order::Set("minecraft:stone".to_owned()),
        ),
        placed(
            WIRE_AT,
            block_order::Order::SetWith(v1::SetBlockWith {
                block: "minecraft:oak_stairs".to_owned(),
                properties: vec![
                    v1::Property {
                        name: "facing".to_owned(),
                        value: "east".to_owned(),
                    },
                    v1::Property {
                        name: "half".to_owned(),
                        value: "top".to_owned(),
                    },
                ],
            }),
        ),
        placed(WIRE_AT, block_order::Order::BreakNaturally(v1::Empty {})),
        worldly(world_order::Order::SetPvp(true)),
    ]);
    for expected in expected {
        assert_eq!(called.session.hear().await, expected);
    }
    called.finished().await;
}

#[tokio::test]
async fn questions_answer_typed_values() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            assert_eq!(
                server.get_state().await.unwrap(),
                ServerState {
                    max_players: 20,
                    online: 2,
                    tps: [19.5, 19.8, 20.0],
                    whitelist_enabled: true,
                }
            );
            let named = server.get_players_named("alex").await.unwrap();
            assert_eq!(named.len(), 1);
            assert_eq!(named[0].name, "Alex");
            assert_eq!(named[0].uuid.to_string(), ALEX);
            assert!(server.get_players_named("nobody").await.unwrap().is_empty());
            assert_eq!(
                server.whitelist().get_names().await.unwrap(),
                [
                    ListedPlayer {
                        name: "Carol".to_owned(),
                        uuid: None,
                    },
                    ListedPlayer {
                        name: "Steve".to_owned(),
                        uuid: Some(steve_uuid()),
                    },
                ]
            );
            assert_eq!(
                server.operators().get_names().await.unwrap(),
                [ListedPlayer {
                    name: "Alice".to_owned(),
                    uuid: None,
                }]
            );
            assert_eq!(
                server.bans().get_entries().await.unwrap(),
                [BanEntry {
                    target: "Steve".to_owned(),
                    uuid: Some(steve_uuid()),
                    created: ban_created(),
                    source: "Alice".to_owned(),
                    expires: Some(UNIX_EPOCH + Duration::from_secs(1000)),
                    reason: Some("grief".to_owned()),
                }]
            );
            assert_eq!(
                server.ip_bans().get_entries().await.unwrap(),
                [IpBanEntry {
                    address: "203.0.113.7".parse().unwrap(),
                    created: ban_created(),
                    source: "Server".to_owned(),
                    expires: None,
                    reason: None,
                }]
            );
            let world = player.world();
            assert_eq!(
                world.get_state().await.unwrap(),
                WorldState {
                    name: "world".to_owned(),
                    uuid: Some(Uuid::from_u128(9)),
                    seed: -42,
                    day_time: 6000,
                    game_time: 120_000,
                    raining: true,
                    thundering: false,
                    difficulty: Difficulty::Hard,
                    spawn: BlockPos { x: 8, y: 64, z: -8 },
                    pvp: true,
                    rules: RULES
                        .iter()
                        .enumerate()
                        .map(|(index, rule)| (*rule, index % 2 == 0))
                        .collect(),
                }
            );
            assert_eq!(
                world.get_block(AT).await.unwrap(),
                BlockState {
                    kind: BlockType::Stone,
                    properties: Vec::new(),
                }
            );
            assert_eq!(
                world.get_block(AT).await.unwrap(),
                BlockState {
                    kind: BlockType::OakStairs,
                    properties: vec![("facing".to_owned(), "east".to_owned())],
                }
            );
            let unloaded = world.get_block(BlockPos { x: 0, y: 0, z: 0 }).await;
            assert!(matches!(unloaded, Err(Error::NotFound)), "{unloaded:?}");
        })
    })
    .await;

    let session = &mut called.session;
    let listed = |name: &str, uuid: Option<&str>| v1::ListedPlayer {
        name: name.to_owned(),
        uuid: uuid.map(str::to_owned),
    };
    for (expected, body) in [
        (
            question::Question::Server(v1::Empty {}),
            answer::Body::Server(v1::ServerState {
                max_players: 20,
                online: 2,
                tps_1m: 19.5,
                tps_5m: 19.8,
                tps_15m: 20.0,
                whitelist_enabled: true,
            }),
        ),
        (
            question::Question::PlayerNamed("alex".to_owned()),
            answer::Body::Players(v1::PlayerList {
                players: vec![alex()],
            }),
        ),
        (
            question::Question::PlayerNamed("nobody".to_owned()),
            answer::Body::Players(v1::PlayerList::default()),
        ),
        (
            question::Question::Whitelist(v1::Empty {}),
            answer::Body::Listed(v1::ListedPlayers {
                players: vec![listed("Carol", None), listed("Steve", Some(STEVE))],
            }),
        ),
        (
            question::Question::Operators(v1::Empty {}),
            answer::Body::Listed(v1::ListedPlayers {
                players: vec![listed("Alice", None)],
            }),
        ),
        (
            question::Question::Bans(v1::Empty {}),
            answer::Body::Bans(v1::BanList {
                entries: vec![v1::BanEntry {
                    target: "Steve".to_owned(),
                    uuid: Some(STEVE.to_owned()),
                    created_ms: 1_793_534_400_000,
                    source: "Alice".to_owned(),
                    expires_ms: Some(1_000_000),
                    reason: Some("grief".to_owned()),
                }],
            }),
        ),
        (
            question::Question::IpBans(v1::Empty {}),
            answer::Body::Bans(v1::BanList {
                entries: vec![v1::BanEntry {
                    target: "203.0.113.7".to_owned(),
                    uuid: None,
                    created_ms: 1_793_534_400_000,
                    source: "Server".to_owned(),
                    expires_ms: None,
                    reason: None,
                }],
            }),
        ),
        (
            question::Question::World(v1::Empty {}),
            answer::Body::World(world_state()),
        ),
        (
            question::Question::Block(WIRE_AT),
            answer::Body::Block(v1::BlockState {
                block: "minecraft:stone".to_owned(),
                properties: Vec::new(),
            }),
        ),
        (
            question::Question::Block(WIRE_AT),
            answer::Body::Block(v1::BlockState {
                block: "minecraft:oak_stairs".to_owned(),
                properties: vec![v1::Property {
                    name: "facing".to_owned(),
                    value: "east".to_owned(),
                }],
            }),
        ),
        (
            question::Question::Block(v1::BlockPos { x: 0, y: 0, z: 0 }),
            not_found(),
        ),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, expected);
        session.say(answer(request, body));
    }
    called.finished().await;
}

#[tokio::test]
async fn spawn_waits_for_the_entity_or_its_refusal() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = player.world();
            let zombie = world.spawn(EntityType::Zombie, HERE).await.unwrap();
            assert_eq!(zombie.entity_id, 12);
            assert_eq!(zombie.uuid, Uuid::from_u128(7));
            assert_eq!(zombie.kind, EntityType::Zombie);
            assert_eq!(zombie.position, HERE);
            let peaceful = world.spawn(EntityType::Zombie, HERE).await;
            assert!(
                matches!(&peaceful, Err(Error::Rejected(reason)) if reason == "Monsters cannot be summoned in Peaceful difficulty"),
                "{peaceful:?}"
            );
            assert_eq!(
                peaceful.unwrap_err().to_string(),
                "rspigot rejected the request: Monsters cannot be summoned in Peaceful difficulty"
            );
            let diamonds = || ItemStack::new(Material::Diamond, 3);
            let dropped = world.drop_item(HERE, diamonds()).await.unwrap();
            assert_eq!(dropped.kind, EntityType::Item);
            let unloaded = world.drop_item(HERE, diamonds()).await;
            assert!(
                matches!(&unloaded, Err(Error::Rejected(reason)) if reason == "Unable to summon entity"),
                "{unloaded:?}"
            );
        })
    })
    .await;

    let session = &mut called.session;
    let spawn = || {
        question::Question::Spawn(v1::Spawn {
            kind: "minecraft:zombie".to_owned(),
            position: Some(vec3(4.5, 64.0, -2.5)),
        })
    };
    let drop = || {
        question::Question::DropItem(v1::DropItem {
            position: Some(vec3(4.5, 64.0, -2.5)),
            item: Some(v1::Item {
                key: "minecraft:diamond".to_owned(),
                count: 3,
                ..v1::Item::default()
            }),
        })
    };
    for (expected, body) in [
        (spawn(), answer::Body::Spawned(zombie("minecraft:zombie"))),
        (
            spawn(),
            refused("Monsters cannot be summoned in Peaceful difficulty"),
        ),
        (drop(), answer::Body::Spawned(zombie("minecraft:item"))),
        (drop(), refused("Unable to summon entity")),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, expected);
        session.say(answer(request, body));
    }
    called.finished().await;
}

#[tokio::test]
async fn entity_filters_and_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = player.world();
            let near = world
                .entities
                .near(HERE, 2.0)
                .of(EntityType::Zombie)
                .get_entities()
                .await
                .unwrap();
            assert_eq!(near.len(), 1);
            let zombie = &near[0];
            assert_eq!(zombie.entity_id, 12);
            assert_eq!(zombie.uuid, Uuid::from_u128(7));
            assert_eq!(zombie.kind, EntityType::Zombie);
            assert_eq!(zombie.position, HERE);
            let cows = world
                .entities
                .of(EntityType::Cow)
                .get_entities()
                .await
                .unwrap();
            assert!(cows.is_empty());
            assert!(world.entities.get_entities().await.unwrap().is_empty());
            let to = Position {
                x: 0.5,
                y: 70.0,
                z: 0.5,
            };
            zombie.teleport(to);
            zombie.remove();
            zombie.damage(2.0);
            zombie.kill();
            zombie.add_effect(Effect::new(EffectType::Slowness, 100, 1));
            zombie.remove_effect(EffectType::Slowness);
            zombie.clear_effects();
            zombie.set_velocity(Vector {
                x: 0.0,
                y: 0.5,
                z: -0.25,
            });
            let state = zombie.get_state().await.unwrap();
            assert_eq!(state.entity.entity_id, 12);
            assert_eq!(state.entity.kind, EntityType::Zombie);
            assert_eq!(state.health, Some(15.0));
            assert_eq!(
                state.velocity,
                Vector {
                    x: 0.0,
                    y: -0.08,
                    z: 0.0,
                }
            );
            assert!(state.on_ground);
            assert_eq!(state.custom_name.as_deref(), Some("§cBoss"));
            assert_eq!(state.fire_ticks, 20);
            let gone = zombie.get_state().await;
            assert!(matches!(gone, Err(Error::NotFound)), "{gone:?}");
            zombie.server().message().broadcast("bye");
        })
    })
    .await;

    let session = &mut called.session;
    for (expected, body) in [
        (
            v1::EntityQuery {
                near: Some(v1::Near {
                    position: Some(vec3(4.5, 64.0, -2.5)),
                    radius: 2.0,
                }),
                kind: Some("minecraft:zombie".to_owned()),
            },
            vec![zombie("minecraft:zombie")],
        ),
        (
            v1::EntityQuery {
                near: None,
                kind: Some("minecraft:cow".to_owned()),
            },
            Vec::new(),
        ),
        (v1::EntityQuery::default(), Vec::new()),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, question::Question::Entities(expected));
        session.say(answer(
            request,
            answer::Body::Entities(v1::EntityList { entities: body }),
        ));
    }
    for expected in [
        entity_order::Order::Teleport(vec3(0.5, 70.0, 0.5)),
        entity_order::Order::Remove(v1::Empty {}),
        entity_order::Order::Damage(2.0),
        entity_order::Order::Kill(v1::Empty {}),
        entity_order::Order::AddEffect(v1::Effect {
            key: "minecraft:slowness".to_owned(),
            duration: 100,
            amplifier: 1,
            ambient: false,
            particles: true,
            icon: true,
        }),
        entity_order::Order::RemoveEffect("minecraft:slowness".to_owned()),
        entity_order::Order::ClearEffects(v1::Empty {}),
        entity_order::Order::SetVelocity(vec3(0.0, 0.5, -0.25)),
    ] {
        assert_eq!(session.hear().await, acted(expected));
    }
    for body in [
        answer::Body::Entity(v1::EntityState {
            entity: Some(zombie("minecraft:zombie")),
            health: Some(15.0),
            velocity: Some(vec3(0.0, -0.08, 0.0)),
            on_ground: true,
            custom_name: Some("§cBoss".to_owned()),
            fire_ticks: 20,
        }),
        not_found(),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, question::Question::Entity(12));
        session.say(answer(request, body));
    }
    assert_eq!(
        session.hear().await,
        served(server_order::Order::Broadcast("bye".to_owned()))
    );
    called.finished().await;
}

#[tokio::test]
async fn targets_return_blocks_players_or_entities() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let target = player.target_block(5.0).await.unwrap().unwrap();
            assert_eq!(target.block.pos, AT);
            assert_eq!(target.face, Face::Up);
            assert_eq!(target.kind, BlockType::Stone);
            target.block.break_naturally();
            for face in [
                Face::Down,
                Face::Up,
                Face::North,
                Face::South,
                Face::West,
                Face::East,
            ] {
                let target = player.target_block(5.0).await.unwrap().unwrap();
                assert_eq!(target.face, face);
            }
            assert!(player.target_block(120.0).await.unwrap().is_none());
            let refused = player.target_block(-1.0).await;
            assert!(
                matches!(&refused, Err(Error::Rejected(reason)) if reason == "max_distance -1 is not a finite number of at least 0"),
                "{refused:?}"
            );
            match player.target_entity(3.0).await.unwrap() {
                Some(Target::Player(alex)) => {
                    assert_eq!(alex.uuid.to_string(), ALEX);
                    assert_eq!(alex.name, "Alex");
                }
                other => panic!("expected Alex, got {other:?}"),
            }
            match player.target_entity(3.0).await.unwrap() {
                Some(Target::Entity(zombie)) => {
                    assert_eq!(zombie.entity_id, 12);
                    assert_eq!(zombie.kind, EntityType::Zombie);
                }
                other => panic!("expected a zombie, got {other:?}"),
            }
            assert!(player.target_entity(3.0).await.unwrap().is_none());
        })
    })
    .await;

    let session = &mut called.session;
    let aimed = |max_distance| v1::TargetQuery {
        player: STEVE.to_owned(),
        max_distance,
    };
    let (request, asked) = session.asked().await;
    assert_eq!(asked, question::Question::TargetBlock(aimed(5.0)));
    session.say(answer(
        request,
        block_hit(v1::Face::Up as i32, "minecraft:stone"),
    ));
    assert_eq!(
        session.hear().await,
        placed(WIRE_AT, block_order::Order::BreakNaturally(v1::Empty {}))
    );
    for face in [
        v1::Face::Down,
        v1::Face::Up,
        v1::Face::North,
        v1::Face::South,
        v1::Face::West,
        v1::Face::East,
    ] {
        let (request, _) = session.asked().await;
        session.say(answer(request, block_hit(face as i32, "minecraft:stone")));
    }
    for (expected, body) in [
        (
            question::Question::TargetBlock(aimed(120.0)),
            answer::Body::Target(v1::Target { hit: None }),
        ),
        (
            question::Question::TargetBlock(aimed(-1.0)),
            refused("max_distance -1 is not a finite number of at least 0"),
        ),
        (
            question::Question::TargetEntity(aimed(3.0)),
            answer::Body::Target(v1::Target {
                hit: Some(target::Hit::Player(alex())),
            }),
        ),
        (
            question::Question::TargetEntity(aimed(3.0)),
            answer::Body::Target(v1::Target {
                hit: Some(target::Hit::Entity(zombie("minecraft:zombie"))),
            }),
        ),
        (
            question::Question::TargetEntity(aimed(3.0)),
            answer::Body::Target(v1::Target { hit: None }),
        ),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, expected);
        session.say(answer(request, body));
    }
    called.finished().await;
}

#[tokio::test]
async fn answers_the_sdk_cannot_read_are_malformed() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let world = player.world();
            malformed(server.get_state().await);
            malformed(server.get_state().await);
            malformed(server.whitelist().get_names().await);
            malformed(server.bans().get_entries().await);
            malformed(server.ip_bans().get_entries().await);
            malformed(world.get_state().await);
            malformed(world.get_state().await);
            malformed(world.get_state().await);
            malformed(world.get_state().await);
            malformed(world.get_block(AT).await);
            malformed(world.entities.get_entities().await);
            malformed(world.entities.get_entities().await);
            malformed(world.spawn(EntityType::Zombie, HERE).await);
            malformed(player.target_block(5.0).await);
            malformed(player.target_block(5.0).await);
            malformed(player.target_block(5.0).await);
            malformed(player.target_entity(5.0).await);
        })
    })
    .await;

    let session = &mut called.session;
    for body in [
        answer::Body::Server(v1::ServerState {
            max_players: -1,
            ..v1::ServerState::default()
        }),
        answer::Body::World(world_state()),
        answer::Body::Listed(v1::ListedPlayers {
            players: vec![v1::ListedPlayer {
                name: "Steve".to_owned(),
                uuid: Some("steve".to_owned()),
            }],
        }),
        answer::Body::Bans(v1::BanList {
            entries: vec![v1::BanEntry {
                target: "Steve".to_owned(),
                uuid: Some("steve".to_owned()),
                ..v1::BanEntry::default()
            }],
        }),
        answer::Body::Bans(v1::BanList {
            entries: vec![v1::BanEntry {
                target: "Steve".to_owned(),
                ..v1::BanEntry::default()
            }],
        }),
        answer::Body::World(v1::WorldState {
            difficulty: 9,
            ..world_state()
        }),
        answer::Body::World(v1::WorldState {
            rules: vec![v1::GameRuleValue {
                rule: 99,
                value: true,
            }],
            ..world_state()
        }),
        answer::Body::World(v1::WorldState {
            uuid: Some("world".to_owned()),
            ..world_state()
        }),
        answer::Body::World(v1::WorldState {
            spawn: None,
            ..world_state()
        }),
        answer::Body::Block(v1::BlockState {
            block: "minecraft:stoned".to_owned(),
            properties: Vec::new(),
        }),
        answer::Body::Entities(v1::EntityList {
            entities: vec![zombie("minecraft:zombie_plus")],
        }),
        answer::Body::Entities(v1::EntityList {
            entities: vec![v1::EntitySnapshot {
                uuid: "zombie".to_owned(),
                ..zombie("minecraft:zombie")
            }],
        }),
        answer::Body::Entities(v1::EntityList::default()),
        block_hit(v1::Face::Unspecified as i32, "minecraft:stone"),
        block_hit(v1::Face::Up as i32, "minecraft:stoned"),
        answer::Body::Target(v1::Target {
            hit: Some(target::Hit::Player(alex())),
        }),
        block_hit(v1::Face::Up as i32, "minecraft:stone"),
    ] {
        let (request, _) = session.asked().await;
        session.say(answer(request, body));
    }
    called.finished().await;
}
