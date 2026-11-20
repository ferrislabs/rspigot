use crate::codec::Position;
use crate::common::Identifier;
use crate::ids::play::clientbound as id;
use crate::ids::play::serverbound as sb;
use crate::play::types::{Hand, Holder, SoundEvent, SoundSource};
use crate::play::wire::{
    Bytes, List, Opt, Plain, UNBOUNDED, Utf, Var, VarL, unit_packet, wire_enum, wire_struct,
};

pub const MAX_JIGSAW_STATE_CHARS: usize = 32_767;
pub const MAX_STRUCTURE_NAME_CHARS: usize = 32_767;
pub const MAX_STRUCTURE_DATA_CHARS: usize = 128;
pub const MAX_TEST_BLOCK_MESSAGE_CHARS: usize = 32_767;
pub const MAX_DEBUG_SUBSCRIPTIONS: usize = 32;
pub const MAX_BIOME_BUFFER_BYTES: usize = 2 * 1024 * 1024;

wire_enum! {
    pub enum EntityAnchor: Strict { Feet, Eyes }
}

wire_enum! {
    pub enum StructureUpdateType: Strict { UpdateData, SaveArea, LoadArea, ScanArea }
}

wire_enum! {
    pub enum StructureMode: Strict { Save, Load, Corner, Data }
}

wire_enum! {
    pub enum StructureMirror: Strict { None, LeftRight, FrontBack }
}

wire_enum! {
    pub enum StructureRotation: Strict { None, Clockwise90, Clockwise180, Counterclockwise90 }
}

