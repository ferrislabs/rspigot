use rspigot_plugin_protocol::v1::{self, answer, block_order, from_plugin, question};

use crate::link::Link;
use crate::meta::MetaHandle;
use crate::tile::{
    BlockBannerHandle, BlockBeaconHandle, BlockCommandHandle, BlockData, BlockDataHandle,
    BlockEndGatewayHandle, BlockFurnaceHandle, BlockInventoryHandle, BlockJukeboxHandle,
    BlockLecternHandle, BlockPotHandle, BlockSignHandle, BlockSkullHandle, BlockSpawnerHandle,
    block_info,
};
use crate::{BlockType, Error};

type Entries = std::sync::Arc<std::collections::BTreeMap<String, crate::MetaValue>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockState {
    pub kind: BlockType,
    pub properties: Vec<(String, String)>,
}

impl BlockState {
    pub(crate) fn from_wire(state: v1::BlockState) -> Result<Self, Error> {
        Ok(Self {
            kind: block_type(&state.block)?,
            properties: state
                .properties
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        })
    }

    pub(crate) fn to_wire(&self) -> v1::BlockState {
        v1::BlockState {
            block: self.kind.key().to_owned(),
            properties: self
                .properties
                .iter()
                .map(|(name, value)| v1::Property {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub pos: BlockPos,
    pub meta: MetaHandle,
    pub volatile_meta: MetaHandle,
    pub data: BlockDataHandle,
    pub sign: BlockSignHandle,
    pub inventory: BlockInventoryHandle,
    pub furnace: BlockFurnaceHandle,
    pub spawner: BlockSpawnerHandle,
    pub skull: BlockSkullHandle,
    pub banner: BlockBannerHandle,
    pub command: BlockCommandHandle,
    pub beacon: BlockBeaconHandle,
    pub jukebox: BlockJukeboxHandle,
    pub lectern: BlockLecternHandle,
    pub end_gateway: BlockEndGatewayHandle,
    pub decorated_pot: BlockPotHandle,
    link: Link,
}

impl Block {
    pub(crate) fn new(pos: BlockPos, link: Link) -> Self {
        Self::with_meta(pos, link, Entries::default(), Entries::default())
    }

    pub(crate) fn with_meta(
        pos: BlockPos,
        link: Link,
        persistent: Entries,
        volatile: Entries,
    ) -> Self {
        Self {
            pos,
            meta: MetaHandle::of_block(pos, false, persistent, link.clone()),
            volatile_meta: MetaHandle::of_block(pos, true, volatile, link.clone()),
            data: BlockDataHandle::new(pos, link.clone()),
            sign: BlockSignHandle::new(pos, link.clone()),
            inventory: BlockInventoryHandle::new(pos, link.clone()),
            furnace: BlockFurnaceHandle::new(pos, link.clone()),
            spawner: BlockSpawnerHandle::new(pos, link.clone()),
            skull: BlockSkullHandle::new(pos, link.clone()),
            banner: BlockBannerHandle::new(pos, link.clone()),
            command: BlockCommandHandle::new(pos, link.clone()),
            beacon: BlockBeaconHandle::new(pos, link.clone()),
            jukebox: BlockJukeboxHandle::new(pos, link.clone()),
            lectern: BlockLecternHandle::new(pos, link.clone()),
            end_gateway: BlockEndGatewayHandle::new(pos, link.clone()),
            decorated_pot: BlockPotHandle::new(pos, link.clone()),
            link,
        }
    }

    pub fn set(&self, kind: BlockType) {
        self.order(block_order::Order::Set(kind.key().to_owned()));
    }

    pub fn set_with(&self, kind: BlockType, properties: &[(&str, &str)]) {
        self.order(block_order::Order::SetWith(v1::SetBlockWith {
            block: kind.key().to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| v1::Property {
                    name: (*name).to_owned(),
                    value: (*value).to_owned(),
                })
                .collect(),
        }));
    }

    pub fn break_naturally(&self) {
        self.order(block_order::Order::BreakNaturally(v1::Empty {}));
    }

    pub async fn get_state(&self) -> Result<BlockState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Block(self.pos.to_wire()),
                "a block state",
                |body| match body {
                    answer::Body::Block(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        BlockState::from_wire(state)
    }

    pub async fn get_data(&self) -> Result<BlockData, Error> {
        self.data.get().await
    }

    pub fn set_data(&self, properties: &[(&str, &str)]) {
        self.data.set(properties);
    }

    pub async fn get_light(&self) -> Result<u8, Error> {
        Ok(light_of(block_info(&self.link, self.pos).await?.light))
    }

    pub async fn get_sky_light(&self) -> Result<u8, Error> {
        Ok(light_of(block_info(&self.link, self.pos).await?.sky_light))
    }

    pub async fn get_block_light(&self) -> Result<u8, Error> {
        Ok(light_of(
            block_info(&self.link, self.pos).await?.block_light,
        ))
    }

    pub async fn get_power(&self) -> Result<u8, Error> {
        Ok(light_of(block_info(&self.link, self.pos).await?.power))
    }

    pub async fn is_powered(&self) -> Result<bool, Error> {
        Ok(block_info(&self.link, self.pos).await?.power > 0)
    }

    pub async fn get_biome(&self) -> Result<String, Error> {
        Ok(block_info(&self.link, self.pos).await?.biome)
    }

    fn order(&self, order: block_order::Order) {
        self.link.order(from_plugin::Body::Block(v1::BlockOrder {
            pos: Some(self.pos.to_wire()),
            order: Some(order),
        }));
    }
}

fn light_of(level: i32) -> u8 {
    u8::try_from(level.clamp(0, 15)).unwrap_or(0)
}

pub(crate) fn block_type(key: &str) -> Result<BlockType, Error> {
    BlockType::from_key(key).ok_or_else(|| Error::Malformed(format!("an unknown block `{key}`")))
}
