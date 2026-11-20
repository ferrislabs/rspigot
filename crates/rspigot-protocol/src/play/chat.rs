use uuid::Uuid;

use crate::codec::{Decode, DecodeError, Encode, NetworkNbt};
use crate::ids::play::clientbound as id;
use crate::play::types::{ByteBitSet, Holder};
use crate::play::wire::{
    Bytes, List, Opt, Plain, UNBOUNDED, Utf, Var, read_array, read_var, wire_enum, wire_struct,
    write_var,
};
use crate::types::Text;

pub const SIGNATURE_BYTES: usize = 256;
pub const MAX_CHAT_CHARS: usize = 256;
pub const MAX_LAST_SEEN: usize = 20;
pub const LAST_SEEN_BITSET_BYTES: usize = 3;
pub const MAX_ARGUMENT_SIGNATURES: usize = 8;
pub const MAX_ARGUMENT_NAME_CHARS: usize = 16;
pub const MAX_PUBLIC_KEY_BYTES: usize = 512;
pub const MAX_KEY_SIGNATURE_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; SIGNATURE_BYTES]);

impl Encode for MessageSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

impl Decode for MessageSignature {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        read_array(input).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackedSignature {
    Id(i32),
    Full(Box<MessageSignature>),
}

impl Encode for PackedSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Id(id) => write_var(id.wrapping_add(1), out),
            Self::Full(signature) => {
                write_var(0, out);
                signature.encode(out);
            }
        }
    }
}

impl Decode for PackedSignature {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        match read_var(input)?.wrapping_sub(1) {
            -1 => MessageSignature::decode(input).map(|signature| Self::Full(Box::new(signature))),
            id => Ok(Self::Id(id)),
        }
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SignedMessageBody {
        pub content: String as Utf<MAX_CHAT_CHARS>,
        pub timestamp: i64,
        pub salt: i64,
        pub last_seen: Vec<PackedSignature> as List<Plain, MAX_LAST_SEEN>,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterMask {
    PassThrough,
    FullyFiltered,
    PartiallyFiltered(ByteBitSet),
}

impl Encode for FilterMask {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::PassThrough => write_var(0, out),
            Self::FullyFiltered => write_var(1, out),
            Self::PartiallyFiltered(mask) => {
                write_var(2, out);
                mask.encode(out);
            }
        }
    }
}

impl Decode for FilterMask {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(match read_var(input)? {
            1 => Self::FullyFiltered,
            2 => Self::PartiallyFiltered(ByteBitSet::decode(input)?),
            _ => Self::PassThrough,
        })
    }
}

