use std::time::SystemTime;

use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};

use crate::commands::Weather;
use crate::plugins::ItemSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleSpec {
    pub title: String,
    pub subtitle: Option<String>,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundSpec {
    pub key: String,
    pub volume: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParticleDataSpec {
    None,
    Item(String),
    Block(String),
    Dust { color: u32, scale: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSpec {
    pub key: String,
    pub data: ParticleDataSpec,
    pub position: Vec3,
    pub count: i32,
    pub spread: Vec3,
    pub speed: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSpec {
    pub key: String,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub particles: bool,
    pub icon: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Player {
        player: PlayerId,
        order: PlayerOrder,
    },
    Inventory {
        player: PlayerId,
        order: InventoryOrder,
    },
    Server(ServerOrder),
    World(WorldOrder),
    Block {
        pos: BlockPos,
        order: BlockOrder,
    },
    Entity {
        entity: EntityId,
        order: EntityOrder,
    },
    CustomInventory {
        inventory: u64,
        order: CustomInventoryOrder,
    },
}

impl Order {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Player { order, .. } => order.name(),
            Self::Inventory { order, .. } => order.name(),
            Self::Server(order) => order.name(),
            Self::World(order) => order.name(),
            Self::Block { order, .. } => order.name(),
            Self::Entity { order, .. } => order.name(),
            Self::CustomInventory { order, .. } => order.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOrder {
    Broadcast(String),
    DispatchCommand(String),
    Shutdown,
    Save,
    SetWhitelistEnabled(bool),
    WhitelistAdd(String),
    WhitelistRemove(String),
    OperatorAdd(String),
    OperatorRemove(String),
    BanAdd {
        name: String,
        reason: Option<String>,
        expires: Option<SystemTime>,
    },
    BanRemove(String),
    IpBanAdd {
        address: String,
        reason: Option<String>,
        expires: Option<SystemTime>,
    },
    IpBanRemove(String),
    CreateInventory {
        id: u64,
        rows: i32,
        title: String,
        locked: bool,
    },
}

impl ServerOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Broadcast(_) => "server.broadcast",
            Self::DispatchCommand(_) => "server.dispatch_command",
            Self::Shutdown => "server.shutdown",
            Self::Save => "server.save",
            Self::SetWhitelistEnabled(_) => "server.set_whitelist_enabled",
            Self::WhitelistAdd(_) => "server.whitelist_add",
            Self::WhitelistRemove(_) => "server.whitelist_remove",
            Self::OperatorAdd(_) => "server.operator_add",
            Self::OperatorRemove(_) => "server.operator_remove",
            Self::BanAdd { .. } => "server.ban_add",
            Self::BanRemove(_) => "server.ban_remove",
            Self::IpBanAdd { .. } => "server.ip_ban_add",
            Self::IpBanRemove(_) => "server.ip_ban_remove",
            Self::CreateInventory { .. } => "server.create_inventory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameRule {
    DaylightCycle,
    NaturalRegeneration,
    KeepInventory,
    ShowDeathMessages,
    ImmediateRespawn,
    FallDamage,
    MobDrops,
    MobGriefing,
    MobExplosionDropDecay,
    BlockDrops,
    UniversalAnger,
}

impl GameRule {
    pub const ALL: [Self; 11] = [
        Self::DaylightCycle,
        Self::NaturalRegeneration,
        Self::KeepInventory,
        Self::ShowDeathMessages,
        Self::ImmediateRespawn,
        Self::FallDamage,
        Self::MobDrops,
        Self::MobGriefing,
        Self::MobExplosionDropDecay,
        Self::BlockDrops,
        Self::UniversalAnger,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldOrder {
    SetTime(i64),
    SetWeather(Weather),
    SetDifficulty(Difficulty),
    SetSpawn(BlockPos),
    SetPvp(bool),
    SetGameRule(GameRule, bool),
    Save,
    PlaySound {
        sound: SoundSpec,
        position: Vec3,
    },
    SpawnParticle(ParticleSpec),
    Explode {
        position: Vec3,
        power: f32,
        breaks_blocks: bool,
    },
}

impl WorldOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetTime(_) => "world.set_time",
            Self::SetWeather(_) => "world.set_weather",
            Self::SetDifficulty(_) => "world.set_difficulty",
            Self::SetSpawn(_) => "world.set_spawn",
            Self::SetPvp(_) => "world.set_pvp",
            Self::SetGameRule(..) => "world.set_game_rule",
            Self::Save => "world.save",
            Self::PlaySound { .. } => "world.play_sound",
            Self::SpawnParticle(_) => "world.spawn_particle",
            Self::Explode { .. } => "world.explode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockOrder {
    Set(String),
    SetWith {
        kind: String,
        properties: Vec<(String, String)>,
    },
    BreakNaturally,
}

impl BlockOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Set(_) => "block.set",
            Self::SetWith { .. } => "block.set_with",
            Self::BreakNaturally => "block.break_naturally",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntityOrder {
    Teleport(Vec3),
    Remove,
    Damage(f64),
    Kill,
    AddEffect(EffectSpec),
    RemoveEffect(String),
    ClearEffects,
    SetVelocity(Vec3),
}

impl EntityOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Teleport(_) => "entity.teleport",
            Self::Remove => "entity.remove",
            Self::Damage(_) => "entity.damage",
            Self::Kill => "entity.kill",
            Self::AddEffect(_) => "entity.add_effect",
            Self::RemoveEffect(_) => "entity.remove_effect",
            Self::ClearEffects => "entity.clear_effects",
            Self::SetVelocity(_) => "entity.set_velocity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryOrder {
    Add(ItemSpec),
    Set { slot: i32, item: Option<ItemSpec> },
    Remove { key: String, count: i32 },
    Clear,
    SetHeldSlot(i32),
}

impl InventoryOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Add(_) => "inventory.add",
            Self::Set { .. } => "inventory.set",
            Self::Remove { .. } => "inventory.remove",
            Self::Clear => "inventory.clear",
            Self::SetHeldSlot(_) => "inventory.set_held_slot",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerOrder {
    SendMessage(String),
    SendActionBar(String),
    SendTitle(TitleSpec),
    ResetTitle,
    SetTabList {
        header: String,
        footer: String,
    },
    Teleport(Vec3),
    TeleportFacing(Vec3, Rotation),
    TeleportTo {
        world: String,
        position: Vec3,
        rotation: Option<Rotation>,
    },
    SetGameMode(GameMode),
    Kick(String),
    SetHealth(f32),
    SetAbsorption(f32),
    Damage(f64),
    Kill,
    SetFoodLevel(i32),
    SetSaturation(f32),
    SetExhaustion(f32),
    GiveExp(i32),
    GiveLevels(i32),
    SetLevel(i32),
    SetExp(f32),
    AddEffect(EffectSpec),
    RemoveEffect(String),
    ClearEffects,
    SetFireTicks(i32),
    SetVelocity(Vec3),
    SetAllowFlight(bool),
    SetFlying(bool),
    SetFlySpeed(f32),
    SetWalkSpeed(f32),
    SetOp(bool),
    Respawn,
    PerformCommand(String),
    Chat(String),
    PlaySound(SoundSpec),
    SpawnParticle(ParticleSpec),
    HidePlayer(PlayerId),
    ShowPlayer(PlayerId),
    OpenInventory(u64),
    CloseInventory,
    UpdateInventory,
    Ban {
        reason: Option<String>,
        expires: Option<SystemTime>,
    },
}

impl PlayerOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SendMessage(_) => "player.send_message",
            Self::SendActionBar(_) => "player.send_action_bar",
            Self::SendTitle(_) => "player.send_title",
            Self::ResetTitle => "player.reset_title",
            Self::SetTabList { .. } => "player.set_tab_list",
            Self::Teleport(_) => "player.teleport",
            Self::TeleportFacing(..) => "player.teleport_facing",
            Self::TeleportTo { .. } => "player.teleport_to",
            Self::SetGameMode(_) => "player.set_game_mode",
            Self::Kick(_) => "player.kick",
            Self::SetHealth(_) => "player.set_health",
            Self::SetAbsorption(_) => "player.set_absorption",
            Self::Damage(_) => "player.damage",
            Self::Kill => "player.kill",
            Self::SetFoodLevel(_) => "player.set_food_level",
            Self::SetSaturation(_) => "player.set_saturation",
            Self::SetExhaustion(_) => "player.set_exhaustion",
            Self::GiveExp(_) => "player.give_exp",
            Self::GiveLevels(_) => "player.give_levels",
            Self::SetLevel(_) => "player.set_level",
            Self::SetExp(_) => "player.set_exp",
            Self::AddEffect(_) => "player.add_effect",
            Self::RemoveEffect(_) => "player.remove_effect",
            Self::ClearEffects => "player.clear_effects",
            Self::SetFireTicks(_) => "player.set_fire_ticks",
            Self::SetVelocity(_) => "player.set_velocity",
            Self::SetAllowFlight(_) => "player.set_allow_flight",
            Self::SetFlying(_) => "player.set_flying",
            Self::SetFlySpeed(_) => "player.set_fly_speed",
            Self::SetWalkSpeed(_) => "player.set_walk_speed",
            Self::SetOp(_) => "player.set_op",
            Self::Respawn => "player.respawn",
            Self::PerformCommand(_) => "player.perform_command",
            Self::Chat(_) => "player.chat",
            Self::PlaySound(_) => "player.play_sound",
            Self::SpawnParticle(_) => "player.spawn_particle",
            Self::HidePlayer(_) => "player.hide_player",
            Self::ShowPlayer(_) => "player.show_player",
            Self::OpenInventory(_) => "player.open_inventory",
            Self::CloseInventory => "player.close_inventory",
            Self::UpdateInventory => "player.update_inventory",
            Self::Ban { .. } => "player.ban",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomInventoryOrder {
    Set { slot: i32, item: Option<ItemSpec> },
    Clear,
    Close,
    Discard,
}

impl CustomInventoryOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "custom_inventory.set",
            Self::Clear => "custom_inventory.clear",
            Self::Close => "custom_inventory.close",
            Self::Discard => "custom_inventory.discard",
        }
    }
}
