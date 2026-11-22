use crate::Packet;
use crate::codec::{Angle, Decode, DecodeError, Encode};
use crate::ids::play::clientbound as id;
use crate::ids::play::serverbound as sb;
use crate::play::types::Vec3;
use crate::play::wire::{List, Plain, UNBOUNDED, Var, read_var, wire_struct, write_var};

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ProjectilePower = id::PROJECTILE_POWER {
        pub entity_id: i32 as Var,
        pub acceleration_power: f64,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MinecartStep {
        pub position: Vec3,
        pub movement: Vec3,
        pub y_rot: Angle,
        pub x_rot: Angle,
        pub weight: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct MoveMinecartAlongTrack = id::MOVE_MINECART_ALONG_TRACK {
        pub entity_id: i32 as Var,
        pub steps: Vec<MinecartStep> as List<Plain, UNBOUNDED>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectatorAction {
    pub spectate_entity_id: Option<i32>,
}

impl Encode for SpectatorAction {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(
            self.spectate_entity_id.map_or(0, |id| id.wrapping_add(1)),
            out,
        );
    }
}

impl Decode for SpectatorAction {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let value = read_var(input)?;
        Ok(Self {
            spectate_entity_id: (value != 0).then(|| value.wrapping_sub(1)),
        })
    }
}

impl Packet for SpectatorAction {
    const ID: i32 = sb::SPECTATOR_ACTION;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::decode_exact;
    use crate::common::Ping;

    fn round_trip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let mut bytes = Vec::new();
        value.encode(&mut bytes);
        assert_eq!(decode_exact::<T>(&bytes).unwrap(), value);
    }

    #[test]
    fn projectile_power_is_a_var_int_then_a_double() {
        let packet = ProjectilePower {
            entity_id: 300,
            acceleration_power: 0.1,
        };
        let mut bytes = Vec::new();
        packet.encode(&mut bytes);
        assert_eq!(bytes.len(), 2 + 8);
        assert_eq!(&bytes[..2], &[0xac, 0x02]);
        round_trip(packet);
    }

    #[test]
    fn a_minecart_move_carries_its_steps() {
        let step = MinecartStep {
            position: Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            movement: Vec3 {
                x: 0.1,
                y: 0.0,
                z: -0.1,
            },
            y_rot: Angle(64),
            x_rot: Angle(0),
            weight: 1.0,
        };
        let packet = MoveMinecartAlongTrack {
            entity_id: 7,
            steps: vec![step, step],
        };
        let mut bytes = Vec::new();
        packet.encode(&mut bytes);
        assert_eq!(bytes.len(), 1 + 1 + 2 * (24 + 24 + 1 + 1 + 4));
        round_trip(packet);
    }

    #[test]
    fn a_spectator_action_uses_the_optional_var_int_encoding() {
        let mut bytes = Vec::new();
        SpectatorAction {
            spectate_entity_id: None,
        }
        .encode(&mut bytes);
        assert_eq!(bytes, [0]);
        round_trip(SpectatorAction {
            spectate_entity_id: Some(0),
        });
        round_trip(SpectatorAction {
            spectate_entity_id: Some(500),
        });
        let mut bytes = Vec::new();
        SpectatorAction {
            spectate_entity_id: Some(0),
        }
        .encode(&mut bytes);
        assert_eq!(bytes, [1]);
    }

    #[test]
    fn a_ping_is_a_plain_int() {
        round_trip(Ping { id: -5 });
    }
}
