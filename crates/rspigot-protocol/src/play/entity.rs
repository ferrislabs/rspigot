use uuid::Uuid;

use crate::Packet;
use crate::codec::{Angle, Decode, DecodeError, Encode, MAX_LIST_LEN, decode_len};
use crate::ids::play::clientbound as id;
use crate::play::slot::ItemStack;
use crate::play::types::{Hand, LpVec3, PositionMoveRotation, Relatives, Vec3};
use crate::play::wire::{List, Plain, UNBOUNDED, Var, read_var, wire_enum, wire_struct, write_var};

pub const MAX_ATTRIBUTES: usize = 128;
pub const DELTA_STEP_MIN_BYTES: usize = 7;
pub const DELTA_UNITS_PER_BLOCK: f64 = 4096.0;

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct AddEntity = id::ADD_ENTITY {
        pub entity_id: i32 as Var,
        pub uuid: Uuid,
        pub kind: i32 as Var,
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub movement: LpVec3,
        pub x_rot: Angle,
        pub y_rot: Angle,
        pub y_head_rot: Angle,
        pub data: i32 as Var,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaStep {
    pub ticks: i32,
    pub xa: i16,
    pub ya: i16,
    pub za: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VecDelta {
    Linear { xa: i16, ya: i16, za: i16 },
    Stepped(Vec<DeltaStep>),
}

impl VecDelta {
    pub const ZERO: Self = Self::Linear {
        xa: 0,
        ya: 0,
        za: 0,
    };

    pub fn encode_component(delta_blocks: f64) -> i64 {
        (delta_blocks * DELTA_UNITS_PER_BLOCK).round() as i64
    }

    pub fn step_count(&self) -> usize {
        match self {
            Self::Linear { .. } => 0,
            Self::Stepped(steps) => steps.len(),
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Linear { xa, ya, za } => {
                xa.encode(out);
                ya.encode(out);
                za.encode(out);
            }
            Self::Stepped(steps) => {
                for step in steps {
                    write_var(step.ticks, out);
                    step.xa.encode(out);
                    step.ya.encode(out);
                    step.za.encode(out);
                }
            }
        }
    }

    fn read(input: &mut &[u8], step_count: usize) -> Result<Self, DecodeError> {
        if step_count == 0 {
            return Ok(Self::Linear {
                xa: i16::decode(input)?,
                ya: i16::decode(input)?,
                za: i16::decode(input)?,
            });
        }
        let allowed = input.len() / DELTA_STEP_MIN_BYTES;
        if step_count > allowed {
            return Err(DecodeError::TooLong {
                len: step_count,
                max: allowed,
            });
        }
        let mut steps = Vec::with_capacity(step_count);
        for _ in 0..step_count {
            steps.push(DeltaStep {
                ticks: read_var(input)?,
                xa: i16::decode(input)?,
                ya: i16::decode(input)?,
                za: i16::decode(input)?,
            });
        }
        Ok(Self::Stepped(steps))
    }
}

fn write_movement_properties(on_ground: bool, delta: &VecDelta, out: &mut Vec<u8>) {
    write_var(
        i32::from(on_ground) | ((delta.step_count() as i32) << 1),
        out,
    );
}

fn read_movement_properties(input: &mut &[u8]) -> Result<(bool, usize), DecodeError> {
    let packed = read_var(input)?;
    Ok((packed & 1 != 0, (packed as u32 >> 1) as usize))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveEntityPos {
    pub entity_id: i32,
    pub on_ground: bool,
    pub delta: VecDelta,
}

impl Encode for MoveEntityPos {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.entity_id, out);
        write_movement_properties(self.on_ground, &self.delta, out);
        self.delta.write(out);
    }
}

impl Decode for MoveEntityPos {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let entity_id = read_var(input)?;
        let (on_ground, steps) = read_movement_properties(input)?;
        Ok(Self {
            entity_id,
            on_ground,
            delta: VecDelta::read(input, steps)?,
        })
    }
}

impl Packet for MoveEntityPos {
    const ID: i32 = id::MOVE_ENTITY_POS;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveEntityPosRot {
    pub entity_id: i32,
    pub on_ground: bool,
    pub delta: VecDelta,
    pub y_rot: Angle,
    pub x_rot: Angle,
}

impl Encode for MoveEntityPosRot {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.entity_id, out);
        write_movement_properties(self.on_ground, &self.delta, out);
        self.delta.write(out);
        self.y_rot.encode(out);
        self.x_rot.encode(out);
    }
}

