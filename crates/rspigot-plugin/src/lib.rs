mod advancements;
mod attribute;
mod block;
mod color;
mod commands;
mod component;
#[doc(hidden)]
pub mod compose;
mod config;
mod connection;
mod contract;
mod custom_inventory;
mod dialog;
mod diff;
mod effect;
mod entity;
mod entity_groups;
mod error;
mod events;
#[rustfmt::skip]
mod generated;
mod inventory;
mod item;
mod item_api;
mod item_meta;
mod location;
mod meta;
#[rustfmt::skip]
pub mod keys;
mod boss_bar;
mod link;
mod listener;
mod npc;
mod payload;
mod permissions;
mod player;
mod recipes;
mod rich;
mod runtime;
mod scoreboard;
mod server;
mod target;
mod text;
mod tile;
mod typed;
mod wire;
mod world;

#[cfg(test)]
mod sdk_chain;

pub use advancements::AdvancementProgress;
pub use attribute::{Attribute, EntityAttributesHandle};
pub use block::{Block, BlockPos, BlockState};
pub use boss_bar::{BarColor, BarFlag, BarStyle, BossBar, BossBarState, BossBars};
pub use color::Color;
pub use commands::{
    Argument, ArgumentKind, Arguments, ChoiceArgument, CommandArgument, CommandSpec, Context,
    GreedyText, Invocation, Typing,
};
pub use component::ComponentValue;
pub use compose::CommandModule;
pub use config::{Config, ConfigError, ConfigValue};
pub use contract::{
    BlockSnapshot, BookMeta, BoundingBox, ChunkRef, CommandSender, DamageSource, DroppedItem,
    EnchantOffer, HolderRef, Input, InventoryOwner, InventoryRef, InventoryView, LecternData,
    Location, LootContext, MerchantRef, OwnerRef, PluginInfo, SignData, SpawnerData, Trade,
    TrialSpawnerData,
};
pub use custom_inventory::{CustomInventory, CustomInventoryState};
pub use dialog::{Dialog, DialogButton, DialogInput};
pub use effect::Effect;
pub use entity::{Entities, Entity, EntityState};
pub use entity_groups::{
    EntityAgeHandle, EntityAiHandle, EntityEquipmentHandle, EntityFlagsHandle, EntityLeashHandle,
    EntityNameHandle, EntityOwnerHandle, EntityPassengersHandle, EntityTagsHandle,
    EntityVitalsHandle, EquipmentState, LivingState,
};
pub use error::{ActionError, ActionErrorKind, Error};
pub use events::*;
pub use generated::*;
pub use inventory::{ArmorSlot, InventorySlot, InventoryState};
pub use item::{CustomModelData, ItemStack};
pub use item_api::ItemMeta;
pub use item_meta::{
    AttributeModifier, BannerLayer, Equippable, FireworkExplosion, FireworkShape, Fireworks,
    ItemFlag, LodestoneTarget, LodestoneTracker, Operation, PotionContents, PotionEffect, Profile,
    ProfileProperty, Rarity, SlotGroup, Tool, ToolRule, Trim, UseCooldown, WrittenBook,
};
pub use keys::{BlockType, EffectType, Enchantment, EntityType, Material, Particle, Sound};
pub use listener::{Access, HandlerSpec, Listeners, Outcome, Plugin, Priority, Verdict};
pub use location::PlayerLocationHandle;
pub use meta::{MetaHandle, MetaValue, WorldMetaHandle};
pub use npc::{DEFAULT_SKIN_LAYERS, Npc, NpcPose, NpcSkin, NpcSlot, NpcSpec, NpcState, Npcs};
pub use payload::{Ban, BanTerms, Facing, ItemRemoval, ParticleBurst, SlotItem, SoundCue, TabList};
pub use permissions::{PermissionGroup, PlayerPermissionsHandle, ServerPermissionsHandle};
pub use player::{GameMode, Player, PlayerState, Position, Rotation, Vector};
pub use recipes::{CookingKind, Ingredient, Recipe, RecipeCategory, RecipeInfo, RecipeKind};
pub use rich::{Click, Part, RichMessage};
pub use rspigot_plugin_macros::{Module, Plugin, command, listeners};
pub use runtime::{run, run_with};
pub use scoreboard::{
    CollisionRule, Criteria, DisplaySlot, Objective, ObjectiveState, RenderType, Score, ScoreEntry,
    Scoreboard, ScoreboardManager, ScoreboardState, Team, TeamColor, TeamState, Visibility,
};
pub use server::{BanEntry, IpBanEntry, ListedPlayer, Server, ServerState};
pub use target::{BlockTarget, Target};
pub use text::{ChatColor, Title};
pub use tile::{
    BeaconState, BlockBannerHandle, BlockBeaconHandle, BlockCommandHandle, BlockData,
    BlockDataHandle, BlockEndGatewayHandle, BlockFurnaceHandle, BlockInventoryHandle,
    BlockJukeboxHandle, BlockLecternHandle, BlockPotHandle, BlockSignHandle, BlockSkullHandle,
    BlockSpawnerHandle, CommandBlockState, FurnaceState, SignContents, SkullOwner, SpawnerState,
};
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
        Listeners, Location, Material, MetaHandle, MetaValue, Module, Particle, Player,
        PlayerState, Plugin, Position, Priority, Rotation, Server, ServerState, Sound, SoundCue,
        Target, Title, Vector, Weather, World, WorldState, command, listeners, run, run_with,
    };
    pub use crate::{
        AttributeModifier, BannerLayer, ComponentValue, Equippable, FireworkExplosion,
        FireworkShape, Fireworks, ItemFlag, ItemMeta, LodestoneTarget, LodestoneTracker, Operation,
        PotionContents, PotionEffect, Profile, ProfileProperty, Rarity, SlotGroup, SlotItem, Tool,
        ToolRule, Trim, UseCooldown, WrittenBook,
    };
    pub use crate::{BarColor, BarFlag, BarStyle, BossBar, BossBars};
    pub use crate::{Click, Part, RichMessage};
    pub use crate::{Config, ConfigValue};
    pub use crate::{CookingKind, Ingredient, Recipe, RecipeCategory, RecipeInfo, RecipeKind};
    pub use crate::{Criteria, DisplaySlot, Objective, Scoreboard, ScoreboardManager, Team};
    pub use crate::{Dialog, DialogButton, DialogInput};
    pub use crate::{Npc, NpcPose, NpcSkin, NpcSlot, NpcSpec, Npcs};
}
