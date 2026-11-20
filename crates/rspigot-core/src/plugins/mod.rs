mod actions;
mod aims;
mod block;
mod blocks;
mod boss_bars;
mod broadcast;
mod burning;
mod changes;
mod chat;
mod chunks;
pub(crate) mod clicks;
mod commands;
mod contract;
mod damage;
mod decisions;
mod digging;
mod edits;
mod entity;
mod entity_edits;
pub mod events;
pub(crate) mod furnaces;
mod heard;
mod inputs;
mod inventory;
mod items;
mod knockback;
mod levels;
mod lifecycle;
mod menus;
pub mod meta;
mod npcs;
mod objects;
mod orders;
mod patch;
mod ping;
mod placing;
mod player;
mod poses;
mod position;
mod potions;
pub(crate) mod prepare;
mod registry;
mod resolve;
mod scoreboard;
mod server;
mod session;
mod shots;
pub(crate) mod spawns;
pub(crate) mod steps;
mod targets;
mod tiles;
mod traits;
pub(crate) mod views;
mod visibility;
mod vitals;
pub(crate) mod windows;
pub(crate) mod world;

use std::net::IpAddr;

use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rustc_hash::FxHashSet;
use uuid::Uuid;

use crate::admin::{BanEntry, IpBanEntry, ListEntry};
use crate::events::Direction;

