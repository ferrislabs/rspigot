use crate::codec::{Decode, DecodeError, Encode, Position, decode_exact, decode_string};
use uuid::Uuid;

use crate::common::{
    ClientInformation, CookieResponse, CustomClickAction, Identifier, KeepAlive, Pong,
    ResourcePackResponse, ServerboundCustomPayload,
};
use crate::ids::play::serverbound as sb;
use crate::play::chat::{
    ArgumentSignature, ChatSession, LastSeenUpdate, MAX_ARGUMENT_SIGNATURES, MAX_CHAT_CHARS,
    MessageSignature,
};
use crate::play::gaps::{
    DebugSubscriptionRequest, JigsawGenerate, SelectBundleItem, SetJigsawBlock, SetStructureBlock,
    SetTestBlock,
};
use crate::play::slot::{HashedStack, ItemStack, Untrusted};
use crate::play::types::{Difficulty, Direction, GameMode, Hand, LpVec3, Vec3};
use crate::play::wire::{
    FieldCodec, List, Opt, Plain, Utf, Var, read_var, unit_packet, wire_enum, wire_struct,
    write_var,
};
use crate::status::PingRequest;

pub const MAX_COMMAND_SUGGESTION_CHARS: usize = 32_500;
pub const MAX_BOOK_PAGES: usize = 100;
pub const MAX_BOOK_PAGE_CHARS: usize = 1024;
pub const MAX_BOOK_TITLE_CHARS: usize = 32;
pub const MAX_SIGN_LINE_CHARS: usize = 384;
pub const SIGN_LINES: usize = 4;
pub const MAX_CHANGED_SLOTS: usize = 128;
pub const MAX_GAME_RULE_ENTRIES: usize = 256;

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct AcceptTeleportation = sb::ACCEPT_TELEPORTATION {
        pub teleport_id: i32 as Var,
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub y_rot: f32,
        pub x_rot: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Attack = sb::ATTACK {
        pub entity_id: i32 as Var,
    }
}

