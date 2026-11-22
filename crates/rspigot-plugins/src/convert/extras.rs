use rspigot_core::commands::Weather;
use rspigot_core::plugins::{
    BedSpawn, BorderOrder, ChunkOrder, ExtraAnswer, ExtraQuestion, PlayerAction, StatisticMode,
    WorldAction,
};
use rspigot_domain::ChunkPos;
use rspigot_plugin_protocol::v1::{
    self, chunk_order, extra_answer, extra_question, player_extra, world_border_order,
};

use super::Malformed;
use super::player;
use super::shared::{self, required};

fn weather(value: i32) -> Result<Weather, Malformed> {
    match v1::Weather::try_from(value) {
        Ok(v1::Weather::Clear) => Ok(Weather::Clear),
        Ok(v1::Weather::Rain) => Ok(Weather::Rain),
        Ok(v1::Weather::Thunder) => Ok(Weather::Thunder),
        Ok(v1::Weather::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "weather",
            value,
        }),
    }
}

const fn wire_weather(weather: Weather) -> v1::Weather {
    match weather {
        Weather::Clear => v1::Weather::Clear,
        Weather::Rain => v1::Weather::Rain,
        Weather::Thunder => v1::Weather::Thunder,
    }
}

fn mode(value: i32) -> Result<StatisticMode, Malformed> {
    match v1::StatisticMode::try_from(value) {
        Ok(v1::StatisticMode::Increment) => Ok(StatisticMode::Increment),
        Ok(v1::StatisticMode::Decrement) => Ok(StatisticMode::Decrement),
        Ok(v1::StatisticMode::Set) => Ok(StatisticMode::Set),
        Ok(v1::StatisticMode::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "statistic mode",
            value,
        }),
    }
}

pub(super) fn player_action(extra: v1::PlayerExtra) -> Result<PlayerAction, Malformed> {
    use player_extra::Order;
    Ok(match required(extra.order)? {
        Order::SetPlayerTime(time) => PlayerAction::SetTime {
            time: time.time,
            relative: time.relative,
        },
        Order::ResetPlayerTime(_) => PlayerAction::ResetTime,
        Order::SetPlayerWeather(value) => PlayerAction::SetWeather(weather(value)?),
        Order::ResetPlayerWeather(_) => PlayerAction::ResetWeather,
        Order::StopSound(stop) => PlayerAction::StopSound {
            sound: stop.sound,
            source: stop.category,
        },
        Order::SetCooldown(cooldown) => PlayerAction::SetCooldown {
            item: cooldown.item,
            ticks: cooldown.ticks,
        },
        Order::Statistic(change) => PlayerAction::Statistic {
            kind: change.kind,
            key: change.key,
            amount: change.amount,
            mode: mode(change.mode)?,
        },
        Order::SetCompassTarget(target) => PlayerAction::SetCompassTarget(shared::vec3(target)),
        Order::SetBedSpawn(spawn) => PlayerAction::SetBedSpawn(bed_spawn(spawn)?),
        Order::ClearBedSpawn(_) => PlayerAction::ClearBedSpawn,
        Order::OpenBook(item) => PlayerAction::OpenBook(shared::item(item)),
    })
}

fn bed_spawn(spawn: v1::BedSpawnSet) -> Result<BedSpawn, Malformed> {
    Ok(BedSpawn {
        pos: shared::block_pos(required(spawn.pos)?),
        world: spawn.world,
        yaw: spawn.yaw,
        force: spawn.force,
    })
}

fn wire_bed_spawn(spawn: BedSpawn) -> v1::BedSpawnSet {
    v1::BedSpawnSet {
        pos: Some(shared::wire_block_pos(spawn.pos)),
        world: spawn.world,
        yaw: spawn.yaw,
        force: spawn.force,
    }
}

