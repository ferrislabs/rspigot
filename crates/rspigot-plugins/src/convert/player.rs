use rspigot_core::events::Direction;
use rspigot_core::plugins::{Order, PlayerOrder, PlayerSnapshot, PlayerState, Question, TargetHit};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_plugin_protocol::v1::{self, player_order::Order as Wire, target};
use uuid::Uuid;

use super::shared::{self, required, wire_block_pos, wire_rotation, wire_vec};
use super::{Malformed, entity};

pub(super) fn order(order: v1::PlayerOrder) -> Result<Order, Malformed> {
    let player = id(order.player)?;
    let order = match required(order.order)? {
        Wire::SendMessage(text) => PlayerOrder::SendMessage(text),
        Wire::SendActionBar(text) => PlayerOrder::SendActionBar(text),
        Wire::SendTitle(title) => PlayerOrder::SendTitle(shared::title(title)),
        Wire::ResetTitle(_) => PlayerOrder::ResetTitle,
        Wire::SetTabList(v1::TabList { header, footer }) => {
            PlayerOrder::SetTabList { header, footer }
        }
        Wire::Teleport(position) => PlayerOrder::Teleport(shared::vec3(position)),
        Wire::TeleportFacing(facing) => PlayerOrder::TeleportFacing(
            shared::vec3(required(facing.position)?),
            shared::rotation(required(facing.rotation)?),
        ),
        Wire::TeleportTo(location) => PlayerOrder::TeleportTo {
            world: location.world,
            position: shared::vec3(required(location.position)?),
            rotation: location.rotation.map(shared::rotation),
        },
        Wire::SetGameMode(mode) => PlayerOrder::SetGameMode(game_mode(mode)?),
        Wire::Kick(reason) => PlayerOrder::Kick(reason),
        Wire::SetHealth(health) => PlayerOrder::SetHealth(health),
        Wire::SetAbsorption(amount) => PlayerOrder::SetAbsorption(amount),
        Wire::Damage(amount) => PlayerOrder::Damage(amount),
        Wire::Kill(_) => PlayerOrder::Kill,
        Wire::SetFoodLevel(food) => PlayerOrder::SetFoodLevel(food),
        Wire::SetSaturation(saturation) => PlayerOrder::SetSaturation(saturation),
        Wire::SetExhaustion(exhaustion) => PlayerOrder::SetExhaustion(exhaustion),
        Wire::GiveExp(points) => PlayerOrder::GiveExp(points),
        Wire::GiveLevels(levels) => PlayerOrder::GiveLevels(levels),
        Wire::SetLevel(level) => PlayerOrder::SetLevel(level),
        Wire::SetExp(progress) => PlayerOrder::SetExp(progress),
        Wire::AddEffect(effect) => PlayerOrder::AddEffect(shared::effect(effect)),
        Wire::RemoveEffect(key) => PlayerOrder::RemoveEffect(key),
        Wire::ClearEffects(_) => PlayerOrder::ClearEffects,
        Wire::SetFireTicks(ticks) => PlayerOrder::SetFireTicks(ticks),
        Wire::SetVelocity(velocity) => PlayerOrder::SetVelocity(shared::vec3(velocity)),
        Wire::SetAllowFlight(allowed) => PlayerOrder::SetAllowFlight(allowed),
        Wire::SetFlying(flying) => PlayerOrder::SetFlying(flying),
        Wire::SetFlySpeed(speed) => PlayerOrder::SetFlySpeed(speed),
        Wire::SetWalkSpeed(speed) => PlayerOrder::SetWalkSpeed(speed),
        Wire::SetOp(operator) => PlayerOrder::SetOp(operator),
        Wire::Respawn(_) => PlayerOrder::Respawn,
        Wire::PerformCommand(line) => PlayerOrder::PerformCommand(line),
        Wire::Chat(text) => PlayerOrder::Chat(text),
        Wire::PlaySound(sound) => PlayerOrder::PlaySound(shared::sound(sound)),
        Wire::SpawnParticle(particle) => PlayerOrder::SpawnParticle(shared::particle(particle)?),
        Wire::CloseInventory(_) => PlayerOrder::CloseInventory,
        Wire::UpdateInventory(_) => PlayerOrder::UpdateInventory,
        Wire::Ban(terms) => PlayerOrder::Ban {
            reason: terms.reason,
            expires: shared::expiry(terms.expires_ms)?,
        },
        Wire::HidePlayer(target) => PlayerOrder::HidePlayer(id(target)?),
        Wire::ShowPlayer(target) => PlayerOrder::ShowPlayer(id(target)?),
        Wire::OpenInventory(inventory) => PlayerOrder::OpenInventory(inventory),
    };
    Ok(Order::Player { player, order })
}

