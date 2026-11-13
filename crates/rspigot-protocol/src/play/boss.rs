use uuid::Uuid;

use crate::Packet;
use crate::codec::{Decode, DecodeError, Encode, VarInt};
use crate::ids::play::clientbound as id;
use crate::play::wire::wire_enum;
use crate::types::Text;

wire_enum! {
    pub enum BossColor: Strict { Pink, Blue, Red, Green, Yellow, Purple, White }
}

wire_enum! {
    pub enum BossOverlay: Strict { Progress, Notched6, Notched10, Notched12, Notched20 }
}

pub const FLAG_DARKEN_SCREEN: u8 = 1;
pub const FLAG_PLAY_MUSIC: u8 = 2;
pub const FLAG_CREATE_WORLD_FOG: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum BossOperation {
    Add {
        name: Text,
        progress: f32,
        color: BossColor,
        overlay: BossOverlay,
        flags: u8,
    },
    Remove,
    UpdateProgress(f32),
    UpdateName(Text),
    UpdateStyle {
        color: BossColor,
        overlay: BossOverlay,
    },
    UpdateProperties(u8),
}

impl BossOperation {
    const fn kind(&self) -> i32 {
        match self {
            Self::Add { .. } => 0,
            Self::Remove => 1,
            Self::UpdateProgress(_) => 2,
            Self::UpdateName(_) => 3,
            Self::UpdateStyle { .. } => 4,
            Self::UpdateProperties(_) => 5,
        }
    }
}

impl Encode for BossOperation {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.kind()).encode(out);
        match self {
            Self::Add {
                name,
                progress,
                color,
                overlay,
                flags,
            } => {
                name.encode(out);
                progress.encode(out);
                color.encode(out);
                overlay.encode(out);
                flags.encode(out);
            }
            Self::Remove => {}
            Self::UpdateProgress(progress) => progress.encode(out),
            Self::UpdateName(name) => name.encode(out),
            Self::UpdateStyle { color, overlay } => {
                color.encode(out);
                overlay.encode(out);
            }
            Self::UpdateProperties(flags) => flags.encode(out),
        }
    }
}

impl Decode for BossOperation {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let kind = VarInt::decode(input)?.0;
        Ok(match kind {
            0 => Self::Add {
                name: Text::decode(input)?,
                progress: f32::decode(input)?,
                color: BossColor::decode(input)?,
                overlay: BossOverlay::decode(input)?,
                flags: u8::decode(input)?,
            },
            1 => Self::Remove,
            2 => Self::UpdateProgress(f32::decode(input)?),
            3 => Self::UpdateName(Text::decode(input)?),
            4 => Self::UpdateStyle {
                color: BossColor::decode(input)?,
                overlay: BossOverlay::decode(input)?,
            },
            5 => Self::UpdateProperties(u8::decode(input)?),
            other => {
                return Err(DecodeError::InvalidValue {
                    field: "boss_event_operation",
                    value: i64::from(other),
                });
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossEvent {
    pub id: Uuid,
    pub operation: BossOperation,
}

impl Encode for BossEvent {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        self.operation.encode(out);
    }
}

impl Decode for BossEvent {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            id: Uuid::decode(input)?,
            operation: BossOperation::decode(input)?,
        })
    }
}

impl Packet for BossEvent {
    const ID: i32 = id::BOSS_EVENT;
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;
    use crate::codec::decode_exact;
    use crate::play::wire::testing::{encoded, roundtrip};

    fn text(s: &str) -> Text {
        Text(Tag::String(s.into()))
    }

    #[test]
    fn the_packet_id_matches_the_packets_report() {
        assert_eq!(<BossEvent as Packet>::ID, 9);
    }

    #[test]
    fn the_add_operation_writes_name_progress_style_and_flags_after_operation_zero() {
        let packet = BossEvent {
            id: Uuid::from_u128(7),
            operation: BossOperation::Add {
                name: text("Wither"),
                progress: 0.5,
                color: BossColor::Purple,
                overlay: BossOverlay::Progress,
                flags: FLAG_DARKEN_SCREEN,
            },
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..16], Uuid::from_u128(7).as_bytes());
        assert_eq!(bytes[16], 0);
        let tail = &bytes[bytes.len() - 7..];
        assert_eq!(tail, &[0x3f, 0x00, 0x00, 0x00, 5, 0, 1]);
    }

    #[test]
    fn every_other_operation_round_trips_with_its_own_discriminant() {
        let id = Uuid::from_u128(1);
        let cases = [
            (BossOperation::Remove, 1u8, 0usize),
            (BossOperation::UpdateProgress(0.25), 2, 4),
            (BossOperation::UpdateName(text("x")), 3, 0),
            (
                BossOperation::UpdateStyle {
                    color: BossColor::Red,
                    overlay: BossOverlay::Notched20,
                },
                4,
                2,
            ),
            (BossOperation::UpdateProperties(6), 5, 1),
        ];
        for (operation, kind, payload) in cases {
            let packet = BossEvent { id, operation };
            let bytes = roundtrip(&packet);
            assert_eq!(bytes[16], kind);
            if payload > 0 {
                assert_eq!(bytes.len(), 17 + payload);
            }
        }
    }

    #[test]
    fn an_unknown_operation_is_refused() {
        let mut bytes = encoded(&Uuid::from_u128(1));
        bytes.push(6);
        assert!(decode_exact::<BossEvent>(&bytes).is_err());
    }
}
