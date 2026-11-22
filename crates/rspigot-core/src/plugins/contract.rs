use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use uuid::Uuid;

use crate::plugins::{BlockState, EntityView, ItemSpec, PlayerSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub world: String,
    pub position: Vec3,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSnapshot {
    pub pos: BlockPos,
    pub state: BlockState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryOwner {
    Custom { plugin: String, id: u64 },
    Player(PlayerId),
    Block(BlockPos),
    Entity(EntityId),
    Detached,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryRef {
    pub owner: InventoryOwner,
    pub size: i32,
    pub kind: String,
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryView {
    pub top: InventoryRef,
    pub bottom: InventoryRef,
    pub title: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRef {
    pub world: String,
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DamageSourceView {
    pub kind: String,
    pub causing: Option<EntityView>,
    pub direct: Option<EntityView>,
    pub location: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SenderView {
    Player(PlayerSnapshot),
    Console,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub result: ItemSpec,
    pub ingredients: Vec<ItemSpec>,
    pub uses: i32,
    pub max_uses: i32,
    pub experience_reward: bool,
    pub villager_experience: i32,
    pub price_multiplier: f32,
    pub demand: i32,
    pub special_price: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerLinkEntry {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerLinks {
    pub links: Vec<ServerLinkEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidRef {
    pub id: i32,
    pub world: String,
    pub center: BlockPos,
    pub status: String,
    pub started: bool,
    pub active_ticks: i64,
    pub bad_omen_level: i32,
    pub spawned_groups: i32,
    pub total_groups: i32,
    pub total_health: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Input {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnerData {
    pub pos: BlockPos,
    pub spawned: Option<String>,
    pub delay: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialSpawnerData {
    pub pos: BlockPos,
    pub ominous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LecternData {
    pub pos: BlockPos,
    pub book: Option<ItemSpec>,
    pub page: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DroppedItem {
    pub entity: EntityView,
    pub item: ItemSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignData {
    pub pos: BlockPos,
    pub front: Vec<String>,
    pub back: Vec<String>,
    pub waxed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantOffer {
    pub enchantment: String,
    pub level: i32,
    pub cost: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OwnerRef {
    Player(PlayerSnapshot),
    Entity(EntityView),
    Offline(Uuid),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HolderRef {
    Player(PlayerSnapshot),
    Entity(EntityView),
    Block(BlockPos),
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum MerchantRef {
    Entity(EntityView),
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookMeta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub pages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LootContext {
    pub location: Location,
    pub luck: f32,
    pub killer: Option<EntityView>,
    pub looted: Option<EntityView>,
}