pub(crate) use actions::Reply;
pub use actions::{ActionError, ActionErrorKind};
pub(crate) use block::{block_state_of, data_of, material_of, snapshot_of};
pub use boss_bars::{BossBarOrder, BossBarSpec, BossBarView};
pub(crate) use broadcast::Broadcasts;
pub(crate) use burning::Combuster;
pub(crate) use commands::PluginTyping;
pub use contract::{
    BlockSnapshot, BookMeta, BoundingBox, ChunkRef, DamageSourceView, DroppedItem, EnchantOffer,
    HolderRef, Input, InventoryOwner, InventoryRef, InventoryView, LecternData, Location,
    LootContext, MerchantRef, OwnerRef, PluginInfo, SenderView, SignData, SpawnerData, Trade,
    TrialSpawnerData,
};
pub(crate) use damage::Impacts;
pub(crate) use decisions::{Continuation, Decisions, Held, Lane};
pub(crate) use digging::Mining;
pub use edits::{EntityEdit, EquipmentView, LivingDetails};
pub use events::{EventKind, EventPatch, EventPayload};
pub use items::ItemSpec;
pub(crate) use items::{spec_of, stack_of};
pub use menus::MAX_PLUGIN_INVENTORIES;
pub(crate) use menus::{PluginInventories, PluginMenu};
pub use meta::{BlockMetaView, MetaChange, MetaEntries, MetaHolder, MetaOrder, MetaSnapshot};
pub use npcs::{NpcOrder, NpcSpec, NpcView};
pub(crate) use objects::{UseMemo, slot_of};
pub use orders::{
    BlockOrder, ClickSpec, CustomInventoryOrder, DialogButtonSpec, DialogInputSpec, DialogSpec,
    EffectSpec, EntityOrder, EntityTrait, GameRule, InventoryOrder, Order, ParticleDataSpec,
    ParticleSpec, PlayerOrder, RichPartSpec, ServerOrder, SoundSpec, TitleSpec, WorldOrder,
};
pub use patch::{ListDiff, SetDiff, Verdict};
pub(crate) use registry::Registry;
pub use scoreboard::{
    ObjectiveView, ScoreView, ScoreboardOrder, ScoreboardView, TeamOptionSpec, TeamView,
};
pub(crate) use session::{Arrival, Refusal};
pub(crate) use shots::face_of as plugin_face_of;
pub(crate) use shots::face_of;
pub(crate) use spawns::{name_of_entity, view_of_entity};
pub(crate) use steps::{
    AfterHurt, ArrowFollow, AttackFollow, Birth, BlastFollow, BulletFollow, BurnFollow, DeathStep,
    Follow, Holder, MobAttackFollow, Potion, RespawnStep, ScatterStep, ShulkerHop, Spawned,
    SplitStep, Struck, SuggestStep, SweepFollow,
};
pub use targets::MAX_TARGET_DISTANCE;
pub(crate) use targets::{direction_of, ray_end};
pub use tiles::{
    BlockInfo, ContentsView, FurnaceEdit, FurnaceView, SignView, SpawnerEdit, SpawnerView,
    TileData, TileEdit, TileView,
};
pub(crate) use visibility::Visibility;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    Observe,
    Decide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Priority {
    Lowest,
    Low,
    #[default]
    Normal,
    High,
    Highest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerSpec {
    pub id: u32,
    pub event: EventKind,
    pub access: Access,
    pub priority: Priority,
    pub ignore_cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginIntro {
    pub name: String,
    pub version: String,
    pub handlers: Vec<HandlerSpec>,
    pub commands: Vec<CommandSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    Player,
    Item,
    Integer,
    Text,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandArgument {
    pub name: String,
    pub kind: ArgumentKind,
    pub optional: bool,
    pub suggested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: u32,
    pub name: String,
    pub permission: Option<String>,
    pub arguments: Vec<CommandArgument>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentValue {
    Player(PlayerSnapshot),
    Item(String),
    Integer(i64),
    Text(String),
    Flag(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IntroError {
    #[error("the plugin name is empty")]
    EmptyName,
    #[error("handler {0} is declared more than once")]
    DuplicateHandler(u32),
    #[error("handler {0} decides an event that cannot be decided")]
    Undecidable(u32),
    #[error("handler {0} decides and ignores cancelled events, which only an observer can do")]
    ObserverOnly(u32),
    #[error("command {0} is declared more than once")]
    DuplicateCommand(u32),
    #[error("command {0} has an empty name")]
    UnnamedCommand(u32),
    #[error("command {0} has a required argument after an optional one")]
    MisplacedOptional(u32),
}

impl PluginIntro {
    pub fn validate(&self) -> Result<(), IntroError> {
        if self.name.is_empty() {
            return Err(IntroError::EmptyName);
        }
        let mut seen = FxHashSet::default();
        for handler in &self.handlers {
            if !seen.insert(handler.id) {
                return Err(IntroError::DuplicateHandler(handler.id));
            }
            if handler.access == Access::Decide {
                if !handler.event.decidable() {
                    return Err(IntroError::Undecidable(handler.id));
                }
                if handler.ignore_cancelled {
                    return Err(IntroError::ObserverOnly(handler.id));
                }
            }
        }
        let mut commands = FxHashSet::default();
        for command in &self.commands {
            if !commands.insert(command.id) {
                return Err(IntroError::DuplicateCommand(command.id));
            }
            if command.name.is_empty() || command.name.contains(char::is_whitespace) {
                return Err(IntroError::UnnamedCommand(command.id));
            }
            let mut rest = command
                .arguments
                .iter()
                .skip_while(|argument| !argument.optional);
            if rest.any(|argument| !argument.optional) {
                return Err(IntroError::MisplacedOptional(command.id));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub id: PlayerId,
    pub name: String,
    pub entity_id: i32,
    pub world: String,
    pub position: Vec3,
    pub rotation: Rotation,
    pub game_mode: GameMode,
    pub permissions: Vec<String>,
    pub meta: MetaSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub player: PlayerSnapshot,
    pub health: f32,
    pub max_health: f32,
    pub absorption: f32,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub level: i32,
    pub exp: f32,
    pub total_exp: i32,
    pub operator: bool,
    pub allow_flight: bool,
    pub flying: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub on_ground: bool,
    pub fire_ticks: i32,
    pub dead: bool,
    pub effects: Vec<EffectSpec>,
    pub ping: i32,
    pub locale: String,
    pub address: IpAddr,
    pub hidden: Vec<PlayerId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryState {
    pub slots: Vec<Option<ItemSpec>>,
    pub held_slot: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomInventoryState {
    pub items: Vec<Option<ItemSpec>>,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerState {
    pub max_players: u32,
    pub online: u32,
    pub tps: [f64; 3],
    pub whitelist_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldState {
    pub name: String,
    pub uuid: Option<Uuid>,
    pub seed: i64,
    pub day_time: i64,
    pub game_time: i64,
    pub raining: bool,
    pub thundering: bool,
    pub difficulty: Difficulty,
    pub spawn: BlockPos,
    pub pvp: bool,
    pub rules: Vec<(GameRule, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockState {
    pub kind: String,
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityView {
    pub id: EntityId,
    pub uuid: Uuid,
    pub kind: String,
    pub position: Vec3,
    pub player: Option<PlayerSnapshot>,
    pub meta: MetaSnapshot,
    pub marks: EntityMarks,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntityMarks(Option<Box<(Option<String>, Vec<String>)>>);

impl EntityMarks {
    pub fn new(custom_name: Option<String>, tags: Vec<String>) -> Self {
        if custom_name.is_none() && tags.is_empty() {
            Self(None)
        } else {
            Self(Some(Box::new((custom_name, tags))))
        }
    }

    pub fn of(base: &rspigot_domain::entity::EntityBase) -> Self {
        Self::new(
            base.custom_name().map(|name| name.to_legacy()),
            base.tags.clone(),
        )
    }

    pub fn custom_name(&self) -> Option<&str> {
        self.0.as_ref().and_then(|marks| marks.0.as_deref())
    }

    pub fn tags(&self) -> &[String] {
        self.0.as_ref().map_or(&[], |marks| &marks.1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDetails {
    pub entity: EntityView,
    pub health: Option<f32>,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub custom_name: Option<String>,
    pub fire_ticks: i32,
    pub flags: EntityFlags,
    pub freeze_ticks: i32,
    pub air: i32,
    pub fall_distance: f32,
    pub rotation: Rotation,
    pub ticks_lived: i32,
    pub tags: Vec<String>,
    pub passengers: Vec<EntityId>,
    pub vehicle: Option<EntityId>,
    pub living: Option<Box<LivingDetails>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityFlags {
    pub custom_name_visible: bool,
    pub glowing: bool,
    pub invisible: bool,
    pub silent: bool,
    pub gravity: bool,
    pub invulnerable: bool,
    pub persistent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TargetHit {
    Block {
        pos: BlockPos,
        face: Direction,
        kind: String,
    },
    Player(PlayerSnapshot),
    Entity(EntityView),
    Npc(NpcView),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Decide,
    Settled,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Player(PlayerState),
    Players(Vec<PlayerSnapshot>),
    Inventory(InventoryState),
    Server(ServerState),
    Names(Vec<ListEntry>),
    Bans(Vec<BanEntry>),
    IpBans(Vec<IpBanEntry>),
    World(WorldState),
    Block(BlockState),
    Tile(TileView),
    BlockInfo(BlockInfo),
    Entities(Vec<EntityView>),
    Entity(EntityDetails),
    AttributeValue(f64),
    Spawned(EntityView),
    Scoreboard(ScoreboardView),
    ScoreboardCreated(u32),
    BossBarCreated(u64),
    BossBar(BossBarView),
    BossBars(Vec<BossBarView>),
    Npc(NpcView),
    Npcs(Vec<NpcView>),
    MetaEntries(Vec<(String, rspigot_domain::meta::MetaValue)>),
    BlockPositions(Vec<BlockPos>),
    Target(Option<TargetHit>),
    CustomInventory(CustomInventoryState),
    Refused(String),
    WorldCreated(String),
    Done,
    Failed(ActionError),
    NotFound,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ToPlugin {
    Accepted,
    Event {
        id: u64,
        handlers: Vec<u32>,
        phase: Phase,
        cancelled: bool,
        payload: EventPayload,
        blocks: Vec<BlockMetaView>,
    },
    Answer {
        request: u64,
        answer: Answer,
    },
    Warning(String),
    Replaced,
    Invocation {
        command: u32,
        sender: PlayerSnapshot,
        arguments: Vec<ArgumentValue>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FromPlugin {
    Decision { event: u64, verdicts: Vec<Verdict> },
    Order(Order),
    Reaction { event: u64, order: Order },
    Question { request: u64, question: Question },
    Action { request: u64, order: Order },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    Player(PlayerId),
    Players,
    PlayerNamed(String),
    Inventory(PlayerId),
    TargetBlock {
        player: PlayerId,
        max_distance: f64,
    },
    TargetEntity {
        player: PlayerId,
        max_distance: f64,
    },
    Server,
    Whitelist,
    Operators,
    Bans,
    IpBans,
    World,
    InWorld {
        world: rspigot_domain::Identifier,
        question: Box<Question>,
    },
    Block(BlockPos),
    Tile(BlockPos),
    BlockInfo(BlockPos),
    Entities {
        near: Option<(Vec3, f64)>,
        kind: Option<String>,
    },
    Entity(EntityId),
    AttributeValue {
        entity: EntityId,
        attribute: String,
    },
    Spawn {
        kind: String,
        position: Vec3,
    },
    SpawnWith {
        kind: String,
        position: Vec3,
        block: Option<String>,
        velocity: Option<Vec3>,
    },
    DropItem {
        position: Vec3,
        item: ItemSpec,
    },
    CustomInventory(u64),
    Scoreboard(u32),
    NewScoreboard,
    CreateBossBar(BossBarSpec),
    BossBar(u64),
    BossBars,
    CreateNpc(NpcSpec),
    NpcByKey(String),
    Npcs,
    Npc(EntityId),
    Meta {
        holder: MetaHolder,
        key: Option<String>,
        volatile: bool,
    },
    FindBlocks {
        key: String,
        value: Option<rspigot_domain::meta::MetaValue>,
        volatile: bool,
    },
    EntitiesWithMeta {
        key: String,
        volatile: bool,
    },
    PlayersWithMeta {
        key: String,
        volatile: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler(id: u32, event: EventKind, access: Access) -> HandlerSpec {
        HandlerSpec {
            id,
            event,
            access,
            priority: Priority::Normal,
            ignore_cancelled: false,
        }
    }

    fn intro(handlers: Vec<HandlerSpec>) -> PluginIntro {
        PluginIntro {
            name: "welcome".to_owned(),
            version: "1.0.0".to_owned(),
            commands: Vec::new(),
            handlers,
        }
    }

    #[test]
    fn an_introduction_with_duplicate_ids_or_an_undecidable_decision_is_invalid() {
        let observer = handler(1, EventKind::PlayerJoin, Access::Observe);
        let decider = handler(2, EventKind::PlayerMove, Access::Decide);
        assert_eq!(intro(vec![observer, decider]).validate(), Ok(()));
        assert_eq!(intro(Vec::new()).validate(), Ok(()));

        let twin = handler(1, EventKind::PlayerLeave, Access::Observe);
        assert_eq!(
            intro(vec![observer, twin]).validate(),
            Err(IntroError::DuplicateHandler(1))
        );

        let join_decider = handler(3, EventKind::InventoryClose, Access::Decide);
        assert_eq!(
            intro(vec![observer, join_decider]).validate(),
            Err(IntroError::Undecidable(3))
        );

        let mut filtered_decider = decider;
        filtered_decider.ignore_cancelled = true;
        assert_eq!(
            intro(vec![observer, filtered_decider]).validate(),
            Err(IntroError::ObserverOnly(2))
        );

        let mut filtered_observer = observer;
        filtered_observer.ignore_cancelled = true;
        assert_eq!(intro(vec![filtered_observer]).validate(), Ok(()));

        let mut nameless = intro(vec![observer]);
        nameless.name.clear();
        assert_eq!(nameless.validate(), Err(IntroError::EmptyName));
    }

    #[test]
    fn an_event_is_decidable_when_it_can_be_cancelled_or_changed() {
        assert!(EventKind::PlayerMove.decidable());
        assert!(EventKind::PlayerJoin.decidable());
        assert!(!EventKind::PlayerJoin.cancellable());
        assert!(EventKind::InventoryClick.decidable());
        assert!(EventKind::PlayerDeath.decidable());
        assert!(!EventKind::PlayerDeath.cancellable());
        assert!(!EventKind::InventoryClose.decidable());
        assert!(!EventKind::WorldLoad.decidable());
    }

    #[test]
    fn a_family_follows_the_bukkit_handler_lists() {
        assert_eq!(
            EventKind::EntityDamageByEntity.receivers(),
            [EventKind::EntityDamage, EventKind::EntityDamageByEntity]
        );
        assert_eq!(
            EventKind::EntityDamage.receivers(),
            [EventKind::EntityDamage]
        );
        assert_eq!(
            EventKind::PlayerDeath.receivers(),
            [EventKind::EntityDeath, EventKind::PlayerDeath]
        );
        assert_eq!(
            EventKind::PlayerTeleport.receivers(),
            [EventKind::PlayerTeleport]
        );
        assert_eq!(
            EventKind::CraftItem.receivers(),
            [EventKind::CraftItem, EventKind::InventoryClick]
        );
    }

    #[test]
    fn the_entity_families_follow_the_bukkit_handler_lists() {
        assert_eq!(
            EventKind::CreatureSpawn.receivers(),
            [EventKind::CreatureSpawn, EventKind::EntitySpawn]
        );
        assert!(
            EventKind::ItemSpawn
                .receivers()
                .contains(&EventKind::EntitySpawn)
        );
        assert!(
            EventKind::ProjectileLaunch
                .receivers()
                .contains(&EventKind::EntitySpawn)
        );
        assert!(
            EventKind::EntityDamageByBlock
                .receivers()
                .contains(&EventKind::EntityDamage)
        );
        assert!(
            EventKind::EntityCombustByEntity
                .receivers()
                .contains(&EventKind::EntityCombust)
        );
        assert!(
            EventKind::EntityCombustByBlock
                .receivers()
                .contains(&EventKind::EntityCombust)
        );
        assert!(
            EventKind::EntityKnockbackByEntity
                .receivers()
                .contains(&EventKind::EntityKnockback)
        );
        assert!(
            EventKind::EntityTargetLivingEntity
                .receivers()
                .contains(&EventKind::EntityTarget)
        );
        for blocked in [
            EventKind::PotionSplash,
            EventKind::LingeringPotionSplash,
            EventKind::ExpBottle,
        ] {
            assert!(!blocked.receivers().contains(&EventKind::ProjectileHit));
        }
    }

    #[test]
    fn an_event_kind_has_a_key_for_plugins_yml() {
        assert_eq!(EventKind::PlayerLeave.key(), "player_leave");
        assert_eq!(
            EventKind::from_key("player_move"),
            Some(EventKind::PlayerMove)
        );
        assert_eq!(
            EventKind::from_key("entity_damage_by_entity"),
            Some(EventKind::EntityDamageByEntity)
        );
        assert_eq!(EventKind::from_key("player_fly"), None);
    }

    #[test]
    fn priorities_run_from_lowest_to_highest_and_default_to_normal() {
        assert!(Priority::Lowest < Priority::Low);
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Highest);
        assert_eq!(Priority::default(), Priority::Normal);
    }
}

#[cfg(test)]
mod world_tp_tests;
