use rspigot_core::commands::Weather;
use rspigot_core::plugins::{
    BedSpawn, BorderOrder, ChunkOrder, ClientChange, ExtraAnswer, ExtraQuestion, PlayerAction,
    StatisticMode, WorldAction,
};
use rspigot_domain::{ChunkPos, EntityId};
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
        Order::SendBlockChanges(blocks) => PlayerAction::Client(ClientChange::Blocks(
            blocks
                .changes
                .into_iter()
                .map(|change| {
                    Ok((
                        shared::block_pos(required(change.pos)?),
                        super::block::state(required(change.state)?),
                    ))
                })
                .collect::<Result<_, Malformed>>()?,
        )),
        Order::SendSignChange(sign) => PlayerAction::Client(ClientChange::Sign {
            pos: shared::block_pos(required(sign.pos)?),
            lines: sign.lines,
            color: sign.color,
            glowing: sign.glowing,
        }),
        Order::SendEquipmentChange(change) => PlayerAction::Client(ClientChange::Equipment {
            entity: EntityId(change.entity),
            slots: change
                .slots
                .into_iter()
                .map(|edit| (edit.slot, edit.item.map(shared::item)))
                .collect(),
        }),
        Order::SendHurtAnimation(hurt) => PlayerAction::Client(ClientChange::Hurt {
            entity: EntityId(hurt.entity),
            yaw: hurt.yaw,
        }),
        Order::SendResourcePack(offer) => PlayerAction::Client(ClientChange::ResourcePack {
            id: pack_id(&offer.id)?,
            url: offer.url,
            hash: offer.hash,
            required: offer.required,
            prompt: offer.prompt,
        }),
        Order::AwardCriterion(order) => PlayerAction::AwardCriterion {
            advancement: order.advancement,
            criterion: order.criterion,
        },
        Order::RevokeCriterion(order) => PlayerAction::RevokeCriterion {
            advancement: order.advancement,
            criterion: order.criterion,
        },
        Order::HideEntity(entity) => PlayerAction::HideEntity(EntityId(entity)),
        Order::ShowEntity(entity) => PlayerAction::ShowEntity(EntityId(entity)),
        Order::SetPlayerBorder(order) => match border(order)? {
            WorldAction::Border(order) => PlayerAction::SetBorder(order),
            _ => return Err(Malformed::Empty),
        },
        Order::ResetPlayerBorder(_) => PlayerAction::ResetBorder,
        Order::StoreCookie(cookie) => PlayerAction::StoreCookie {
            key: cookie.key,
            payload: cookie.payload,
        },
        Order::SendLinks(links) => PlayerAction::SendLinks(
            links
                .links
                .into_iter()
                .map(|link| (link.label, link.url))
                .collect(),
        ),
        Order::RemoveResourcePack(removal) => PlayerAction::Client(
            ClientChange::RemoveResourcePack(removal.id.as_deref().map(pack_id).transpose()?),
        ),
    })
}

fn pack_id(text: &str) -> Result<uuid::Uuid, Malformed> {
    uuid::Uuid::parse_str(text).map_err(|_| Malformed::Uuid(text.to_owned()))
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
        Query::RetrieveCookie(query) => ExtraQuestion::RetrieveCookie {
            player: player::id(query.player)?,
            key: query.key,
        },
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
            ExtraAnswer::Cookie(payload) => Answer::Cookie(v1::CookieValue { payload }),
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
    fn client_side_changes_convert_and_a_bad_pack_id_is_malformed() {
        use rspigot_core::plugins::{BlockState, ClientChange, ItemSpec};
        use rspigot_domain::BlockPos;

        let stone = v1::BlockState {
            block: "minecraft:stone".to_owned(),
            properties: Vec::new(),
        };
        assert_eq!(
            player_action(extra(player_extra::Order::SendBlockChanges(
                v1::ClientBlocks {
                    changes: vec![v1::ClientBlock {
                        pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
                        state: Some(stone.clone()),
                    }]
                }
            )))
            .unwrap(),
            PlayerAction::Client(ClientChange::Blocks(vec![(
                BlockPos::new(1, 2, 3),
                BlockState {
                    kind: "minecraft:stone".to_owned(),
                    properties: Vec::new(),
                }
            )]))
        );
        assert!(
            player_action(extra(player_extra::Order::SendBlockChanges(
                v1::ClientBlocks {
                    changes: vec![v1::ClientBlock {
                        pos: None,
                        state: Some(stone),
                    }]
                }
            )))
            .is_err()
        );
        assert_eq!(
            player_action(extra(player_extra::Order::SendEquipmentChange(
                v1::ClientEquipment {
                    entity: 4,
                    slots: vec![v1::EquipmentEdit {
                        slot: "head".to_owned(),
                        item: Some(v1::Item {
                            key: "minecraft:stick".to_owned(),
                            count: 1,
                            ..Default::default()
                        }),
                    }],
                }
            )))
            .unwrap(),
            PlayerAction::Client(ClientChange::Equipment {
                entity: EntityId(4),
                slots: vec![("head".to_owned(), Some(ItemSpec::of("minecraft:stick", 1)))],
            })
        );
        let offer = |id: &str| v1::PackOffer {
            id: id.to_owned(),
            url: "https://x.org/p.zip".to_owned(),
            hash: "h".to_owned(),
            required: true,
            prompt: Some("p".to_owned()),
        };
        assert!(
            player_action(extra(player_extra::Order::SendResourcePack(offer("nope")))).is_err()
        );
        assert!(matches!(
            player_action(extra(player_extra::Order::SendResourcePack(offer(
                "00000000-0000-0000-0000-000000000009"
            ))))
            .unwrap(),
            PlayerAction::Client(ClientChange::ResourcePack { required: true, .. })
        ));
        assert_eq!(
            player_action(extra(player_extra::Order::RemoveResourcePack(
                v1::PackRemoval { id: None }
            )))
            .unwrap(),
            PlayerAction::Client(ClientChange::RemoveResourcePack(None))
        );
        assert!(
            player_action(extra(player_extra::Order::RemoveResourcePack(
                v1::PackRemoval {
                    id: Some("nope".to_owned())
                }
            )))
            .is_err()
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
