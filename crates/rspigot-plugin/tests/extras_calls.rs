mod fake_host;

use std::time::Duration;

use fake_host::*;
use rspigot_plugin::{
    BedSpawn, BlockPos, BlockType, EntityType, ItemStack, Material, Position, Sound, SoundCategory,
    StatisticKind, Weather,
};
use rspigot_plugin_protocol::v1::{
    self, answer, chunk_order, extra_answer, extra_question, from_plugin, player_extra, question,
    server_order, world_border_order, world_order,
};

const fn vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
    v1::Vec3 { x, y, z }
}

fn worldly(order: world_order::Order) -> from_plugin::Body {
    from_plugin::Body::World(v1::WorldOrder { order: Some(order) })
}

fn border(change: world_border_order::Change) -> from_plugin::Body {
    worldly(world_order::Order::Border(v1::WorldBorderOrder {
        change: Some(change),
    }))
}

fn chunked(order: chunk_order::Order) -> from_plugin::Body {
    worldly(world_order::Order::Chunk(v1::ChunkOrder {
        order: Some(order),
    }))
}

fn extra(order: player_extra::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(v1::player_order::Order::Extra(v1::PlayerExtra {
            order: Some(order),
        })),
    })
}

fn extra_query(query: extra_question::Query) -> question::Question {
    question::Question::Extra(v1::ExtraQuestion { query: Some(query) })
}

fn extra_body(answer_body: extra_answer::Answer) -> answer::Body {
    answer::Body::Extra(v1::ExtraAnswer {
        answer: Some(answer_body),
    })
}

#[tokio::test]
async fn world_border_lightning_and_chunk_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = &player.world;
            world.border.set_size(100.0);
            world.border.resize(40.0, Duration::from_secs(10));
            world.border.set_center(10.5, -20.0);
            world.border.set_damage_amount(2.5);
            world.border.set_damage_buffer(1.0);
            world.border.set_warning_distance(7);
            world.border.set_warning_time(Duration::from_secs(15));
            let at = Position {
                x: 1.5,
                y: 64.0,
                z: -2.5,
            };
            world.lightning.strike(at);
            world.lightning.strike_effect(at);
            let chunk = world.chunks.at(3, -4);
            chunk.load();
            chunk.set_force_loaded(true);
            chunk.regenerate();
        })
    })
    .await;

    for expected in [
        border(world_border_order::Change::Resize(v1::BorderResize {
            size: 100.0,
            ticks: 0,
        })),
        border(world_border_order::Change::Resize(v1::BorderResize {
            size: 40.0,
            ticks: 200,
        })),
        border(world_border_order::Change::SetCenter(v1::BorderCenter {
            x: 10.5,
            z: -20.0,
        })),
        border(world_border_order::Change::SetDamageAmount(2.5)),
        border(world_border_order::Change::SetDamageBuffer(1.0)),
        border(world_border_order::Change::SetWarningDistance(7)),
        border(world_border_order::Change::SetWarningTimeTicks(300)),
        worldly(world_order::Order::StrikeLightning(v1::StrikeLightning {
            position: Some(vec3(1.5, 64.0, -2.5)),
            effect_only: false,
        })),
        worldly(world_order::Order::StrikeLightning(v1::StrikeLightning {
            position: Some(vec3(1.5, 64.0, -2.5)),
            effect_only: true,
        })),
        chunked(chunk_order::Order::Load(v1::ChunkCoords { x: 3, z: -4 })),
        chunked(chunk_order::Order::SetForceLoaded(v1::ChunkForce {
            x: 3,
            z: -4,
            forced: true,
        })),
        chunked(chunk_order::Order::Regenerate(v1::ChunkCoords {
            x: 3,
            z: -4,
        })),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }
    called.finished().await;
}

