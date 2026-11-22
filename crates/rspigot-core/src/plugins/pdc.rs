use std::collections::BTreeMap;
use std::mem::discriminant;

use rspigot_domain::item::menu_slot;
use rspigot_domain::meta::{self, MAX_BYTES, MAX_DEPTH, MAX_KEYS, MetaValue};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_nbt::{Compound, Tag, read_network_compound, write_network};

use crate::plugins::{ActionError, ActionErrorKind};
use crate::ports::ClientLink;
use crate::server::Server;

const HOLDER_KEY: &str = "rspigot:pdc";
const CUSTOM_DATA: &str = "minecraft:custom_data";
const ITEM_KEY: &str = "PublicBukkitValues";
const MAX_NESTED_KEY: usize = meta::MAX_KEY_LENGTH;
const SPIGOT_SLOTS: usize = 41;

#[derive(Debug, Clone, PartialEq)]
pub enum PdcValue {
    Text(String),
    Byte(i8),
    Int(i32),
    Long(i64),
    Double(f64),
    Bytes(Vec<u8>),
    Container(BTreeMap<String, PdcValue>),
    List(Vec<PdcValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdcChange {
    Set { key: String, value: PdcValue },
    Remove(String),
    Clear,
}

impl PdcChange {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "pdc.set",
            Self::Remove(_) => "pdc.remove",
            Self::Clear => "pdc.clear",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdcHolder {
    Entity(EntityId),
    Block(BlockPos),
    Item { player: PlayerId, slot: i32 },
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

impl PdcValue {
    fn depth(&self) -> usize {
        match self {
            Self::Container(entries) => 1 + entries.values().map(Self::depth).max().unwrap_or(0),
            Self::List(items) => 1 + items.iter().map(Self::depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    fn check(&self) -> Result<(), ActionError> {
        match self {
            Self::Container(entries) => {
                for (key, value) in entries {
                    if key.is_empty() || key.len() > MAX_NESTED_KEY {
                        return Err(invalid(format!("`{key}` is not a usable container key")));
                    }
                    value.check()?;
                }
                Ok(())
            }
            Self::List(items) => {
                if let Some(first) = items.first()
                    && items
                        .iter()
                        .any(|item| discriminant(item) != discriminant(first))
                {
                    return Err(invalid("a list holds one type of value"));
                }
                items.iter().try_for_each(Self::check)
            }
            _ => Ok(()),
        }
    }

    pub fn to_tag(&self) -> Tag {
        match self {
            Self::Text(text) => Tag::String(text.clone()),
            Self::Byte(value) => Tag::Byte(*value),
            Self::Int(value) => Tag::Int(*value),
            Self::Long(value) => Tag::Long(*value),
            Self::Double(value) => Tag::Double(*value),
            Self::Bytes(bytes) => Tag::ByteArray(bytes.clone()),
            Self::Container(entries) => Tag::Compound(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_tag()))
                    .collect(),
            ),
            Self::List(items) => Tag::List(items.iter().map(Self::to_tag).collect()),
        }
    }

    pub fn from_tag(tag: &Tag) -> Option<Self> {
        Some(match tag {
            Tag::String(text) => Self::Text(text.clone()),
            Tag::Byte(value) => Self::Byte(*value),
            Tag::Int(value) => Self::Int(*value),
            Tag::Long(value) => Self::Long(*value),
            Tag::Double(value) => Self::Double(*value),
            Tag::ByteArray(bytes) => Self::Bytes(bytes.clone()),
            Tag::Compound(entries) => Self::Container(entries_of(entries)),
            Tag::List(items) => Self::List(items.iter().filter_map(Self::from_tag).collect()),
            _ => return None,
        })
    }
}

fn entries_of(compound: &Compound) -> BTreeMap<String, PdcValue> {
    compound
        .iter()
        .filter_map(|(key, tag)| Some((key.to_owned(), PdcValue::from_tag(tag)?)))
        .collect()
}

fn bytes_of(compound: &Compound) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_network(&mut bytes, &Tag::Compound(compound.clone()));
    bytes
}

fn compound_of(bytes: &[u8]) -> Compound {
    read_network_compound(&mut &bytes[..])
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn change_compound(compound: &mut Compound, change: PdcChange) -> Result<(), ActionError> {
    match change {
        PdcChange::Set { key, value } => {
            meta::validate_key(&key).map_err(|error| invalid(error.to_string()))?;
            value.check()?;
            if value.depth() > MAX_DEPTH {
                return Err(invalid(format!("a value nests at most {MAX_DEPTH} levels")));
            }
            if !compound.contains_key(&key) && compound.len() >= MAX_KEYS {
                return Err(ActionError::new(
                    ActionErrorKind::LimitReached,
                    format!("a holder takes at most {MAX_KEYS} keys"),
                ));
            }
            compound.insert(key, value.to_tag());
            if bytes_of(compound).len() > MAX_BYTES {
                return Err(ActionError::new(
                    ActionErrorKind::LimitReached,
                    format!("a holder takes at most {MAX_BYTES} encoded bytes"),
                ));
            }
        }
        PdcChange::Remove(key) => {
            compound.remove(&key);
        }
        PdcChange::Clear => *compound = Compound::new(),
    }
    Ok(())
}

fn gone() -> ActionError {
    ActionError::new(ActionErrorKind::TargetGone, "the holder is gone")
}

fn stored(compound: &Compound) -> Option<MetaValue> {
    (!compound.is_empty()).then(|| MetaValue::Bytes(bytes_of(compound)))
}

fn meta_failure(error: &meta::MetaError) -> ActionError {
    let kind = if error.is_limit() {
        ActionErrorKind::LimitReached
    } else {
        ActionErrorKind::InvalidArgument
    };
    ActionError::new(kind, error.to_string())
}

fn compound_in(value: Option<MetaValue>) -> Compound {
    match value {
        Some(MetaValue::Bytes(bytes)) => compound_of(&bytes),
        _ => Compound::new(),
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn pdc_change(
        &mut self,
        holder: PdcHolder,
        change: PdcChange,
    ) -> Result<(), ActionError> {
        match holder {
            PdcHolder::Entity(id) => {
                let entity = self.entities.store.get_mut(id).ok_or_else(gone)?;
                let mut compound = compound_in(entity.base.meta.get(HOLDER_KEY));
                change_compound(&mut compound, change)?;
                match stored(&compound) {
                    Some(value) => entity
                        .base
                        .meta
                        .set(HOLDER_KEY, value)
                        .map_err(|error| meta_failure(&error)),
                    None => {
                        entity.base.meta.remove(HOLDER_KEY);
                        Ok(())
                    }
                }
            }
            PdcHolder::Block(pos) => {
                let height = self.world.dimension.height;
                if pos.y < height.min_y || pos.y > height.max_y() {
                    return Err(invalid(format!("y {} is outside the world", pos.y)));
                }
                if !self.world.chunks.is_loaded(pos.chunk()) {
                    return Err(ActionError::new(
                        ActionErrorKind::NotFound,
                        "the chunk is not loaded",
                    ));
                }
                let mut compound = compound_in(
                    self.world
                        .chunks
                        .block_meta(pos)
                        .and_then(|held| held.persistent.get(HOLDER_KEY)),
                );
                change_compound(&mut compound, change)?;
                match stored(&compound) {
                    Some(value) => self
                        .world
                        .chunks
                        .set_block_meta(pos, HOLDER_KEY, value, false)
                        .ok_or_else(gone)?
                        .map_err(|error| meta_failure(&error)),
                    None => {
                        self.world.chunks.remove_block_meta(pos, HOLDER_KEY, false);
                        Ok(())
                    }
                }
            }
            PdcHolder::Item { player, slot } => self.item_pdc_change(player, slot, change),
        }
    }

    fn item_slot(slot: i32) -> Result<usize, ActionError> {
        usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < SPIGOT_SLOTS)
            .and_then(menu_slot)
            .ok_or_else(|| invalid(format!("slot {slot} is outside 0..=40")))
    }

    fn item_pdc_change(
        &mut self,
        id: PlayerId,
        slot: i32,
        change: PdcChange,
    ) -> Result<(), ActionError> {
        let menu = Self::item_slot(slot)?;
        let player = self
            .players
            .get_mut(&id)
            .ok_or_else(ActionError::player_gone)?;
        let mut stack = player
            .inventory
            .get(menu)
            .cloned()
            .ok_or_else(|| invalid(format!("slot {slot} holds no item")))?;
        let mut custom = stack
            .components
            .get_compound(CUSTOM_DATA)
            .cloned()
            .unwrap_or_default();
        let mut compound = custom.get_compound(ITEM_KEY).cloned().unwrap_or_default();
        change_compound(&mut compound, change)?;
        if compound.is_empty() {
            custom.remove(ITEM_KEY);
        } else {
            custom.insert(ITEM_KEY, compound);
        }
        if custom.is_empty() {
            stack.components.remove(CUSTOM_DATA);
        } else {
            stack.components.insert(CUSTOM_DATA, custom);
        }
        let before = Self::equipment_of(player);
        player.inventory.set(menu, Some(stack));
        self.sync_inventory(id);
        self.broadcast_equipment_changes(id, before);
        Ok(())
    }

    pub(crate) fn pdc_entries(
        &self,
        holder: &PdcHolder,
    ) -> Result<BTreeMap<String, PdcValue>, ActionError> {
        match holder {
            PdcHolder::Entity(id) => {
                let entity = self.entities.store.get(*id).ok_or_else(gone)?;
                Ok(entries_of(&compound_in(entity.base.meta.get(HOLDER_KEY))))
            }
            PdcHolder::Block(pos) => {
                if !self.world.chunks.is_loaded(pos.chunk()) {
                    return Err(ActionError::new(
                        ActionErrorKind::NotFound,
                        "the chunk is not loaded",
                    ));
                }
                Ok(entries_of(&compound_in(
                    self.world
                        .chunks
                        .block_meta(*pos)
                        .and_then(|held| held.persistent.get(HOLDER_KEY)),
                )))
            }
            PdcHolder::Item { player, slot } => {
                let menu = Self::item_slot(*slot)?;
                let stack = self
                    .players
                    .get(player)
                    .ok_or_else(ActionError::player_gone)?
                    .inventory
                    .get(menu)
                    .ok_or_else(|| invalid(format!("slot {slot} holds no item")))?;
                Ok(stack
                    .components
                    .get_compound(CUSTOM_DATA)
                    .and_then(|custom| custom.get_compound(ITEM_KEY))
                    .map(entries_of)
                    .unwrap_or_default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(compound: &mut Compound, key: &str, value: PdcValue) -> Result<(), ActionError> {
        change_compound(
            compound,
            PdcChange::Set {
                key: key.to_owned(),
                value,
            },
        )
    }

    #[test]
    fn every_value_type_survives_a_tag_round_trip() {
        let nested = PdcValue::Container(BTreeMap::from([(
            "inner".to_owned(),
            PdcValue::List(vec![PdcValue::Long(1), PdcValue::Long(2)]),
        )]));
        for value in [
            PdcValue::Text("hi".to_owned()),
            PdcValue::Byte(-3),
            PdcValue::Int(70_000),
            PdcValue::Long(i64::MAX),
            PdcValue::Double(0.25),
            PdcValue::Bytes(vec![1, 2, 3]),
            nested,
        ] {
            assert_eq!(PdcValue::from_tag(&value.to_tag()), Some(value));
        }
    }

    #[test]
    fn a_stored_compound_survives_the_byte_encoding() {
        let mut compound = Compound::new();
        set(&mut compound, "plugin:level", PdcValue::Int(7)).unwrap();
        set(&mut compound, "plugin:name", PdcValue::Text("x".to_owned())).unwrap();
        assert_eq!(compound_of(&bytes_of(&compound)), compound);
    }

    #[test]
    fn keys_must_be_namespaced_and_lists_must_be_uniform() {
        let mut compound = Compound::new();
        assert!(set(&mut compound, "level", PdcValue::Int(1)).is_err());
        let mixed = PdcValue::List(vec![PdcValue::Int(1), PdcValue::Long(2)]);
        assert!(set(&mut compound, "plugin:mixed", mixed).is_err());
        assert!(compound.is_empty());
    }

    #[test]
    fn a_holder_takes_a_bounded_number_of_keys_and_levels() {
        let mut compound = Compound::new();
        for index in 0..MAX_KEYS {
            set(
                &mut compound,
                &format!("plugin:k{index}"),
                PdcValue::Byte(1),
            )
            .unwrap();
        }
        let refused = set(&mut compound, "plugin:more", PdcValue::Byte(1)).unwrap_err();
        assert_eq!(refused.kind, ActionErrorKind::LimitReached);
        let mut deep = PdcValue::Int(1);
        for _ in 0..=MAX_DEPTH {
            deep = PdcValue::List(vec![deep]);
        }
        let mut fresh = Compound::new();
        assert!(set(&mut fresh, "plugin:deep", deep).is_err());
    }

    #[test]
    fn an_entity_holder_survives_a_save_and_a_load() {
        use rspigot_domain::entity::save::{load_entity, save_entity};
        let nbt = Compound::new().with("id", "minecraft:cow");
        let mut entity = load_entity(&nbt, EntityId(1), uuid::Uuid::from_u128(1)).expect("a cow");
        let mut compound = Compound::new();
        set(&mut compound, "plugin:level", PdcValue::Int(7)).unwrap();
        set(
            &mut compound,
            "plugin:ids",
            PdcValue::List(vec![PdcValue::Long(9)]),
        )
        .unwrap();
        let value = stored(&compound).expect("a value to keep");
        entity.base.meta.set(HOLDER_KEY, value).unwrap();

        let saved = save_entity(&entity, None).expect("a saved cow");
        let again = load_entity(&saved, EntityId(2), uuid::Uuid::from_u128(2)).expect("a cow");

        assert_eq!(compound_in(again.base.meta.get(HOLDER_KEY)), compound);
    }
}
