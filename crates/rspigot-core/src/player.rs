use std::net::IpAddr;

use rspigot_domain::effect::ActiveEffects;
use rspigot_domain::item::Inventory;
use rspigot_domain::menu::InventoryMenu;
use rspigot_domain::player::{ClientSettings, GameMode, GameProfile, PlayerId};
use rspigot_domain::{EntityId, Rotation, Vec3};
use rustc_hash::FxHashSet;

use crate::bow::UsingItem;
use crate::containers::OpenMenu;
use crate::drops::DropLimits;
use crate::events::Abilities;
use crate::health::Vitals;
use crate::plugins::PluginMenu;
use crate::streaming::ChunkStreamer;

pub struct Player<L> {
    pub id: PlayerId,
    pub profile: GameProfile,
    pub link: L,
    pub address: IpAddr,
    pub session: u64,
    pub entity_id: EntityId,
    pub position: Vec3,
    pub rotation: Rotation,
    pub on_ground: bool,
    pub dim: crate::world_state::DimensionKey,
    pub game_mode: GameMode,
    pub previous_game_mode: Option<GameMode>,
    pub inventory: Inventory,
    pub inventory_state: i32,
    pub menu: InventoryMenu,
    pub(crate) drops: DropLimits,
    pub settings: ClientSettings,
    pub streamer: ChunkStreamer,
    pub pending_teleport: Option<i32>,
    pub teleport_counter: i32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub flying: bool,
    pub allow_flight: Option<bool>,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub latency: i32,
    pub tracked: FxHashSet<EntityId>,
    pub loaded: bool,
    pub operator: bool,
    pub effects: ActiveEffects,
    pub vitals: Vitals,
    pub known_movement: Vec3,
    pub portal_cooldown: i32,
    pub moved_this_tick: bool,
    pub using_item: Option<UsingItem>,
    pub(crate) open_menu: Option<OpenMenu>,
    pub(crate) plugin_menu: Option<PluginMenu>,
    pub(crate) window_counter: u8,
    pub(crate) inventory_ids: [u64; 2],
    pub ender_items: Vec<rspigot_domain::item::Slot>,
    pub kick_leave_message: Option<String>,
    pub last_input: crate::events::InputState,
    pub(crate) mining: Option<crate::plugins::Mining>,
    pub(crate) delayed_destroy: Option<crate::plugins::Mining>,
    pub(crate) use_memo: Option<crate::plugins::UseMemo>,
}

impl<L> Player<L> {
    pub fn name(&self) -> &str {
        &self.profile.name
    }

    pub fn abilities(&self) -> Abilities {
        Abilities {
            invulnerable: matches!(self.game_mode, GameMode::Creative | GameMode::Spectator),
            flying: self.flying || self.game_mode == GameMode::Spectator,
            may_fly: self.allow_flight.unwrap_or(self.game_mode.can_fly()),
            instant_build: self.game_mode == GameMode::Creative,
            fly_speed: self.fly_speed,
            walk_speed: self.walk_speed,
        }
    }

    pub fn next_teleport_id(&mut self) -> i32 {
        self.teleport_counter = self.teleport_counter.wrapping_add(1);
        self.pending_teleport = Some(self.teleport_counter);
        self.teleport_counter
    }

    pub fn next_inventory_state(&mut self) -> i32 {
        self.inventory_state = self.inventory_state.wrapping_add(1) & 0x7FFF;
        self.inventory_state
    }

    pub fn shared_flags(&self) -> i8 {
        let mut flags = 0i8;
        if self.vitals.fire_ticks > 0
            && self.game_mode != rspigot_domain::player::GameMode::Spectator
        {
            flags |= 0x01;
        }
        if self.sneaking {
            flags |= 0x02;
        }
        if self.sprinting {
            flags |= 0x08;
        }
        if self.effects.has("minecraft:invisibility") {
            flags |= 0x20;
        }
        if self.effects.has("minecraft:glowing") {
            flags |= 0x40;
        }
        flags
    }

    pub fn eye_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x,
            self.position.y + self.eye_height(),
            self.position.z,
        )
    }

    pub fn eye_height(&self) -> f64 {
        if self.sneaking { 1.27 } else { 1.62 }
    }

    pub fn height(&self) -> f64 {
        if self.sneaking { 1.5 } else { 1.8 }
    }
}