pub(super) fn id(raw: String) -> Result<PlayerId, Malformed> {
    Uuid::parse_str(&raw)
        .map(PlayerId)
        .map_err(|_| Malformed::Player(raw))
}

pub(super) fn target_block(query: v1::TargetQuery) -> Result<Question, Malformed> {
    Ok(Question::TargetBlock {
        player: id(query.player)?,
        max_distance: query.max_distance,
    })
}

pub(super) fn target_entity(query: v1::TargetQuery) -> Result<Question, Malformed> {
    Ok(Question::TargetEntity {
        player: id(query.player)?,
        max_distance: query.max_distance,
    })
}

pub(super) fn wire_target(hit: Option<TargetHit>) -> v1::Target {
    v1::Target {
        hit: hit.map(|hit| match hit {
            TargetHit::Block { pos, face, kind } => target::Hit::Block(v1::BlockHit {
                pos: Some(wire_block_pos(pos)),
                face: wire_face(face) as i32,
                block: kind,
            }),
            TargetHit::Player(player) => target::Hit::Player(wire_player(player)),
            TargetHit::Entity(view) => target::Hit::Entity(entity::wire_view(view)),
        }),
    }
}

const fn wire_face(face: Direction) -> v1::Face {
    match face {
        Direction::Down => v1::Face::Down,
        Direction::Up => v1::Face::Up,
        Direction::North => v1::Face::North,
        Direction::South => v1::Face::South,
        Direction::West => v1::Face::West,
        Direction::East => v1::Face::East,
    }
}

pub(super) fn wire_state(state: PlayerState) -> v1::PlayerState {
    v1::PlayerState {
        player: Some(wire_player(state.player)),
        health: state.health,
        food: state.food,
        level: state.level,
        max_health: state.max_health,
        absorption: state.absorption,
        saturation: state.saturation,
        exhaustion: state.exhaustion,
        exp: state.exp,
        total_exp: state.total_exp,
        op: state.operator,
        allow_flight: state.allow_flight,
        flying: state.flying,
        fly_speed: state.fly_speed,
        walk_speed: state.walk_speed,
        sneaking: state.sneaking,
        sprinting: state.sprinting,
        on_ground: state.on_ground,
        fire_ticks: state.fire_ticks,
        dead: state.dead,
        effects: state.effects.into_iter().map(shared::wire_effect).collect(),
        ping: state.ping,
        locale: state.locale,
        address: state.address.to_string(),
        hidden: state
            .hidden
            .into_iter()
            .map(|hidden| hidden.0.hyphenated().to_string())
            .collect(),
    }
}

pub(super) fn snapshot(player: v1::PlayerSnapshot) -> Result<PlayerSnapshot, Malformed> {
    Ok(PlayerSnapshot {
        id: id(player.uuid)?,
        name: player.name,
        entity_id: player.entity_id,
        world: player.world,
        position: shared::vec3(required(player.position)?),
        rotation: shared::rotation(required(player.rotation)?),
        game_mode: game_mode(player.game_mode)?,
    })
}

pub(super) fn wire_player(player: PlayerSnapshot) -> v1::PlayerSnapshot {
    v1::PlayerSnapshot {
        uuid: player.id.0.hyphenated().to_string(),
        name: player.name,
        entity_id: player.entity_id,
        world: player.world,
        position: Some(wire_vec(player.position)),
        game_mode: wire_game_mode(player.game_mode) as i32,
        rotation: Some(wire_rotation(player.rotation)),
    }
}

fn game_mode(value: i32) -> Result<GameMode, Malformed> {
    match v1::GameMode::try_from(value) {
        Ok(v1::GameMode::Survival) => Ok(GameMode::Survival),
        Ok(v1::GameMode::Creative) => Ok(GameMode::Creative),
        Ok(v1::GameMode::Adventure) => Ok(GameMode::Adventure),
        Ok(v1::GameMode::Spectator) => Ok(GameMode::Spectator),
        Ok(v1::GameMode::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "game_mode",
            value,
        }),
    }
}