wire_enum! {
    pub enum ChatTypeParameter: Zero { Sender, Target, Content }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct ChatTypeDecoration {
        pub translation_key: String,
        pub parameters: Vec<ChatTypeParameter> as List<Plain, UNBOUNDED>,
        pub style: NetworkNbt,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct ChatType {
        pub chat: ChatTypeDecoration,
        pub narration: ChatTypeDecoration,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct ChatTypeBound {
        pub chat_type: Holder<ChatType>,
        pub name: Text,
        pub target_name: Option<Text> as Opt<Plain>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChatSession {
        pub session_id: Uuid,
        pub expires_at: i64,
        pub public_key: Vec<u8> as Bytes<MAX_PUBLIC_KEY_BYTES>,
        pub key_signature: Vec<u8> as Bytes<MAX_KEY_SIGNATURE_BYTES>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LastSeenBits(pub [u8; LAST_SEEN_BITSET_BYTES]);

impl LastSeenBits {
    pub const fn get(&self, index: usize) -> bool {
        index < MAX_LAST_SEEN && self.0[index / 8] & (1 << (index % 8)) != 0
    }
}

impl Encode for LastSeenBits {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}

impl Decode for LastSeenBits {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        read_array(input).map(Self)
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LastSeenUpdate {
        pub offset: i32 as Var,
        pub acknowledged: LastSeenBits,
        pub checksum: i8,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ArgumentSignature {
        pub name: String as Utf<MAX_ARGUMENT_NAME_CHARS>,
        pub signature: MessageSignature,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SystemChat = id::SYSTEM_CHAT {
        pub content: Text,
        pub overlay: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SetTitleText = id::SET_TITLE_TEXT {
        pub text: Text,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SetSubtitleText = id::SET_SUBTITLE_TEXT {
        pub text: Text,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetTitlesAnimation = id::SET_TITLES_ANIMATION {
        pub fade_in: i32,
        pub stay: i32,
        pub fade_out: i32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ClearTitles = id::CLEAR_TITLES {
        pub reset_times: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct DisguisedChat = id::DISGUISED_CHAT {
        pub message: Text,
        pub chat_type: ChatTypeBound,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct PlayerChat = id::PLAYER_CHAT {
        pub global_index: i32 as Var,
        pub sender: Uuid,
        pub index: i32 as Var,
        pub signature: Option<MessageSignature> as Opt<Plain>,
        pub body: SignedMessageBody,
        pub unsigned_content: Option<Text> as Opt<Plain>,
        pub filter_mask: FilterMask,
        pub chat_type: ChatTypeBound,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DeleteChat = id::DELETE_CHAT {
        pub signature: PackedSignature,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::{Compound, Tag};

    use super::*;
    use crate::codec::decode_exact;
    use crate::play::wire::testing::{encoded, roundtrip};

    fn text(s: &str) -> Text {
        Text(Tag::String(s.into()))
    }

    fn bound() -> ChatTypeBound {
        ChatTypeBound {
            chat_type: Holder::Reference(0),
            name: text("Steve"),
            target_name: None,
        }
    }

    #[test]
    fn system_chat_golden() {
        let packet = SystemChat {
            content: text("hi"),
            overlay: true,
        };
        assert_eq!(roundtrip(&packet), vec![0x08, 0, 2, b'h', b'i', 1]);
    }

    #[test]
    fn disguised_chat_uses_holder_offset() {
        let packet = DisguisedChat {
            message: text("m"),
            chat_type: bound(),
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..5], &[0x08, 0, 1, b'm', 1]);
        assert_eq!(*bytes.last().unwrap(), 0);
        let direct = ChatTypeBound {
            chat_type: Holder::Direct(ChatType {
                chat: ChatTypeDecoration {
                    translation_key: "chat.type.text".into(),
                    parameters: vec![ChatTypeParameter::Sender, ChatTypeParameter::Content],
                    style: NetworkNbt(Some(Tag::Compound(Compound::new()))),
                },
                narration: ChatTypeDecoration {
                    translation_key: "chat.type.text.narrate".into(),
                    parameters: vec![ChatTypeParameter::Content],
                    style: NetworkNbt(Some(Tag::Compound(Compound::new()))),
                },
            }),
            name: text("n"),
            target_name: Some(text("t")),
        };
        let bytes = roundtrip(&direct);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 14);
    }

    #[test]
    fn player_chat_field_order() {
        let packet = PlayerChat {
            global_index: 7,
            sender: Uuid::from_u128(1),
            index: 2,
            signature: Some(MessageSignature([0xAB; SIGNATURE_BYTES])),
            body: SignedMessageBody {
                content: "hello".into(),
                timestamp: 1_700_000_000_000,
                salt: -9,
                last_seen: vec![
                    PackedSignature::Id(3),
                    PackedSignature::Full(Box::new(MessageSignature([1; SIGNATURE_BYTES]))),
                ],
            },
            unsigned_content: None,
            filter_mask: FilterMask::PartiallyFiltered(ByteBitSet::from_bits([1])),
            chat_type: bound(),
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes[0], 7);
        assert_eq!(&bytes[1..17], &Uuid::from_u128(1).into_bytes());
        assert_eq!(&bytes[17..19], &[2, 1]);
        assert_eq!(bytes[19], 0xAB);
        let body = 19 + SIGNATURE_BYTES;
        assert_eq!(&bytes[body..body + 6], &[5, b'h', b'e', b'l', b'l', b'o']);
        let last_seen = body + 6 + 16;
        assert_eq!(&bytes[last_seen..last_seen + 3], &[2, 4, 0]);
        let tail = last_seen + 3 + SIGNATURE_BYTES;
        assert_eq!(&bytes[tail..tail + 4], &[0, 2, 1, 0x02]);
    }

    #[test]
    fn last_seen_update_is_three_fixed_bytes() {
        let update = LastSeenUpdate {
            offset: 1,
            acknowledged: LastSeenBits([0x01, 0x00, 0x08]),
            checksum: -1,
        };
        assert_eq!(roundtrip(&update), vec![1, 0x01, 0x00, 0x08, 0xFF]);
        assert!(update.acknowledged.get(0));
        assert!(update.acknowledged.get(19));
        assert!(!update.acknowledged.get(20));
    }

    #[test]
    fn chat_session_bounds() {
        let session = ChatSession {
            session_id: Uuid::from_u128(5),
            expires_at: 10,
            public_key: vec![1, 2, 3],
            key_signature: vec![4],
        };
        let bytes = roundtrip(&session);
        assert_eq!(&bytes[16..], &[0, 0, 0, 0, 0, 0, 0, 10, 3, 1, 2, 3, 1, 4]);
        let mut oversized = encoded(&ChatSession {
            public_key: vec![0; MAX_PUBLIC_KEY_BYTES + 1],
            ..session
        });
        oversized.truncate(oversized.len() - 2);
        assert!(decode_exact::<ChatSession>(&oversized).is_err());
    }

    #[test]
    fn delete_chat_golden() {
        let by_id = DeleteChat {
            signature: PackedSignature::Id(4),
        };
        assert_eq!(roundtrip(&by_id), vec![5]);
        let full = DeleteChat {
            signature: PackedSignature::Full(Box::new(MessageSignature([7; SIGNATURE_BYTES]))),
        };
        let bytes = roundtrip(&full);
        assert_eq!(&bytes[..2], &[0, 7]);
        assert_eq!(bytes.len(), 1 + SIGNATURE_BYTES);
    }

    #[test]
    fn player_chat_golden_minimal() {
        let packet = PlayerChat {
            global_index: 0,
            sender: Uuid::from_u128(1),
            index: 0,
            signature: None,
            body: SignedMessageBody {
                content: "a".into(),
                timestamp: 1,
                salt: 2,
                last_seen: Vec::new(),
            },
            unsigned_content: Some(text("b")),
            filter_mask: FilterMask::PassThrough,
            chat_type: bound(),
        };
        let mut expected = vec![0];
        expected.extend(Uuid::from_u128(1).into_bytes());
        expected.extend([0, 0, 1, b'a']);
        expected.extend(1i64.to_be_bytes());
        expected.extend(2i64.to_be_bytes());
        expected.extend([0, 1, 8, 0, 1, b'b', 0, 1, 8, 0, 5]);
        assert_eq!(&roundtrip(&packet)[..expected.len()], &expected[..]);
    }

    #[test]
    fn filter_mask_ids() {
        assert_eq!(roundtrip(&FilterMask::PassThrough), vec![0]);
        assert_eq!(roundtrip(&FilterMask::FullyFiltered), vec![1]);
    }
}
