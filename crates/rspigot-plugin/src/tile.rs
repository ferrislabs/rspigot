use std::collections::BTreeMap;

use rspigot_plugin_protocol::v1::{
    self, answer, block_order, from_plugin, question, tile_edit::Edit, tile_state,
};
use uuid::Uuid;

use crate::block::BlockPos;
use crate::link::Link;
use crate::{
    BlockType, DyeColor, EffectType, EntityType, Error, ItemStack, Material, Side,
    wire::{material, parse_uuid},
};

const DYES: [DyeColor; 16] = [
    DyeColor::White,
    DyeColor::Orange,
    DyeColor::Magenta,
    DyeColor::LightBlue,
    DyeColor::Yellow,
    DyeColor::Lime,
    DyeColor::Pink,
    DyeColor::Gray,
    DyeColor::LightGray,
    DyeColor::Cyan,
    DyeColor::Purple,
    DyeColor::Blue,
    DyeColor::Brown,
    DyeColor::Green,
    DyeColor::Red,
    DyeColor::Black,
];
const DYE_KEYS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

fn dye_key(color: DyeColor) -> &'static str {
    DYES.iter()
        .position(|known| *known == color)
        .map_or("white", |index| DYE_KEYS[index])
}

fn dye_of(key: &str) -> Result<DyeColor, Error> {
    DYE_KEYS
        .iter()
        .position(|known| *known == key)
        .map(|index| DYES[index])
        .ok_or_else(|| Error::Malformed(format!("an unknown dye colour `{key}`")))
}

const fn front(side: Side) -> bool {
    matches!(side, Side::Front)
}

macro_rules! tiles {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone)]
            pub struct $name {
                pos: BlockPos,
                link: Link,
            }

            impl $name {
                pub(crate) const fn new(pos: BlockPos, link: Link) -> Self {
                    Self { pos, link }
                }

                fn edit(&self, edit: Edit) {
                    self.link.order(from_plugin::Body::Block(v1::BlockOrder {
                        pos: Some(self.pos.to_wire()),
                        order: Some(block_order::Order::Tile(v1::TileEdit {
                            edit: Some(edit),
                        })),
                    }));
                }

                async fn tile(&self) -> Result<tile_state::State, Error> {
                    ask_tile(&self.link, self.pos).await
                }
            }
        )+
    };
}

async fn ask_tile(link: &Link, pos: BlockPos) -> Result<tile_state::State, Error> {
    let tile = link
        .ask(
            question::Question::Tile(pos.to_wire()),
            "a tile entity",
            |body| match body {
                answer::Body::Tile(tile) => Some(tile),
                _ => None,
            },
        )
        .await?;
    tile.state
        .ok_or_else(|| Error::Malformed("a tile entity without a state".to_owned()))
}

fn unexpected<T>(kind: &str) -> Result<T, Error> {
    Err(Error::Malformed(format!(
        "a tile entity that is not {kind}"
    )))
}