wire_enum! {
    pub enum TestBlockMode: Strict { Start, Log, Fail, Accept }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetBorderCenter = id::SET_BORDER_CENTER {
        pub new_center_x: f64,
        pub new_center_z: f64,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetBorderSize = id::SET_BORDER_SIZE {
        pub size: f64,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetBorderLerpSize = id::SET_BORDER_LERP_SIZE {
        pub old_size: f64,
        pub new_size: f64,
        pub lerp_time: i64 as VarL,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetBorderWarningDelay = id::SET_BORDER_WARNING_DELAY {
        pub warning_delay: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetBorderWarningDistance = id::SET_BORDER_WARNING_DISTANCE {
        pub warning_blocks: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenBook = id::OPEN_BOOK {
        pub hand: Hand,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LookAtEntity {
        pub entity_id: i32 as Var,
        pub to_anchor: EntityAnchor,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerLookAt = id::PLAYER_LOOK_AT {
        pub from_anchor: EntityAnchor,
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub at_entity: Option<LookAtEntity> as Opt<Plain>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SoundEntity = id::SOUND_ENTITY {
        pub sound: Holder<SoundEvent>,
        pub source: SoundSource,
        pub entity_id: i32 as Var,
        pub volume: f32,
        pub pitch: f32,
        pub seed: i64,
    }
}

unit_packet! {
    pub struct PlayerCombatEnter = id::PLAYER_COMBAT_ENTER;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PlayerCombatEnd = id::PLAYER_COMBAT_END {
        pub duration: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChunkBiomeData {
        pub x: i32,
        pub z: i32,
        pub buffer: Vec<u8> as Bytes<MAX_BIOME_BUFFER_BYTES>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChunksBiomes = id::CHUNKS_BIOMES {
        pub chunks: Vec<ChunkBiomeData> as List<Plain, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SelectBundleItem = sb::BUNDLE_ITEM_SELECTED {
        pub slot_id: i32 as Var,
        pub selected_item_index: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct JigsawGenerate = sb::JIGSAW_GENERATE {
        pub pos: Position,
        pub levels: i32 as Var,
        pub keep_jigsaws: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetJigsawBlock = sb::SET_JIGSAW_BLOCK {
        pub pos: Position,
        pub name: Identifier,
        pub target: Identifier,
        pub pool: Identifier,
        pub final_state: String as Utf<MAX_JIGSAW_STATE_CHARS>,
        pub joint: String as Utf<MAX_STRUCTURE_DATA_CHARS>,
        pub selection_priority: i32 as Var,
        pub placement_priority: i32 as Var,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetStructureBlock {
    pub pos: Position,
    pub update_type: StructureUpdateType,
    pub mode: StructureMode,
    pub name: String,
    pub offset: [i8; 3],
    pub size: [i8; 3],
    pub mirror: StructureMirror,
    pub rotation: StructureRotation,
    pub data: String,
    pub integrity: f32,
    pub seed: i64,
    pub ignore_entities: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub strict: bool,
}

impl SetStructureBlock {
    const FLAG_IGNORE_ENTITIES: u8 = 1;
    const FLAG_SHOW_AIR: u8 = 2;
    const FLAG_SHOW_BOUNDING_BOX: u8 = 4;
    const FLAG_STRICT: u8 = 8;
}

impl crate::codec::Encode for SetStructureBlock {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.update_type.encode(out);
        self.mode.encode(out);
        self.name.encode(out);
        for value in self.offset.iter().chain(self.size.iter()) {
            value.encode(out);
        }
        self.mirror.encode(out);
        self.rotation.encode(out);
        self.data.encode(out);
        self.integrity.encode(out);
        crate::codec::VarLong(self.seed).encode(out);
        let mut flags = 0;
        if self.ignore_entities {
            flags |= Self::FLAG_IGNORE_ENTITIES;
        }
        if self.show_air {
            flags |= Self::FLAG_SHOW_AIR;
        }
        if self.show_bounding_box {
            flags |= Self::FLAG_SHOW_BOUNDING_BOX;
        }
        if self.strict {
            flags |= Self::FLAG_STRICT;
        }
        out.push(flags);
    }
}

impl crate::codec::Decode for SetStructureBlock {
    fn decode(input: &mut &[u8]) -> Result<Self, crate::codec::DecodeError> {
        let pos = Position::decode(input)?;
        let update_type = StructureUpdateType::decode(input)?;
        let mode = StructureMode::decode(input)?;
        let name = crate::codec::decode_string(input, MAX_STRUCTURE_NAME_CHARS)?;
        let mut offset = [0i8; 3];
        let mut size = [0i8; 3];
        for value in offset.iter_mut().chain(size.iter_mut()) {
            *value = i8::decode(input)?;
        }
        let mirror = StructureMirror::decode(input)?;
        let rotation = StructureRotation::decode(input)?;
        let data = crate::codec::decode_string(input, MAX_STRUCTURE_DATA_CHARS)?;
        let integrity = f32::decode(input)?;
        let seed = crate::codec::VarLong::decode(input)?.0;
        let flags = u8::decode(input)?;
        Ok(Self {
            pos,
            update_type,
            mode,
            name,
            offset,
            size,
            mirror,
            rotation,
            data,
            integrity,
            seed,
            ignore_entities: flags & Self::FLAG_IGNORE_ENTITIES != 0,
            show_air: flags & Self::FLAG_SHOW_AIR != 0,
            show_bounding_box: flags & Self::FLAG_SHOW_BOUNDING_BOX != 0,
            strict: flags & Self::FLAG_STRICT != 0,
        })
    }
}

impl crate::Packet for SetStructureBlock {
    const ID: i32 = sb::SET_STRUCTURE_BLOCK;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetTestBlock = sb::SET_TEST_BLOCK {
        pub pos: Position,
        pub mode: TestBlockMode,
        pub message: String as Utf<MAX_TEST_BLOCK_MESSAGE_CHARS>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DebugSubscriptionRequest = sb::DEBUG_SUBSCRIPTION_REQUEST {
        pub subscriptions: Vec<i32> as List<Var, MAX_DEBUG_SUBSCRIPTIONS>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Packet;
    use crate::codec::Encode;
    use crate::play::wire::testing::{encoded, roundtrip};

    #[test]
    fn border_packets_match_vanilla_layout() {
        let center = SetBorderCenter {
            new_center_x: 1.0,
            new_center_z: -2.0,
        };
        let mut expected = 1.0f64.to_be_bytes().to_vec();
        expected.extend_from_slice(&(-2.0f64).to_be_bytes());
        assert_eq!(roundtrip(&center), expected);
        assert_eq!(<SetBorderCenter as Packet>::ID, 90);
        assert_eq!(
            roundtrip(&SetBorderSize { size: 4.0 }),
            4.0f64.to_be_bytes().to_vec()
        );
        assert_eq!(<SetBorderSize as Packet>::ID, 92);
        let lerp = SetBorderLerpSize {
            old_size: 10.0,
            new_size: 20.0,
            lerp_time: 300,
        };
        let mut expected = 10.0f64.to_be_bytes().to_vec();
        expected.extend_from_slice(&20.0f64.to_be_bytes());
        expected.extend_from_slice(&[0xAC, 0x02]);
        assert_eq!(roundtrip(&lerp), expected);
        assert_eq!(<SetBorderLerpSize as Packet>::ID, 91);
        assert_eq!(
            roundtrip(&SetBorderWarningDelay { warning_delay: 300 }),
            vec![0xAC, 0x02]
        );
        assert_eq!(<SetBorderWarningDelay as Packet>::ID, 93);
        assert_eq!(
            roundtrip(&SetBorderWarningDistance { warning_blocks: 5 }),
            vec![5]
        );
        assert_eq!(<SetBorderWarningDistance as Packet>::ID, 94);
    }

    #[test]
    fn open_book_writes_the_hand() {
        assert_eq!(
            roundtrip(&OpenBook {
                hand: Hand::OffHand
            }),
            vec![1]
        );
        assert_eq!(<OpenBook as Packet>::ID, 59);
    }

    #[test]
    fn look_at_a_position_and_at_an_entity() {
        let at_position = PlayerLookAt {
            from_anchor: EntityAnchor::Eyes,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            at_entity: None,
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&1.0f64.to_be_bytes());
        expected.extend_from_slice(&2.0f64.to_be_bytes());
        expected.extend_from_slice(&3.0f64.to_be_bytes());
        expected.push(0);
        assert_eq!(roundtrip(&at_position), expected);
        let at_entity = PlayerLookAt {
            at_entity: Some(LookAtEntity {
                entity_id: 300,
                to_anchor: EntityAnchor::Feet,
            }),
            ..at_position
        };
        let mut expected_entity = expected[..expected.len() - 1].to_vec();
        expected_entity.extend_from_slice(&[1, 0xAC, 0x02, 0]);
        assert_eq!(roundtrip(&at_entity), expected_entity);
        assert_eq!(<PlayerLookAt as Packet>::ID, 72);
    }

    #[test]
    fn sound_entity_layout() {
        let packet = SoundEntity {
            sound: Holder::Reference(5),
            source: SoundSource::Players,
            entity_id: 9,
            volume: 1.0,
            pitch: 0.5,
            seed: 7,
        };
        let mut expected = vec![6, 7, 9];
        expected.extend_from_slice(&1.0f32.to_be_bytes());
        expected.extend_from_slice(&0.5f32.to_be_bytes());
        expected.extend_from_slice(&7i64.to_be_bytes());
        assert_eq!(roundtrip(&packet), expected);
        assert_eq!(<SoundEntity as Packet>::ID, 118);
    }

    #[test]
    fn combat_enter_is_empty_and_end_writes_the_duration() {
        assert_eq!(encoded(&PlayerCombatEnter), Vec::<u8>::new());
        assert_eq!(<PlayerCombatEnter as Packet>::ID, 68);
        assert_eq!(
            roundtrip(&PlayerCombatEnd { duration: 300 }),
            vec![0xAC, 0x02]
        );
        assert_eq!(<PlayerCombatEnd as Packet>::ID, 67);
    }

    #[test]
    fn chunks_biomes_layout() {
        let packet = ChunksBiomes {
            chunks: vec![ChunkBiomeData {
                x: -1,
                z: 2,
                buffer: vec![9, 8],
            }],
        };
        let mut expected = vec![1];
        expected.extend_from_slice(&(-1i32).to_be_bytes());
        expected.extend_from_slice(&2i32.to_be_bytes());
        expected.extend_from_slice(&[2, 9, 8]);
        assert_eq!(roundtrip(&packet), expected);
        assert_eq!(<ChunksBiomes as Packet>::ID, 13);
    }

    #[test]
    fn bundle_selection_and_jigsaw_generate_layouts() {
        assert_eq!(
            roundtrip(&SelectBundleItem {
                slot_id: 36,
                selected_item_index: 2
            }),
            vec![36, 2]
        );
        assert_eq!(<SelectBundleItem as Packet>::ID, 3);
        let generate = JigsawGenerate {
            pos: Position { x: 1, y: 64, z: -2 },
            levels: 7,
            keep_jigsaws: true,
        };
        let mut expected = Vec::new();
        generate.pos.encode(&mut expected);
        expected.extend_from_slice(&[7, 1]);
        assert_eq!(roundtrip(&generate), expected);
        assert_eq!(<JigsawGenerate as Packet>::ID, 27);
    }

    #[test]
    fn jigsaw_block_layout() {
        let packet = SetJigsawBlock {
            pos: Position { x: 0, y: 0, z: 0 },
            name: Identifier::parse("a").unwrap(),
            target: Identifier::parse("b").unwrap(),
            pool: Identifier::parse("c").unwrap(),
            final_state: "air".to_owned(),
            joint: "rollable".to_owned(),
            selection_priority: 1,
            placement_priority: 2,
        };
        let bytes = roundtrip(&packet);
        assert!(bytes.ends_with(&[8, b'r', b'o', b'l', b'l', b'a', b'b', b'l', b'e', 1, 2]));
        assert_eq!(<SetJigsawBlock as Packet>::ID, 59);
    }

    #[test]
    fn structure_block_layout() {
        let packet = SetStructureBlock {
            pos: Position { x: 1, y: 2, z: 3 },
            update_type: StructureUpdateType::LoadArea,
            mode: StructureMode::Load,
            name: "n".to_owned(),
            offset: [1, -2, 3],
            size: [4, 5, 6],
            mirror: StructureMirror::FrontBack,
            rotation: StructureRotation::Clockwise180,
            data: String::new(),
            integrity: 0.5,
            seed: 3,
            ignore_entities: true,
            show_air: false,
            show_bounding_box: true,
            strict: true,
        };
        let bytes = roundtrip(&packet);
        let mut tail = vec![2, 1, 1, b'n', 1, 0xFE, 3, 4, 5, 6, 2, 2, 0];
        tail.extend_from_slice(&0.5f32.to_be_bytes());
        tail.extend_from_slice(&[3, 0b1101]);
        assert!(bytes.ends_with(&tail));
        assert_eq!(<SetStructureBlock as Packet>::ID, 60);
    }

    #[test]
    fn test_block_and_debug_subscription_layouts() {
        let packet = SetTestBlock {
            pos: Position { x: 0, y: 1, z: 0 },
            mode: TestBlockMode::Accept,
            message: "m".to_owned(),
        };
        let bytes = roundtrip(&packet);
        assert!(bytes.ends_with(&[3, 1, b'm']));
        assert_eq!(<SetTestBlock as Packet>::ID, 61);
        assert_eq!(
            roundtrip(&DebugSubscriptionRequest {
                subscriptions: vec![1, 2]
            }),
            vec![2, 1, 2]
        );
        assert_eq!(<DebugSubscriptionRequest as Packet>::ID, 23);
    }
}
