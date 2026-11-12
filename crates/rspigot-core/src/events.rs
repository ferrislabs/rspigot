use std::net::IpAddr;
use std::sync::Arc;

use crossbeam_channel::Sender;
use futures_channel::oneshot;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{AttributeState, EffectId, EffectInstance};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::{ClientSettings, GameMode, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Rotation, Vec3};

use crate::commands::CommandTree;
use crate::plugins::{FromPlugin, PluginIntro};
use crate::ports::PluginOutbox;
use crate::status::{StatusPing, StatusReply};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            0 => Self::Down,
            1 => Self::Up,
            2 => Self::North,
            3 => Self::South,
            4 => Self::West,
            5 => Self::East,
            _ => return None,
        })
    }

    pub const fn offset(self) -> (i32, i32, i32) {
        match self {
            Self::Down => (0, -1, 0),
            Self::Up => (0, 1, 0),
            Self::North => (0, 0, -1),
            Self::South => (0, 0, 1),
            Self::West => (-1, 0, 0),
            Self::East => (1, 0, 0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigAction {
    Start,
    Cancel,
    Finish,
    DropStack,
    DropItem,
    ReleaseUseItem,
    SwapHands,
    Stab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementAction {
    StartSprinting,
    StopSprinting,
    LeaveBed,
    StartRidingJump,
    StopRidingJump,
    OpenVehicleInventory,
    StartFallFlying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerInput {
    Move {
        position: Option<Vec3>,
        rotation: Option<Rotation>,
        on_ground: bool,
    },
    TeleportConfirm(i32),
    Chat(String),
    Command(String),
    Dig {
        action: DigAction,
        pos: BlockPos,
        face: Direction,
        sequence: i32,
    },
    UseItemOn {
        hand: Hand,
        pos: BlockPos,
        face: Direction,
        cursor: Vec3,
        inside_block: bool,
        sequence: i32,
    },
    UseItem {
        hand: Hand,
        sequence: i32,
        rotation: Rotation,
    },
    HeldSlot(u8),
    CreativeSlot {
        slot: i16,
        stack: Slot,
    },
    Swing(Hand),
    Movement(MovementAction),
    Input(InputState),
    Settings(ClientSettings),
    ChunkBatchReceived {
        chunks_per_tick: f32,
    },
    Loaded,
    Abilities {
        flying: bool,
    },
    Respawn,
    ClientTickEnd,
    Attack(EntityId),
    Interact {
        target: EntityId,
        hand: Hand,
        sneaking: bool,
        location: Vec3,
    },
    PickBlock {
        pos: BlockPos,
        include_data: bool,
    },
    ChangeGameMode(GameMode),
    CommandSuggestion {
        id: i32,
        text: String,
    },
    ContainerClick(ContainerClick),
    ContainerClose(i32),
    ContainerButtonClick {
        window: i32,
        button: i32,
    },
    SetBeacon {
        primary: Option<i32>,
        secondary: Option<i32>,
    },
    Latency(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerClick {
    pub container: i32,
    pub state_id: i32,
    pub slot: i16,
    pub button: i8,
    pub input: ClickInput,
    pub changed: Vec<(i16, HashedSlot)>,
    pub carried: HashedSlot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreLoginVerdict {
    Allowed,
    Refused(String),
}

pub enum Inbound<L> {
    PreLogin {
        name: String,
        id: PlayerId,
        address: IpAddr,
        transferred: bool,
        reply: Sender<PreLoginVerdict>,
    },
    Join {
        profile: GameProfile,
        settings: ClientSettings,
        link: L,
        address: IpAddr,
        session: u64,
    },
    Input {
        player: PlayerId,
        input: PlayerInput,
    },
    Leave {
        player: PlayerId,
        session: u64,
    },
    Console(String),
    StatusPing {
        ping: StatusPing,
        reply: Option<oneshot::Sender<StatusReply>>,
    },
    PluginConnected {
        connection: u64,
        intro: PluginIntro,
        outbox: Box<dyn PluginOutbox>,
    },
    PluginMessage {
        connection: u64,
        message: FromPlugin,
    },
    PluginDisconnected {
        connection: u64,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinInfo {
    pub entity_id: EntityId,
    pub hardcore: bool,
    pub dimension_names: Vec<Identifier>,
    pub max_players: u32,
    pub view_distance: u8,
    pub simulation_distance: u8,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
    pub do_limited_crafting: bool,
    pub spawn: SpawnInfo,
    pub enforces_secure_chat: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnInfo {
    pub dimension: Dimension,
    pub hashed_seed: i64,
    pub game_mode: GameMode,
    pub previous_game_mode: Option<GameMode>,
    pub is_debug: bool,
    pub is_flat: bool,
    pub death_location: Option<(Identifier, BlockPos)>,
    pub portal_cooldown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GameEvent {
    NoRespawnBlockAvailable,
    StartRaining,
    StopRaining,
    ChangeGameMode(GameMode),
    WinGame,
    DemoEvent,
    ArrowHitPlayer,
    RainLevelChange(f32),
    ThunderLevelChange(f32),
    PufferFishSting,
    GuardianElderEffect,
    ImmediateRespawn(bool),
    LimitedCrafting(bool),
    LevelChunksLoadStart,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerListEntry {
    pub profile: GameProfile,
    pub game_mode: GameMode,
    pub latency_ms: i32,
    pub listed: bool,
    pub display_name: Option<Component>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Player,
    Item,
    FallingBlock,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animation {
    SwingMainArm,
    SwingOffHand,
    WakeUp,
    CriticalHit,
    MagicCriticalHit,
}

pub use rspigot_domain::entity::Pose;
pub use rspigot_domain::entity::tracker::{DeltaStep, MoveDelta, Movement, PositionPath};
pub use rspigot_domain::menu::{ClickInput, HashedItem, HashedSlot};

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    Byte(i8),
    VarInt(i32),
    Float(f32),
    Pose(Pose),
    OptionalComponent(Option<Component>),
    Boolean(bool),
    Item(Slot),
    BlockPos(BlockPos),
    Variant(VariantKind, u32),
    Long(i64),
    Direction(u8),
    State(StateKind, i32),
    OptionalUnsignedInt(Option<u32>),
    OptionalEntity(Option<uuid::Uuid>),
    OptionalBlockState(Option<u32>),
    VillagerData {
        kind: u32,
        profession: u32,
        level: i32,
    },
}

pub use rspigot_domain::entity::{StateKind, VariantKind};
pub use rspigot_domain::particle::Particle;

#[derive(Debug, Clone, PartialEq)]
pub struct MetadataEntry {
    pub index: u8,
    pub value: MetadataValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityStatus {
    Death,
    Poof,
    EquipmentBreak(EquipmentSlot),
    Event(i8),
}

pub use rspigot_entities::SoundSource;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub may_fly: bool,
    pub instant_build: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Join(Box<JoinInfo>),
    Respawn {
        spawn: SpawnInfo,
        keep_data: u8,
    },
    Difficulty {
        difficulty: Difficulty,
        locked: bool,
    },
    Abilities(Abilities),
    HeldSlot(u8),
    RecipeSync,
    SpawnPosition {
        dimension: Identifier,
        pos: BlockPos,
        yaw: f32,
        pitch: f32,
    },
    Teleport {
        id: i32,
        position: Vec3,
        rotation: Rotation,
    },
    ChunkBatchStart,
    Chunk {
        dimension: Identifier,
        chunk: Arc<ChunkColumn>,
    },
    ChunkBatchFinished {
        count: u32,
    },
    ForgetChunk(ChunkPos),
    CenterChunk(ChunkPos),
    ViewDistance(u8),
    SimulationDistance(u8),
    BlockUpdate {
        pos: BlockPos,
        state: BlockStateId,
    },
    BlockEntityUpdate {
        pos: BlockPos,
        kind: &'static str,
        data: rspigot_nbt::Compound,
    },
    BlockEvent {
        pos: BlockPos,
        action: u8,
        data: u8,
        block: u16,
    },
    BlockChangedAck(i32),
    BlockDestruction {
        entity: EntityId,
        pos: BlockPos,
        stage: i8,
    },
    GameEvent(GameEvent),
    Time {
        game_time: i64,
        day_time: i64,
        advance_time: bool,
    },
    SystemChat {
        message: Component,
        overlay: bool,
    },
    PlayerChat {
        sender: PlayerId,
        sender_name: Component,
        message: String,
    },
    PlayerInfo(Vec<PlayerListEntry>),
    PlayerInfoGameMode {
        player: PlayerId,
        game_mode: GameMode,
    },
    PlayerInfoRemove(Vec<PlayerId>),
    TabList {
        header: Component,
        footer: Component,
    },
    TitleText(Component),
    SubtitleText(Component),
    TitleTimes {
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    ClearTitles {
        reset: bool,
    },
    SpawnEntity {
        entity: EntityId,
        uuid: uuid::Uuid,
        kind: EntityKind,
        position: Vec3,
        rotation: Rotation,
        head_yaw: f32,
        data: i32,
        velocity: Vec3,
    },
    EntityMoved {
        entity: EntityId,
        from: Vec3,
        to: Vec3,
        rotation: Option<Rotation>,
        on_ground: bool,
    },
    EntityMove {
        entity: EntityId,
        movement: Movement,
    },
    BundleDelimiter,
    EntityHeadRotation {
        entity: EntityId,
        yaw: f32,
    },
    EntityVelocity {
        entity: EntityId,
        velocity: Vec3,
    },
    EntityAnimation {
        entity: EntityId,
        animation: Animation,
    },
    EntityMetadata {
        entity: EntityId,
        entries: Vec<MetadataEntry>,
    },
    EntityEquipment {
        entity: EntityId,
        slots: Vec<(EquipmentSlot, Slot)>,
    },
    RemoveEntities(Vec<EntityId>),
    InventoryContent {
        state_id: i32,
        slots: Vec<Slot>,
        carried: Slot,
    },
    OpenScreen {
        window: i32,
        menu: i32,
        title: Component,
    },
    ContainerContent {
        window: i32,
        state_id: i32,
        slots: Vec<Slot>,
        carried: Slot,
    },
    ContainerSlot {
        window: i32,
        state_id: i32,
        slot: i16,
        stack: Slot,
    },
    ContainerData {
        window: i32,
        id: i16,
        value: i16,
    },
    ContainerClose(i32),
    InventorySlot {
        state_id: i32,
        slot: i16,
        stack: Slot,
    },
    CursorItem(Slot),
    PlayerInventorySlot {
        slot: i32,
        stack: Slot,
    },
    Health {
        health: f32,
        food: i32,
        saturation: f32,
    },
    Experience {
        progress: f32,
        level: i32,
        total: i32,
    },
    Commands(Arc<CommandTree>),
    CommandSuggestions {
        id: i32,
        start: i32,
        length: i32,
        matches: Vec<String>,
    },
    ItemPickup {
        collected: EntityId,
        collector: EntityId,
        amount: i32,
    },
    OperatorLevel {
        entity: EntityId,
        level: u8,
    },
    EffectApplied {
        entity: EntityId,
        effect: EffectInstance,
        blend: bool,
    },
    EffectRemoved {
        entity: EntityId,
        effect: EffectId,
    },
    Attributes {
        entity: EntityId,
        attributes: Vec<AttributeState>,
    },
    DamageEvent {
        entity: EntityId,
        kind: DamageType,
        cause: Option<EntityId>,
        direct: Option<EntityId>,
        position: Option<Vec3>,
    },
    HurtAnimation {
        entity: EntityId,
        yaw: f32,
    },
    CombatKill {
        player: EntityId,
        message: Component,
    },
    EntityStatus {
        entity: EntityId,
        status: EntityStatus,
    },
    Sound {
        sound: &'static str,
        source: SoundSource,
        position: Vec3,
        volume: f32,
        pitch: f32,
        seed: i64,
    },
    Explode {
        center: Vec3,
        radius: f32,
        block_count: i32,
        knockback: Option<Vec3>,
        large: bool,
        play_sound: bool,
    },
    LevelEvent {
        event: i32,
        pos: BlockPos,
        data: i32,
    },
    Particle(Particle),
    Disconnect(Component),
}

impl ClientEvent {
    pub fn system(message: Component) -> Self {
        Self::SystemChat {
            message,
            overlay: false,
        }
    }
}

pub fn stack_count(stack: &Option<ItemStack>) -> u8 {
    stack.as_ref().map_or(0, |s| s.count.get())
}