unit_packet! {
    pub struct Punch = sb::PUNCH;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Chat = sb::CHAT {
        pub message: String as Utf<MAX_CHAT_CHARS>,
        pub timestamp: i64,
        pub salt: i64,
        pub signature: Option<MessageSignature> as Opt<Plain>,
        pub last_seen: LastSeenUpdate,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChatAck = sb::CHAT_ACK {
        pub offset: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChatCommand = sb::CHAT_COMMAND {
        pub command: String,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChatCommandSigned = sb::CHAT_COMMAND_SIGNED {
        pub command: String,
        pub timestamp: i64,
        pub salt: i64,
        pub argument_signatures: Vec<ArgumentSignature> as List<Plain, MAX_ARGUMENT_SIGNATURES>,
        pub last_seen: LastSeenUpdate,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChatSessionUpdate = sb::CHAT_SESSION_UPDATE {
        pub session: ChatSession,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ChunkBatchReceived = sb::CHUNK_BATCH_RECEIVED {
        pub desired_chunks_per_tick: f32,
    }
}

wire_enum! {
    pub enum ClientCommandAction: Strict { PerformRespawn, RequestStats, RequestGameRuleValues }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ClientCommand = sb::CLIENT_COMMAND {
        pub action: ClientCommandAction,
    }
}

unit_packet! {
    pub struct ClientTickEnd = sb::CLIENT_TICK_END;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CommandSuggestion = sb::COMMAND_SUGGESTION {
        pub id: i32 as Var,
        pub command: String as Utf<MAX_COMMAND_SUGGESTION_CHARS>,
    }
}

unit_packet! {
    pub struct ConfigurationAcknowledged = sb::CONFIGURATION_ACKNOWLEDGED;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContainerButtonClick = sb::CONTAINER_BUTTON_CLICK {
        pub container_id: i32 as Var,
        pub button_id: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContainerSlotStateChanged = sb::CONTAINER_SLOT_STATE_CHANGED {
        pub slot_id: i32 as Var,
        pub container_id: i32 as Var,
        pub new_state: bool,
    }
}

wire_enum! {
    pub enum ContainerInput: Zero { Pickup, QuickMove, Swap, Clone, Throw, QuickCraft, PickupAll }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ChangedSlot {
        pub slot: i16,
        pub item: HashedStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ContainerClick = sb::CONTAINER_CLICK {
        pub container_id: i32 as Var,
        pub state_id: i32 as Var,
        pub slot: i16,
        pub button: i8,
        pub input: ContainerInput,
        pub changed_slots: Vec<ChangedSlot> as List<Plain, MAX_CHANGED_SLOTS>,
        pub carried: HashedStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContainerClose = sb::CONTAINER_CLOSE {
        pub container_id: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EditBook = sb::EDIT_BOOK {
        pub slot: i32 as Var,
        pub pages: Vec<String> as List<Utf<MAX_BOOK_PAGE_CHARS>, MAX_BOOK_PAGES>,
        pub title: Option<String> as Opt<Utf<MAX_BOOK_TITLE_CHARS>>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Interact = sb::INTERACT {
        pub entity_id: i32 as Var,
        pub hand: Hand,
        pub location: LpVec3,
        pub using_secondary_action: bool,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MoveFlags {
    pub on_ground: bool,
    pub horizontal_collision: bool,
}

impl MoveFlags {
    pub const ON_GROUND: u8 = 0x01;
    pub const HORIZONTAL_COLLISION: u8 = 0x02;
}

impl Encode for MoveFlags {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut flags = 0;
        if self.on_ground {
            flags |= Self::ON_GROUND;
        }
        if self.horizontal_collision {
            flags |= Self::HORIZONTAL_COLLISION;
        }
        out.push(flags);
    }
}

impl Decode for MoveFlags {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let flags = u8::decode(input)?;
        Ok(Self {
            on_ground: flags & Self::ON_GROUND != 0,
            horizontal_collision: flags & Self::HORIZONTAL_COLLISION != 0,
        })
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MovePlayerPos = sb::MOVE_PLAYER_POS {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub flags: MoveFlags,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MovePlayerPosRot = sb::MOVE_PLAYER_POS_ROT {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub y_rot: f32,
        pub x_rot: f32,
        pub flags: MoveFlags,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MovePlayerRot = sb::MOVE_PLAYER_ROT {
        pub y_rot: f32,
        pub x_rot: f32,
        pub flags: MoveFlags,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MovePlayerStatusOnly = sb::MOVE_PLAYER_STATUS_ONLY {
        pub flags: MoveFlags,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct MoveVehicle = sb::MOVE_VEHICLE {
        pub position: Vec3,
        pub y_rot: f32,
        pub x_rot: f32,
        pub on_ground: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PaddleBoat = sb::PADDLE_BOAT {
        pub left: bool,
        pub right: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PickItemFromBlock = sb::PICK_ITEM_FROM_BLOCK {
        pub pos: Position,
        pub include_data: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PickItemFromEntity = sb::PICK_ITEM_FROM_ENTITY {
        pub entity_id: i32 as Var,
        pub include_data: bool,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerAbilities {
    pub flying: bool,
}

impl PlayerAbilities {
    pub const FLAG_FLYING: u8 = 0x02;
}

impl Encode for PlayerAbilities {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(if self.flying { Self::FLAG_FLYING } else { 0 });
    }
}

impl Decode for PlayerAbilities {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            flying: u8::decode(input)? & Self::FLAG_FLYING != 0,
        })
    }
}

impl crate::Packet for PlayerAbilities {
    const ID: i32 = sb::PLAYER_ABILITIES;
}

wire_enum! {
    pub enum PlayerActionKind: Strict {
        StartDestroyBlock,
        ChangeDestroyDirection,
        AbortDestroyBlock,
        StopDestroyBlock,
        DropAllItems,
        DropItem,
        ReleaseUseItem,
        SwapItemWithOffhand,
        Stab,
    }
}

pub struct DirectionByte;

impl FieldCodec<Direction> for DirectionByte {
    fn write(value: &Direction, out: &mut Vec<u8>) {
        out.push(value.id() as u8);
    }

    fn read(input: &mut &[u8]) -> Result<Direction, DecodeError> {
        let raw = i32::from(u8::decode(input)?);
        Ok(Direction::from_id(raw % 6).unwrap_or(Direction::Down))
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PlayerAction = sb::PLAYER_ACTION {
        pub action: PlayerActionKind,
        pub pos: Position,
        pub direction: Direction as DirectionByte,
        pub sequence: i32 as Var,
    }
}

wire_enum! {
    pub enum PlayerCommandAction: Strict {
        StopSleeping,
        StartSprinting,
        StopSprinting,
        StartRidingJump,
        StopRidingJump,
        OpenInventory,
        StartFallFlying,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PlayerCommand = sb::PLAYER_COMMAND {
        pub entity_id: i32 as Var,
        pub action: PlayerCommandAction,
        pub data: i32 as Var,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerInput {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}

impl PlayerInput {
    pub const FLAG_FORWARD: u8 = 0x01;
    pub const FLAG_BACKWARD: u8 = 0x02;
    pub const FLAG_LEFT: u8 = 0x04;
    pub const FLAG_RIGHT: u8 = 0x08;
    pub const FLAG_JUMP: u8 = 0x10;
    pub const FLAG_SHIFT: u8 = 0x20;
    pub const FLAG_SPRINT: u8 = 0x40;

    pub const fn flags(&self) -> u8 {
        (self.forward as u8)
            | ((self.backward as u8) << 1)
            | ((self.left as u8) << 2)
            | ((self.right as u8) << 3)
            | ((self.jump as u8) << 4)
            | ((self.shift as u8) << 5)
            | ((self.sprint as u8) << 6)
    }

    pub const fn from_flags(flags: u8) -> Self {
        Self {
            forward: flags & Self::FLAG_FORWARD != 0,
            backward: flags & Self::FLAG_BACKWARD != 0,
            left: flags & Self::FLAG_LEFT != 0,
            right: flags & Self::FLAG_RIGHT != 0,
            jump: flags & Self::FLAG_JUMP != 0,
            shift: flags & Self::FLAG_SHIFT != 0,
            sprint: flags & Self::FLAG_SPRINT != 0,
        }
    }
}

impl Encode for PlayerInput {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.flags());
    }
}

impl Decode for PlayerInput {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        u8::decode(input).map(Self::from_flags)
    }
}

impl crate::Packet for PlayerInput {
    const ID: i32 = sb::PLAYER_INPUT;
}

unit_packet! {
    pub struct PlayerLoaded = sb::PLAYER_LOADED;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetCarriedItem = sb::SET_CARRIED_ITEM {
        pub slot: i16,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetCreativeModeSlot = sb::SET_CREATIVE_MODE_SLOT {
        pub slot: i16,
        pub item: ItemStack as Untrusted,
    }
}

wire_enum! {
    pub enum SignTextSlot: Zero { Back, Front }
}

wire_enum! {
    pub enum RecipeBookType: Strict { Crafting, Furnace, BlastFurnace, Smoker }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChangeRecipeBookSettings = sb::RECIPE_BOOK_CHANGE_SETTINGS {
        pub book: RecipeBookType,
        pub open: bool,
        pub filtering: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PlaceRecipe = sb::PLACE_RECIPE {
        pub container_id: i32 as Var,
        pub recipe: i32 as Var,
        pub use_max_items: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RecipeBookSeen = sb::RECIPE_BOOK_SEEN_RECIPE {
        pub recipe: i32 as Var,
    }
}

pub struct SignLines;

impl FieldCodec<[String; SIGN_LINES]> for SignLines {
    fn write(value: &[String; SIGN_LINES], out: &mut Vec<u8>) {
        for line in value {
            line.encode(out);
        }
    }

    fn read(input: &mut &[u8]) -> Result<[String; SIGN_LINES], DecodeError> {
        Ok([
            decode_string(input, MAX_SIGN_LINE_CHARS)?,
            decode_string(input, MAX_SIGN_LINE_CHARS)?,
            decode_string(input, MAX_SIGN_LINE_CHARS)?,
            decode_string(input, MAX_SIGN_LINE_CHARS)?,
        ])
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SignUpdate = sb::SIGN_UPDATE {
        pub pos: Position,
        pub lines: [String; SIGN_LINES] as SignLines,
        pub slot: SignTextSlot,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct BlockHitResult {
        pub pos: Position,
        pub direction: Direction,
        pub cursor_x: f32,
        pub cursor_y: f32,
        pub cursor_z: f32,
        pub inside: bool,
        pub world_border_hit: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct UseItemOn = sb::USE_ITEM_ON {
        pub hand: Hand,
        pub hit: BlockHitResult,
        pub sequence: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct UseItem = sb::USE_ITEM {
        pub hand: Hand,
        pub sequence: i32 as Var,
        pub y_rot: f32,
        pub x_rot: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChangeGameMode = sb::CHANGE_GAME_MODE {
        pub mode: GameMode,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChangeDifficulty = sb::CHANGE_DIFFICULTY {
        pub difficulty: Difficulty,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LockDifficulty = sb::LOCK_DIFFICULTY {
        pub locked: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TeleportToEntity = sb::TELEPORT_TO_ENTITY {
        pub target: Uuid,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockEntityTagQuery = sb::BLOCK_ENTITY_TAG_QUERY {
        pub transaction_id: i32 as Var,
        pub pos: Position,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EntityTagQuery = sb::ENTITY_TAG_QUERY {
        pub transaction_id: i32 as Var,
        pub entity_id: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GameRuleEntry {
        pub rule: Identifier,
        pub value: String,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetGameRule = sb::SET_GAME_RULE {
        pub entries: Vec<GameRuleEntry> as List<Plain, MAX_GAME_RULE_ENTRIES>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RenameItem = sb::RENAME_ITEM {
        pub name: String,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenAdvancements {
    OpenedTab { tab: String },
    ClosedScreen,
}

impl Encode for SeenAdvancements {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::OpenedTab { tab } => {
                write_var(0, out);
                tab.encode(out);
            }
            Self::ClosedScreen => write_var(1, out),
        }
    }
}

impl Decode for SeenAdvancements {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        match read_var(input)? {
            0 => Ok(Self::OpenedTab {
                tab: String::decode(input)?,
            }),
            1 => Ok(Self::ClosedScreen),
            other => Err(DecodeError::InvalidValue {
                field: "seen advancements action",
                value: i64::from(other),
            }),
        }
    }
}

impl crate::Packet for SeenAdvancements {
    const ID: i32 = sb::SEEN_ADVANCEMENTS;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SelectTrade = sb::SELECT_TRADE {
        pub item: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetBeacon = sb::SET_BEACON {
        pub primary: Option<i32> as Opt<Var>,
        pub secondary: Option<i32> as Opt<Var>,
    }
}

wire_enum! {
    pub enum CommandBlockMode: Strict { Sequence, Auto, Redstone }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCommandBlock {
    pub pos: Position,
    pub command: String,
    pub mode: CommandBlockMode,
    pub track_output: bool,
    pub conditional: bool,
    pub automatic: bool,
}

impl SetCommandBlock {
    pub const FLAG_TRACK_OUTPUT: u8 = 0x01;
    pub const FLAG_CONDITIONAL: u8 = 0x02;
    pub const FLAG_AUTOMATIC: u8 = 0x04;
}

impl Encode for SetCommandBlock {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        self.command.encode(out);
        self.mode.encode(out);
        let mut flags = 0;
        if self.track_output {
            flags |= Self::FLAG_TRACK_OUTPUT;
        }
        if self.conditional {
            flags |= Self::FLAG_CONDITIONAL;
        }
        if self.automatic {
            flags |= Self::FLAG_AUTOMATIC;
        }
        out.push(flags);
    }
}

impl Decode for SetCommandBlock {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let pos = Position::decode(input)?;
        let command = String::decode(input)?;
        let mode = CommandBlockMode::decode(input)?;
        let flags = u8::decode(input)?;
        Ok(Self {
            pos,
            command,
            mode,
            track_output: flags & Self::FLAG_TRACK_OUTPUT != 0,
            conditional: flags & Self::FLAG_CONDITIONAL != 0,
            automatic: flags & Self::FLAG_AUTOMATIC != 0,
        })
    }
}

impl crate::Packet for SetCommandBlock {
    const ID: i32 = sb::SET_COMMAND_BLOCK;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetCommandMinecart = sb::SET_COMMAND_MINECART {
        pub entity_id: i32 as Var,
        pub command: String,
        pub track_output: bool,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerboundPlay {
    AcceptTeleportation(AcceptTeleportation),
    Attack(Attack),
    BlockEntityTagQuery(BlockEntityTagQuery),
    ChangeDifficulty(ChangeDifficulty),
    ChangeGameMode(ChangeGameMode),
    ChatAck(ChatAck),
    ChatCommand(ChatCommand),
    ChatCommandSigned(ChatCommandSigned),
    Chat(Chat),
    ChatSessionUpdate(ChatSessionUpdate),
    ChunkBatchReceived(ChunkBatchReceived),
    ClientCommand(ClientCommand),
    ClientTickEnd(ClientTickEnd),
    ClientInformation(ClientInformation),
    CommandSuggestion(CommandSuggestion),
    ConfigurationAcknowledged(ConfigurationAcknowledged),
    ContainerButtonClick(ContainerButtonClick),
    ContainerClick(ContainerClick),
    ContainerClose(ContainerClose),
    ContainerSlotStateChanged(ContainerSlotStateChanged),
    CookieResponse(CookieResponse),
    CustomPayload(ServerboundCustomPayload),
    EditBook(EditBook),
    EntityTagQuery(EntityTagQuery),
    Interact(Interact),
    KeepAlive(KeepAlive),
    LockDifficulty(LockDifficulty),
    MovePlayerPos(MovePlayerPos),
    MovePlayerPosRot(MovePlayerPosRot),
    MovePlayerRot(MovePlayerRot),
    MovePlayerStatusOnly(MovePlayerStatusOnly),
    MoveVehicle(MoveVehicle),
    PaddleBoat(PaddleBoat),
    PickItemFromBlock(PickItemFromBlock),
    PickItemFromEntity(PickItemFromEntity),
    PingRequest(PingRequest),
    PlayerAbilities(PlayerAbilities),
    PlayerAction(PlayerAction),
    PlayerCommand(PlayerCommand),
    PlayerInput(PlayerInput),
    PlayerLoaded(PlayerLoaded),
    Pong(Pong),
    Punch(Punch),
    RecipeBookSettings(ChangeRecipeBookSettings),
    PlaceRecipe(PlaceRecipe),
    RecipeBookSeen(RecipeBookSeen),
    RenameItem(RenameItem),
    ResourcePack(ResourcePackResponse),
    SeenAdvancements(SeenAdvancements),
    SelectTrade(SelectTrade),
    SetBeacon(SetBeacon),
    SetCarriedItem(SetCarriedItem),
    SetCommandBlock(SetCommandBlock),
    SetCommandMinecart(SetCommandMinecart),
    SetCreativeModeSlot(SetCreativeModeSlot),
    SetGameRule(SetGameRule),
    SignUpdate(SignUpdate),
    TeleportToEntity(TeleportToEntity),
    UseItemOn(UseItemOn),
    UseItem(UseItem),
    CustomClickAction(CustomClickAction),
    SelectBundleItem(SelectBundleItem),
    JigsawGenerate(JigsawGenerate),
    SetJigsawBlock(SetJigsawBlock),
    SetStructureBlock(SetStructureBlock),
    SetTestBlock(SetTestBlock),
    SpectatorAction(crate::play::l7::SpectatorAction),
    DebugSubscriptionRequest(DebugSubscriptionRequest),
    Unhandled { id: i32 },
}

impl ServerboundPlay {
    pub fn decode(id: i32, body: &[u8]) -> Result<Self, DecodeError> {
        Ok(match id {
            sb::ACCEPT_TELEPORTATION => Self::AcceptTeleportation(decode_exact(body)?),
            sb::ATTACK => Self::Attack(decode_exact(body)?),
            sb::BLOCK_ENTITY_TAG_QUERY => Self::BlockEntityTagQuery(decode_exact(body)?),
            sb::CHANGE_DIFFICULTY => Self::ChangeDifficulty(decode_exact(body)?),
            sb::CHANGE_GAME_MODE => Self::ChangeGameMode(decode_exact(body)?),
            sb::CHAT_ACK => Self::ChatAck(decode_exact(body)?),
            sb::CHAT_COMMAND => Self::ChatCommand(decode_exact(body)?),
            sb::CHAT_COMMAND_SIGNED => Self::ChatCommandSigned(decode_exact(body)?),
            sb::CHAT => Self::Chat(decode_exact(body)?),
            sb::CHAT_SESSION_UPDATE => Self::ChatSessionUpdate(decode_exact(body)?),
            sb::CHUNK_BATCH_RECEIVED => Self::ChunkBatchReceived(decode_exact(body)?),
            sb::CLIENT_COMMAND => Self::ClientCommand(decode_exact(body)?),
            sb::CLIENT_TICK_END => Self::ClientTickEnd(decode_exact(body)?),
            sb::CLIENT_INFORMATION => Self::ClientInformation(decode_exact(body)?),
            sb::COMMAND_SUGGESTION => Self::CommandSuggestion(decode_exact(body)?),
            sb::CONFIGURATION_ACKNOWLEDGED => Self::ConfigurationAcknowledged(decode_exact(body)?),
            sb::CONTAINER_BUTTON_CLICK => Self::ContainerButtonClick(decode_exact(body)?),
            sb::CONTAINER_CLICK => Self::ContainerClick(decode_exact(body)?),
            sb::CONTAINER_CLOSE => Self::ContainerClose(decode_exact(body)?),
            sb::CONTAINER_SLOT_STATE_CHANGED => {
                Self::ContainerSlotStateChanged(decode_exact(body)?)
            }
            sb::COOKIE_RESPONSE => Self::CookieResponse(decode_exact(body)?),
            sb::CUSTOM_PAYLOAD => Self::CustomPayload(decode_exact(body)?),
            sb::EDIT_BOOK => Self::EditBook(decode_exact(body)?),
            sb::ENTITY_TAG_QUERY => Self::EntityTagQuery(decode_exact(body)?),
            sb::INTERACT => Self::Interact(decode_exact(body)?),
            sb::KEEP_ALIVE => Self::KeepAlive(decode_exact(body)?),
            sb::LOCK_DIFFICULTY => Self::LockDifficulty(decode_exact(body)?),
            sb::MOVE_PLAYER_POS => Self::MovePlayerPos(decode_exact(body)?),
            sb::MOVE_PLAYER_POS_ROT => Self::MovePlayerPosRot(decode_exact(body)?),
            sb::MOVE_PLAYER_ROT => Self::MovePlayerRot(decode_exact(body)?),
            sb::MOVE_PLAYER_STATUS_ONLY => Self::MovePlayerStatusOnly(decode_exact(body)?),
            sb::MOVE_VEHICLE => Self::MoveVehicle(decode_exact(body)?),
            sb::PADDLE_BOAT => Self::PaddleBoat(decode_exact(body)?),
            sb::PICK_ITEM_FROM_BLOCK => Self::PickItemFromBlock(decode_exact(body)?),
            sb::PICK_ITEM_FROM_ENTITY => Self::PickItemFromEntity(decode_exact(body)?),
            sb::PING_REQUEST => Self::PingRequest(decode_exact(body)?),
            sb::PLAYER_ABILITIES => Self::PlayerAbilities(decode_exact(body)?),
            sb::PLAYER_ACTION => Self::PlayerAction(decode_exact(body)?),
            sb::PLAYER_COMMAND => Self::PlayerCommand(decode_exact(body)?),
            sb::PLAYER_INPUT => Self::PlayerInput(decode_exact(body)?),
            sb::PLAYER_LOADED => Self::PlayerLoaded(decode_exact(body)?),
            sb::PONG => Self::Pong(decode_exact(body)?),
            sb::PUNCH => Self::Punch(decode_exact(body)?),
            sb::RECIPE_BOOK_CHANGE_SETTINGS => Self::RecipeBookSettings(decode_exact(body)?),
            sb::PLACE_RECIPE => Self::PlaceRecipe(decode_exact(body)?),
            sb::RECIPE_BOOK_SEEN_RECIPE => Self::RecipeBookSeen(decode_exact(body)?),
            sb::RENAME_ITEM => Self::RenameItem(decode_exact(body)?),
            sb::RESOURCE_PACK => Self::ResourcePack(decode_exact(body)?),
            sb::SEEN_ADVANCEMENTS => Self::SeenAdvancements(decode_exact(body)?),
            sb::SELECT_TRADE => Self::SelectTrade(decode_exact(body)?),
            sb::SET_BEACON => Self::SetBeacon(decode_exact(body)?),
            sb::SET_CARRIED_ITEM => Self::SetCarriedItem(decode_exact(body)?),
            sb::SET_COMMAND_BLOCK => Self::SetCommandBlock(decode_exact(body)?),
            sb::SET_COMMAND_MINECART => Self::SetCommandMinecart(decode_exact(body)?),
            sb::SET_CREATIVE_MODE_SLOT => Self::SetCreativeModeSlot(decode_exact(body)?),
            sb::SET_GAME_RULE => Self::SetGameRule(decode_exact(body)?),
            sb::TELEPORT_TO_ENTITY => Self::TeleportToEntity(decode_exact(body)?),
            sb::SIGN_UPDATE => Self::SignUpdate(decode_exact(body)?),
            sb::USE_ITEM_ON => Self::UseItemOn(decode_exact(body)?),
            sb::USE_ITEM => Self::UseItem(decode_exact(body)?),
            sb::CUSTOM_CLICK_ACTION => Self::CustomClickAction(decode_exact(body)?),
            sb::BUNDLE_ITEM_SELECTED => Self::SelectBundleItem(decode_exact(body)?),
            sb::DEBUG_SUBSCRIPTION_REQUEST => Self::DebugSubscriptionRequest(decode_exact(body)?),
            sb::JIGSAW_GENERATE => Self::JigsawGenerate(decode_exact(body)?),
            sb::SET_JIGSAW_BLOCK => Self::SetJigsawBlock(decode_exact(body)?),
            sb::SET_STRUCTURE_BLOCK => Self::SetStructureBlock(decode_exact(body)?),
            sb::SET_TEST_BLOCK => Self::SetTestBlock(decode_exact(body)?),
            sb::SPECTATOR_ACTION => Self::SpectatorAction(decode_exact(body)?),
            sb::TEST_INSTANCE_BLOCK_ACTION => Self::Unhandled { id },
            other => return Err(DecodeError::UnknownPacket(other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::Packet;
    use crate::play::chat::{LastSeenBits, SIGNATURE_BYTES};
    use crate::play::slot::{ComponentValue, DataComponentPatch, HashedItem};
    use crate::play::wire::testing::{encoded, roundtrip};

    fn decode_packet<P: Packet>(packet: &P) -> ServerboundPlay {
        ServerboundPlay::decode(P::ID, &encoded(packet)).unwrap()
    }

    #[test]
    fn every_serverbound_id_is_covered() {
        for id in 0..=68 {
            assert!(!matches!(
                ServerboundPlay::decode(id, &[]),
                Err(DecodeError::UnknownPacket(_))
            ));
        }
        assert_eq!(
            ServerboundPlay::decode(69, &[]),
            Err(DecodeError::UnknownPacket(69))
        );
        assert_eq!(
            ServerboundPlay::decode(-1, &[]),
            Err(DecodeError::UnknownPacket(-1))
        );
    }

    #[test]
    fn unhandled_ids_skip_body() {
        assert_eq!(
            ServerboundPlay::decode(sb::TEST_INSTANCE_BLOCK_ACTION, &[1, 0, 9, 9]).unwrap(),
            ServerboundPlay::Unhandled {
                id: sb::TEST_INSTANCE_BLOCK_ACTION
            }
        );
        assert_eq!(
            ServerboundPlay::decode(sb::SPECTATOR_ACTION, &[6]).unwrap(),
            ServerboundPlay::SpectatorAction(crate::play::l7::SpectatorAction {
                spectate_entity_id: Some(5)
            })
        );
    }

    #[test]
    fn locking_the_difficulty_reads_one_flag() {
        assert_eq!(
            ServerboundPlay::decode(sb::LOCK_DIFFICULTY, &[1]).unwrap(),
            ServerboundPlay::LockDifficulty(LockDifficulty { locked: true })
        );
        assert!(ServerboundPlay::decode(sb::LOCK_DIFFICULTY, &[]).is_err());
    }

    #[test]
    fn teleporting_to_an_entity_reads_its_uuid() {
        let mut body = 7u128.to_be_bytes().to_vec();
        assert_eq!(
            ServerboundPlay::decode(sb::TELEPORT_TO_ENTITY, &body).unwrap(),
            ServerboundPlay::TeleportToEntity(TeleportToEntity {
                target: Uuid::from_u128(7)
            })
        );
        body.pop();
        assert!(ServerboundPlay::decode(sb::TELEPORT_TO_ENTITY, &body).is_err());
    }

    #[test]
    fn tag_queries_read_the_transaction_and_the_target() {
        let mut body = vec![5];
        Position { x: 1, y: 64, z: -2 }.encode(&mut body);
        assert_eq!(
            ServerboundPlay::decode(sb::BLOCK_ENTITY_TAG_QUERY, &body).unwrap(),
            ServerboundPlay::BlockEntityTagQuery(BlockEntityTagQuery {
                transaction_id: 5,
                pos: Position { x: 1, y: 64, z: -2 },
            })
        );
        assert_eq!(
            ServerboundPlay::decode(sb::ENTITY_TAG_QUERY, &[5, 0x80, 0x01]).unwrap(),
            ServerboundPlay::EntityTagQuery(EntityTagQuery {
                transaction_id: 5,
                entity_id: 128,
            })
        );
    }

    #[test]
    fn setting_game_rules_reads_key_and_text_value_pairs() {
        let mut body = vec![2];
        for text in [
            "minecraft:keep_inventory",
            "true",
            "minecraft:random_tick_speed",
            "7",
        ] {
            text.to_owned().encode(&mut body);
        }
        let ServerboundPlay::SetGameRule(packet) =
            ServerboundPlay::decode(sb::SET_GAME_RULE, &body).unwrap()
        else {
            panic!("not a game rule packet");
        };
        assert_eq!(packet.entries.len(), 2);
        assert_eq!(packet.entries[0].rule.as_str(), "minecraft:keep_inventory");
        assert_eq!(packet.entries[0].value, "true");
        assert_eq!(packet.entries[1].value, "7");
    }

    #[test]
    fn paddle_boat_reads_both_paddles() {
        assert_eq!(
            ServerboundPlay::decode(sb::PADDLE_BOAT, &[1, 0]).unwrap(),
            ServerboundPlay::PaddleBoat(PaddleBoat {
                left: true,
                right: false
            })
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        assert_eq!(
            ServerboundPlay::decode(sb::CHAT_ACK, &[1, 2]),
            Err(DecodeError::TrailingBytes(1))
        );
        assert_eq!(
            ServerboundPlay::decode(sb::CLIENT_TICK_END, &[0]),
            Err(DecodeError::TrailingBytes(1))
        );
        assert_eq!(
            ServerboundPlay::decode(sb::ATTACK, &[]),
            Err(DecodeError::UnexpectedEof)
        );
    }

    #[test]
    fn move_status_only_flags_byte() {
        let mut bytes = vec![0x03];
        assert_eq!(
            ServerboundPlay::decode(sb::MOVE_PLAYER_STATUS_ONLY, &bytes).unwrap(),
            ServerboundPlay::MovePlayerStatusOnly(MovePlayerStatusOnly {
                flags: MoveFlags {
                    on_ground: true,
                    horizontal_collision: true
                }
            })
        );
        bytes[0] = 0xFE;
        assert_eq!(
            ServerboundPlay::decode(sb::MOVE_PLAYER_STATUS_ONLY, &bytes).unwrap(),
            ServerboundPlay::MovePlayerStatusOnly(MovePlayerStatusOnly {
                flags: MoveFlags {
                    on_ground: false,
                    horizontal_collision: true
                }
            })
        );
    }

    #[test]
    fn movement_packets_layout() {
        let pos = MovePlayerPos {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            flags: MoveFlags {
                on_ground: true,
                horizontal_collision: false,
            },
        };
        let bytes = roundtrip(&pos);
        assert_eq!(bytes.len(), 25);
        assert_eq!(bytes[24], 1);
        let pos_rot = MovePlayerPosRot {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            y_rot: 90.0,
            x_rot: -10.0,
            flags: MoveFlags::default(),
        };
        assert_eq!(roundtrip(&pos_rot).len(), 33);
        let rot = MovePlayerRot {
            y_rot: 1.0,
            x_rot: 2.0,
            flags: MoveFlags::default(),
        };
        assert_eq!(roundtrip(&rot), vec![0x3F, 0x80, 0, 0, 0x40, 0, 0, 0, 0]);
        let vehicle = MoveVehicle {
            position: Vec3::new(1.0, 2.0, 3.0),
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: true,
        };
        assert_eq!(roundtrip(&vehicle).len(), 33);
        for packet in [
            decode_packet(&pos),
            decode_packet(&pos_rot),
            decode_packet(&rot),
            decode_packet(&vehicle),
        ] {
            assert!(!matches!(packet, ServerboundPlay::Unhandled { .. }));
        }
    }

    #[test]
    fn accept_teleportation_carries_position() {
        let packet = AcceptTeleportation {
            teleport_id: 7,
            x: 0.5,
            y: 64.0,
            z: 0.5,
            y_rot: 0.0,
            x_rot: 0.0,
        };
        assert_eq!(roundtrip(&packet).len(), 1 + 24 + 8);
        assert_eq!(
            decode_packet(&packet),
            ServerboundPlay::AcceptTeleportation(packet)
        );
    }

    #[test]
    fn combat_and_interaction() {
        assert_eq!(roundtrip(&Attack { entity_id: 300 }), vec![0xAC, 0x02]);
        assert!(roundtrip(&Punch).is_empty());
        assert_eq!(
            ServerboundPlay::decode(sb::PUNCH, &[]).unwrap(),
            ServerboundPlay::Punch(Punch)
        );
        let interact = Interact {
            entity_id: 1,
            hand: Hand::OffHand,
            location: LpVec3(Vec3::ZERO),
            using_secondary_action: true,
        };
        assert_eq!(roundtrip(&interact), vec![1, 1, 0, 1]);
        let use_on = UseItemOn {
            hand: Hand::MainHand,
            hit: BlockHitResult {
                pos: Position { x: 0, y: 0, z: 0 },
                direction: Direction::Up,
                cursor_x: 0.5,
                cursor_y: 1.0,
                cursor_z: 0.5,
                inside: false,
                world_border_hit: false,
            },
            sequence: 3,
        };
        let bytes = roundtrip(&use_on);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[9], 1);
        assert_eq!(&bytes[bytes.len() - 3..], &[0, 0, 3]);
        let use_item = UseItem {
            hand: Hand::OffHand,
            sequence: 2,
            y_rot: 0.0,
            x_rot: 0.0,
        };
        assert_eq!(roundtrip(&use_item), vec![1, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn player_action_uses_direction_byte() {
        let packet = PlayerAction {
            action: PlayerActionKind::Stab,
            pos: Position { x: 0, y: 0, z: 0 },
            direction: Direction::East,
            sequence: 1,
        };
        assert_eq!(roundtrip(&packet), vec![8, 0, 0, 0, 0, 0, 0, 0, 0, 5, 1]);
        let wrapped =
            ServerboundPlay::decode(sb::PLAYER_ACTION, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0]);
        assert!(matches!(
            wrapped,
            Ok(ServerboundPlay::PlayerAction(PlayerAction {
                action: PlayerActionKind::ChangeDestroyDirection,
                direction: Direction::Up,
                ..
            }))
        ));
        assert!(
            ServerboundPlay::decode(sb::PLAYER_ACTION, &[9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err()
        );
    }

    #[test]
    fn chat_packets_layout() {
        let chat = Chat {
            message: "hi".into(),
            timestamp: 1,
            salt: 2,
            signature: None,
            last_seen: LastSeenUpdate {
                offset: 0,
                acknowledged: LastSeenBits::default(),
                checksum: 0,
            },
        };
        let bytes = roundtrip(&chat);
        assert_eq!(&bytes[..3], &[2, b'h', b'i']);
        assert_eq!(&bytes[19..], &[0, 0, 0, 0, 0, 0]);
        let signed = ChatCommandSigned {
            command: "msg a b".into(),
            timestamp: 0,
            salt: 0,
            argument_signatures: vec![ArgumentSignature {
                name: "message".into(),
                signature: MessageSignature([7; SIGNATURE_BYTES]),
            }],
            last_seen: chat.last_seen,
        };
        assert_eq!(
            roundtrip(&signed).len(),
            8 + 16 + 1 + 8 + SIGNATURE_BYTES + 5
        );
        let too_long = Chat {
            message: "x".repeat(MAX_CHAT_CHARS + 1),
            ..chat.clone()
        };
        assert!(ServerboundPlay::decode(sb::CHAT, &encoded(&too_long)).is_err());
        assert_eq!(
            roundtrip(&ChatCommand {
                command: "help".into()
            }),
            vec![4, b'h', b'e', b'l', b'p']
        );
        assert_eq!(roundtrip(&ChatAck { offset: 3 }), vec![3]);
        let session = ChatSessionUpdate {
            session: ChatSession {
                session_id: Uuid::from_u128(1),
                expires_at: 0,
                public_key: vec![1],
                key_signature: vec![2],
            },
        };
        assert_eq!(roundtrip(&session).len(), 16 + 8 + 2 + 2);
    }

    #[test]
    fn container_click_with_hashed_slots() {
        let packet = ContainerClick {
            container_id: 1,
            state_id: 5,
            slot: 36,
            button: 0,
            input: ContainerInput::QuickMove,
            changed_slots: vec![ChangedSlot {
                slot: 36,
                item: HashedStack::EMPTY,
            }],
            carried: HashedStack(Some(HashedItem {
                item: 1,
                count: 64,
                added: Vec::new(),
                removed: Vec::new(),
            })),
        };
        assert_eq!(
            roundtrip(&packet),
            vec![1, 5, 0, 36, 0, 1, 1, 0, 36, 0, 1, 1, 64, 0, 0]
        );
        assert_eq!(
            roundtrip(&ContainerButtonClick {
                container_id: 2,
                button_id: 3
            }),
            vec![2, 3]
        );
        assert_eq!(roundtrip(&ContainerClose { container_id: 0 }), vec![0]);
        assert_eq!(
            roundtrip(&ContainerSlotStateChanged {
                slot_id: 4,
                container_id: 2,
                new_state: true
            }),
            vec![4, 2, 1]
        );
    }

    #[test]
    fn creative_slot_uses_length_prefixed_components() {
        let packet = SetCreativeModeSlot {
            slot: 36,
            item: ItemStack {
                item: 1,
                count: 1,
                components: DataComponentPatch {
                    added: vec![ComponentValue {
                        type_id: 6,
                        data: vec![0x08, 0x00, 0x01, b'x'],
                    }],
                    removed: Vec::new(),
                },
            },
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes, &[0, 36, 1, 1, 1, 0, 6, 4, 0x08, 0x00, 0x01, b'x']);
        assert_eq!(
            ServerboundPlay::decode(sb::SET_CREATIVE_MODE_SLOT, &bytes).unwrap(),
            ServerboundPlay::SetCreativeModeSlot(packet)
        );
        let empty = SetCreativeModeSlot {
            slot: -1,
            item: ItemStack::EMPTY,
        };
        assert_eq!(roundtrip(&empty), vec![0xFF, 0xFF, 0]);
    }

    #[test]
    fn misc_player_packets() {
        assert_eq!(roundtrip(&PlayerAbilities { flying: true }), vec![0x02]);
        assert_eq!(
            decode_exact::<PlayerAbilities>(&[0x07]).unwrap(),
            PlayerAbilities { flying: true }
        );
        let input = PlayerInput {
            forward: true,
            jump: true,
            sprint: true,
            ..PlayerInput::default()
        };
        assert_eq!(roundtrip(&input), vec![0x51]);
        assert_eq!(
            roundtrip(&PlayerCommand {
                entity_id: 1,
                action: PlayerCommandAction::StartFallFlying,
                data: 0
            }),
            vec![1, 6, 0]
        );
        assert_eq!(roundtrip(&SetCarriedItem { slot: 8 }), vec![0, 8]);
        assert_eq!(
            roundtrip(&ChunkBatchReceived {
                desired_chunks_per_tick: 7.0
            }),
            vec![0x40, 0xE0, 0, 0]
        );
        assert_eq!(
            roundtrip(&ClientCommand {
                action: ClientCommandAction::RequestGameRuleValues
            }),
            vec![2]
        );
        assert!(ServerboundPlay::decode(sb::CLIENT_COMMAND, &[3]).is_err());
        assert_eq!(
            roundtrip(&PickItemFromBlock {
                pos: Position { x: 0, y: 1, z: 0 },
                include_data: true
            }),
            vec![0, 0, 0, 0, 0, 0, 0, 1, 1]
        );
        assert_eq!(
            roundtrip(&PickItemFromEntity {
                entity_id: 4,
                include_data: false
            }),
            vec![4, 0]
        );
        assert_eq!(
            roundtrip(&CommandSuggestion {
                id: 1,
                command: "/t".into()
            }),
            vec![1, 2, b'/', b't']
        );
        assert!(roundtrip(&ConfigurationAcknowledged).is_empty());
        assert!(roundtrip(&PlayerLoaded).is_empty());
        assert_eq!(
            roundtrip(&ChangeGameMode {
                mode: GameMode::Spectator
            }),
            vec![3]
        );
        assert_eq!(
            roundtrip(&ChangeDifficulty {
                difficulty: Difficulty::Peaceful
            }),
            vec![0]
        );
    }

    #[test]
    fn edit_book_sign_and_blocks() {
        let book = EditBook {
            slot: 0,
            pages: vec!["a".into(), "b".into()],
            title: Some("T".into()),
        };
        assert_eq!(roundtrip(&book), vec![0, 2, 1, b'a', 1, b'b', 1, 1, b'T']);
        let too_many = EditBook {
            pages: vec![String::new(); MAX_BOOK_PAGES + 1],
            ..book
        };
        assert!(ServerboundPlay::decode(sb::EDIT_BOOK, &encoded(&too_many)).is_err());
        let sign = SignUpdate {
            pos: Position { x: 0, y: 0, z: 0 },
            lines: ["a".into(), String::new(), String::new(), "d".into()],
            slot: SignTextSlot::Front,
        };
        assert_eq!(
            roundtrip(&sign),
            vec![0, 0, 0, 0, 0, 0, 0, 0, 1, b'a', 0, 0, 1, b'd', 1]
        );
        let command_block = SetCommandBlock {
            pos: Position { x: 0, y: 0, z: 0 },
            command: "say".into(),
            mode: CommandBlockMode::Redstone,
            track_output: true,
            conditional: false,
            automatic: true,
        };
        assert_eq!(
            &roundtrip(&command_block)[8..],
            &[3, b's', b'a', b'y', 2, 0x05]
        );
        let minecart = SetCommandMinecart {
            entity_id: 300,
            command: "say".into(),
            track_output: true,
        };
        assert_eq!(
            roundtrip(&minecart),
            vec![0xac, 0x02, 3, b's', b'a', b'y', 1]
        );
        assert_eq!(
            ServerboundPlay::decode(sb::SET_COMMAND_MINECART, &encoded(&minecart)).unwrap(),
            ServerboundPlay::SetCommandMinecart(minecart)
        );
    }

    #[test]
    fn rename_trade_beacon_advancements() {
        assert_eq!(roundtrip(&RenameItem { name: "x".into() }), vec![1, b'x']);
        assert_eq!(
            roundtrip(&ChangeRecipeBookSettings {
                book: RecipeBookType::BlastFurnace,
                open: true,
                filtering: false,
            }),
            vec![2, 1, 0]
        );
        assert!(decode_exact::<ChangeRecipeBookSettings>(&[4, 0, 0]).is_err());
        assert_eq!(roundtrip(&SelectTrade { item: 2 }), vec![2]);
        assert_eq!(
            roundtrip(&SetBeacon {
                primary: Some(1),
                secondary: None
            }),
            vec![1, 1, 0]
        );
        assert_eq!(
            roundtrip(&SeenAdvancements::OpenedTab {
                tab: "minecraft:story/root".into()
            })[..2],
            [0, 20]
        );
        assert_eq!(roundtrip(&SeenAdvancements::ClosedScreen), vec![1]);
    }

    #[test]
    fn shared_common_payloads_decode_in_play() {
        assert!(matches!(
            ServerboundPlay::decode(sb::KEEP_ALIVE, &[0, 0, 0, 0, 0, 0, 0, 9]).unwrap(),
            ServerboundPlay::KeepAlive(KeepAlive { id: 9 })
        ));
        assert!(matches!(
            ServerboundPlay::decode(sb::PONG, &[0, 0, 0, 1]).unwrap(),
            ServerboundPlay::Pong(Pong { id: 1 })
        ));
        assert!(matches!(
            ServerboundPlay::decode(sb::PING_REQUEST, &[0, 0, 0, 0, 0, 0, 0, 2]).unwrap(),
            ServerboundPlay::PingRequest(PingRequest { time: 2 })
        ));
    }
}