#[tokio::test]
async fn player_time_weather_sound_cooldown_and_statistic_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.clock.set_time(6000);
            player.clock.set_relative_time(-500);
            player.clock.reset_time();
            player.clock.set_weather(Weather::Thunder);
            player.clock.reset_weather();
            player.sound.stop_all();
            player.sound.stop(Sound::EntityCowHurt);
            player.sound.stop_category(SoundCategory::Records);
            player
                .sound
                .stop_in(SoundCategory::Hostile, Sound::EntityCowHurt);
            player.cooldown.set(Material::EnderPearl, 100);
            player.cooldown.clear(Material::EnderPearl);
            let mined = StatisticKind::mined(BlockType::Stone);
            player.statistics.increment(&mined, 3);
            player.statistics.decrement(&mined, 1);
            player
                .statistics
                .set(&StatisticKind::custom("minecraft:jump"), 9);
            player
                .statistics
                .increment(&StatisticKind::killed(EntityType::Zombie), 1);
        })
    })
    .await;

    let statistic = |kind: &str, key: &str, amount: i32, mode: v1::StatisticMode| {
        extra(player_extra::Order::Statistic(v1::StatisticChange {
            kind: kind.to_owned(),
            key: key.to_owned(),
            amount,
            mode: mode as i32,
        }))
    };
    let stop = |sound: Option<&str>, category: Option<&str>| {
        extra(player_extra::Order::StopSound(v1::StopSound {
            sound: sound.map(str::to_owned),
            category: category.map(str::to_owned),
        }))
    };
    let cooldown = |ticks: i32| {
        extra(player_extra::Order::SetCooldown(v1::CooldownSet {
            item: "minecraft:ender_pearl".to_owned(),
            ticks,
        }))
    };
    for expected in [
        extra(player_extra::Order::SetPlayerTime(v1::PlayerTime {
            time: 6000,
            relative: false,
        })),
        extra(player_extra::Order::SetPlayerTime(v1::PlayerTime {
            time: -500,
            relative: true,
        })),
        extra(player_extra::Order::ResetPlayerTime(v1::Empty {})),
        extra(player_extra::Order::SetPlayerWeather(
            v1::Weather::Thunder as i32,
        )),
        extra(player_extra::Order::ResetPlayerWeather(v1::Empty {})),
        stop(None, None),
        stop(Some("minecraft:entity.cow.hurt"), None),
        stop(None, Some("record")),
        stop(Some("minecraft:entity.cow.hurt"), Some("hostile")),
        cooldown(100),
        cooldown(0),
        statistic(
            "minecraft:mined",
            "minecraft:stone",
            3,
            v1::StatisticMode::Increment,
        ),
        statistic(
            "minecraft:mined",
            "minecraft:stone",
            1,
            v1::StatisticMode::Decrement,
        ),
        statistic(
            "minecraft:custom",
            "minecraft:jump",
            9,
            v1::StatisticMode::Set,
        ),
        statistic(
            "minecraft:killed",
            "minecraft:zombie",
            1,
            v1::StatisticMode::Increment,
        ),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }
    called.finished().await;
}

#[tokio::test]
async fn compass_bed_book_and_server_setting_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.compass.set_target(Position {
                x: 1.5,
                y: 2.0,
                z: 3.5,
            });
            player.bed.set_spawn(
                BedSpawn::new(BlockPos { x: 7, y: 64, z: 9 })
                    .world("minecraft:the_nether")
                    .yaw(90.0)
                    .force(true),
            );
            player.bed.clear_spawn();
            player.book.open(&ItemStack::new(Material::WrittenBook, 1));
            let server = player.server();
            server.settings.set_motd("§6Hello");
            server.settings.set_max_players(12);
        })
    })
    .await;

    for expected in [
        extra(player_extra::Order::SetCompassTarget(vec3(1.5, 2.0, 3.5))),
        extra(player_extra::Order::SetBedSpawn(v1::BedSpawnSet {
            pos: Some(v1::BlockPos { x: 7, y: 64, z: 9 }),
            world: "minecraft:the_nether".to_owned(),
            yaw: 90.0,
            force: true,
        })),
        extra(player_extra::Order::ClearBedSpawn(v1::Empty {})),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }
    match called.session.hear().await {
        from_plugin::Body::Player(v1::PlayerOrder {
            order:
                Some(v1::player_order::Order::Extra(v1::PlayerExtra {
                    order: Some(player_extra::Order::OpenBook(item)),
                })),
            ..
        }) => assert_eq!(item.key, "minecraft:written_book"),
        other => panic!("expected an open book order, heard {other:?}"),
    }
    for expected in [
        server_order::Order::SetMotd("§6Hello".to_owned()),
        server_order::Order::SetMaxPlayers(12),
    ] {
        assert_eq!(
            called.session.hear().await,
            from_plugin::Body::Server(v1::ServerOrder {
                order: Some(expected)
            })
        );
    }
    called.finished().await;
}

