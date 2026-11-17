mod attribute;
mod block;
mod color;
mod commands;
mod connection;
mod contract;
mod custom_inventory;
mod diff;
mod effect;
mod entity;
mod error;
mod events;
#[rustfmt::skip]
mod generated;
mod inventory;
mod item;
mod location;
#[rustfmt::skip]
pub mod keys;
mod boss_bar;
mod link;
mod listener;
mod payload;
mod permissions;
mod player;
mod runtime;
mod scoreboard;
mod server;
mod target;
mod text;
mod typed;
mod wire;
mod world;

#[cfg(test)]
mod sdk_chain;

pub use attribute::{Attribute, EntityAttributesHandle};
pub use block::{Block, BlockPos, BlockState};
pub use boss_bar::{BarColor, BarFlag, BarStyle, BossBar, BossBarState, BossBars};
pub use color::Color;
pub use commands::{
    Argument, ArgumentKind, Arguments, CommandArgument, CommandSpec, Context, Invocation, Typing,
};
pub use contract::{
    BlockSnapshot, BookMeta, BoundingBox, ChunkRef, CommandSender, DamageSource, DroppedItem,
    EnchantOffer, HolderRef, Input, InventoryOwner, InventoryRef, InventoryView, LecternData,
    Location, LootContext, MerchantRef, OwnerRef, PluginInfo, SignData, SpawnerData, Trade,
    TrialSpawnerData,
};
pub use custom_inventory::{CustomInventory, CustomInventoryState};
pub use effect::Effect;
pub use entity::{Entities, Entity, EntityState};
pub use error::{ActionError, ActionErrorKind, Error};
pub use events::*;
pub use generated::*;
pub use inventory::{ArmorSlot, InventorySlot, InventoryState};
pub use item::{CustomModelData, ItemStack};
pub use keys::{BlockType, EffectType, Enchantment, EntityType, Material, Particle, Sound};
pub use listener::{Access, HandlerSpec, Listeners, Outcome, Plugin, Priority, Verdict};
pub use location::PlayerLocationHandle;
pub use payload::{Ban, BanTerms, Facing, ItemRemoval, ParticleBurst, SlotItem, SoundCue, TabList};
pub use permissions::PlayerPermissionsHandle;
pub use player::{GameMode, Player, PlayerState, Position, Rotation, Vector};
pub use rspigot_plugin_macros::{Plugin, command, listeners};
pub use runtime::{run, run_with};
pub use scoreboard::{
    CollisionRule, Criteria, DisplaySlot, Objective, ObjectiveState, RenderType, Score, ScoreEntry,
    Scoreboard, ScoreboardManager, ScoreboardState, Team, TeamColor, TeamState, Visibility,
};
pub use server::{BanEntry, IpBanEntry, ListedPlayer, Server, ServerState};
pub use target::{BlockTarget, Target};
pub use text::{ChatColor, Title};
pub use typed::{
    Cat, Creeper, Profession, Rabbit, RabbitVariant, Sheep, Slime, Villager, Wolf, Zombie,
};
pub use uuid::Uuid;
pub use world::{
    Difficulty, Dimension, Explosion, GameRule, GameRuleSetting, Generator, Layer, Weather, World,
    WorldOptions, WorldState,
};

pub mod prelude {
    pub use crate::events::*;
    pub use crate::{
        ActionError, ActionErrorKind, ArmorSlot, Attribute, Block, BlockPos, BlockSnapshot,
        BlockState, BlockTarget, BlockType, ChatColor, Color, CommandSender, Context,
        CustomInventory, CustomInventoryState, CustomModelData, DamageSource, Difficulty, Effect,
        EffectType, Enchantment, Entity, EntityState, EntityType, Error, GameMode, GameRule,
        InventoryOwner, InventoryRef, InventorySlot, InventoryState, InventoryView, ItemStack,
        Listeners, Location, Material, Particle, Player, PlayerState, Plugin, Position, Priority,
        Rotation, Server, ServerState, Sound, SoundCue, Target, Title, Vector, Weather, World,
        WorldState, command, listeners, run, run_with,
    };
    pub use crate::{BarColor, BarFlag, BarStyle, BossBar, BossBars};
    pub use crate::{Criteria, DisplaySlot, Objective, Scoreboard, ScoreboardManager, Team};
}