pub(super) fn border(order: v1::WorldBorderOrder) -> Result<WorldAction, Malformed> {
    use world_border_order::Change;
    Ok(WorldAction::Border(match required(order.change)? {
        Change::Resize(resize) => BorderOrder::Resize {
            size: resize.size,
            ticks: resize.ticks,
        },
        Change::SetCenter(center) => BorderOrder::SetCenter {
            x: center.x,
            z: center.z,
        },
        Change::SetDamageAmount(amount) => BorderOrder::SetDamageAmount(amount),
        Change::SetDamageBuffer(distance) => BorderOrder::SetDamageBuffer(distance),
        Change::SetWarningDistance(blocks) => BorderOrder::SetWarningDistance(blocks),
        Change::SetWarningTimeTicks(ticks) => BorderOrder::SetWarningTime(ticks),
    }))
}

pub(super) fn lightning(strike: v1::StrikeLightning) -> Result<WorldAction, Malformed> {
    Ok(WorldAction::StrikeLightning {
        position: shared::vec3(required(strike.position)?),
        effect_only: strike.effect_only,
    })
}

pub(super) fn chunk(order: v1::ChunkOrder) -> Result<WorldAction, Malformed> {
    Ok(WorldAction::Chunk(match required(order.order)? {
        chunk_order::Order::Load(at) => ChunkOrder::Load(ChunkPos::new(at.x, at.z)),
        chunk_order::Order::SetForceLoaded(force) => ChunkOrder::SetForceLoaded {
            pos: ChunkPos::new(force.x, force.z),
            forced: force.forced,
        },
        chunk_order::Order::Regenerate(at) => ChunkOrder::Regenerate(ChunkPos::new(at.x, at.z)),
    }))
}

pub(super) fn question(extra: v1::ExtraQuestion) -> Result<ExtraQuestion, Malformed> {
    use extra_question::Query;
    Ok(match required(extra.query)? {
        Query::Cooldown(query) => ExtraQuestion::Cooldown {
            player: player::id(query.player)?,
            item: query.item,
        },
        Query::Statistic(query) => ExtraQuestion::Statistic {
            player: player::id(query.player)?,
            kind: query.kind,
            key: query.key,
        },
        Query::BedSpawn(target) => ExtraQuestion::BedSpawn(player::id(target)?),
        Query::CompassTarget(target) => ExtraQuestion::CompassTarget(player::id(target)?),
        Query::PlayerClock(target) => ExtraQuestion::PlayerClock(player::id(target)?),
        Query::Chunk(query) => ExtraQuestion::Chunk {
            pos: ChunkPos::new(query.x, query.z),
            load: query.load,
        },
    })
}

