mod block;
mod color;
mod contract;
mod custom_inventory;
mod diff;
mod effect;
mod entity;
mod error;
mod events;
mod inventory;
mod item;
#[rustfmt::skip]
pub mod keys;
mod link;
mod listener;
mod player;
mod runtime;
mod server;
mod target;
mod text;
mod wire;
mod world;

pub use block::{Block, BlockPos, BlockState};
pub use color::Color;
pub use contract::{
    BlockSnapshot, BookMeta, BoundingBox, ChunkRef, CommandSender, DamageSource, DroppedItem,
    EnchantOffer, HolderRef, Input, InventoryOwner, InventoryRef, InventoryView, LecternData,
    Location, LootContext, MerchantRef, OwnerRef, PluginInfo, SignData, SpawnerData, Trade,
    TrialSpawnerData,
};
pub use custom_inventory::{CustomInventory, CustomInventoryState};
pub use effect::Effect;
pub use entity::{Entities, Entity, EntityState};
pub use error::Error;
pub use events::*;
pub use inventory::{ArmorSlot, Inventory, InventorySlot, InventoryState};
pub use item::{CustomModelData, ItemStack};
pub use keys::{BlockType, EffectType, Enchantment, EntityType, Material, Particle, Sound};
pub use listener::{Access, HandlerSpec, Listeners, Outcome, Plugin, Priority, Verdict};
pub use player::{GameMode, Player, PlayerState, Position, Rotation, Vector};
pub use rspigot_plugin_macros::{Plugin, listeners};
pub use runtime::{run, run_with};
pub use server::{
    BanEntry, BanList, BanListKind, Bans, ByAddress, ByName, IpBanEntry, IpBans, ListedPlayer,
    NameList, NameListKind, NamedPlayers, OperatorKind, Operators, Players, Server, ServerState,
    Whitelist, WhitelistKind,
};
pub use target::{BlockTarget, Target};
pub use text::{ChatColor, Title};
pub use uuid::Uuid;
pub use world::{Difficulty, GameRule, Weather, World, WorldState};

pub mod prelude {
    pub use crate::events::*;
    pub use crate::{
        ArmorSlot, Block, BlockPos, BlockSnapshot, BlockState, BlockTarget, BlockType, ChatColor,
        Color, CommandSender, CustomInventory, CustomInventoryState, CustomModelData, DamageSource,
        Difficulty, Effect, EffectType, Enchantment, Entity, EntityState, EntityType, Error,
        GameMode, GameRule, Inventory, InventoryOwner, InventoryRef, InventorySlot, InventoryState,
        InventoryView, ItemStack, Listeners, Location, Material, Particle, Player, PlayerState,
        Plugin, Position, Priority, Rotation, Server, ServerState, Sound, Target, Title, Vector,
        Weather, World, WorldState, listeners, run, run_with,
    };
}