tiles!(
    BlockSignHandle,
    BlockInventoryHandle,
    BlockFurnaceHandle,
    BlockSpawnerHandle,
    BlockSkullHandle,
    BlockBannerHandle,
    BlockCommandHandle,
    BlockBeaconHandle,
    BlockJukeboxHandle,
    BlockLecternHandle,
    BlockEndGatewayHandle,
    BlockPotHandle,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignContents {
    pub front: Vec<String>,
    pub back: Vec<String>,
    pub waxed: bool,
    pub front_color: DyeColor,
    pub back_color: DyeColor,
    pub front_glowing: bool,
    pub back_glowing: bool,
}

impl BlockSignHandle {
    pub fn set_lines<S: AsRef<str>>(&self, side: Side, lines: &[S]) {
        self.edit(Edit::SignLines(v1::SignLines {
            front: front(side),
            lines: lines.iter().map(|line| line.as_ref().to_owned()).collect(),
        }));
    }

    pub fn set_color(&self, side: Side, color: DyeColor) {
        self.edit(Edit::SignStyle(v1::SignStyle {
            front: front(side),
            color: Some(dye_key(color).to_owned()),
            glowing: None,
        }));
    }

    pub fn set_glowing(&self, side: Side, glowing: bool) {
        self.edit(Edit::SignStyle(v1::SignStyle {
            front: front(side),
            color: None,
            glowing: Some(glowing),
        }));
    }

    pub fn set_waxed(&self, waxed: bool) {
        self.edit(Edit::SignWaxed(waxed));
    }

    pub async fn get(&self) -> Result<SignContents, Error> {
        let tile_state::State::Sign(sign) = self.tile().await? else {
            return unexpected("a sign");
        };
        Ok(SignContents {
            front: sign.front,
            back: sign.back,
            waxed: sign.waxed,
            front_color: dye_of(&sign.front_color)?,
            back_color: dye_of(&sign.back_color)?,
            front_glowing: sign.front_glowing,
            back_glowing: sign.back_glowing,
        })
    }
}

fn items_of(contents: v1::InventoryContents) -> Result<Vec<(i32, ItemStack)>, Error> {
    contents.slots.into_iter().map(crate::wire::slot).collect()
}

impl BlockInventoryHandle {
    pub fn set(&self, slot: i32, item: &ItemStack) {
        self.edit(Edit::InventorySet(v1::SetSlot {
            slot,
            item: Some(item.to_wire()),
        }));
    }

    pub fn clear_slot(&self, slot: i32) {
        self.edit(Edit::InventorySet(v1::SetSlot { slot, item: None }));
    }

    pub fn clear(&self) {
        self.edit(Edit::InventoryClear(v1::Empty {}));
    }

    pub fn add(&self, item: &ItemStack) {
        self.edit(Edit::InventoryAdd(item.to_wire()));
    }

    pub async fn get_items(&self) -> Result<Vec<(i32, ItemStack)>, Error> {
        match self.tile().await? {
            tile_state::State::Inventory(contents) => items_of(contents),
            tile_state::State::Furnace(furnace) => items_of(furnace.contents.unwrap_or_default()),
            _ => unexpected("a container"),
        }
    }

    pub async fn get_size(&self) -> Result<i32, Error> {
        match self.tile().await? {
            tile_state::State::Inventory(contents) => Ok(contents.size),
            tile_state::State::Furnace(furnace) => Ok(furnace.contents.unwrap_or_default().size),
            _ => unexpected("a container"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FurnaceState {
    pub burn_time: i32,
    pub cook_time: i32,
    pub cook_time_total: i32,
    pub lit: bool,
}

impl BlockFurnaceHandle {
    pub fn set_burn_time(&self, ticks: i32) {
        self.edit(Edit::Furnace(v1::FurnaceEdit {
            burn_time: Some(ticks),
            ..Default::default()
        }));
    }

    pub fn set_cook_time(&self, ticks: i32) {
        self.edit(Edit::Furnace(v1::FurnaceEdit {
            cook_time: Some(ticks),
            ..Default::default()
        }));
    }

    pub fn set_cook_time_total(&self, ticks: i32) {
        self.edit(Edit::Furnace(v1::FurnaceEdit {
            cook_time_total: Some(ticks),
            ..Default::default()
        }));
    }

    pub async fn get(&self) -> Result<FurnaceState, Error> {
        let tile_state::State::Furnace(furnace) = self.tile().await? else {
            return unexpected("a furnace");
        };
        Ok(FurnaceState {
            burn_time: furnace.burn_time,
            cook_time: furnace.cook_time,
            cook_time_total: furnace.cook_time_total,
            lit: furnace.lit,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnerState {
    pub entity: Option<EntityType>,
    pub delay: i32,
    pub min_delay: i32,
    pub max_delay: i32,
    pub spawn_count: i32,
    pub max_nearby: i32,
    pub required_player_range: i32,
    pub spawn_range: i32,
}

macro_rules! spawner_setters {
    ($($method:ident => $field:ident),+ $(,)?) => {
        $(
            pub fn $method(&self, value: i32) {
                self.edit(Edit::Spawner(v1::SpawnerEdit {
                    $field: Some(value),
                    ..Default::default()
                }));
            }
        )+
    };
}

impl BlockSpawnerHandle {
    pub fn set_entity_type(&self, kind: EntityType) {
        self.edit(Edit::Spawner(v1::SpawnerEdit {
            entity: Some(kind.key().to_owned()),
            ..Default::default()
        }));
    }

    spawner_setters!(
        set_delay => delay,
        set_min_delay => min_delay,
        set_max_delay => max_delay,
        set_spawn_count => spawn_count,
        set_max_nearby => max_nearby,
        set_required_player_range => required_player_range,
        set_spawn_range => spawn_range,
    );

    pub async fn get(&self) -> Result<SpawnerState, Error> {
        let tile_state::State::Spawner(spawner) = self.tile().await? else {
            return unexpected("a spawner");
        };
        Ok(SpawnerState {
            entity: EntityType::from_key(&spawner.entity),
            delay: spawner.delay,
            min_delay: spawner.min_delay,
            max_delay: spawner.max_delay,
            spawn_count: spawner.spawn_count,
            max_nearby: spawner.max_nearby,
            required_player_range: spawner.required_player_range,
            spawn_range: spawner.spawn_range,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkullOwner {
    pub name: Option<String>,
    pub uuid: Option<Uuid>,
}

impl BlockSkullHandle {
    pub fn set_owner(&self, name: impl Into<String>) {
        self.edit(Edit::SkullOwner(v1::SkullOwner {
            name: Some(name.into()),
            uuid: None,
        }));
    }

    pub fn set_owner_uuid(&self, uuid: Uuid) {
        self.edit(Edit::SkullOwner(v1::SkullOwner {
            name: None,
            uuid: Some(uuid.hyphenated().to_string()),
        }));
    }

    pub fn clear_owner(&self) {
        self.edit(Edit::SkullOwner(v1::SkullOwner::default()));
    }

    pub async fn get_owner(&self) -> Result<SkullOwner, Error> {
        let tile_state::State::Skull(owner) = self.tile().await? else {
            return unexpected("a skull");
        };
        Ok(SkullOwner {
            name: owner.name,
            uuid: owner
                .uuid
                .map(|uuid| parse_uuid(&uuid, "a skull owner"))
                .transpose()?,
        })
    }
}

impl BlockBannerHandle {
    pub fn set_patterns<S: AsRef<str>>(&self, patterns: &[(S, DyeColor)]) {
        self.edit(Edit::BannerPatterns(v1::BannerPatterns {
            patterns: patterns
                .iter()
                .map(|(pattern, color)| v1::BannerPattern {
                    pattern: pattern.as_ref().to_owned(),
                    color: dye_key(*color).to_owned(),
                })
                .collect(),
        }));
    }

    pub async fn get_patterns(&self) -> Result<Vec<(String, DyeColor)>, Error> {
        let tile_state::State::Banner(banner) = self.tile().await? else {
            return unexpected("a banner");
        };
        banner
            .patterns
            .into_iter()
            .map(|pattern| Ok((pattern.pattern, dye_of(&pattern.color)?)))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandBlockState {
    pub command: String,
    pub name: String,
}

impl BlockCommandHandle {
    pub fn set_command(&self, command: impl Into<String>) {
        self.edit(Edit::CommandBlock(v1::CommandEdit {
            command: Some(command.into()),
            name: None,
        }));
    }

    pub fn set_name(&self, name: impl Into<String>) {
        self.edit(Edit::CommandBlock(v1::CommandEdit {
            command: None,
            name: Some(name.into()),
        }));
    }

    pub async fn get(&self) -> Result<CommandBlockState, Error> {
        let tile_state::State::CommandBlock(command) = self.tile().await? else {
            return unexpected("a command block");
        };
        Ok(CommandBlockState {
            command: command.command,
            name: command.name,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeaconState {
    pub levels: i32,
    pub primary: Option<EffectType>,
    pub secondary: Option<EffectType>,
}

impl BlockBeaconHandle {
    pub fn set_effects(&self, primary: Option<EffectType>, secondary: Option<EffectType>) {
        self.edit(Edit::Beacon(v1::BeaconEffects {
            primary: primary.map(|effect| effect.key().to_owned()),
            secondary: secondary.map(|effect| effect.key().to_owned()),
        }));
    }

    pub async fn get(&self) -> Result<BeaconState, Error> {
        let tile_state::State::Beacon(beacon) = self.tile().await? else {
            return unexpected("a beacon");
        };
        Ok(BeaconState {
            levels: beacon.levels,
            primary: beacon.primary.map(crate::wire::effect_type).transpose()?,
            secondary: beacon.secondary.map(crate::wire::effect_type).transpose()?,
        })
    }
}

impl BlockJukeboxHandle {
    pub fn set_disc(&self, disc: &ItemStack) {
        self.edit(Edit::JukeboxDisc(disc.to_wire()));
    }

    pub fn clear(&self) {
        self.edit(Edit::JukeboxClear(v1::Empty {}));
    }

    pub async fn get_disc(&self) -> Result<Option<ItemStack>, Error> {
        let tile_state::State::Jukebox(disc) = self.tile().await? else {
            return unexpected("a jukebox");
        };
        disc.item.map(ItemStack::from_wire).transpose()
    }
}

impl BlockLecternHandle {
    pub fn set_book(&self, book: &ItemStack) {
        self.edit(Edit::LecternBook(book.to_wire()));
    }

    pub fn clear(&self) {
        self.edit(Edit::LecternClear(v1::Empty {}));
    }

    pub async fn get_book(&self) -> Result<Option<ItemStack>, Error> {
        let tile_state::State::Lectern(book) = self.tile().await? else {
            return unexpected("a lectern");
        };
        book.item.map(ItemStack::from_wire).transpose()
    }
}

impl BlockEndGatewayHandle {
    pub fn set_exit(&self, exit: BlockPos, exact: bool) {
        self.edit(Edit::EndGateway(v1::GatewayExit {
            exit: Some(exit.to_wire()),
            exact,
        }));
    }

    pub async fn get_exit(&self) -> Result<Option<(BlockPos, bool)>, Error> {
        let tile_state::State::EndGateway(gateway) = self.tile().await? else {
            return unexpected("an end gateway");
        };
        gateway
            .exit
            .map(|exit| Ok((BlockPos::from_wire(Some(exit), "exit")?, gateway.exact)))
            .transpose()
    }
}

impl BlockPotHandle {
    pub fn set_sherds(&self, sherds: [Material; 4]) {
        self.edit(Edit::DecoratedPot(v1::PotSherds {
            sherds: sherds
                .into_iter()
                .map(|sherd| sherd.key().to_owned())
                .collect(),
        }));
    }

    pub async fn get_sherds(&self) -> Result<Vec<Material>, Error> {
        let tile_state::State::DecoratedPot(pot) = self.tile().await? else {
            return unexpected("a decorated pot");
        };
        pot.sherds.into_iter().map(material).collect()
    }
}

#[derive(Debug, Clone)]
pub struct BlockDataHandle {
    pos: BlockPos,
    link: Link,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockData {
    pub kind: BlockType,
    pub properties: BTreeMap<String, String>,
}

impl BlockDataHandle {
    pub(crate) const fn new(pos: BlockPos, link: Link) -> Self {
        Self { pos, link }
    }

    pub fn set(&self, properties: &[(&str, &str)]) {
        self.link.order(from_plugin::Body::Block(v1::BlockOrder {
            pos: Some(self.pos.to_wire()),
            order: Some(block_order::Order::SetData(v1::BlockDataEdit {
                properties: properties
                    .iter()
                    .map(|(name, value)| v1::Property {
                        name: (*name).to_owned(),
                        value: (*value).to_owned(),
                    })
                    .collect(),
            })),
        }));
    }

    pub async fn get(&self) -> Result<BlockData, Error> {
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
        Ok(BlockData {
            kind: crate::block::block_type(&state.block)?,
            properties: state
                .properties
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        })
    }

    pub async fn get_property(&self, name: &str) -> Result<Option<String>, Error> {
        Ok(self.get().await?.properties.remove(name))
    }
}

pub(crate) async fn block_info(link: &Link, pos: BlockPos) -> Result<v1::BlockInfo, Error> {
    link.ask(
        question::Question::BlockInfo(pos.to_wire()),
        "block information",
        |body| match body {
            answer::Body::BlockInfo(info) => Some(info),
            _ => None,
        },
    )
    .await
}