pub(super) const fn wire_game_mode(mode: GameMode) -> v1::GameMode {
    match mode {
        GameMode::Survival => v1::GameMode::Survival,
        GameMode::Creative => v1::GameMode::Creative,
        GameMode::Adventure => v1::GameMode::Adventure,
        GameMode::Spectator => v1::GameMode::Spectator,
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::time::{Duration, UNIX_EPOCH};

    use rspigot_core::plugins::{
        Answer, EffectSpec, EntityView, FromPlugin, ParticleDataSpec, ParticleSpec, PlayerOrder,
        PlayerState, TitleSpec, ToPlugin,
    };
    use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
    use rspigot_plugin_protocol::v1::{answer, particle, player_order};

    use super::*;
    use crate::convert::{from_plugin, question, to_plugin};

    const STEVE: &str = "00000000-0000-0000-0000-000000000001";

    fn convert(order: player_order::Order) -> Result<PlayerOrder, Malformed> {
        let converted = super::order(v1::PlayerOrder {
            player: STEVE.into(),
            order: Some(order),
        })?;
        let Order::Player { player, order } = converted else {
            panic!("expected a player order");
        };
        assert_eq!(player, PlayerId(Uuid::from_u128(1)));
        Ok(order)
    }

    fn wire_vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
        v1::Vec3 { x, y, z }
    }

    fn snapshot() -> PlayerSnapshot {
        PlayerSnapshot {
            id: PlayerId(Uuid::from_u128(1)),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.0, 64.0, -3.5),
            rotation: Rotation::new(90.0, -30.0),
            game_mode: GameMode::Survival,
        }
    }

    #[test]
    fn a_player_snapshot_survives_the_round_trip() {
        assert_eq!(
            super::snapshot(wire_player(snapshot())).ok(),
            Some(snapshot())
        );
        let mut stranger = wire_player(snapshot());
        stranger.uuid = "nobody".to_owned();
        assert!(matches!(
            super::snapshot(stranger),
            Err(Malformed::Player(_))
        ));
        let mut lost = wire_player(snapshot());
        lost.position = None;
        assert!(super::snapshot(lost).is_err());
        assert_eq!(
            shared::uuid("00000000-0000-0000-0000-000000000001".to_owned()).ok(),
            Some(Uuid::from_u128(1))
        );
        assert!(
            matches!(shared::uuid("nobody".to_owned()), Err(Malformed::Uuid(raw)) if raw == "nobody")
        );
    }

    #[test]
    fn player_orders_convert_with_their_values() {
        assert_eq!(
            convert(player_order::Order::SetHealth(5.5)).unwrap(),
            PlayerOrder::SetHealth(5.5)
        );
        assert_eq!(
            convert(player_order::Order::SendTitle(v1::Title {
                title: "§6Hi".into(),
                subtitle: None,
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            }))
            .unwrap(),
            PlayerOrder::SendTitle(TitleSpec {
                title: "§6Hi".into(),
                subtitle: None,
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            })
        );
        assert_eq!(
            convert(player_order::Order::TeleportFacing(v1::Facing {
                position: Some(wire_vec3(1.5, 70.0, -2.5)),
                rotation: Some(v1::Rotation {
                    yaw: 90.0,
                    pitch: 10.0
                }),
            }))
            .unwrap(),
            PlayerOrder::TeleportFacing(Vec3::new(1.5, 70.0, -2.5), Rotation::new(90.0, 10.0))
        );
        assert_eq!(
            convert(player_order::Order::TeleportTo(v1::Location {
                world: "minecraft:the_end".into(),
                position: Some(wire_vec3(0.5, 80.0, 0.5)),
                rotation: None,
            }))
            .unwrap(),
            PlayerOrder::TeleportTo {
                world: "minecraft:the_end".into(),
                position: Vec3::new(0.5, 80.0, 0.5),
                rotation: None,
            }
        );
        assert!(
            convert(player_order::Order::TeleportTo(v1::Location {
                world: String::new(),
                position: None,
                rotation: None,
            }))
            .is_err()
        );
        assert_eq!(
            convert(player_order::Order::SpawnParticle(v1::Particle {
                key: "minecraft:dust".into(),
                data: Some(particle::Data::Dust(v1::Dust {
                    color: 0xFF0000,
                    scale: 1.0
                })),
                position: Some(wire_vec3(0.5, 65.0, 0.5)),
                count: 4,
                spread: Some(wire_vec3(0.25, 0.0, 0.25)),
                speed: 0.1,
            }))
            .unwrap(),
            PlayerOrder::SpawnParticle(ParticleSpec {
                key: "minecraft:dust".into(),
                data: ParticleDataSpec::Dust {
                    color: 0xFF0000,
                    scale: 1.0
                },
                position: Vec3::new(0.5, 65.0, 0.5),
                count: 4,
                spread: Vec3::new(0.25, 0.0, 0.25),
                speed: 0.1,
            })
        );
        assert_eq!(
            convert(player_order::Order::AddEffect(v1::Effect {
                key: "minecraft:speed".into(),
                duration: -1,
                amplifier: 2,
                ambient: true,
                particles: false,
                icon: true,
            }))
            .unwrap(),
            PlayerOrder::AddEffect(EffectSpec {
                key: "minecraft:speed".into(),
                duration: -1,
                amplifier: 2,
                ambient: true,
                particles: false,
                icon: true,
            })
        );
        assert_eq!(
            convert(player_order::Order::SetGameMode(
                v1::GameMode::Adventure as i32
            ))
            .unwrap(),
            PlayerOrder::SetGameMode(GameMode::Adventure)
        );
        assert!(matches!(
            convert(player_order::Order::SetGameMode(0)),
            Err(Malformed::Enum {
                field: "game_mode",
                value: 0
            })
        ));
        assert!(matches!(
            convert(player_order::Order::TeleportFacing(v1::Facing {
                position: None,
                rotation: None,
            })),
            Err(Malformed::Empty)
        ));
    }

    fn state() -> PlayerState {
        PlayerState {
            player: snapshot(),
            health: 10.0,
            max_health: 20.0,
            absorption: 4.0,
            food: 5,
            saturation: 2.5,
            exhaustion: 1.5,
            level: 3,
            exp: 0.5,
            total_exp: 27,
            operator: true,
            allow_flight: true,
            flying: true,
            fly_speed: 0.2,
            walk_speed: 0.4,
            sneaking: true,
            sprinting: false,
            on_ground: true,
            fire_ticks: 100,
            dead: false,
            effects: vec![EffectSpec {
                key: "minecraft:speed".into(),
                duration: 200,
                amplifier: 1,
                ambient: false,
                particles: true,
                icon: true,
            }],
            ping: 43,
            locale: "fr_fr".into(),
            address: IpAddr::from([203, 0, 113, 7]),
            hidden: Vec::new(),
        }
    }

    #[test]
    fn a_full_player_state_converts_to_the_wire() {
        assert_eq!(
            wire_state(state()),
            v1::PlayerState {
                player: Some(wire_player(snapshot())),
                health: 10.0,
                food: 5,
                level: 3,
                max_health: 20.0,
                absorption: 4.0,
                saturation: 2.5,
                exhaustion: 1.5,
                exp: 0.5,
                total_exp: 27,
                op: true,
                allow_flight: true,
                flying: true,
                fly_speed: 0.2,
                walk_speed: 0.4,
                sneaking: true,
                sprinting: false,
                on_ground: true,
                fire_ticks: 100,
                dead: false,
                effects: vec![v1::Effect {
                    key: "minecraft:speed".into(),
                    duration: 200,
                    amplifier: 1,
                    ambient: false,
                    particles: true,
                    icon: true,
                }],
                ping: 43,
                locale: "fr_fr".into(),
                address: "203.0.113.7".into(),
                hidden: Vec::new(),
            }
        );
    }

    #[test]
    fn hide_and_show_convert() {
        const ALEX: &str = "00000000-0000-0000-0000-000000000002";
        let alex = PlayerId(Uuid::from_u128(2));
        assert_eq!(
            convert(player_order::Order::HidePlayer(ALEX.into())).unwrap(),
            PlayerOrder::HidePlayer(alex)
        );
        assert_eq!(
            convert(player_order::Order::ShowPlayer(ALEX.into())).unwrap(),
            PlayerOrder::ShowPlayer(alex)
        );
        assert!(matches!(
            convert(player_order::Order::HidePlayer("alex".into())),
            Err(Malformed::Player(raw)) if raw == "alex"
        ));
        let hiding = PlayerState {
            hidden: vec![alex, PlayerId(Uuid::from_u128(3))],
            ..state()
        };
        assert_eq!(
            wire_state(hiding).hidden,
            [ALEX, "00000000-0000-0000-0000-000000000003"]
        );
    }

    fn ask(asked: question::Question) -> Result<FromPlugin, Malformed> {
        from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 5,
                    question: Some(asked),
                })),
                origin: None,
            },
            &mut Vec::new(),
        )
    }

    fn answered(answer: Answer) -> Option<answer::Body> {
        match to_plugin(ToPlugin::Answer { request: 5, answer }).body {
            Some(to_plugin::Body::Answer(v1::Answer { request: 5, body })) => body,
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    fn sight() -> v1::TargetQuery {
        v1::TargetQuery {
            player: STEVE.into(),
            max_distance: 8.5,
        }
    }

    #[test]
    fn target_questions_and_answers_convert() {
        let steve = PlayerId(Uuid::from_u128(1));
        assert_eq!(
            ask(question::Question::TargetBlock(sight())).unwrap(),
            FromPlugin::Question {
                request: 5,
                question: Question::TargetBlock {
                    player: steve,
                    max_distance: 8.5
                }
            }
        );
        assert_eq!(
            ask(question::Question::TargetEntity(sight())).unwrap(),
            FromPlugin::Question {
                request: 5,
                question: Question::TargetEntity {
                    player: steve,
                    max_distance: 8.5
                }
            }
        );
        assert!(matches!(
            ask(question::Question::TargetBlock(v1::TargetQuery {
                player: "steve".into(),
                max_distance: 8.5,
            })),
            Err(Malformed::Player(raw)) if raw == "steve"
        ));

        assert_eq!(
            answered(Answer::Target(Some(TargetHit::Block {
                pos: BlockPos::new(0, 65, 3),
                face: Direction::North,
                kind: "minecraft:stone".into(),
            }))),
            Some(answer::Body::Target(v1::Target {
                hit: Some(target::Hit::Block(v1::BlockHit {
                    pos: Some(v1::BlockPos { x: 0, y: 65, z: 3 }),
                    face: v1::Face::North as i32,
                    block: "minecraft:stone".into(),
                })),
            }))
        );
        assert_eq!(
            answered(Answer::Target(Some(TargetHit::Player(snapshot())))),
            Some(answer::Body::Target(v1::Target {
                hit: Some(target::Hit::Player(wire_player(snapshot()))),
            }))
        );
        assert_eq!(
            answered(Answer::Target(Some(TargetHit::Entity(EntityView {
                id: EntityId(42),
                uuid: Uuid::from_u128(7),
                kind: "minecraft:skeleton".into(),
                position: Vec3::new(0.5, 64.0, 3.5),
            })))),
            Some(answer::Body::Target(v1::Target {
                hit: Some(target::Hit::Entity(v1::EntitySnapshot {
                    entity_id: 42,
                    uuid: "00000000-0000-0000-0000-000000000007".into(),
                    kind: "minecraft:skeleton".into(),
                    position: Some(wire_vec3(0.5, 64.0, 3.5)),
                })),
            }))
        );
        assert_eq!(
            answered(Answer::Target(None)),
            Some(answer::Body::Target(v1::Target { hit: None }))
        );
    }

    #[test]
    fn player_ban_and_address_convert() {
        assert_eq!(
            convert(player_order::Order::Ban(v1::BanTerms {
                reason: None,
                expires_ms: None,
            }))
            .unwrap(),
            PlayerOrder::Ban {
                reason: None,
                expires: None,
            }
        );
        assert_eq!(
            convert(player_order::Order::Ban(v1::BanTerms {
                reason: Some("cheating".into()),
                expires_ms: Some(-1_500),
            }))
            .unwrap(),
            PlayerOrder::Ban {
                reason: Some("cheating".into()),
                expires: Some(UNIX_EPOCH - Duration::from_millis(1_500)),
            }
        );
        let ipv6 = PlayerState {
            address: "2001:db8::7".parse().expect("an address"),
            ..state()
        };
        assert_eq!(wire_state(ipv6).address, "2001:db8::7");
    }
}
