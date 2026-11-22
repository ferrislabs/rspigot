use uuid::Uuid;

use crate::codec::{Decode, DecodeError, Encode, VarInt};
use crate::common::Identifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaypointOperation {
    Track,
    Untrack,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaypointKey {
    Uuid(Uuid),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaypointData {
    Empty,
    Vec3i(i32, i32, i32),
    Chunk(i32, i32),
    Azimuth(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackedWaypoint {
    pub operation: WaypointOperation,
    pub key: WaypointKey,
    pub style: Identifier,
    pub color: Option<u32>,
    pub data: WaypointData,
}

impl Encode for TrackedWaypoint {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(match self.operation {
            WaypointOperation::Track => 0,
            WaypointOperation::Untrack => 1,
            WaypointOperation::Update => 2,
        })
        .encode(out);
        match &self.key {
            WaypointKey::Uuid(id) => {
                true.encode(out);
                id.encode(out);
            }
            WaypointKey::Name(name) => {
                false.encode(out);
                name.encode(out);
            }
        }
        self.style.encode(out);
        match self.color {
            Some(rgb) => {
                true.encode(out);
                out.extend_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
            }
            None => false.encode(out),
        }
        match self.data {
            WaypointData::Empty => VarInt(0).encode(out),
            WaypointData::Vec3i(x, y, z) => {
                VarInt(1).encode(out);
                VarInt(x).encode(out);
                VarInt(y).encode(out);
                VarInt(z).encode(out);
            }
            WaypointData::Chunk(x, z) => {
                VarInt(2).encode(out);
                VarInt(x).encode(out);
                VarInt(z).encode(out);
            }
            WaypointData::Azimuth(angle) => {
                VarInt(3).encode(out);
                angle.encode(out);
            }
        }
    }
}

impl Decode for TrackedWaypoint {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let operation = match VarInt::decode(input)?.0 {
            0 => WaypointOperation::Track,
            1 => WaypointOperation::Untrack,
            2 => WaypointOperation::Update,
            other => {
                return Err(DecodeError::InvalidValue {
                    field: "waypoint operation",
                    value: i64::from(other),
                });
            }
        };
        let key = if bool::decode(input)? {
            WaypointKey::Uuid(Uuid::decode(input)?)
        } else {
            WaypointKey::Name(String::decode(input)?)
        };
        let style = Identifier::decode(input)?;
        let color = if bool::decode(input)? {
            let (head, rest) = input.split_at_checked(3).ok_or(DecodeError::InvalidValue {
                field: "waypoint color",
                value: 0,
            })?;
            *input = rest;
            Some((u32::from(head[0]) << 16) | (u32::from(head[1]) << 8) | u32::from(head[2]))
        } else {
            None
        };
        let data = match VarInt::decode(input)?.0 {
            0 => WaypointData::Empty,
            1 => WaypointData::Vec3i(
                VarInt::decode(input)?.0,
                VarInt::decode(input)?.0,
                VarInt::decode(input)?.0,
            ),
            2 => WaypointData::Chunk(VarInt::decode(input)?.0, VarInt::decode(input)?.0),
            3 => WaypointData::Azimuth(f32::decode(input)?),
            other => {
                return Err(DecodeError::InvalidValue {
                    field: "waypoint type",
                    value: i64::from(other),
                });
            }
        };
        Ok(Self {
            operation,
            key,
            style,
            color,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tracked_waypoint_round_trips() {
        let packet = TrackedWaypoint {
            operation: WaypointOperation::Update,
            key: WaypointKey::Uuid(Uuid::from_u128(7)),
            style: Identifier::parse("minecraft:default").unwrap(),
            color: Some(0x00ff_aa11),
            data: WaypointData::Vec3i(-5, 70, 300),
        };
        let mut bytes = Vec::new();
        packet.encode(&mut bytes);
        let mut input = bytes.as_slice();
        assert_eq!(TrackedWaypoint::decode(&mut input).unwrap(), packet);
        assert!(input.is_empty());
    }

    #[test]
    fn an_azimuth_waypoint_without_color_round_trips() {
        let packet = TrackedWaypoint {
            operation: WaypointOperation::Track,
            key: WaypointKey::Name("spawn".to_owned()),
            style: Identifier::parse("minecraft:bowtie").unwrap(),
            color: None,
            data: WaypointData::Azimuth(1.5),
        };
        let mut bytes = Vec::new();
        packet.encode(&mut bytes);
        let mut input = bytes.as_slice();
        assert_eq!(TrackedWaypoint::decode(&mut input).unwrap(), packet);
    }
}
