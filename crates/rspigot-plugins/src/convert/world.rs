use rspigot_core::commands::Weather;
use rspigot_core::plugins::{GameRule, Order, WorldOrder, WorldState};
use rspigot_domain::world::Difficulty;
use rspigot_plugin_protocol::v1::{self, world_order::Order as Wire};

use super::Malformed;
use super::shared::{self, required};

pub(super) fn order(order: v1::WorldOrder) -> Result<Order, Malformed> {
    let order = match required(order.order)? {
        Wire::SetTime(time) => WorldOrder::SetTime(time),
        Wire::SetWeather(value) => WorldOrder::SetWeather(weather(value)?),
        Wire::SetDifficulty(value) => WorldOrder::SetDifficulty(difficulty(value)?),
        Wire::SetSpawn(pos) => WorldOrder::SetSpawn(shared::block_pos(pos)),
        Wire::SetPvp(pvp) => WorldOrder::SetPvp(pvp),
        Wire::SetGameRule(v1::SetGameRule { rule, value }) => {
            WorldOrder::SetGameRule(game_rule(rule)?, value)
        }
        Wire::Save(_) => WorldOrder::Save,
        Wire::PlaySound(sound) => WorldOrder::PlaySound {
            position: shared::vec3(required(sound.position)?),
            sound: shared::sound(sound),
        },
        Wire::SpawnParticle(particle) => WorldOrder::SpawnParticle(shared::particle(particle)?),
        Wire::Explode(v1::Explode {
            position,
            power,
            breaks_blocks,
        }) => WorldOrder::Explode {
            position: shared::vec3(required(position)?),
            power,
            breaks_blocks,
        },
    };
    Ok(Order::World(order))
}

pub(super) fn wire_state(state: WorldState) -> v1::WorldState {
    v1::WorldState {
        name: state.name,
        uuid: state.uuid.map(|uuid| uuid.hyphenated().to_string()),
        seed: state.seed,
        day_time: state.day_time,
        game_time: state.game_time,
        raining: state.raining,
        thundering: state.thundering,
        difficulty: wire_difficulty(state.difficulty) as i32,
        spawn: Some(shared::wire_block_pos(state.spawn)),
        pvp: state.pvp,
        rules: state
            .rules
            .into_iter()
            .map(|(rule, value)| v1::GameRuleValue {
                rule: wire_game_rule(rule) as i32,
                value,
            })
            .collect(),
    }
}

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

fn difficulty(value: i32) -> Result<Difficulty, Malformed> {
    match v1::Difficulty::try_from(value) {
        Ok(v1::Difficulty::Peaceful) => Ok(Difficulty::Peaceful),
        Ok(v1::Difficulty::Easy) => Ok(Difficulty::Easy),
        Ok(v1::Difficulty::Normal) => Ok(Difficulty::Normal),
        Ok(v1::Difficulty::Hard) => Ok(Difficulty::Hard),
        Ok(v1::Difficulty::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "difficulty",
            value,
        }),
    }
}

const fn wire_difficulty(difficulty: Difficulty) -> v1::Difficulty {
    match difficulty {
        Difficulty::Peaceful => v1::Difficulty::Peaceful,
        Difficulty::Easy => v1::Difficulty::Easy,
        Difficulty::Normal => v1::Difficulty::Normal,
        Difficulty::Hard => v1::Difficulty::Hard,
    }
}

fn game_rule(value: i32) -> Result<GameRule, Malformed> {
    match v1::GameRule::try_from(value) {
        Ok(v1::GameRule::DaylightCycle) => Ok(GameRule::DaylightCycle),
        Ok(v1::GameRule::NaturalRegeneration) => Ok(GameRule::NaturalRegeneration),
        Ok(v1::GameRule::KeepInventory) => Ok(GameRule::KeepInventory),
        Ok(v1::GameRule::ShowDeathMessages) => Ok(GameRule::ShowDeathMessages),
        Ok(v1::GameRule::ImmediateRespawn) => Ok(GameRule::ImmediateRespawn),
        Ok(v1::GameRule::FallDamage) => Ok(GameRule::FallDamage),
        Ok(v1::GameRule::MobDrops) => Ok(GameRule::MobDrops),
        Ok(v1::GameRule::MobGriefing) => Ok(GameRule::MobGriefing),
        Ok(v1::GameRule::MobExplosionDropDecay) => Ok(GameRule::MobExplosionDropDecay),
        Ok(v1::GameRule::BlockDrops) => Ok(GameRule::BlockDrops),
        Ok(v1::GameRule::UniversalAnger) => Ok(GameRule::UniversalAnger),
        Ok(v1::GameRule::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "game_rule",
            value,
        }),
    }
}