pub(super) fn wire_answer(answer: ExtraAnswer) -> v1::ExtraAnswer {
    use extra_answer::Answer;
    v1::ExtraAnswer {
        answer: Some(match answer {
            ExtraAnswer::Count(count) => Answer::Count(count),
            ExtraAnswer::BedSpawn(spawn) => Answer::BedSpawn(v1::BedSpawnInfo {
                spawn: spawn.map(wire_bed_spawn),
            }),
            ExtraAnswer::CompassTarget(target) => Answer::CompassTarget(shared::wire_vec(target)),
            ExtraAnswer::Clock { time, weather } => Answer::Clock(v1::PlayerClock {
                time: time.map(|(time, relative)| v1::PlayerTime { time, relative }),
                weather: weather.map(|weather| wire_weather(weather) as i32),
            }),
            ExtraAnswer::Chunk {
                pos,
                loaded,
                force_loaded,
            } => Answer::Chunk(v1::ChunkInfo {
                x: pos.x,
                z: pos.z,
                loaded,
                force_loaded,
            }),
        }),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_plugin_protocol::v1::PlayerExtra;

    use super::*;

    const PLAYER: &str = "00000000-0000-0000-0000-000000000001";

    fn extra(order: player_extra::Order) -> v1::PlayerExtra {
        PlayerExtra { order: Some(order) }
    }

    #[test]
    fn border_lightning_and_chunk_orders_reach_the_core_types() {
        let resize = v1::WorldBorderOrder {
            change: Some(world_border_order::Change::Resize(v1::BorderResize {
                size: 50.0,
                ticks: 40,
            })),
        };
        assert_eq!(
            border(resize).unwrap(),
            WorldAction::Border(BorderOrder::Resize {
                size: 50.0,
                ticks: 40
            })
        );
        let strike = v1::StrikeLightning {
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            effect_only: true,
        };
        assert!(matches!(
            lightning(strike).unwrap(),
            WorldAction::StrikeLightning {
                effect_only: true,
                ..
            }
        ));
        let regenerate = v1::ChunkOrder {
            order: Some(chunk_order::Order::Regenerate(v1::ChunkCoords {
                x: -2,
                z: 5,
            })),
        };
        assert_eq!(
            chunk(regenerate).unwrap(),
            WorldAction::Chunk(ChunkOrder::Regenerate(ChunkPos::new(-2, 5)))
        );
    }

    #[test]
    fn an_empty_extra_order_is_malformed_and_a_missing_position_too() {
        assert!(player_action(PlayerExtra { order: None }).is_err());
        assert!(lightning(v1::StrikeLightning::default()).is_err());
        assert!(
            player_action(extra(player_extra::Order::Statistic(v1::StatisticChange {
                kind: "minecraft:mined".to_owned(),
                key: "minecraft:stone".to_owned(),
                amount: 1,
                mode: 0,
            })))
            .is_err()
        );
    }

    #[test]
    fn player_orders_reach_the_core_types() {
        let action = player_action(extra(player_extra::Order::Statistic(v1::StatisticChange {
            kind: "minecraft:mined".to_owned(),
            key: "minecraft:stone".to_owned(),
            amount: 4,
            mode: v1::StatisticMode::Decrement as i32,
        })))
        .unwrap();
        assert_eq!(
            action,
            PlayerAction::Statistic {
                kind: "minecraft:mined".to_owned(),
                key: "minecraft:stone".to_owned(),
                amount: 4,
                mode: StatisticMode::Decrement,
            }
        );
        let action = player_action(extra(player_extra::Order::SetPlayerWeather(
            v1::Weather::Rain as i32,
        )))
        .unwrap();
        assert_eq!(action, PlayerAction::SetWeather(Weather::Rain));
    }

    #[test]
    fn questions_and_answers_survive_the_conversion() {
        let asked = question(v1::ExtraQuestion {
            query: Some(extra_question::Query::Chunk(v1::ChunkQuery {
                x: 1,
                z: 2,
                load: true,
            })),
        })
        .unwrap();
        assert_eq!(
            asked,
            ExtraQuestion::Chunk {
                pos: ChunkPos::new(1, 2),
                load: true
            }
        );
        assert!(
            question(v1::ExtraQuestion {
                query: Some(extra_question::Query::BedSpawn("nobody".to_owned())),
            })
            .is_err()
        );
        assert!(
            question(v1::ExtraQuestion {
                query: Some(extra_question::Query::CompassTarget(PLAYER.to_owned())),
            })
            .is_ok()
        );

        let clock = wire_answer(ExtraAnswer::Clock {
            time: Some((6000, false)),
            weather: Some(Weather::Thunder),
        });
        assert_eq!(
            clock.answer,
            Some(extra_answer::Answer::Clock(v1::PlayerClock {
                time: Some(v1::PlayerTime {
                    time: 6000,
                    relative: false
                }),
                weather: Some(v1::Weather::Thunder as i32),
            }))
        );
        let bed = wire_answer(ExtraAnswer::BedSpawn(Some(BedSpawn {
            pos: rspigot_domain::BlockPos::new(1, 2, 3),
            world: "minecraft:overworld".to_owned(),
            yaw: 45.0,
            force: true,
        })));
        assert!(matches!(
            bed.answer,
            Some(extra_answer::Answer::BedSpawn(v1::BedSpawnInfo { spawn: Some(spawn) }))
                if spawn.force && spawn.yaw == 45.0
        ));
    }
}