#[tokio::test]
async fn cooldown_statistic_compass_bed_clock_and_chunk_questions_are_answered() {
    let mut called = calling(|player| {
        Box::pin(async move {
            assert_eq!(player.cooldown.get(Material::EnderPearl).await.unwrap(), 60);
            assert!(player.cooldown.has(Material::EnderPearl).await.unwrap());
            let mined = StatisticKind::mined(BlockType::Stone);
            assert_eq!(player.statistics.get(&mined).await.unwrap(), 12);
            let target = player.compass.get_target().await.unwrap();
            assert_eq!(
                target,
                Position {
                    x: 1.5,
                    y: 2.0,
                    z: 3.5
                }
            );
            let bed = player.bed.get_spawn().await.unwrap().unwrap();
            assert_eq!(bed.pos, BlockPos { x: 7, y: 64, z: 9 });
            assert!(bed.force);
            assert_eq!(player.bed.get_spawn().await.unwrap(), None);
            let clock = player.clock.get_state().await.unwrap();
            assert_eq!(
                clock.time.map(|time| (time.time, time.relative)),
                Some((1000, true))
            );
            assert_eq!(clock.weather, Some(Weather::Rain));
            let chunk = player.world.chunks.at(3, -4);
            let state = chunk.get_state(true).await.unwrap();
            assert!(state.loaded && state.force_loaded);
            assert!(!chunk.is_loaded().await.unwrap());
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::Cooldown(v1::CooldownQuery {
            player: STEVE.to_owned(),
            item: "minecraft:ender_pearl".to_owned(),
        }))
    );
    called
        .session
        .say(answer(request, extra_body(extra_answer::Answer::Count(60))));
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, extra_body(extra_answer::Answer::Count(60))));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::Statistic(v1::StatisticQuery {
            player: STEVE.to_owned(),
            kind: "minecraft:mined".to_owned(),
            key: "minecraft:stone".to_owned(),
        }))
    );
    called
        .session
        .say(answer(request, extra_body(extra_answer::Answer::Count(12))));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::CompassTarget(STEVE.to_owned()))
    );
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::CompassTarget(vec3(1.5, 2.0, 3.5))),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::BedSpawn(STEVE.to_owned()))
    );
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::BedSpawn(v1::BedSpawnInfo {
            spawn: Some(v1::BedSpawnSet {
                pos: Some(v1::BlockPos { x: 7, y: 64, z: 9 }),
                world: "minecraft:overworld".to_owned(),
                yaw: 0.0,
                force: true,
            }),
        })),
    ));
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::BedSpawn(v1::BedSpawnInfo {
            spawn: None,
        })),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::PlayerClock(STEVE.to_owned()))
    );
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::Clock(v1::PlayerClock {
            time: Some(v1::PlayerTime {
                time: 1000,
                relative: true,
            }),
            weather: Some(v1::Weather::Rain as i32),
        })),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::Chunk(v1::ChunkQuery {
            x: 3,
            z: -4,
            load: true,
        }))
    );
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::Chunk(v1::ChunkInfo {
            x: 3,
            z: -4,
            loaded: true,
            force_loaded: true,
        })),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        extra_query(extra_question::Query::Chunk(v1::ChunkQuery {
            x: 3,
            z: -4,
            load: false,
        }))
    );
    called.session.say(answer(
        request,
        extra_body(extra_answer::Answer::Chunk(v1::ChunkInfo {
            x: 3,
            z: -4,
            loaded: false,
            force_loaded: false,
        })),
    ));
    called.finished().await;
}