const fn wire_game_rule(rule: GameRule) -> v1::GameRule {
    match rule {
        GameRule::DaylightCycle => v1::GameRule::DaylightCycle,
        GameRule::NaturalRegeneration => v1::GameRule::NaturalRegeneration,
        GameRule::KeepInventory => v1::GameRule::KeepInventory,
        GameRule::ShowDeathMessages => v1::GameRule::ShowDeathMessages,
        GameRule::ImmediateRespawn => v1::GameRule::ImmediateRespawn,
        GameRule::FallDamage => v1::GameRule::FallDamage,
        GameRule::MobDrops => v1::GameRule::MobDrops,
        GameRule::MobGriefing => v1::GameRule::MobGriefing,
        GameRule::MobExplosionDropDecay => v1::GameRule::MobExplosionDropDecay,
        GameRule::BlockDrops => v1::GameRule::BlockDrops,
        GameRule::UniversalAnger => v1::GameRule::UniversalAnger,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::commands::Weather;
    use rspigot_core::plugins::{
        Answer, FromPlugin, GameRule, Order, Question, SoundSpec, ToPlugin, WorldOrder, WorldState,
    };
    use rspigot_domain::world::Difficulty;
    use rspigot_domain::{BlockPos, Vec3};
    use rspigot_plugin_protocol::v1::{self, answer, world_order};
    use uuid::Uuid;

    use super::*;
    use crate::convert::{Malformed, from_plugin, question, to_plugin};

    fn convert(order: Option<world_order::Order>) -> Result<WorldOrder, Malformed> {
        let converted = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::World(v1::WorldOrder { order })),
                origin: None,
                request: None,
            },
            &mut Vec::new(),
        )?;
        let FromPlugin::Order(Order::World(order)) = converted else {
            panic!("expected a world order");
        };
        Ok(order)
    }

    fn wire_vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
        v1::Vec3 { x, y, z }
    }

    fn state() -> WorldState {
        WorldState {
            name: "world".into(),
            uuid: None,
            seed: 42,
            day_time: 6000,
            game_time: 12000,
            raining: true,
            thundering: false,
            difficulty: Difficulty::Hard,
            spawn: BlockPos::new(5, 64, -5),
            pvp: false,
            rules: vec![
                (GameRule::DaylightCycle, false),
                (GameRule::KeepInventory, true),
            ],
        }
    }

    #[test]
    fn world_orders_and_state_convert() {
        assert_eq!(
            convert(Some(world_order::Order::SetWeather(
                v1::Weather::Thunder as i32
            )))
            .unwrap(),
            WorldOrder::SetWeather(Weather::Thunder)
        );
        assert!(matches!(
            convert(Some(world_order::Order::SetWeather(0))),
            Err(Malformed::Enum {
                field: "weather",
                value: 0
            })
        ));
        assert_eq!(
            convert(Some(world_order::Order::SetDifficulty(
                v1::Difficulty::Peaceful as i32
            )))
            .unwrap(),
            WorldOrder::SetDifficulty(Difficulty::Peaceful)
        );
        assert_eq!(
            convert(Some(world_order::Order::SetGameRule(v1::SetGameRule {
                rule: v1::GameRule::MobGriefing as i32,
                value: false,
            })))
            .unwrap(),
            WorldOrder::SetGameRule(GameRule::MobGriefing, false)
        );
        assert!(matches!(
            convert(Some(world_order::Order::SetGameRule(v1::SetGameRule {
                rule: 99,
                value: true,
            }))),
            Err(Malformed::Enum {
                field: "game_rule",
                value: 99
            })
        ));
        assert_eq!(
            convert(Some(world_order::Order::PlaySound(v1::Sound {
                key: "minecraft:entity.cow.hurt".into(),
                volume: 0.5,
                pitch: 1.5,
                position: Some(wire_vec3(1.0, 64.0, -2.0)),
            })))
            .unwrap(),
            WorldOrder::PlaySound {
                sound: SoundSpec {
                    key: "minecraft:entity.cow.hurt".into(),
                    volume: 0.5,
                    pitch: 1.5,
                },
                position: Vec3::new(1.0, 64.0, -2.0),
            }
        );
        assert!(matches!(
            convert(Some(world_order::Order::PlaySound(v1::Sound {
                key: "minecraft:entity.cow.hurt".into(),
                volume: 1.0,
                pitch: 1.0,
                position: None,
            }))),
            Err(Malformed::Empty)
        ));
        assert_eq!(
            convert(Some(world_order::Order::Explode(v1::Explode {
                position: Some(wire_vec3(3.5, 64.5, 3.5)),
                power: 4.0,
                breaks_blocks: true,
            })))
            .unwrap(),
            WorldOrder::Explode {
                position: Vec3::new(3.5, 64.5, 3.5),
                power: 4.0,
                breaks_blocks: true,
            }
        );
        assert_eq!(
            convert(Some(world_order::Order::SetSpawn(v1::BlockPos {
                x: 5,
                y: 64,
                z: -5
            })))
            .unwrap(),
            WorldOrder::SetSpawn(BlockPos::new(5, 64, -5))
        );
        assert!(matches!(convert(None), Err(Malformed::Empty)));

        let wire = v1::WorldState {
            name: "world".into(),
            uuid: None,
            seed: 42,
            day_time: 6000,
            game_time: 12000,
            raining: true,
            thundering: false,
            difficulty: v1::Difficulty::Hard as i32,
            spawn: Some(v1::BlockPos { x: 5, y: 64, z: -5 }),
            pvp: false,
            rules: vec![
                v1::GameRuleValue {
                    rule: v1::GameRule::DaylightCycle as i32,
                    value: false,
                },
                v1::GameRuleValue {
                    rule: v1::GameRule::KeepInventory as i32,
                    value: true,
                },
            ],
        };
        assert_eq!(wire_state(state()), wire);
        let named = WorldState {
            uuid: Some(Uuid::from_u128(7)),
            ..state()
        };
        assert_eq!(
            wire_state(named).uuid.as_deref(),
            Some("00000000-0000-0000-0000-000000000007")
        );

        let asked = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 4,
                    question: Some(question::Question::World(v1::Empty {})),
                })),
                origin: None,
                request: None,
            },
            &mut Vec::new(),
        );
        assert_eq!(
            asked.unwrap(),
            FromPlugin::Question {
                request: 4,
                question: Question::World
            }
        );
        let answered = to_plugin(ToPlugin::Answer {
            request: 4,
            answer: Answer::World(state()),
        });
        assert_eq!(
            answered.body,
            Some(to_plugin::Body::Answer(v1::Answer {
                request: 4,
                body: Some(answer::Body::World(wire)),
            }))
        );
    }
}