impl Decode for MoveEntityPosRot {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let entity_id = read_var(input)?;
        let (on_ground, steps) = read_movement_properties(input)?;
        let delta = VecDelta::read(input, steps)?;
        Ok(Self {
            entity_id,
            on_ground,
            delta,
            y_rot: Angle::decode(input)?,
            x_rot: Angle::decode(input)?,
        })
    }
}

impl Packet for MoveEntityPosRot {
    const ID: i32 = id::MOVE_ENTITY_POS_ROT;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MoveEntityRot = id::MOVE_ENTITY_ROT {
        pub entity_id: i32 as Var,
        pub on_ground: bool,
        pub y_rot: Angle,
        pub x_rot: Angle,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PositionStep {
        pub position: Vec3,
        pub tick_offset: i32 as Var,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionPath {
    Linear(Vec3),
    Stepped(Vec<PositionStep>),
}

impl Encode for PositionPath {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Linear(end) => {
                write_var(0, out);
                end.encode(out);
            }
            Self::Stepped(steps) => {
                write_var(1, out);
                steps.encode(out);
            }
        }
    }
}

impl Decode for PositionPath {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        match read_var(input)? {
            1 => Vec::<PositionStep>::decode(input).map(Self::Stepped),
            _ => Vec3::decode(input).map(Self::Linear),
        }
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct EntityPositionSync = id::ENTITY_POSITION_SYNC {
        pub entity_id: i32 as Var,
        pub position: PositionPath,
        pub y_rot: f32,
        pub x_rot: f32,
        pub on_ground: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct TeleportEntity = id::TELEPORT_ENTITY {
        pub entity_id: i32 as Var,
        pub change: PositionMoveRotation,
        pub relatives: Relatives,
        pub on_ground: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RotateHead = id::ROTATE_HEAD {
        pub entity_id: i32 as Var,
        pub y_head_rot: Angle,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Animate = id::ANIMATE {
        pub entity_id: i32 as Var,
        pub action: u8,
    }
}

impl Animate {
    pub const WAKE_UP: u8 = 0;
    pub const CRITICAL_HIT: u8 = 1;
    pub const MAGIC_CRITICAL_HIT: u8 = 2;
}

wire_enum! {
    pub enum SwingAnimationType: Zero { None, Whack, Stab }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SwingAnimationStyle {
        pub kind: SwingAnimationType,
        pub duration: i32 as Var,
    }
}

impl SwingAnimationStyle {
    pub const DEFAULT: Self = Self {
        kind: SwingAnimationType::Whack,
        duration: 6,
    };
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SwingAnimation = id::SWING_ANIMATION {
        pub entity_id: i32 as Var,
        pub hand: Hand,
        pub animation: SwingAnimationStyle,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetEntityMotion = id::SET_ENTITY_MOTION {
        pub entity_id: i32 as Var,
        pub movement: LpVec3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

impl EquipmentSlot {
    pub const ALL: [Self; 8] = [
        Self::MainHand,
        Self::OffHand,
        Self::Feet,
        Self::Legs,
        Self::Chest,
        Self::Head,
        Self::Body,
        Self::Saddle,
    ];

    pub const fn ordinal(self) -> u8 {
        self as u8
    }
}

const CONTINUATION_BIT: u8 = 0x80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetEquipment {
    pub entity_id: i32,
    pub slots: Vec<(EquipmentSlot, ItemStack)>,
}

impl Encode for SetEquipment {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.entity_id, out);
        let last = self.slots.len().saturating_sub(1);
        for (index, (slot, item)) in self.slots.iter().enumerate() {
            let continued = if index == last { 0 } else { CONTINUATION_BIT };
            out.push(slot.ordinal() | continued);
            item.encode(out);
        }
    }
}

impl Decode for SetEquipment {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let entity_id = read_var(input)?;
        let mut slots = Vec::new();
        loop {
            let marker = u8::decode(input)?;
            let ordinal = marker & !CONTINUATION_BIT;
            let slot = EquipmentSlot::ALL
                .get(usize::from(ordinal))
                .copied()
                .ok_or(DecodeError::InvalidValue {
                    field: "equipment slot",
                    value: i64::from(ordinal),
                })?;
            slots.push((slot, ItemStack::decode(input)?));
            if marker & CONTINUATION_BIT == 0 {
                break;
            }
        }
        Ok(Self { entity_id, slots })
    }
}

impl Packet for SetEquipment {
    const ID: i32 = id::SET_EQUIPMENT;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RemoveEntities = id::REMOVE_ENTITIES {
        pub entity_ids: Vec<i32> as List<Var, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetPassengers = id::SET_PASSENGERS {
        pub vehicle: i32 as Var,
        pub passengers: Vec<i32> as List<Var, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetEntityLink = id::SET_ENTITY_LINK {
        pub source: i32,
        pub dest: i32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TakeItemEntity = id::TAKE_ITEM_ENTITY {
        pub item_id: i32 as Var,
        pub player_id: i32 as Var,
        pub amount: i32 as Var,
    }
}

wire_enum! {
    pub enum AttributeOperation: Zero { AddValue, AddMultipliedBase, AddMultipliedTotal }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct AttributeModifier {
        pub id: String,
        pub amount: f64,
        pub operation: AttributeOperation,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct AttributeSnapshot {
        pub attribute: i32 as Var,
        pub base: f64,
        pub modifiers: Vec<AttributeModifier> as List<Plain, UNBOUNDED>,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateAttributes {
    pub entity_id: i32,
    pub attributes: Vec<AttributeSnapshot>,
}

impl Encode for UpdateAttributes {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.entity_id, out);
        self.attributes.encode(out);
    }
}

impl Decode for UpdateAttributes {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let entity_id = read_var(input)?;
        let count = decode_len(input, MAX_ATTRIBUTES.min(MAX_LIST_LEN))?;
        let mut attributes = Vec::with_capacity(count);
        for _ in 0..count {
            attributes.push(AttributeSnapshot::decode(input)?);
        }
        Ok(Self {
            entity_id,
            attributes,
        })
    }
}

impl Packet for UpdateAttributes {
    const ID: i32 = id::UPDATE_ATTRIBUTES;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::decode_exact;
    use crate::play::wire::testing::{encoded, roundtrip};

    #[test]
    fn add_entity_layout() {
        let packet = AddEntity {
            entity_id: 5,
            uuid: Uuid::from_u128(1),
            kind: 159,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            movement: LpVec3(Vec3::ZERO),
            x_rot: Angle(0),
            y_rot: Angle(64),
            y_head_rot: Angle(128),
            data: 0,
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes[0], 5);
        assert_eq!(&bytes[17..19], &[0x9F, 0x01]);
        assert_eq!(&bytes[19..27], &1.0f64.to_be_bytes());
        assert_eq!(&bytes[43..], &[0, 0, 64, 128, 0]);
    }

    #[test]
    fn linear_move_packs_on_ground_flag() {
        let packet = MoveEntityPos {
            entity_id: 3,
            on_ground: true,
            delta: VecDelta::Linear {
                xa: 4096,
                ya: -1,
                za: 0,
            },
        };
        assert_eq!(roundtrip(&packet), vec![3, 1, 0x10, 0x00, 0xFF, 0xFF, 0, 0]);
    }

    #[test]
    fn stepped_move_packs_step_count() {
        let packet = MoveEntityPosRot {
            entity_id: 1,
            on_ground: false,
            delta: VecDelta::Stepped(vec![
                DeltaStep {
                    ticks: 1,
                    xa: 1,
                    ya: 2,
                    za: 3,
                },
                DeltaStep {
                    ticks: 2,
                    xa: 0,
                    ya: 0,
                    za: 0,
                },
            ]),
            y_rot: Angle(10),
            x_rot: Angle(20),
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..9], &[1, 4, 1, 0, 1, 0, 2, 0, 3]);
        assert_eq!(&bytes[bytes.len() - 2..], &[10, 20]);
        assert!(decode_exact::<MoveEntityPos>(&[1, 0xFE, 0x01, 0]).is_err());
    }

    #[test]
    fn rotation_only_move_puts_ground_first() {
        let packet = MoveEntityRot {
            entity_id: 9,
            on_ground: true,
            y_rot: Angle(1),
            x_rot: Angle(2),
        };
        assert_eq!(roundtrip(&packet), vec![9, 1, 1, 2]);
    }

    #[test]
    fn position_sync_and_teleport() {
        let sync = EntityPositionSync {
            entity_id: 2,
            position: PositionPath::Linear(Vec3::new(1.0, 2.0, 3.0)),
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: true,
        };
        let bytes = roundtrip(&sync);
        assert_eq!(&bytes[..2], &[2, 0]);
        assert_eq!(bytes.len(), 2 + 24 + 8 + 1);
        let stepped = EntityPositionSync {
            position: PositionPath::Stepped(vec![PositionStep {
                position: Vec3::ZERO,
                tick_offset: 3,
            }]),
            ..sync
        };
        let bytes = roundtrip(&stepped);
        assert_eq!(&bytes[..3], &[2, 1, 1]);
        assert_eq!(bytes[27], 3);
        let teleport = TeleportEntity {
            entity_id: 2,
            change: PositionMoveRotation::default(),
            relatives: Relatives::NONE,
            on_ground: false,
        };
        assert_eq!(roundtrip(&teleport).len(), 1 + 56 + 4 + 1);
    }

    #[test]
    fn equipment_continuation_bits() {
        let packet = SetEquipment {
            entity_id: 4,
            slots: vec![
                (EquipmentSlot::MainHand, ItemStack::new(1, 1)),
                (EquipmentSlot::Head, ItemStack::EMPTY),
            ],
        };
        assert_eq!(roundtrip(&packet), vec![4, 0x80, 1, 1, 0, 0, 0x05, 0]);
        assert!(decode_exact::<SetEquipment>(&[4, 0x09, 0]).is_err());
    }

    #[test]
    fn simple_entity_packets() {
        assert_eq!(
            roundtrip(&RotateHead {
                entity_id: 1,
                y_head_rot: Angle(200)
            }),
            vec![1, 200]
        );
        assert_eq!(
            roundtrip(&Animate {
                entity_id: 1,
                action: Animate::CRITICAL_HIT
            }),
            vec![1, 1]
        );
        assert_eq!(
            roundtrip(&SwingAnimation {
                entity_id: 1,
                hand: Hand::OffHand,
                animation: SwingAnimationStyle::DEFAULT
            }),
            vec![1, 1, 1, 6]
        );
        assert_eq!(
            encoded(&SetEntityMotion {
                entity_id: 1,
                movement: LpVec3(Vec3::new(1.0, 0.0, 0.0))
            }),
            vec![1, 0xF1, 0xFF, 0x7F, 0xFE, 0xFF, 0xFF]
        );
        assert_eq!(
            roundtrip(&RemoveEntities {
                entity_ids: vec![1, 300]
            }),
            vec![2, 1, 0xAC, 0x02]
        );
        assert_eq!(
            roundtrip(&SetPassengers {
                vehicle: 1,
                passengers: vec![300]
            }),
            vec![1, 1, 0xAC, 0x02]
        );
        assert_eq!(
            roundtrip(&SetEntityLink { source: 5, dest: 0 }),
            vec![0, 0, 0, 5, 0, 0, 0, 0]
        );
        assert_eq!(
            roundtrip(&TakeItemEntity {
                item_id: 1,
                player_id: 2,
                amount: 3
            }),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn attributes_layout() {
        let packet = UpdateAttributes {
            entity_id: 1,
            attributes: vec![AttributeSnapshot {
                attribute: 17,
                base: 0.1,
                modifiers: vec![AttributeModifier {
                    id: "minecraft:sprinting".into(),
                    amount: 0.3,
                    operation: AttributeOperation::AddMultipliedTotal,
                }],
            }],
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..3], &[1, 1, 17]);
        assert_eq!(&bytes[3..11], &0.1f64.to_be_bytes());
        assert_eq!(bytes[11], 1);
        assert_eq!(*bytes.last().unwrap(), 2);
        let mut too_many = vec![1];
        crate::codec::encode_len(MAX_ATTRIBUTES + 1, &mut too_many);
        assert!(decode_exact::<UpdateAttributes>(&too_many).is_err());
    }
}
