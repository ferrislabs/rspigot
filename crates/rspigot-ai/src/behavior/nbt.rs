use rspigot_domain::BlockPos;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::behavior::memory::{Codec, ExpirableValue, GlobalPos, MemoryModuleType, MemoryValue};

pub const MEMORIES_KEY: &str = "memories";
pub const VALUE_KEY: &str = "value";
pub const TTL_KEY: &str = "ttl";
const DIMENSION_KEY: &str = "dimension";
const POS_KEY: &str = "pos";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackedMemories {
    pub entries: Vec<(MemoryModuleType, ExpirableValue)>,
}

impl PackedMemories {
    pub fn to_nbt(&self) -> Compound {
        let mut memories = Compound::new();
        for (ty, expirable) in &self.entries {
            let Some(value) = encode_value(ty.codec(), &expirable.value) else {
                continue;
            };
            let mut entry = Compound::new().with(VALUE_KEY, value);
            if let Some(time_to_live) = expirable.time_to_live {
                entry.insert(TTL_KEY, time_to_live);
            }
            memories.insert(ty.name(), entry);
        }
        Compound::new().with(MEMORIES_KEY, memories)
    }

    pub fn from_nbt(packed: &Compound) -> Self {
        let mut entries = Vec::new();
        let Some(memories) = packed.get_compound(MEMORIES_KEY) else {
            return Self { entries };
        };
        for (name, tag) in memories.iter() {
            let Some(ty) = MemoryModuleType::from_name(name) else {
                continue;
            };
            if !ty.can_serialize() {
                continue;
            }
            let Tag::Compound(entry) = tag else {
                continue;
            };
            let Some(value) = entry
                .get(VALUE_KEY)
                .and_then(|value| decode_value(ty.codec(), value))
            else {
                continue;
            };
            let time_to_live = entry.get_i64(TTL_KEY);
            entries.push((
                ty,
                ExpirableValue {
                    value,
                    time_to_live,
                },
            ));
        }
        Self { entries }
    }
}

fn encode_block(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x, pos.y, pos.z])
}

fn decode_block(tag: &Tag) -> Option<BlockPos> {
    match tag {
        Tag::IntArray(values) => match values.as_slice() {
            [x, y, z] => Some(BlockPos::new(*x, *y, *z)),
            _ => None,
        },
        _ => None,
    }
}

fn encode_uuid(uuid: Uuid) -> Tag {
    let value = uuid.as_u128();
    Tag::IntArray(vec![
        (value >> 96) as i32,
        (value >> 64) as i32,
        (value >> 32) as i32,
        value as i32,
    ])
}

fn decode_uuid(tag: &Tag) -> Option<Uuid> {
    let Tag::IntArray(values) = tag else {
        return None;
    };
    let [a, b, c, d] = values.as_slice() else {
        return None;
    };
    let part = |value: &i32| u128::from(*value as u32);
    Some(Uuid::from_u128(
        part(a) << 96 | part(b) << 64 | part(c) << 32 | part(d),
    ))
}

fn encode_global_pos(pos: &GlobalPos) -> Tag {
    Tag::Compound(
        Compound::new()
            .with(DIMENSION_KEY, pos.dimension.as_str())
            .with(POS_KEY, encode_block(pos.pos)),
    )
}

fn decode_global_pos(tag: &Tag) -> Option<GlobalPos> {
    let Tag::Compound(compound) = tag else {
        return None;
    };
    Some(GlobalPos {
        dimension: compound.get_str(DIMENSION_KEY)?.to_owned(),
        pos: decode_block(compound.get(POS_KEY)?)?,
    })
}

pub fn encode_value(codec: Codec, value: &MemoryValue) -> Option<Tag> {
    Some(match (codec, value) {
        (Codec::GlobalPos, MemoryValue::GlobalPos(pos)) => encode_global_pos(pos),
        (Codec::GlobalPosList, MemoryValue::GlobalPosList(positions)) => {
            Tag::List(positions.iter().map(encode_global_pos).collect())
        }
        (Codec::Bool, MemoryValue::Bool(value)) => Tag::Byte(i8::from(*value)),
        (Codec::Long, MemoryValue::Long(value)) => Tag::Long(*value),
        (Codec::Int, MemoryValue::Int(value)) => Tag::Int(*value),
        (Codec::Unit, MemoryValue::Unit) => Tag::Compound(Compound::new()),
        (Codec::Uuid, MemoryValue::Uuid(uuid)) => encode_uuid(*uuid),
        (Codec::Block, MemoryValue::Block(pos)) => encode_block(*pos),
        _ => return None,
    })
}

pub fn decode_value(codec: Codec, tag: &Tag) -> Option<MemoryValue> {
    Some(match codec {
        Codec::Transient => return None,
        Codec::GlobalPos => MemoryValue::GlobalPos(decode_global_pos(tag)?),
        Codec::GlobalPosList => {
            let Tag::List(entries) = tag else {
                return None;
            };
            MemoryValue::GlobalPosList(
                entries
                    .iter()
                    .map(decode_global_pos)
                    .collect::<Option<Vec<_>>>()?,
            )
        }
        Codec::Bool => MemoryValue::Bool(tag.as_i64()? != 0),
        Codec::Long => MemoryValue::Long(tag.as_i64()?),
        Codec::Int => MemoryValue::Int(tag.as_i64()? as i32),
        Codec::Unit => MemoryValue::Unit,
        Codec::Uuid => MemoryValue::Uuid(decode_uuid(tag)?),
        Codec::Block => MemoryValue::Block(decode_block(tag)?),
    })
}
