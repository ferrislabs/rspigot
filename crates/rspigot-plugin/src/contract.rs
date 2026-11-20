use rspigot_plugin_protocol::v1::{
    self, command_sender, holder_ref, inventory_ref, merchant_ref, owner_ref,
};
use uuid::Uuid;

use crate::custom_inventory::CustomInventory;
use crate::link::Link;
use crate::player::Rotation;
use crate::wire::{block, enchantment, entity_type, parse_uuid, required};
use crate::{
    Block, BlockPos, BlockState, Enchantment, Entity, EntityType, Error, ItemStack, Player,
    Position,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub world: String,
    pub position: Position,
    pub rotation: Rotation,
}

impl Location {
    pub(crate) fn from_wire(location: v1::Location, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            world: location.world,
            position: Position::from_wire(location.position, "position")?,
            rotation: Rotation::from_wire(location.rotation)?,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::Location {
        v1::Location {
            world: self.world.clone(),
            position: Some(self.position.to_wire()),
            rotation: Some(self.rotation.to_wire()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSnapshot {
    pub pos: BlockPos,
    pub state: BlockState,
}

impl BlockSnapshot {
    pub(crate) fn from_wire(snapshot: v1::BlockSnapshot, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(snapshot.pos, "pos")?,
            state: BlockState::from_wire(required(snapshot.state, "state")?)?,
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum InventoryOwner {
    Custom(CustomInventory),
    Player(Uuid),
    Block(Block),
    Entity(i32),
    Detached,
}

#[derive(Debug, Clone)]
pub struct InventoryRef {
    pub owner: InventoryOwner,
    pub size: i32,
    pub kind: String,
    pub id: u64,
}

impl InventoryRef {
    pub(crate) fn from_wire(inventory: v1::InventoryRef, link: &Link) -> Result<Self, Error> {
        let owner = match required(inventory.owner, "owner")? {
            inventory_ref::Owner::Custom(id) => {
                InventoryOwner::Custom(CustomInventory::owned_by(id, inventory.plugin, link))
            }
            inventory_ref::Owner::Player(id) => {
                InventoryOwner::Player(parse_uuid(&id, "a player")?)
            }
            inventory_ref::Owner::Block(pos) => InventoryOwner::Block(block(pos, link)),
            inventory_ref::Owner::Entity(id) => InventoryOwner::Entity(id),
            inventory_ref::Owner::Detached(_) => InventoryOwner::Detached,
        };
        Ok(Self {
            owner,
            size: inventory.size,
            kind: inventory.kind,
            id: inventory.id,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::InventoryRef {
        let plugin = self
            .custom()
            .map(|inventory| inventory.plugin().to_owned())
            .unwrap_or_default();
        let owner = match &self.owner {
            InventoryOwner::Custom(inventory) => inventory_ref::Owner::Custom(inventory.id()),
            InventoryOwner::Player(id) => inventory_ref::Owner::Player(id.hyphenated().to_string()),
            InventoryOwner::Block(block) => inventory_ref::Owner::Block(block.pos.to_wire()),
            InventoryOwner::Entity(id) => inventory_ref::Owner::Entity(*id),
            InventoryOwner::Detached => inventory_ref::Owner::Detached(v1::Empty {}),
        };
        v1::InventoryRef {
            owner: Some(owner),
            size: self.size,
            kind: self.kind.clone(),
            plugin,
            id: self.id,
        }
    }

    pub const fn custom(&self) -> Option<&CustomInventory> {
        match &self.owner {
            InventoryOwner::Custom(inventory) => Some(inventory),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InventoryView {
    pub top: InventoryRef,
    pub bottom: InventoryRef,
    pub title: String,
    pub player: Player,
}

impl InventoryView {
    pub(crate) fn from_wire(view: v1::InventoryView, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            top: InventoryRef::from_wire(required(view.top, "top")?, link)?,
            bottom: InventoryRef::from_wire(required(view.bottom, "bottom")?, link)?,
            title: view.title,
            player: Player::from_wire(view.player, link)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChunkRef {
    pub world: String,
    pub x: i32,
    pub z: i32,
}

impl ChunkRef {
    pub(crate) fn from_wire(chunk: v1::ChunkRef, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            world: chunk.world,
            x: chunk.x,
            z: chunk.z,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DamageSource {
    pub kind: String,
    pub causing: Option<Entity>,
    pub direct: Option<Entity>,
    pub location: Option<Position>,
}

impl DamageSource {
    pub(crate) fn from_wire(source: v1::DamageSource, link: &Link) -> Result<Self, Error> {
        let entity = |snapshot: Option<v1::EntitySnapshot>| {
            snapshot
                .map(|snapshot| Entity::from_wire(snapshot, link))
                .transpose()
        };
        Ok(Self {
            kind: source.kind,
            causing: entity(source.causing)?,
            direct: entity(source.direct)?,
            location: source
                .location
                .map(|location| Position::from_wire(Some(location), "location"))
                .transpose()?,
        })
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CommandSender {
    Player(Player),
    Console,
}

impl CommandSender {
    pub(crate) fn from_wire(sender: v1::CommandSender, link: &Link) -> Result<Self, Error> {
        Ok(match required(sender.sender, "sender")? {
            command_sender::Sender::Player(player) => {
                Self::Player(Player::from_snapshot(player, link)?)
            }
            command_sender::Sender::Console(_) => Self::Console,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::CommandSender {
        let sender = match self {
            Self::Player(player) => command_sender::Sender::Player(player.to_snapshot()),
            Self::Console => command_sender::Sender::Console(v1::Empty {}),
        };
        v1::CommandSender {
            sender: Some(sender),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub result: ItemStack,
    pub ingredients: Vec<ItemStack>,
    pub uses: i32,
    pub max_uses: i32,
    pub experience_reward: bool,
    pub villager_experience: i32,
    pub price_multiplier: f32,
    pub demand: i32,
    pub special_price: i32,
}

impl Trade {
    pub(crate) fn from_wire(trade: v1::Trade, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            result: ItemStack::from_wire(required(trade.result, "result")?)?,
            ingredients: trade
                .ingredients
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<_, _>>()?,
            uses: trade.uses,
            max_uses: trade.max_uses,
            experience_reward: trade.experience_reward,
            villager_experience: trade.villager_experience,
            price_multiplier: trade.price_multiplier,
            demand: trade.demand,
            special_price: trade.special_price,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::Trade {
        v1::Trade {
            result: Some(self.result.to_wire()),
            ingredients: self.ingredients.iter().map(ItemStack::to_wire).collect(),
            uses: self.uses,
            max_uses: self.max_uses,
            experience_reward: self.experience_reward,
            villager_experience: self.villager_experience,
            price_multiplier: self.price_multiplier,
            demand: self.demand,
            special_price: self.special_price,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
}

impl PluginInfo {
    pub(crate) fn from_wire(plugin: v1::PluginInfo, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            name: plugin.name,
            version: plugin.version,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: Position,
    pub max: Position,
}

impl BoundingBox {
    pub(crate) fn from_wire(bounds: v1::BoundingBox, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            min: Position::from_wire(bounds.min, "min")?,
            max: Position::from_wire(bounds.max, "max")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Input {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
}

impl Input {
    pub(crate) const fn from_wire(input: v1::Input, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            forward: input.forward,
            backward: input.backward,
            left: input.left,
            right: input.right,
            jump: input.jump,
            sneak: input.sneak,
            sprint: input.sprint,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnerData {
    pub pos: BlockPos,
    pub spawned: Option<EntityType>,
    pub delay: i32,
}

impl SpawnerData {
    pub(crate) fn from_wire(spawner: v1::SpawnerData, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(spawner.pos, "pos")?,
            spawned: spawner.spawned.map(entity_type).transpose()?,
            delay: spawner.delay,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialSpawnerData {
    pub pos: BlockPos,
    pub ominous: bool,
}

impl TrialSpawnerData {
    pub(crate) fn from_wire(spawner: v1::TrialSpawnerData, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(spawner.pos, "pos")?,
            ominous: spawner.ominous,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LecternData {
    pub pos: BlockPos,
    pub book: Option<ItemStack>,
    pub page: i32,
}

impl LecternData {
    pub(crate) fn from_wire(lectern: v1::LecternData, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(lectern.pos, "pos")?,
            book: lectern.book.map(ItemStack::from_wire).transpose()?,
            page: lectern.page,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DroppedItem {
    pub entity: Entity,
    pub item: ItemStack,
}

impl DroppedItem {
    pub(crate) fn from_wire(dropped: v1::DroppedItem, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            entity: Entity::from_wire(required(dropped.entity, "entity")?, link)?,
            item: ItemStack::from_wire(required(dropped.item, "item")?)?,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::DroppedItem {
        v1::DroppedItem {
            entity: Some(self.entity.to_wire()),
            item: Some(self.item.to_wire()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignData {
    pub pos: BlockPos,
    pub front: Vec<String>,
    pub back: Vec<String>,
    pub waxed: bool,
}

impl SignData {
    pub(crate) fn from_wire(sign: v1::SignData, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(sign.pos, "pos")?,
            front: sign.front,
            back: sign.back,
            waxed: sign.waxed,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnchantOffer {
    pub enchantment: Enchantment,
    pub level: i32,
    pub cost: i32,
}

impl EnchantOffer {
    pub(crate) fn from_wire(offer: v1::EnchantOffer, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            enchantment: enchantment(offer.enchantment)?,
            level: offer.level,
            cost: offer.cost,
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum OwnerRef {
    Player(Player),
    Entity(Entity),
    Offline(Uuid),
}

impl OwnerRef {
    pub(crate) fn from_wire(owner: v1::OwnerRef, link: &Link) -> Result<Self, Error> {
        Ok(match required(owner.owner, "owner")? {
            owner_ref::Owner::Player(player) => Self::Player(Player::from_snapshot(player, link)?),
            owner_ref::Owner::Entity(entity) => Self::Entity(Entity::from_wire(entity, link)?),
            owner_ref::Owner::Offline(id) => Self::Offline(parse_uuid(&id, "a player")?),
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum HolderRef {
    Player(Player),
    Entity(Entity),
    Block(Block),
}

impl HolderRef {
    pub(crate) fn from_wire(holder: v1::HolderRef, link: &Link) -> Result<Self, Error> {
        Ok(match required(holder.holder, "holder")? {
            holder_ref::Holder::Player(player) => {
                Self::Player(Player::from_snapshot(player, link)?)
            }
            holder_ref::Holder::Entity(entity) => Self::Entity(Entity::from_wire(entity, link)?),
            holder_ref::Holder::Block(pos) => Self::Block(block(pos, link)),
        })
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum MerchantRef {
    Entity(Entity),
    Custom,
}

impl MerchantRef {
    pub(crate) fn from_wire(merchant: v1::MerchantRef, link: &Link) -> Result<Self, Error> {
        Ok(match required(merchant.merchant, "merchant")? {
            merchant_ref::Merchant::Entity(entity) => {
                Self::Entity(Entity::from_wire(entity, link)?)
            }
            merchant_ref::Merchant::Custom(_) => Self::Custom,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookMeta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub pages: Vec<String>,
}

impl BookMeta {
    pub(crate) fn from_wire(book: v1::BookMeta, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            title: book.title,
            author: book.author,
            pages: book.pages,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::BookMeta {
        v1::BookMeta {
            title: self.title.clone(),
            author: self.author.clone(),
            pages: self.pages.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LootContext {
    pub location: Location,
    pub luck: f32,
    pub killer: Option<Entity>,
    pub looted: Option<Entity>,
}

impl LootContext {
    pub(crate) fn from_wire(context: v1::LootContext, link: &Link) -> Result<Self, Error> {
        let entity = |snapshot: Option<v1::EntitySnapshot>| {
            snapshot
                .map(|snapshot| Entity::from_wire(snapshot, link))
                .transpose()
        };
        Ok(Self {
            location: Location::from_wire(required(context.location, "location")?, link)?,
            luck: context.luck,
            killer: entity(context.killer)?,
            looted: entity(context.looted)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steve() -> v1::PlayerSnapshot {
        v1::PlayerSnapshot {
            permissions: Vec::new(),
            uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 64.0,
                z: -3.5,
            }),
            rotation: Some(v1::Rotation {
                yaw: 90.0,
                pitch: -30.0,
            }),
            game_mode: v1::GameMode::Creative as i32,
            ..Default::default()
        }
    }

    #[test]
    fn a_player_and_a_sender_go_back_to_the_wire_unchanged() {
        let link = Link::default();
        let player = Player::from_snapshot(steve(), &link).unwrap();
        assert_eq!(player.to_snapshot(), steve());
        for sender in [
            command_sender::Sender::Player(steve()),
            command_sender::Sender::Console(v1::Empty {}),
        ] {
            let wire = v1::CommandSender {
                sender: Some(sender),
            };
            let read = CommandSender::from_wire(wire.clone(), &link).unwrap();
            assert_eq!(read.to_wire(), wire);
        }
    }
}
