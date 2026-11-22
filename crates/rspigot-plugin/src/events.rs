use rspigot_plugin_protocol::v1::{self, event, patch};

use crate::Block;
use crate::BlockSnapshot;
use crate::BlockState;
use crate::BookMeta;
use crate::BoundingBox;
use crate::ChunkRef;
use crate::CommandSender;
use crate::DamageSource;
use crate::DroppedItem;
use crate::Effect;
use crate::EffectType;
use crate::EnchantOffer;
use crate::Enchantment;
use crate::Entity;
use crate::EntityType;
use crate::Error;
use crate::GameMode;
use crate::HolderRef;
use crate::Input;
use crate::InventoryRef;
use crate::InventoryView;
use crate::ItemStack;
use crate::LecternData;
use crate::Location;
use crate::LootContext;
use crate::Material;
use crate::MerchantRef;
use crate::OwnerRef;
use crate::Player;
use crate::PluginInfo;
use crate::RaidRef;
use crate::ServerLinks;
use crate::SignData;
use crate::SpawnerData;
use crate::Trade;
use crate::TrialSpawnerData;
use crate::Uuid;
use crate::Vector;
use crate::World;
use crate::diff;
use crate::link::Link;
use crate::listener::Verdict;
use crate::runtime::{Meta, Received};
use crate::wire::address;
use crate::wire::block;
use crate::wire::byte;
use crate::wire::effect_type;
use crate::wire::enchant;
use crate::wire::enchantment;
use crate::wire::entity_type;
use crate::wire::material;
use crate::wire::note;
use crate::wire::player_entity;
use crate::wire::required;
use crate::wire::slot;
use crate::wire::uuid;
use crate::wire::vector;
use crate::wire::world;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    LeftClickBlock,
    RightClickBlock,
    LeftClickAir,
    RightClickAir,
    Physical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsyncPlayerPreLoginResult {
    Allowed,
    KickFull,
    KickBanned,
    KickWhitelist,
    KickOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsyncStructureGenerateCause {
    Command,
    WorldGeneration,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockIgniteCause {
    Lava,
    FlintAndSteel,
    Spread,
    Lightning,
    Fireball,
    EnderCrystal,
    Explosion,
    Arrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CauldronLevelChangeReason {
    BucketFill,
    BucketEmpty,
    BottleFill,
    BottleEmpty,
    BannerWash,
    ArmorWash,
    ShulkerWash,
    Extinguish,
    Evaporate,
    NaturalFill,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClickKind {
    Left,
    Right,
    ShiftLeft,
    ShiftRight,
    NumberKey,
    SwapOffhand,
    Drop,
    ControlDrop,
    DoubleClick,
    Middle,
    WindowBorderLeft,
    WindowBorderRight,
    Creative,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreatureSpawnReason {
    Natural,
    Jockey,
    ChunkGen,
    Spawner,
    TrialSpawner,
    Egg,
    SpawnerEgg,
    Bucket,
    Lightning,
    BuildSnowman,
    BuildIrongolem,
    BuildCoppergolem,
    BuildWither,
    VillageDefense,
    VillageInvasion,
    Breeding,
    SlimeSplit,
    Reinforcements,
    NetherPortal,
    DispenseEgg,
    Infection,
    Cured,
    OcelotBaby,
    SilverfishBlock,
    Mount,
    Trap,
    EnderPearl,
    ShoulderEntity,
    Drowned,
    Sheared,
    Explosion,
    Raid,
    Patrol,
    Beehive,
    PiglinZombified,
    Spell,
    Frozen,
    Metamorphosis,
    Duplication,
    Command,
    Enchantment,
    PotionEffect,
    Reanimate,
    Custom,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreeperPowerCause {
    Lightning,
    SetOn,
    SetOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragType {
    Single,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DyeColor {
    White,
    Orange,
    Magenta,
    LightBlue,
    Yellow,
    Lime,
    Pink,
    Gray,
    LightGray,
    Cyan,
    Purple,
    Blue,
    Brown,
    Green,
    Red,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnderDragonPhase {
    Circling,
    Strafing,
    FlyToPortal,
    LandOnPortal,
    LeavePortal,
    BreathAttack,
    SearchForBreathAttackTarget,
    RoarBeforeAttack,
    ChargePlayer,
    Dying,
    Hover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityDamageCause {
    Kill,
    WorldBorder,
    Contact,
    EntityAttack,
    EntitySweepAttack,
    Projectile,
    Suffocation,
    Fall,
    Fire,
    FireTick,
    Melting,
    Lava,
    Drowning,
    BlockExplosion,
    EntityExplosion,
    Void,
    Lightning,
    Suicide,
    Starvation,
    Poison,
    Magic,
    Wither,
    FallingBlock,
    Thorns,
    DragonBreath,
    Custom,
    FlyIntoWall,
    HotFloor,
    Campfire,
    Cramming,
    Dryout,
    Freeze,
    SonicBoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityExhaustionReason {
    BlockMined,
    HungerEffect,
    Damaged,
    Attack,
    JumpSprint,
    Jump,
    Swim,
    WalkUnderwater,
    WalkOnWater,
    Sprint,
    Crouch,
    Walk,
    Regen,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKnockbackCause {
    Damage,
    EntityAttack,
    Explosion,
    ShieldBlock,
    SweepAttack,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityPotionEffectAction {
    Added,
    Changed,
    Cleared,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityPotionEffectCause {
    AreaEffectCloud,
    Arrow,
    Attack,
    Axolotl,
    Beacon,
    Command,
    Conduit,
    Conversion,
    Death,
    Dolphin,
    Expiration,
    Food,
    Illusion,
    Milk,
    PatrolCaptain,
    Plugin,
    PotionDrink,
    PotionSplash,
    SpiderSpawn,
    Totem,
    TurtleHelmet,
    Unknown,
    VillagerTrade,
    Warden,
    WitherRose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityRegainHealthRegainReason {
    Regen,
    Satiated,
    Eating,
    EnderCrystal,
    Magic,
    MagicRegen,
    WitherSpawn,
    Wither,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityRemoveCause {
    Death,
    Despawn,
    Drop,
    EnterBlock,
    Explode,
    Hit,
    Merge,
    OutOfWorld,
    Pickup,
    PlayerQuit,
    Plugin,
    Transformation,
    Unload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityTargetReason {
    TargetDied,
    ClosestPlayer,
    TargetAttackedEntity,
    PigZombieTarget,
    ForgotTarget,
    TargetAttackedOwner,
    OwnerAttackedTarget,
    RandomTarget,
    DefendVillage,
    TargetAttackedNearbyEntity,
    ReinforcementTarget,
    Collision,
    Custom,
    ClosestEntity,
    FollowLeader,
    Tempt,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityTransformReason {
    Cured,
    Frozen,
    Infection,
    Drowned,
    Sheared,
    Lightning,
    Split,
    PiglinZombified,
    Metamorphosis,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityUnleashReason {
    HolderGone,
    PlayerUnleash,
    Distance,
    Shear,
    Firework,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipmentSlot {
    Hand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventResult {
    Deny,
    Default,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExplosionResult {
    Keep,
    Destroy,
    DestroyWithDecay,
    TriggerBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
    WestNorthWest,
    NorthNorthWest,
    NorthNorthEast,
    EastNorthEast,
    EastSouthEast,
    SouthSouthEast,
    SouthSouthWest,
    WestSouthWest,
    Itself,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HangingBreakRemoveCause {
    Entity,
    Explosion,
    Obstruction,
    Physics,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HopperInventorySearchContainerType {
    Source,
    Destination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instrument {
    Piano,
    BassDrum,
    SnareDrum,
    Sticks,
    BassGuitar,
    Flute,
    Bell,
    Guitar,
    Chime,
    Xylophone,
    IronXylophone,
    CowBell,
    Didgeridoo,
    Bit,
    Banjo,
    Pling,
    Zombie,
    Trumpet,
    TrumpetExposed,
    TrumpetOxidized,
    TrumpetWeathered,
    Skeleton,
    Creeper,
    Dragon,
    WitherSkeleton,
    Piglin,
    CustomHead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryAction {
    Nothing,
    PickupAll,
    PickupSome,
    PickupHalf,
    PickupOne,
    PlaceAll,
    PlaceSome,
    PlaceOne,
    SwapWithCursor,
    DropAllCursor,
    DropOneCursor,
    DropAllSlot,
    DropOneSlot,
    MoveToOtherInventory,
    HotbarMoveAndReadd,
    HotbarSwap,
    CloneStack,
    CollectToCursor,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InventoryTypeSlotType {
    Result,
    Crafting,
    Armor,
    Container,
    Quickbar,
    Outside,
    Fuel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightningStrikeCause {
    Command,
    Custom,
    Spawner,
    Trident,
    Trap,
    Weather,
    Enchantment,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainHand {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerAnimationType {
    ArmSwing,
    OffArmSwing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerBedEnterResult {
    Ok,
    NotPossibleHere,
    NotPossibleNow,
    TooFarAway,
    NotSafe,
    OtherProblem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerExpCooldownChangeReason {
    PickupOrb,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerFishState {
    Fishing,
    CaughtFish,
    CaughtEntity,
    InGround,
    FailedAttempt,
    ReelIn,
    Bite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerLoginResult {
    Allowed,
    KickFull,
    KickBanned,
    KickWhitelist,
    KickOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerRecipeBookSettingsChangeRecipeBookType {
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerRespawnReason {
    Death,
    EndPortal,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerSignOpenCause {
    Interact,
    Place,
    Plugin,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerSpawnChangeCause {
    Command,
    Bed,
    RespawnAnchor,
    Plugin,
    Reset,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerTeleportCause {
    EnderPearl,
    Command,
    Plugin,
    NetherPortal,
    EndPortal,
    Spectate,
    EndGateway,
    ChorusFruit,
    Dismount,
    ExitBed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortalCreateReason {
    Fire,
    NetherPair,
    EndPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pose {
    Standing,
    FallFlying,
    Sleeping,
    Swimming,
    SpinAttack,
    Sneaking,
    LongJumping,
    Dying,
    Croaking,
    UsingTongue,
    Sitting,
    Roaring,
    Sniffing,
    Emerging,
    Digging,
    Sliding,
    Shooting,
    Inhaling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RaidStopReason {
    Peace,
    Timeout,
    Finished,
    Unspawnable,
    NotInVillage,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourcePackStatus {
    SuccessfullyLoaded,
    Declined,
    FailedDownload,
    Accepted,
    Downloaded,
    InvalidUrl,
    FailedReload,
    Discarded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerLoadType {
    Startup,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpellcasterSpell {
    None,
    SummonVex,
    Fangs,
    Wololo,
    Disappear,
    Blindness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Statistic {
    DamageDealt,
    DamageTaken,
    Deaths,
    MobKills,
    PlayerKills,
    FishCaught,
    AnimalsBred,
    LeaveGame,
    Jump,
    DropCount,
    Drop,
    Pickup,
    PlayOneMinute,
    TotalWorldTime,
    WalkOneCm,
    WalkOnWaterOneCm,
    FallOneCm,
    SneakTime,
    ClimbOneCm,
    FlyOneCm,
    WalkUnderWaterOneCm,
    MinecartOneCm,
    BoatOneCm,
    PigOneCm,
    HappyGhastOneCm,
    HorseOneCm,
    SprintOneCm,
    CrouchOneCm,
    AviateOneCm,
    MineBlock,
    UseItem,
    BreakItem,
    CraftItem,
    KillEntity,
    EntityKilledBy,
    TimeSinceDeath,
    TalkedToVillager,
    TradedWithVillager,
    CakeSlicesEaten,
    CauldronFilled,
    CauldronUsed,
    ArmorCleaned,
    BannerCleaned,
    BrewingstandInteraction,
    BeaconInteraction,
    DropperInspected,
    HopperInspected,
    DispenserInspected,
    NoteblockPlayed,
    NoteblockTuned,
    FlowerPotted,
    TrappedChestTriggered,
    EnderchestOpened,
    ItemEnchanted,
    RecordPlayed,
    FurnaceInteraction,
    CraftingTableInteraction,
    ChestOpened,
    SleepInBed,
    SleepInStrawBed,
    ShulkerBoxOpened,
    TimeSinceRest,
    SwimOneCm,
    DamageDealtAbsorbed,
    DamageDealtResisted,
    DamageBlockedByShield,
    DamageAbsorbed,
    DamageResisted,
    CleanShulkerBox,
    OpenBarrel,
    InteractWithBlastFurnace,
    InteractWithSmoker,
    InteractWithLectern,
    InteractWithCampfire,
    InteractWithCartographyTable,
    InteractWithLoom,
    InteractWithStonecutter,
    BellRing,
    RaidTrigger,
    RaidWin,
    InteractWithAnvil,
    InteractWithGrindstone,
    TargetHit,
    InteractWithSmithingTable,
    StriderOneCm,
    NautilusOneCm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TNTPrimeCause {
    Fire,
    Redstone,
    Player,
    Explosion,
    Projectile,
    BlockBreak,
    Dispenser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeSkipReason {
    Command,
    Custom,
    NightSkip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeType {
    Tree,
    BigTree,
    Redwood,
    TallRedwood,
    Birch,
    Jungle,
    SmallJungle,
    CocoaTree,
    JungleBush,
    RedMushroom,
    BrownMushroom,
    Swamp,
    Acacia,
    DarkOak,
    MegaRedwood,
    MegaPine,
    TallBirch,
    ChorusPlant,
    CrimsonFungus,
    WarpedFungus,
    Azalea,
    Mangrove,
    TallMangrove,
    Cherry,
    PaleOak,
    PaleOakCreaking,
    RedPoplar,
    OrangePoplar,
    YellowPoplar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VillagerCareerChangeReason {
    LosingJob,
    Employed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    AreaEffectCloudApply,
    ArrowBodyCountChange,
    AsyncPlayerChat,
    AsyncPlayerPreLogin,
    AsyncStructureGenerate,
    AsyncStructureSpawn,
    BatToggleSleep,
    BellResonate,
    BellRing,
    BlockBreak,
    BlockBrush,
    BlockBurn,
    BlockCanBuild,
    BlockCook,
    BlockDamage,
    BlockDamageAbort,
    BlockDispense,
    BlockDispenseArmor,
    BlockDispenseLoot,
    BlockDropItem,
    BlockExp,
    BlockExplode,
    BlockFade,
    BlockFertilize,
    BlockForm,
    BlockFromTo,
    BlockGrow,
    BlockIgnite,
    BlockMultiPlace,
    BlockPhysics,
    BlockPistonExtend,
    BlockPistonRetract,
    BlockPlace,
    BlockReceiveGame,
    BlockRedstone,
    BlockShearEntity,
    BlockSpread,
    Brew,
    BrewingStandFuel,
    BrewingStart,
    BroadcastMessage,
    CampfireStart,
    CauldronLevelChange,
    ChunkLoad,
    ChunkPopulate,
    ChunkUnload,
    CraftItem,
    CrafterCraft,
    CreatureSpawn,
    CreeperPower,
    CubeMobSplit,
    EnchantItem,
    EnderDragonChangePhase,
    EntitiesLoad,
    EntitiesUnload,
    EntityAirChange,
    EntityBlockForm,
    EntityBreakDoor,
    EntityBreed,
    EntityChangeBlock,
    EntityCombust,
    EntityCombustByBlock,
    EntityCombustByEntity,
    EntityDamage,
    EntityDamageByBlock,
    EntityDamageByEntity,
    EntityDeath,
    EntityDismount,
    EntityDropItem,
    EntityEnterBlock,
    EntityEnterLoveMode,
    EntityExhaustion,
    EntityExplode,
    EntityInteract,
    EntityKnockback,
    EntityKnockbackByEntity,
    EntityMount,
    EntityPickupItem,
    EntityPlace,
    EntityPortal,
    EntityPortalEnter,
    EntityPortalExit,
    EntityPoseChange,
    EntityPotionEffect,
    EntityRegainHealth,
    EntityRemove,
    EntityResurrect,
    EntityShootBow,
    EntitySpawn,
    EntitySpellCast,
    EntityTame,
    EntityTarget,
    EntityTargetBlock,
    EntityTargetLivingEntity,
    EntityTeleport,
    EntityToggleGlide,
    EntityToggleSwim,
    EntityTransform,
    EntityUnleash,
    ExpBottle,
    ExplosionPrime,
    FireworkExplode,
    FluidLevelChange,
    FoodLevelChange,
    FurnaceBurn,
    FurnaceExtract,
    FurnaceSmelt,
    FurnaceStartSmelt,
    GenericGame,
    HangingBreak,
    HangingBreakByEntity,
    HangingPlace,
    HopperInventorySearch,
    HorseJump,
    InventoryClick,
    InventoryClose,
    InventoryCreative,
    InventoryDrag,
    InventoryMoveItem,
    InventoryOpen,
    InventoryPickupItem,
    ItemDespawn,
    ItemMerge,
    ItemSpawn,
    LeavesDecay,
    LightningStrike,
    LingeringPotionSplash,
    LootGenerate,
    MapInitialize,
    MoistureChange,
    NotePlay,
    PigZap,
    PigZombieAnger,
    PiglinBarter,
    PlayerAdvancementCriterionGrant,
    PlayerAdvancementDone,
    PlayerAnimation,
    PlayerArmorStandManipulate,
    PlayerBedEnter,
    PlayerBedLeave,
    PlayerBucketEmpty,
    PlayerBucketEntity,
    PlayerBucketFill,
    PlayerChangedMainHand,
    PlayerChangedWorld,
    PlayerCommandPreprocess,
    PlayerCommandSend,
    PlayerCustomClick,
    PlayerDeath,
    PlayerDropItem,
    PlayerEditBook,
    PlayerEggThrow,
    PlayerExpChange,
    PlayerExpCooldownChange,
    PlayerFish,
    PlayerGameModeChange,
    PlayerHarvestBlock,
    PlayerHideEntity,
    PlayerInput,
    PlayerInteract,
    PlayerInteractAtEntity,
    PlayerInteractEntity,
    PlayerItemBreak,
    PlayerItemConsume,
    PlayerItemDamage,
    PlayerItemHeld,
    PlayerItemMend,
    PlayerJoin,
    PlayerKick,
    PlayerLeashEntity,
    PlayerLeave,
    PlayerLevelChange,
    PlayerLinksSend,
    PlayerLocaleChange,
    PlayerLogin,
    PlayerMove,
    PlayerPickupArrow,
    PlayerPortal,
    PlayerRecipeBookClick,
    PlayerRecipeBookSettingsChange,
    PlayerRecipeDiscover,
    PlayerResourcePackStatus,
    PlayerRespawn,
    PlayerRiptide,
    PlayerShearEntity,
    PlayerShowEntity,
    PlayerSignOpen,
    PlayerSpawnChange,
    PlayerStatisticIncrement,
    PlayerSwapHandItems,
    PlayerTakeLecternBook,
    PlayerTeleport,
    PlayerToggleFlight,
    PlayerToggleSneak,
    PlayerToggleSprint,
    PlayerUnleashEntity,
    PlayerVelocity,
    PluginDisable,
    PluginEnable,
    PortalCreate,
    PotionSplash,
    PrepareAnvil,
    PrepareGrindstone,
    PrepareItemCraft,
    PrepareItemEnchant,
    PrepareSmithing,
    ProjectileHit,
    ProjectileLaunch,
    RaidFinish,
    RaidSpawnWave,
    RaidStop,
    RaidTrigger,
    RemoteServerCommand,
    SculkBloom,
    ServerCommand,
    ServerListPing,
    ServerLoad,
    SheepDyeWool,
    SheepRegrowWool,
    SignChange,
    SmithItem,
    SpawnChange,
    SpawnerSpawn,
    SpongeAbsorb,
    StriderTemperatureChange,
    StructureGrow,
    TNTPrime,
    TabComplete,
    ThunderChange,
    TimeSkip,
    TradeSelect,
    TrialSpawnerSpawn,
    VaultDisplayItem,
    VehicleBlockCollision,
    VehicleCreate,
    VehicleDamage,
    VehicleDestroy,
    VehicleEnter,
    VehicleEntityCollision,
    VehicleExit,
    VehicleMove,
    VehicleUpdate,
    VillagerAcquireTrade,
    VillagerCareerChange,
    VillagerReplenishTrade,
    VillagerReputationChange,
    WeatherChange,
    WorldInit,
    WorldLoad,
    WorldSave,
    WorldUnload,
}

impl EventKind {
    pub(crate) const fn to_wire(self) -> v1::EventKind {
        match self {
            Self::AreaEffectCloudApply => v1::EventKind::AreaEffectCloudApply,
            Self::ArrowBodyCountChange => v1::EventKind::ArrowBodyCountChange,
            Self::AsyncPlayerChat => v1::EventKind::AsyncPlayerChat,
            Self::AsyncPlayerPreLogin => v1::EventKind::AsyncPlayerPreLogin,
            Self::AsyncStructureGenerate => v1::EventKind::AsyncStructureGenerate,
            Self::AsyncStructureSpawn => v1::EventKind::AsyncStructureSpawn,
            Self::BatToggleSleep => v1::EventKind::BatToggleSleep,
            Self::BellResonate => v1::EventKind::BellResonate,
            Self::BellRing => v1::EventKind::BellRing,
            Self::BlockBreak => v1::EventKind::BlockBreak,
            Self::BlockBrush => v1::EventKind::BlockBrush,
            Self::BlockBurn => v1::EventKind::BlockBurn,
            Self::BlockCanBuild => v1::EventKind::BlockCanBuild,
            Self::BlockCook => v1::EventKind::BlockCook,
            Self::BlockDamage => v1::EventKind::BlockDamage,
            Self::BlockDamageAbort => v1::EventKind::BlockDamageAbort,
            Self::BlockDispense => v1::EventKind::BlockDispense,
            Self::BlockDispenseArmor => v1::EventKind::BlockDispenseArmor,
            Self::BlockDispenseLoot => v1::EventKind::BlockDispenseLoot,
            Self::BlockDropItem => v1::EventKind::BlockDropItem,
            Self::BlockExp => v1::EventKind::BlockExp,
            Self::BlockExplode => v1::EventKind::BlockExplode,
            Self::BlockFade => v1::EventKind::BlockFade,
            Self::BlockFertilize => v1::EventKind::BlockFertilize,
            Self::BlockForm => v1::EventKind::BlockForm,
            Self::BlockFromTo => v1::EventKind::BlockFromTo,
            Self::BlockGrow => v1::EventKind::BlockGrow,
            Self::BlockIgnite => v1::EventKind::BlockIgnite,
            Self::BlockMultiPlace => v1::EventKind::BlockMultiPlace,
            Self::BlockPhysics => v1::EventKind::BlockPhysics,
            Self::BlockPistonExtend => v1::EventKind::BlockPistonExtend,
            Self::BlockPistonRetract => v1::EventKind::BlockPistonRetract,
            Self::BlockPlace => v1::EventKind::BlockPlace,
            Self::BlockReceiveGame => v1::EventKind::BlockReceiveGame,
            Self::BlockRedstone => v1::EventKind::BlockRedstone,
            Self::BlockShearEntity => v1::EventKind::BlockShearEntity,
            Self::BlockSpread => v1::EventKind::BlockSpread,
            Self::Brew => v1::EventKind::Brew,
            Self::BrewingStandFuel => v1::EventKind::BrewingStandFuel,
            Self::BrewingStart => v1::EventKind::BrewingStart,
            Self::BroadcastMessage => v1::EventKind::BroadcastMessage,
            Self::CampfireStart => v1::EventKind::CampfireStart,
            Self::CauldronLevelChange => v1::EventKind::CauldronLevelChange,
            Self::ChunkLoad => v1::EventKind::ChunkLoad,
            Self::ChunkPopulate => v1::EventKind::ChunkPopulate,
            Self::ChunkUnload => v1::EventKind::ChunkUnload,
            Self::CraftItem => v1::EventKind::CraftItem,
            Self::CrafterCraft => v1::EventKind::CrafterCraft,
            Self::CreatureSpawn => v1::EventKind::CreatureSpawn,
            Self::CreeperPower => v1::EventKind::CreeperPower,
            Self::CubeMobSplit => v1::EventKind::CubeMobSplit,
            Self::EnchantItem => v1::EventKind::EnchantItem,
            Self::EnderDragonChangePhase => v1::EventKind::EnderDragonChangePhase,
            Self::EntitiesLoad => v1::EventKind::EntitiesLoad,
            Self::EntitiesUnload => v1::EventKind::EntitiesUnload,
            Self::EntityAirChange => v1::EventKind::EntityAirChange,
            Self::EntityBlockForm => v1::EventKind::EntityBlockForm,
            Self::EntityBreakDoor => v1::EventKind::EntityBreakDoor,
            Self::EntityBreed => v1::EventKind::EntityBreed,
            Self::EntityChangeBlock => v1::EventKind::EntityChangeBlock,
            Self::EntityCombust => v1::EventKind::EntityCombust,
            Self::EntityCombustByBlock => v1::EventKind::EntityCombustByBlock,
            Self::EntityCombustByEntity => v1::EventKind::EntityCombustByEntity,
            Self::EntityDamage => v1::EventKind::EntityDamage,
            Self::EntityDamageByBlock => v1::EventKind::EntityDamageByBlock,
            Self::EntityDamageByEntity => v1::EventKind::EntityDamageByEntity,
            Self::EntityDeath => v1::EventKind::EntityDeath,
            Self::EntityDismount => v1::EventKind::EntityDismount,
            Self::EntityDropItem => v1::EventKind::EntityDropItem,
            Self::EntityEnterBlock => v1::EventKind::EntityEnterBlock,
            Self::EntityEnterLoveMode => v1::EventKind::EntityEnterLoveMode,
            Self::EntityExhaustion => v1::EventKind::EntityExhaustion,
            Self::EntityExplode => v1::EventKind::EntityExplode,
            Self::EntityInteract => v1::EventKind::EntityInteract,
            Self::EntityKnockback => v1::EventKind::EntityKnockback,
            Self::EntityKnockbackByEntity => v1::EventKind::EntityKnockbackByEntity,
            Self::EntityMount => v1::EventKind::EntityMount,
            Self::EntityPickupItem => v1::EventKind::EntityPickupItem,
            Self::EntityPlace => v1::EventKind::EntityPlace,
            Self::EntityPortal => v1::EventKind::EntityPortal,
            Self::EntityPortalEnter => v1::EventKind::EntityPortalEnter,
            Self::EntityPortalExit => v1::EventKind::EntityPortalExit,
            Self::EntityPoseChange => v1::EventKind::EntityPoseChange,
            Self::EntityPotionEffect => v1::EventKind::EntityPotionEffect,
            Self::EntityRegainHealth => v1::EventKind::EntityRegainHealth,
            Self::EntityRemove => v1::EventKind::EntityRemove,
            Self::EntityResurrect => v1::EventKind::EntityResurrect,
            Self::EntityShootBow => v1::EventKind::EntityShootBow,
            Self::EntitySpawn => v1::EventKind::EntitySpawn,
            Self::EntitySpellCast => v1::EventKind::EntitySpellCast,
            Self::EntityTame => v1::EventKind::EntityTame,
            Self::EntityTarget => v1::EventKind::EntityTarget,
            Self::EntityTargetBlock => v1::EventKind::EntityTargetBlock,
            Self::EntityTargetLivingEntity => v1::EventKind::EntityTargetLivingEntity,
            Self::EntityTeleport => v1::EventKind::EntityTeleport,
            Self::EntityToggleGlide => v1::EventKind::EntityToggleGlide,
            Self::EntityToggleSwim => v1::EventKind::EntityToggleSwim,
            Self::EntityTransform => v1::EventKind::EntityTransform,
            Self::EntityUnleash => v1::EventKind::EntityUnleash,
            Self::ExpBottle => v1::EventKind::ExpBottle,
            Self::ExplosionPrime => v1::EventKind::ExplosionPrime,
            Self::FireworkExplode => v1::EventKind::FireworkExplode,
            Self::FluidLevelChange => v1::EventKind::FluidLevelChange,
            Self::FoodLevelChange => v1::EventKind::FoodLevelChange,
            Self::FurnaceBurn => v1::EventKind::FurnaceBurn,
            Self::FurnaceExtract => v1::EventKind::FurnaceExtract,
            Self::FurnaceSmelt => v1::EventKind::FurnaceSmelt,
            Self::FurnaceStartSmelt => v1::EventKind::FurnaceStartSmelt,
            Self::GenericGame => v1::EventKind::GenericGame,
            Self::HangingBreak => v1::EventKind::HangingBreak,
            Self::HangingBreakByEntity => v1::EventKind::HangingBreakByEntity,
            Self::HangingPlace => v1::EventKind::HangingPlace,
            Self::HopperInventorySearch => v1::EventKind::HopperInventorySearch,
            Self::HorseJump => v1::EventKind::HorseJump,
            Self::InventoryClick => v1::EventKind::InventoryClick,
            Self::InventoryClose => v1::EventKind::InventoryClose,
            Self::InventoryCreative => v1::EventKind::InventoryCreative,
            Self::InventoryDrag => v1::EventKind::InventoryDrag,
            Self::InventoryMoveItem => v1::EventKind::InventoryMoveItem,
            Self::InventoryOpen => v1::EventKind::InventoryOpen,
            Self::InventoryPickupItem => v1::EventKind::InventoryPickupItem,
            Self::ItemDespawn => v1::EventKind::ItemDespawn,
            Self::ItemMerge => v1::EventKind::ItemMerge,
            Self::ItemSpawn => v1::EventKind::ItemSpawn,
            Self::LeavesDecay => v1::EventKind::LeavesDecay,
            Self::LightningStrike => v1::EventKind::LightningStrike,
            Self::LingeringPotionSplash => v1::EventKind::LingeringPotionSplash,
            Self::LootGenerate => v1::EventKind::LootGenerate,
            Self::MapInitialize => v1::EventKind::MapInitialize,
            Self::MoistureChange => v1::EventKind::MoistureChange,
            Self::NotePlay => v1::EventKind::NotePlay,
            Self::PigZap => v1::EventKind::PigZap,
            Self::PigZombieAnger => v1::EventKind::PigZombieAnger,
            Self::PiglinBarter => v1::EventKind::PiglinBarter,
            Self::PlayerAdvancementCriterionGrant => v1::EventKind::PlayerAdvancementCriterionGrant,
            Self::PlayerAdvancementDone => v1::EventKind::PlayerAdvancementDone,
            Self::PlayerAnimation => v1::EventKind::PlayerAnimation,
            Self::PlayerArmorStandManipulate => v1::EventKind::PlayerArmorStandManipulate,
            Self::PlayerBedEnter => v1::EventKind::PlayerBedEnter,
            Self::PlayerBedLeave => v1::EventKind::PlayerBedLeave,
            Self::PlayerBucketEmpty => v1::EventKind::PlayerBucketEmpty,
            Self::PlayerBucketEntity => v1::EventKind::PlayerBucketEntity,
            Self::PlayerBucketFill => v1::EventKind::PlayerBucketFill,
            Self::PlayerChangedMainHand => v1::EventKind::PlayerChangedMainHand,
            Self::PlayerChangedWorld => v1::EventKind::PlayerChangedWorld,
            Self::PlayerCommandPreprocess => v1::EventKind::PlayerCommandPreprocess,
            Self::PlayerCommandSend => v1::EventKind::PlayerCommandSend,
            Self::PlayerCustomClick => v1::EventKind::PlayerCustomClick,
            Self::PlayerDeath => v1::EventKind::PlayerDeath,
            Self::PlayerDropItem => v1::EventKind::PlayerDropItem,
            Self::PlayerEditBook => v1::EventKind::PlayerEditBook,
            Self::PlayerEggThrow => v1::EventKind::PlayerEggThrow,
            Self::PlayerExpChange => v1::EventKind::PlayerExpChange,
            Self::PlayerExpCooldownChange => v1::EventKind::PlayerExpCooldownChange,
            Self::PlayerFish => v1::EventKind::PlayerFish,
            Self::PlayerGameModeChange => v1::EventKind::PlayerGameModeChange,
            Self::PlayerHarvestBlock => v1::EventKind::PlayerHarvestBlock,
            Self::PlayerHideEntity => v1::EventKind::PlayerHideEntity,
            Self::PlayerInput => v1::EventKind::PlayerInput,
            Self::PlayerInteract => v1::EventKind::PlayerInteract,
            Self::PlayerInteractAtEntity => v1::EventKind::PlayerInteractAtEntity,
            Self::PlayerInteractEntity => v1::EventKind::PlayerInteractEntity,
            Self::PlayerItemBreak => v1::EventKind::PlayerItemBreak,
            Self::PlayerItemConsume => v1::EventKind::PlayerItemConsume,
            Self::PlayerItemDamage => v1::EventKind::PlayerItemDamage,
            Self::PlayerItemHeld => v1::EventKind::PlayerItemHeld,
            Self::PlayerItemMend => v1::EventKind::PlayerItemMend,
            Self::PlayerJoin => v1::EventKind::PlayerJoin,
            Self::PlayerKick => v1::EventKind::PlayerKick,
            Self::PlayerLeashEntity => v1::EventKind::PlayerLeashEntity,
            Self::PlayerLeave => v1::EventKind::PlayerLeave,
            Self::PlayerLevelChange => v1::EventKind::PlayerLevelChange,
            Self::PlayerLinksSend => v1::EventKind::PlayerLinksSend,
            Self::PlayerLocaleChange => v1::EventKind::PlayerLocaleChange,
            Self::PlayerLogin => v1::EventKind::PlayerLogin,
            Self::PlayerMove => v1::EventKind::PlayerMove,
            Self::PlayerPickupArrow => v1::EventKind::PlayerPickupArrow,
            Self::PlayerPortal => v1::EventKind::PlayerPortal,
            Self::PlayerRecipeBookClick => v1::EventKind::PlayerRecipeBookClick,
            Self::PlayerRecipeBookSettingsChange => v1::EventKind::PlayerRecipeBookSettingsChange,
            Self::PlayerRecipeDiscover => v1::EventKind::PlayerRecipeDiscover,
            Self::PlayerResourcePackStatus => v1::EventKind::PlayerResourcePackStatus,
            Self::PlayerRespawn => v1::EventKind::PlayerRespawn,
            Self::PlayerRiptide => v1::EventKind::PlayerRiptide,
            Self::PlayerShearEntity => v1::EventKind::PlayerShearEntity,
            Self::PlayerShowEntity => v1::EventKind::PlayerShowEntity,
            Self::PlayerSignOpen => v1::EventKind::PlayerSignOpen,
            Self::PlayerSpawnChange => v1::EventKind::PlayerSpawnChange,
            Self::PlayerStatisticIncrement => v1::EventKind::PlayerStatisticIncrement,
            Self::PlayerSwapHandItems => v1::EventKind::PlayerSwapHandItems,
            Self::PlayerTakeLecternBook => v1::EventKind::PlayerTakeLecternBook,
            Self::PlayerTeleport => v1::EventKind::PlayerTeleport,
            Self::PlayerToggleFlight => v1::EventKind::PlayerToggleFlight,
            Self::PlayerToggleSneak => v1::EventKind::PlayerToggleSneak,
            Self::PlayerToggleSprint => v1::EventKind::PlayerToggleSprint,
            Self::PlayerUnleashEntity => v1::EventKind::PlayerUnleashEntity,
            Self::PlayerVelocity => v1::EventKind::PlayerVelocity,
            Self::PluginDisable => v1::EventKind::PluginDisable,
            Self::PluginEnable => v1::EventKind::PluginEnable,
            Self::PortalCreate => v1::EventKind::PortalCreate,
            Self::PotionSplash => v1::EventKind::PotionSplash,
            Self::PrepareAnvil => v1::EventKind::PrepareAnvil,
            Self::PrepareGrindstone => v1::EventKind::PrepareGrindstone,
            Self::PrepareItemCraft => v1::EventKind::PrepareItemCraft,
            Self::PrepareItemEnchant => v1::EventKind::PrepareItemEnchant,
            Self::PrepareSmithing => v1::EventKind::PrepareSmithing,
            Self::ProjectileHit => v1::EventKind::ProjectileHit,
            Self::ProjectileLaunch => v1::EventKind::ProjectileLaunch,
            Self::RaidFinish => v1::EventKind::RaidFinish,
            Self::RaidSpawnWave => v1::EventKind::RaidSpawnWave,
            Self::RaidStop => v1::EventKind::RaidStop,
            Self::RaidTrigger => v1::EventKind::RaidTrigger,
            Self::RemoteServerCommand => v1::EventKind::RemoteServerCommand,
            Self::SculkBloom => v1::EventKind::SculkBloom,
            Self::ServerCommand => v1::EventKind::ServerCommand,
            Self::ServerListPing => v1::EventKind::ServerListPing,
            Self::ServerLoad => v1::EventKind::ServerLoad,
            Self::SheepDyeWool => v1::EventKind::SheepDyeWool,
            Self::SheepRegrowWool => v1::EventKind::SheepRegrowWool,
            Self::SignChange => v1::EventKind::SignChange,
            Self::SmithItem => v1::EventKind::SmithItem,
            Self::SpawnChange => v1::EventKind::SpawnChange,
            Self::SpawnerSpawn => v1::EventKind::SpawnerSpawn,
            Self::SpongeAbsorb => v1::EventKind::SpongeAbsorb,
            Self::StriderTemperatureChange => v1::EventKind::StriderTemperatureChange,
            Self::StructureGrow => v1::EventKind::StructureGrow,
            Self::TNTPrime => v1::EventKind::TntPrime,
            Self::TabComplete => v1::EventKind::TabComplete,
            Self::ThunderChange => v1::EventKind::ThunderChange,
            Self::TimeSkip => v1::EventKind::TimeSkip,
            Self::TradeSelect => v1::EventKind::TradeSelect,
            Self::TrialSpawnerSpawn => v1::EventKind::TrialSpawnerSpawn,
            Self::VaultDisplayItem => v1::EventKind::VaultDisplayItem,
            Self::VehicleBlockCollision => v1::EventKind::VehicleBlockCollision,
            Self::VehicleCreate => v1::EventKind::VehicleCreate,
            Self::VehicleDamage => v1::EventKind::VehicleDamage,
            Self::VehicleDestroy => v1::EventKind::VehicleDestroy,
            Self::VehicleEnter => v1::EventKind::VehicleEnter,
            Self::VehicleEntityCollision => v1::EventKind::VehicleEntityCollision,
            Self::VehicleExit => v1::EventKind::VehicleExit,
            Self::VehicleMove => v1::EventKind::VehicleMove,
            Self::VehicleUpdate => v1::EventKind::VehicleUpdate,
            Self::VillagerAcquireTrade => v1::EventKind::VillagerAcquireTrade,
            Self::VillagerCareerChange => v1::EventKind::VillagerCareerChange,
            Self::VillagerReplenishTrade => v1::EventKind::VillagerReplenishTrade,
            Self::VillagerReputationChange => v1::EventKind::VillagerReputationChange,
            Self::WeatherChange => v1::EventKind::WeatherChange,
            Self::WorldInit => v1::EventKind::WorldInit,
            Self::WorldLoad => v1::EventKind::WorldLoad,
            Self::WorldSave => v1::EventKind::WorldSave,
            Self::WorldUnload => v1::EventKind::WorldUnload,
        }
    }
}

#[derive(Debug)]
pub struct AreaEffectCloudApply {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    affected_entities: Vec<Entity>,
    entity: Entity,
    entity_type: EntityType,
}

impl AreaEffectCloudApply {
    fn from_wire(
        message: v1::AreaEffectCloudApply,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            affected_entities: message
                .affected_entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn affected_entities(&self) -> &[Entity] {
        &self.affected_entities
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<AreaEffectCloudApply> for Verdict {
    fn from(event: AreaEffectCloudApply) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct ArrowBodyCountChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    new_amount: i32,
    old_amount: i32,
    reset: bool,
    patch: v1::ArrowBodyCountChangePatch,
}

impl ArrowBodyCountChange {
    fn from_wire(
        message: v1::ArrowBodyCountChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            new_amount: message.new_amount,
            old_amount: message.old_amount,
            reset: message.reset,
            patch: v1::ArrowBodyCountChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn new_amount(&self) -> i32 {
        self.new_amount
    }

    pub fn old_amount(&self) -> i32 {
        self.old_amount
    }

    pub fn is_reset(&self) -> bool {
        self.reset
    }

    pub fn set_new_amount(&mut self, value: i32) {
        self.patch.new_amount = Some(value);
        self.new_amount = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ArrowBodyCountChange> for Verdict {
    fn from(event: ArrowBodyCountChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::ArrowBodyCountChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ArrowBodyCountChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct AsyncPlayerChat {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    format: String,
    message: String,
    player: Player,
    recipients: Vec<Player>,
    recipients_snapshot: Option<Vec<Player>>,
    patch: v1::AsyncPlayerChatPatch,
}

impl AsyncPlayerChat {
    fn from_wire(message: v1::AsyncPlayerChat, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            format: message.format,
            message: message.message,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            recipients: message
                .recipients
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            recipients_snapshot: None,
            patch: v1::AsyncPlayerChatPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn recipients(&self) -> &[Player] {
        &self.recipients
    }

    pub fn set_format(&mut self, value: String) {
        self.patch.format = Some(value.clone());
        self.format = value;
    }

    pub fn set_message(&mut self, value: String) {
        self.patch.message = Some(value.clone());
        self.message = value;
    }

    pub fn set_recipients(&mut self, value: Vec<Player>) {
        let snapshot = self
            .recipients_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.recipients));
        self.patch.recipients = diff::list(
            &snapshot
                .iter()
                .map(|value| value.to_snapshot())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_snapshot()).collect(),
        )
        .map(|diff| v1::PlayerSnapshotListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::PlayerSnapshotReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.recipients = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<AsyncPlayerChat> for Verdict {
    fn from(event: AsyncPlayerChat) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::AsyncPlayerChatPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::AsyncPlayerChat(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct AsyncPlayerPreLogin {
    id: u64,
    address: IpAddr,
    kick_message: String,
    login_result: AsyncPlayerPreLoginResult,
    name: String,
    transferred: bool,
    unique_id: Uuid,
    patch: v1::AsyncPlayerPreLoginPatch,
}

impl AsyncPlayerPreLogin {
    fn from_wire(
        message: v1::AsyncPlayerPreLogin,
        meta: Meta,
        _link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            address: address(message.address)?,
            kick_message: message.kick_message,
            login_result: AsyncPlayerPreLoginResult::from_wire(message.login_result)?,
            name: message.name,
            transferred: message.transferred,
            unique_id: uuid(message.unique_id)?,
            patch: v1::AsyncPlayerPreLoginPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn kick_message(&self) -> &str {
        &self.kick_message
    }

    pub fn login_result(&self) -> AsyncPlayerPreLoginResult {
        self.login_result
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_transferred(&self) -> bool {
        self.transferred
    }

    pub fn unique_id(&self) -> Uuid {
        self.unique_id
    }

    pub fn set_kick_message(&mut self, value: String) {
        self.patch.kick_message = Some(value.clone());
        self.kick_message = value;
    }

    pub fn set_login_result(&mut self, value: AsyncPlayerPreLoginResult) {
        self.patch.login_result = Some(value.to_wire());
        self.login_result = value;
    }
}

impl From<AsyncPlayerPreLogin> for Verdict {
    fn from(event: AsyncPlayerPreLogin) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::AsyncPlayerPreLoginPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::AsyncPlayerPreLogin(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncStructureGenerate {
    id: u64,
    bounding_box: BoundingBox,
    cause: AsyncStructureGenerateCause,
    chunk_x: i32,
    chunk_z: i32,
    structure: String,
    world: World,
}

impl AsyncStructureGenerate {
    fn from_wire(
        message: v1::AsyncStructureGenerate,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            bounding_box: BoundingBox::from_wire(
                required(message.bounding_box, "bounding_box")?,
                link,
            )?,
            cause: AsyncStructureGenerateCause::from_wire(message.cause)?,
            chunk_x: message.chunk_x,
            chunk_z: message.chunk_z,
            structure: message.structure,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bounding_box(&self) -> BoundingBox {
        self.bounding_box
    }

    pub fn cause(&self) -> AsyncStructureGenerateCause {
        self.cause
    }

    pub fn chunk_x(&self) -> i32 {
        self.chunk_x
    }

    pub fn chunk_z(&self) -> i32 {
        self.chunk_z
    }

    pub fn structure(&self) -> &str {
        &self.structure
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct AsyncStructureSpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    bounding_box: BoundingBox,
    chunk_x: i32,
    chunk_z: i32,
    structure: String,
    world: World,
}

impl AsyncStructureSpawn {
    fn from_wire(message: v1::AsyncStructureSpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            bounding_box: BoundingBox::from_wire(
                required(message.bounding_box, "bounding_box")?,
                link,
            )?,
            chunk_x: message.chunk_x,
            chunk_z: message.chunk_z,
            structure: message.structure,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bounding_box(&self) -> BoundingBox {
        self.bounding_box
    }

    pub fn chunk_x(&self) -> i32 {
        self.chunk_x
    }

    pub fn chunk_z(&self) -> i32 {
        self.chunk_z
    }

    pub fn structure(&self) -> &str {
        &self.structure
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<AsyncStructureSpawn> for Verdict {
    fn from(event: AsyncStructureSpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BatToggleSleep {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    awake: bool,
    entity: Entity,
    entity_type: EntityType,
}

impl BatToggleSleep {
    fn from_wire(message: v1::BatToggleSleep, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            awake: message.awake,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn is_awake(&self) -> bool {
        self.awake
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BatToggleSleep> for Verdict {
    fn from(event: BatToggleSleep) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BellResonate {
    id: u64,
    block: Block,
    resonated_entities: Vec<Entity>,
}

impl BellResonate {
    fn from_wire(message: v1::BellResonate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            resonated_entities: message
                .resonated_entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn resonated_entities(&self) -> &[Entity] {
        &self.resonated_entities
    }
}

#[derive(Debug)]
pub struct BellRing {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    direction: Face,
    entity: Option<Entity>,
}

impl BellRing {
    fn from_wire(message: v1::BellRing, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            direction: Face::from_wire(message.direction)?,
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn direction(&self) -> Face {
        self.direction
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BellRing> for Verdict {
    fn from(event: BellRing) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockBreak {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    drop_items: bool,
    exp_to_drop: i32,
    player: Player,
    patch: v1::BlockBreakPatch,
}

impl BlockBreak {
    fn from_wire(message: v1::BlockBreak, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            drop_items: message.drop_items,
            exp_to_drop: message.exp_to_drop,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::BlockBreakPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn is_drop_items(&self) -> bool {
        self.drop_items
    }

    pub fn exp_to_drop(&self) -> i32 {
        self.exp_to_drop
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_drop_items(&mut self, value: bool) {
        self.patch.drop_items = Some(value);
        self.drop_items = value;
    }

    pub fn set_exp_to_drop(&mut self, value: i32) {
        self.patch.exp_to_drop = Some(value);
        self.exp_to_drop = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockBreak> for Verdict {
    fn from(event: BlockBreak) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockBreakPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockBreak(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockBrush {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
    player: Player,
}

impl BlockBrush {
    fn from_wire(message: v1::BlockBrush, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockBrush> for Verdict {
    fn from(event: BlockBrush) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockBurn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    igniting_block: Option<Block>,
}

impl BlockBurn {
    fn from_wire(message: v1::BlockBurn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            igniting_block: message.igniting_block.map(|value| block(value, link)),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn igniting_block(&self) -> Option<&Block> {
        self.igniting_block.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockBurn> for Verdict {
    fn from(event: BlockBurn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockCanBuild {
    id: u64,
    block: Block,
    block_data: BlockState,
    buildable: bool,
    material: Material,
    player: Option<Player>,
    patch: v1::BlockCanBuildPatch,
}

impl BlockCanBuild {
    fn from_wire(message: v1::BlockCanBuild, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            block_data: BlockState::from_wire(required(message.block_data, "block_data")?)?,
            buildable: message.buildable,
            material: material(message.material)?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
            patch: v1::BlockCanBuildPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_data(&self) -> &BlockState {
        &self.block_data
    }

    pub fn is_buildable(&self) -> bool {
        self.buildable
    }

    pub fn material(&self) -> Material {
        self.material
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub fn set_buildable(&mut self, value: bool) {
        self.patch.buildable = Some(value);
        self.buildable = value;
    }
}

impl From<BlockCanBuild> for Verdict {
    fn from(event: BlockCanBuild) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::BlockCanBuildPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockCanBuild(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockCook {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    result: ItemStack,
    source: ItemStack,
    patch: v1::BlockCookPatch,
}

impl BlockCook {
    fn from_wire(message: v1::BlockCook, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            result: ItemStack::from_wire(required(message.result, "result")?)?,
            source: ItemStack::from_wire(required(message.source, "source")?)?,
            patch: v1::BlockCookPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn result(&self) -> &ItemStack {
        &self.result
    }

    pub fn source(&self) -> &ItemStack {
        &self.source
    }

    pub fn set_result(&mut self, value: ItemStack) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockCook> for Verdict {
    fn from(event: BlockCook) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockCookPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockCook(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockDamage {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    insta_break: bool,
    item_in_hand: ItemStack,
    player: Player,
    patch: v1::BlockDamagePatch,
}

impl BlockDamage {
    fn from_wire(message: v1::BlockDamage, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            insta_break: message.insta_break,
            item_in_hand: ItemStack::from_wire(required(message.item_in_hand, "item_in_hand")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::BlockDamagePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn insta_break(&self) -> bool {
        self.insta_break
    }

    pub fn item_in_hand(&self) -> &ItemStack {
        &self.item_in_hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_insta_break(&mut self, value: bool) {
        self.patch.insta_break = Some(value);
        self.insta_break = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockDamage> for Verdict {
    fn from(event: BlockDamage) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockDamagePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockDamage(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockDamageAbort {
    id: u64,
    block: Block,
    item_in_hand: ItemStack,
    player: Player,
}

impl BlockDamageAbort {
    fn from_wire(message: v1::BlockDamageAbort, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            item_in_hand: ItemStack::from_wire(required(message.item_in_hand, "item_in_hand")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn item_in_hand(&self) -> &ItemStack {
        &self.item_in_hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct BlockDispense {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    item: ItemStack,
    velocity: Vector,
    patch: v1::BlockDispensePatch,
}

impl BlockDispense {
    fn from_wire(message: v1::BlockDispense, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            velocity: vector(required(message.velocity, "velocity")?),
            patch: v1::BlockDispensePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn velocity(&self) -> Vector {
        self.velocity
    }

    pub fn set_item(&mut self, value: ItemStack) {
        self.patch.item = Some(value.to_wire());
        self.item = value;
    }

    pub fn set_velocity(&mut self, value: Vector) {
        self.patch.velocity = Some(value.to_wire());
        self.velocity = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockDispense> for Verdict {
    fn from(event: BlockDispense) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockDispensePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockDispense(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockDispenseArmor {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    item: ItemStack,
    target_entity: Entity,
    velocity: Vector,
    patch: v1::BlockDispenseArmorPatch,
}

impl BlockDispenseArmor {
    fn from_wire(message: v1::BlockDispenseArmor, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            target_entity: Entity::from_wire(
                required(message.target_entity, "target_entity")?,
                link,
            )?,
            velocity: vector(required(message.velocity, "velocity")?),
            patch: v1::BlockDispenseArmorPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn target_entity(&self) -> &Entity {
        &self.target_entity
    }

    pub fn velocity(&self) -> Vector {
        self.velocity
    }

    pub fn set_item(&mut self, value: ItemStack) {
        self.patch.item = Some(value.to_wire());
        self.item = value;
    }

    pub fn set_velocity(&mut self, value: Vector) {
        self.patch.velocity = Some(value.to_wire());
        self.velocity = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockDispenseArmor> for Verdict {
    fn from(event: BlockDispenseArmor) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockDispenseArmorPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockDispenseArmor(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockDispenseLoot {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    dispensed_loot: Vec<ItemStack>,
    player: Option<Player>,
    dispensed_loot_snapshot: Option<Vec<ItemStack>>,
    patch: v1::BlockDispenseLootPatch,
}

impl BlockDispenseLoot {
    fn from_wire(message: v1::BlockDispenseLoot, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            dispensed_loot: message
                .dispensed_loot
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
            dispensed_loot_snapshot: None,
            patch: v1::BlockDispenseLootPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn dispensed_loot(&self) -> &[ItemStack] {
        &self.dispensed_loot
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub fn set_dispensed_loot(&mut self, value: Vec<ItemStack>) {
        let snapshot = self
            .dispensed_loot_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.dispensed_loot));
        self.patch.dispensed_loot = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_item,
        )
        .map(|diff| v1::ItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::ItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.dispensed_loot = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockDispenseLoot> for Verdict {
    fn from(event: BlockDispenseLoot) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockDispenseLootPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockDispenseLoot(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockDropItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_state: BlockSnapshot,
    items: Vec<DroppedItem>,
    player: Player,
    items_snapshot: Option<Vec<DroppedItem>>,
    patch: v1::BlockDropItemPatch,
}

impl BlockDropItem {
    fn from_wire(message: v1::BlockDropItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_state: BlockSnapshot::from_wire(
                required(message.block_state, "block_state")?,
                link,
            )?,
            items: message
                .items
                .into_iter()
                .map(|value| DroppedItem::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            items_snapshot: None,
            patch: v1::BlockDropItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_state(&self) -> &BlockSnapshot {
        &self.block_state
    }

    pub fn items(&self) -> &[DroppedItem] {
        &self.items
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_items(&mut self, value: Vec<DroppedItem>) {
        let snapshot = self
            .items_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.items));
        self.patch.items = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_entity,
        )
        .map(|diff| v1::DroppedItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::DroppedItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.items = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockDropItem> for Verdict {
    fn from(event: BlockDropItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockDropItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockDropItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockExp {
    id: u64,
    block: Block,
    exp_to_drop: i32,
    patch: v1::BlockExpPatch,
}

impl BlockExp {
    fn from_wire(message: v1::BlockExp, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            exp_to_drop: message.exp_to_drop,
            patch: v1::BlockExpPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn exp_to_drop(&self) -> i32 {
        self.exp_to_drop
    }

    pub fn set_exp_to_drop(&mut self, value: i32) {
        self.patch.exp_to_drop = Some(value);
        self.exp_to_drop = value;
    }
}

impl From<BlockExp> for Verdict {
    fn from(event: BlockExp) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::BlockExpPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockExp(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockExplode {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_list: Vec<Block>,
    exploded_block_state: BlockSnapshot,
    explosion_result: ExplosionResult,
    explosion_yield: f32,
    block_list_snapshot: Option<Vec<Block>>,
    patch: v1::BlockExplodePatch,
}

impl BlockExplode {
    fn from_wire(message: v1::BlockExplode, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_list: message
                .block_list
                .into_iter()
                .map(|value| block(value, link))
                .collect(),
            exploded_block_state: BlockSnapshot::from_wire(
                required(message.exploded_block_state, "exploded_block_state")?,
                link,
            )?,
            explosion_result: ExplosionResult::from_wire(message.explosion_result)?,
            explosion_yield: message.explosion_yield,
            block_list_snapshot: None,
            patch: v1::BlockExplodePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_list(&self) -> &[Block] {
        &self.block_list
    }

    pub fn exploded_block_state(&self) -> &BlockSnapshot {
        &self.exploded_block_state
    }

    pub fn explosion_result(&self) -> ExplosionResult {
        self.explosion_result
    }

    pub fn explosion_yield(&self) -> f32 {
        self.explosion_yield
    }

    pub fn set_block_list(&mut self, value: Vec<Block>) {
        let snapshot = self
            .block_list_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.block_list));
        self.patch.block_list = diff::list(
            &snapshot
                .iter()
                .map(|value| value.pos.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.pos.to_wire()).collect(),
        )
        .map(|diff| v1::BlockPosListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::BlockPosReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.block_list = value;
    }

    pub fn set_yield(&mut self, value: f32) {
        self.patch.explosion_yield = Some(value);
        self.explosion_yield = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockExplode> for Verdict {
    fn from(event: BlockExplode) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockExplodePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockExplode(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockFade {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
}

impl BlockFade {
    fn from_wire(message: v1::BlockFade, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockFade> for Verdict {
    fn from(event: BlockFade) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockFertilize {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    blocks: Vec<BlockSnapshot>,
    player: Option<Player>,
}

impl BlockFertilize {
    fn from_wire(message: v1::BlockFertilize, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            blocks: message
                .blocks
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn blocks(&self) -> &[BlockSnapshot] {
        &self.blocks
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockFertilize> for Verdict {
    fn from(event: BlockFertilize) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockForm {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
}

impl BlockForm {
    fn from_wire(message: v1::BlockForm, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockForm> for Verdict {
    fn from(event: BlockForm) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockFromTo {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    face: Face,
    to_block: Block,
}

impl BlockFromTo {
    fn from_wire(message: v1::BlockFromTo, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            face: Face::from_wire(message.face)?,
            to_block: block(required(message.to_block, "to_block")?, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn face(&self) -> Face {
        self.face
    }

    pub fn to_block(&self) -> &Block {
        &self.to_block
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockFromTo> for Verdict {
    fn from(event: BlockFromTo) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockGrow {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
}

impl BlockGrow {
    fn from_wire(message: v1::BlockGrow, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockGrow> for Verdict {
    fn from(event: BlockGrow) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockIgnite {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    cause: BlockIgniteCause,
    igniting_block: Option<Block>,
    igniting_entity: Option<Entity>,
    player: Option<Player>,
}

impl BlockIgnite {
    fn from_wire(message: v1::BlockIgnite, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            cause: BlockIgniteCause::from_wire(message.cause)?,
            igniting_block: message.igniting_block.map(|value| block(value, link)),
            igniting_entity: message
                .igniting_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn cause(&self) -> BlockIgniteCause {
        self.cause
    }

    pub fn igniting_block(&self) -> Option<&Block> {
        self.igniting_block.as_ref()
    }

    pub fn igniting_entity(&self) -> Option<&Entity> {
        self.igniting_entity.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockIgnite> for Verdict {
    fn from(event: BlockIgnite) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockMultiPlace {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_against: Block,
    block_placed: Block,
    block_replaced_state: BlockSnapshot,
    build: bool,
    hand: EquipmentSlot,
    item_in_hand: ItemStack,
    player: Player,
    replaced_block_states: Vec<BlockSnapshot>,
    placed_state: BlockSnapshot,
    placed_states: Vec<BlockSnapshot>,
    patch: v1::BlockMultiPlacePatch,
}

impl BlockMultiPlace {
    fn from_wire(message: v1::BlockMultiPlace, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_against: block(required(message.block_against, "block_against")?, link),
            block_placed: block(required(message.block_placed, "block_placed")?, link),
            block_replaced_state: BlockSnapshot::from_wire(
                required(message.block_replaced_state, "block_replaced_state")?,
                link,
            )?,
            build: message.build,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item_in_hand: ItemStack::from_wire(required(message.item_in_hand, "item_in_hand")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            replaced_block_states: message
                .replaced_block_states
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            placed_state: BlockSnapshot::from_wire(
                required(message.placed_state, "placed_state")?,
                link,
            )?,
            placed_states: message
                .placed_states
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            patch: v1::BlockMultiPlacePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_against(&self) -> &Block {
        &self.block_against
    }

    pub fn block_placed(&self) -> &Block {
        &self.block_placed
    }

    pub fn block_replaced_state(&self) -> &BlockSnapshot {
        &self.block_replaced_state
    }

    pub fn can_build(&self) -> bool {
        self.build
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item_in_hand(&self) -> &ItemStack {
        &self.item_in_hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn replaced_block_states(&self) -> &[BlockSnapshot] {
        &self.replaced_block_states
    }

    pub fn placed_state(&self) -> &BlockSnapshot {
        &self.placed_state
    }

    pub fn placed_states(&self) -> &[BlockSnapshot] {
        &self.placed_states
    }

    pub fn set_build(&mut self, value: bool) {
        self.patch.build = Some(value);
        self.build = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockMultiPlace> for Verdict {
    fn from(event: BlockMultiPlace) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockMultiPlacePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockMultiPlace(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockPhysics {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    changed_type: Material,
    source_block: Block,
}

impl BlockPhysics {
    fn from_wire(message: v1::BlockPhysics, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            changed_type: material(message.changed_type)?,
            source_block: block(required(message.source_block, "source_block")?, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn changed_type(&self) -> Material {
        self.changed_type
    }

    pub fn source_block(&self) -> &Block {
        &self.source_block
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockPhysics> for Verdict {
    fn from(event: BlockPhysics) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockPistonExtend {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    blocks: Vec<Block>,
    direction: Face,
    sticky: bool,
}

impl BlockPistonExtend {
    fn from_wire(message: v1::BlockPistonExtend, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            blocks: message
                .blocks
                .into_iter()
                .map(|value| block(value, link))
                .collect(),
            direction: Face::from_wire(message.direction)?,
            sticky: message.sticky,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn direction(&self) -> Face {
        self.direction
    }

    pub fn is_sticky(&self) -> bool {
        self.sticky
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockPistonExtend> for Verdict {
    fn from(event: BlockPistonExtend) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockPistonRetract {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    blocks: Vec<Block>,
    direction: Face,
    sticky: bool,
}

impl BlockPistonRetract {
    fn from_wire(message: v1::BlockPistonRetract, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            blocks: message
                .blocks
                .into_iter()
                .map(|value| block(value, link))
                .collect(),
            direction: Face::from_wire(message.direction)?,
            sticky: message.sticky,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn direction(&self) -> Face {
        self.direction
    }

    pub fn is_sticky(&self) -> bool {
        self.sticky
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockPistonRetract> for Verdict {
    fn from(event: BlockPistonRetract) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockPlace {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_against: Block,
    block_placed: Block,
    block_replaced_state: BlockSnapshot,
    build: bool,
    hand: EquipmentSlot,
    item_in_hand: ItemStack,
    player: Player,
    placed_state: BlockSnapshot,
    patch: v1::BlockPlacePatch,
}

impl BlockPlace {
    fn from_wire(message: v1::BlockPlace, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_against: block(required(message.block_against, "block_against")?, link),
            block_placed: block(required(message.block_placed, "block_placed")?, link),
            block_replaced_state: BlockSnapshot::from_wire(
                required(message.block_replaced_state, "block_replaced_state")?,
                link,
            )?,
            build: message.build,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item_in_hand: ItemStack::from_wire(required(message.item_in_hand, "item_in_hand")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            placed_state: BlockSnapshot::from_wire(
                required(message.placed_state, "placed_state")?,
                link,
            )?,
            patch: v1::BlockPlacePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_against(&self) -> &Block {
        &self.block_against
    }

    pub fn block_placed(&self) -> &Block {
        &self.block_placed
    }

    pub fn block_replaced_state(&self) -> &BlockSnapshot {
        &self.block_replaced_state
    }

    pub fn can_build(&self) -> bool {
        self.build
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item_in_hand(&self) -> &ItemStack {
        &self.item_in_hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn placed_state(&self) -> &BlockSnapshot {
        &self.placed_state
    }

    pub fn set_build(&mut self, value: bool) {
        self.patch.build = Some(value);
        self.build = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockPlace> for Verdict {
    fn from(event: BlockPlace) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BlockPlacePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockPlace(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockReceiveGame {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Option<Entity>,
    event: String,
}

impl BlockReceiveGame {
    fn from_wire(message: v1::BlockReceiveGame, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            event: message.event,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub fn event(&self) -> &str {
        &self.event
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockReceiveGame> for Verdict {
    fn from(event: BlockReceiveGame) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockRedstone {
    id: u64,
    block: Block,
    new_current: i32,
    old_current: i32,
    patch: v1::BlockRedstonePatch,
}

impl BlockRedstone {
    fn from_wire(message: v1::BlockRedstone, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            new_current: message.new_current,
            old_current: message.old_current,
            patch: v1::BlockRedstonePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_current(&self) -> i32 {
        self.new_current
    }

    pub fn old_current(&self) -> i32 {
        self.old_current
    }

    pub fn set_new_current(&mut self, value: i32) {
        self.patch.new_current = Some(value);
        self.new_current = value;
    }
}

impl From<BlockRedstone> for Verdict {
    fn from(event: BlockRedstone) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::BlockRedstonePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BlockRedstone(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BlockShearEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Entity,
    tool: ItemStack,
}

impl BlockShearEntity {
    fn from_wire(message: v1::BlockShearEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            tool: ItemStack::from_wire(required(message.tool, "tool")?)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn tool(&self) -> &ItemStack {
        &self.tool
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockShearEntity> for Verdict {
    fn from(event: BlockShearEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BlockSpread {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
    source: Block,
}

impl BlockSpread {
    fn from_wire(message: v1::BlockSpread, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
            source: block(required(message.source, "source")?, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub fn source(&self) -> &Block {
        &self.source
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BlockSpread> for Verdict {
    fn from(event: BlockSpread) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct Brew {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    contents: InventoryRef,
    fuel_level: i32,
    results: Vec<ItemStack>,
}

impl Brew {
    fn from_wire(message: v1::Brew, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            contents: InventoryRef::from_wire(required(message.contents, "contents")?, link)?,
            fuel_level: message.fuel_level,
            results: message
                .results
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn contents(&self) -> &InventoryRef {
        &self.contents
    }

    pub fn fuel_level(&self) -> i32 {
        self.fuel_level
    }

    pub fn results(&self) -> &[ItemStack] {
        &self.results
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<Brew> for Verdict {
    fn from(event: Brew) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct BrewingStandFuel {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    consuming: bool,
    fuel: ItemStack,
    fuel_power: i32,
    patch: v1::BrewingStandFuelPatch,
}

impl BrewingStandFuel {
    fn from_wire(message: v1::BrewingStandFuel, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            consuming: message.consuming,
            fuel: ItemStack::from_wire(required(message.fuel, "fuel")?)?,
            fuel_power: message.fuel_power,
            patch: v1::BrewingStandFuelPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn is_consuming(&self) -> bool {
        self.consuming
    }

    pub fn fuel(&self) -> &ItemStack {
        &self.fuel
    }

    pub fn fuel_power(&self) -> i32 {
        self.fuel_power
    }

    pub fn set_consuming(&mut self, value: bool) {
        self.patch.consuming = Some(value);
        self.consuming = value;
    }

    pub fn set_fuel_power(&mut self, value: i32) {
        self.patch.fuel_power = Some(value);
        self.fuel_power = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BrewingStandFuel> for Verdict {
    fn from(event: BrewingStandFuel) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BrewingStandFuelPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BrewingStandFuel(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BrewingStart {
    id: u64,
    block: Block,
    source: ItemStack,
    total_brew_time: i32,
    patch: v1::BrewingStartPatch,
}

impl BrewingStart {
    fn from_wire(message: v1::BrewingStart, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            source: ItemStack::from_wire(required(message.source, "source")?)?,
            total_brew_time: message.total_brew_time,
            patch: v1::BrewingStartPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn source(&self) -> &ItemStack {
        &self.source
    }

    pub fn total_brew_time(&self) -> i32 {
        self.total_brew_time
    }

    pub fn set_total_brew_time(&mut self, value: i32) {
        self.patch.total_brew_time = Some(value);
        self.total_brew_time = value;
    }
}

impl From<BrewingStart> for Verdict {
    fn from(event: BrewingStart) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::BrewingStartPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BrewingStart(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct BroadcastMessage {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    message: String,
    recipients: Vec<CommandSender>,
    recipients_snapshot: Option<Vec<CommandSender>>,
    patch: v1::BroadcastMessagePatch,
}

impl BroadcastMessage {
    fn from_wire(message: v1::BroadcastMessage, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            message: message.message,
            recipients: message
                .recipients
                .into_iter()
                .map(|value| CommandSender::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            recipients_snapshot: None,
            patch: v1::BroadcastMessagePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn recipients(&self) -> &[CommandSender] {
        &self.recipients
    }

    pub fn set_message(&mut self, value: String) {
        self.patch.message = Some(value.clone());
        self.message = value;
    }

    pub fn set_recipients(&mut self, value: Vec<CommandSender>) {
        let snapshot = self
            .recipients_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.recipients));
        self.patch.recipients = diff::list(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
        )
        .map(|diff| v1::CommandSenderListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::CommandSenderReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.recipients = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<BroadcastMessage> for Verdict {
    fn from(event: BroadcastMessage) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::BroadcastMessagePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::BroadcastMessage(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct CampfireStart {
    id: u64,
    block: Block,
    recipe: String,
    source: ItemStack,
    total_cook_time: i32,
    patch: v1::CampfireStartPatch,
}

impl CampfireStart {
    fn from_wire(message: v1::CampfireStart, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            recipe: message.recipe,
            source: ItemStack::from_wire(required(message.source, "source")?)?,
            total_cook_time: message.total_cook_time,
            patch: v1::CampfireStartPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub fn source(&self) -> &ItemStack {
        &self.source
    }

    pub fn total_cook_time(&self) -> i32 {
        self.total_cook_time
    }

    pub fn set_total_cook_time(&mut self, value: i32) {
        self.patch.total_cook_time = Some(value);
        self.total_cook_time = value;
    }
}

impl From<CampfireStart> for Verdict {
    fn from(event: CampfireStart) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::CampfireStartPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::CampfireStart(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct CauldronLevelChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Option<Entity>,
    new_state: BlockSnapshot,
    reason: CauldronLevelChangeReason,
}

impl CauldronLevelChange {
    fn from_wire(message: v1::CauldronLevelChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
            reason: CauldronLevelChangeReason::from_wire(message.reason)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub fn reason(&self) -> CauldronLevelChangeReason {
        self.reason
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CauldronLevelChange> for Verdict {
    fn from(event: CauldronLevelChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkLoad {
    id: u64,
    chunk: ChunkRef,
    new_chunk: bool,
    world: World,
}

impl ChunkLoad {
    fn from_wire(message: v1::ChunkLoad, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            chunk: ChunkRef::from_wire(required(message.chunk, "chunk")?, link)?,
            new_chunk: message.new_chunk,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn chunk(&self) -> &ChunkRef {
        &self.chunk
    }

    pub fn is_new_chunk(&self) -> bool {
        self.new_chunk
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct ChunkPopulate {
    id: u64,
    chunk: ChunkRef,
    world: World,
}

impl ChunkPopulate {
    fn from_wire(message: v1::ChunkPopulate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            chunk: ChunkRef::from_wire(required(message.chunk, "chunk")?, link)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn chunk(&self) -> &ChunkRef {
        &self.chunk
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct ChunkUnload {
    id: u64,
    chunk: ChunkRef,
    save_chunk: bool,
    world: World,
    patch: v1::ChunkUnloadPatch,
}

impl ChunkUnload {
    fn from_wire(message: v1::ChunkUnload, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            chunk: ChunkRef::from_wire(required(message.chunk, "chunk")?, link)?,
            save_chunk: message.save_chunk,
            world: world(message.world, link),
            patch: v1::ChunkUnloadPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn chunk(&self) -> &ChunkRef {
        &self.chunk
    }

    pub fn is_save_chunk(&self) -> bool {
        self.save_chunk
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn set_save_chunk(&mut self, value: bool) {
        self.patch.save_chunk = Some(value);
        self.save_chunk = value;
    }
}

impl From<ChunkUnload> for Verdict {
    fn from(event: ChunkUnload) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::ChunkUnloadPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ChunkUnload(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct CraftItem {
    id: u64,
    rolled_back: bool,
    action: InventoryAction,
    click: ClickKind,
    clicked_inventory: Option<InventoryRef>,
    current_item: Option<ItemStack>,
    cursor: Option<ItemStack>,
    hotbar_button: i32,
    inventory: InventoryRef,
    left_click: bool,
    raw_slot: i32,
    recipe: String,
    result: EventResult,
    right_click: bool,
    shift_click: bool,
    slot: i32,
    slot_type: InventoryTypeSlotType,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::CraftItemPatch,
}

impl CraftItem {
    fn from_wire(message: v1::CraftItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            action: InventoryAction::from_wire(message.action)?,
            click: ClickKind::from_wire(message.click)?,
            clicked_inventory: message
                .clicked_inventory
                .map(|value| InventoryRef::from_wire(value, link))
                .transpose()?,
            current_item: message.current_item.map(ItemStack::from_wire).transpose()?,
            cursor: message.cursor.map(ItemStack::from_wire).transpose()?,
            hotbar_button: message.hotbar_button,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            left_click: message.left_click,
            raw_slot: message.raw_slot,
            recipe: message.recipe,
            result: EventResult::from_wire(message.result)?,
            right_click: message.right_click,
            shift_click: message.shift_click,
            slot: message.slot,
            slot_type: InventoryTypeSlotType::from_wire(message.slot_type)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::CraftItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> InventoryAction {
        self.action
    }

    pub fn click(&self) -> ClickKind {
        self.click
    }

    pub fn clicked_inventory(&self) -> Option<&InventoryRef> {
        self.clicked_inventory.as_ref()
    }

    pub fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }

    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    pub fn hotbar_button(&self) -> i32 {
        self.hotbar_button
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn is_left_click(&self) -> bool {
        self.left_click
    }

    pub fn raw_slot(&self) -> i32 {
        self.raw_slot
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn is_right_click(&self) -> bool {
        self.right_click
    }

    pub fn is_shift_click(&self) -> bool {
        self.shift_click
    }

    pub fn slot(&self) -> i32 {
        self.slot
    }

    pub fn slot_type(&self) -> InventoryTypeSlotType {
        self.slot_type
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_current_item(&mut self, value: Option<ItemStack>) {
        self.patch.current_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.current_item = value;
    }

    pub fn set_cursor(&mut self, value: Option<ItemStack>) {
        self.patch.cursor = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.cursor = value;
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CraftItem> for Verdict {
    fn from(event: CraftItem) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::CraftItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::CraftItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct CrafterCraft {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    recipe: String,
    remaining_items: Vec<ItemStack>,
    result: ItemStack,
    patch: v1::CrafterCraftPatch,
}

impl CrafterCraft {
    fn from_wire(message: v1::CrafterCraft, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            recipe: message.recipe,
            remaining_items: message
                .remaining_items
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            result: ItemStack::from_wire(required(message.result, "result")?)?,
            patch: v1::CrafterCraftPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub fn remaining_items(&self) -> &[ItemStack] {
        &self.remaining_items
    }

    pub fn result(&self) -> &ItemStack {
        &self.result
    }

    pub fn set_result(&mut self, value: ItemStack) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CrafterCraft> for Verdict {
    fn from(event: CrafterCraft) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::CrafterCraftPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::CrafterCraft(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct CreatureSpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
    spawn_reason: CreatureSpawnReason,
}

impl CreatureSpawn {
    fn from_wire(message: v1::CreatureSpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            spawn_reason: CreatureSpawnReason::from_wire(message.spawn_reason)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn spawn_reason(&self) -> CreatureSpawnReason {
        self.spawn_reason
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CreatureSpawn> for Verdict {
    fn from(event: CreatureSpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct CreeperPower {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: CreeperPowerCause,
    entity: Entity,
    entity_type: EntityType,
    lightning: Option<Entity>,
}

impl CreeperPower {
    fn from_wire(message: v1::CreeperPower, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: CreeperPowerCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            lightning: message
                .lightning
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> CreeperPowerCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn lightning(&self) -> Option<&Entity> {
        self.lightning.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CreeperPower> for Verdict {
    fn from(event: CreeperPower) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct CubeMobSplit {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    count: i32,
    entity: Entity,
    entity_type: EntityType,
    patch: v1::CubeMobSplitPatch,
}

impl CubeMobSplit {
    fn from_wire(message: v1::CubeMobSplit, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            count: message.count,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            patch: v1::CubeMobSplitPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_count(&mut self, value: i32) {
        self.patch.count = Some(value);
        self.count = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<CubeMobSplit> for Verdict {
    fn from(event: CubeMobSplit) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::CubeMobSplitPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::CubeMobSplit(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EnchantItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    enchant_block: Block,
    enchanter: Player,
    enchantment_hint: Enchantment,
    enchants_to_add: Vec<(Enchantment, u32)>,
    exp_level_cost: i32,
    inventory: InventoryRef,
    item: ItemStack,
    level_hint: i32,
    view: InventoryView,
    viewers: Vec<Player>,
    which_button: i32,
    patch: v1::EnchantItemPatch,
}

impl EnchantItem {
    fn from_wire(message: v1::EnchantItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            enchant_block: block(required(message.enchant_block, "enchant_block")?, link),
            enchanter: Player::from_snapshot(required(message.enchanter, "enchanter")?, link)?,
            enchantment_hint: enchantment(message.enchantment_hint)?,
            enchants_to_add: message
                .enchants_to_add
                .into_iter()
                .map(enchant)
                .collect::<Result<Vec<_>, _>>()?,
            exp_level_cost: message.exp_level_cost,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            level_hint: message.level_hint,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            which_button: message.which_button,
            patch: v1::EnchantItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn enchant_block(&self) -> &Block {
        &self.enchant_block
    }

    pub fn player(&self) -> &Player {
        &self.enchanter
    }

    pub fn enchantment_hint(&self) -> Enchantment {
        self.enchantment_hint
    }

    pub fn enchants_to_add(&self) -> &[(Enchantment, u32)] {
        &self.enchants_to_add
    }

    pub fn exp_level_cost(&self) -> i32 {
        self.exp_level_cost
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn level_hint(&self) -> i32 {
        self.level_hint
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn which_button(&self) -> i32 {
        self.which_button
    }

    pub fn set_exp_level_cost(&mut self, value: i32) {
        self.patch.exp_level_cost = Some(value);
        self.exp_level_cost = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EnchantItem> for Verdict {
    fn from(event: EnchantItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EnchantItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EnchantItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EnderDragonChangePhase {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    current_phase: Option<EnderDragonPhase>,
    entity: Entity,
    entity_type: EntityType,
    new_phase: EnderDragonPhase,
    patch: v1::EnderDragonChangePhasePatch,
}

impl EnderDragonChangePhase {
    fn from_wire(
        message: v1::EnderDragonChangePhase,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            current_phase: message
                .current_phase
                .map(EnderDragonPhase::from_wire)
                .transpose()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            new_phase: EnderDragonPhase::from_wire(message.new_phase)?,
            patch: v1::EnderDragonChangePhasePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn current_phase(&self) -> Option<EnderDragonPhase> {
        self.current_phase
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn new_phase(&self) -> EnderDragonPhase {
        self.new_phase
    }

    pub fn set_new_phase(&mut self, value: EnderDragonPhase) {
        self.patch.new_phase = Some(value.to_wire());
        self.new_phase = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EnderDragonChangePhase> for Verdict {
    fn from(event: EnderDragonChangePhase) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EnderDragonChangePhasePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EnderDragonChangePhase(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntitiesLoad {
    id: u64,
    chunk: ChunkRef,
    entities: Vec<Entity>,
    world: World,
}

impl EntitiesLoad {
    fn from_wire(message: v1::EntitiesLoad, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            chunk: ChunkRef::from_wire(required(message.chunk, "chunk")?, link)?,
            entities: message
                .entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn chunk(&self) -> &ChunkRef {
        &self.chunk
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct EntitiesUnload {
    id: u64,
    chunk: ChunkRef,
    entities: Vec<Entity>,
    world: World,
}

impl EntitiesUnload {
    fn from_wire(message: v1::EntitiesUnload, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            chunk: ChunkRef::from_wire(required(message.chunk, "chunk")?, link)?,
            entities: message
                .entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn chunk(&self) -> &ChunkRef {
        &self.chunk
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct EntityAirChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    amount: i32,
    entity: Entity,
    entity_type: EntityType,
    patch: v1::EntityAirChangePatch,
}

impl EntityAirChange {
    fn from_wire(message: v1::EntityAirChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            amount: message.amount,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            patch: v1::EntityAirChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn amount(&self) -> i32 {
        self.amount
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_amount(&mut self, value: i32) {
        self.patch.amount = Some(value);
        self.amount = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityAirChange> for Verdict {
    fn from(event: EntityAirChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityAirChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityAirChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityBlockForm {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Entity,
    new_state: BlockSnapshot,
}

impl EntityBlockForm {
    fn from_wire(message: v1::EntityBlockForm, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityBlockForm> for Verdict {
    fn from(event: EntityBlockForm) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityBreakDoor {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_data: BlockState,
    entity: Entity,
    entity_type: EntityType,
    to: Material,
}

impl EntityBreakDoor {
    fn from_wire(message: v1::EntityBreakDoor, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_data: BlockState::from_wire(required(message.block_data, "block_data")?)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            to: material(message.to)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_data(&self) -> &BlockState {
        &self.block_data
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn to(&self) -> Material {
        self.to
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityBreakDoor> for Verdict {
    fn from(event: EntityBreakDoor) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityBreed {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    bred_with: Option<ItemStack>,
    breeder: Option<Entity>,
    entity: Entity,
    entity_type: EntityType,
    experience: i32,
    father: Entity,
    mother: Entity,
    patch: v1::EntityBreedPatch,
}

impl EntityBreed {
    fn from_wire(message: v1::EntityBreed, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            bred_with: message.bred_with.map(ItemStack::from_wire).transpose()?,
            breeder: message
                .breeder
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            experience: message.experience,
            father: Entity::from_wire(required(message.father, "father")?, link)?,
            mother: Entity::from_wire(required(message.mother, "mother")?, link)?,
            patch: v1::EntityBreedPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bred_with(&self) -> Option<&ItemStack> {
        self.bred_with.as_ref()
    }

    pub fn breeder(&self) -> Option<&Entity> {
        self.breeder.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.breeder
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn experience(&self) -> i32 {
        self.experience
    }

    pub fn father(&self) -> &Entity {
        &self.father
    }

    pub fn mother(&self) -> &Entity {
        &self.mother
    }

    pub fn set_experience(&mut self, value: i32) {
        self.patch.experience = Some(value);
        self.experience = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityBreed> for Verdict {
    fn from(event: EntityBreed) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityBreedPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityBreed(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityChangeBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_data: BlockState,
    entity: Entity,
    entity_type: EntityType,
    to: Material,
}

impl EntityChangeBlock {
    fn from_wire(message: v1::EntityChangeBlock, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_data: BlockState::from_wire(required(message.block_data, "block_data")?)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            to: material(message.to)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_data(&self) -> &BlockState {
        &self.block_data
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn to(&self) -> Material {
        self.to
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityChangeBlock> for Verdict {
    fn from(event: EntityChangeBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityCombust {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    duration: f32,
    entity: Entity,
    entity_type: EntityType,
    patch: v1::EntityCombustPatch,
}

impl EntityCombust {
    fn from_wire(message: v1::EntityCombust, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            duration: message.duration,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            patch: v1::EntityCombustPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_duration(&mut self, value: f32) {
        self.patch.duration = Some(value);
        self.duration = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityCombust> for Verdict {
    fn from(event: EntityCombust) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityCombustPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityCombust(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityCombustByBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    combuster: Option<Block>,
    duration: f32,
    entity: Entity,
    entity_type: EntityType,
    patch: v1::EntityCombustByBlockPatch,
}

impl EntityCombustByBlock {
    fn from_wire(
        message: v1::EntityCombustByBlock,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            combuster: message.combuster.map(|value| block(value, link)),
            duration: message.duration,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            patch: v1::EntityCombustByBlockPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn combuster(&self) -> Option<&Block> {
        self.combuster.as_ref()
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_duration(&mut self, value: f32) {
        self.patch.duration = Some(value);
        self.duration = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityCombustByBlock> for Verdict {
    fn from(event: EntityCombustByBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityCombustByBlockPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityCombustByBlock(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityCombustByEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    combuster: Entity,
    duration: f32,
    entity: Entity,
    entity_type: EntityType,
    patch: v1::EntityCombustByEntityPatch,
}

impl EntityCombustByEntity {
    fn from_wire(
        message: v1::EntityCombustByEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            combuster: Entity::from_wire(required(message.combuster, "combuster")?, link)?,
            duration: message.duration,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            patch: v1::EntityCombustByEntityPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn combuster(&self) -> &Entity {
        &self.combuster
    }

    pub fn player(&self) -> Option<&Player> {
        self.combuster.player.as_ref()
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_duration(&mut self, value: f32) {
        self.patch.duration = Some(value);
        self.duration = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityCombustByEntity> for Verdict {
    fn from(event: EntityCombustByEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityCombustByEntityPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityCombustByEntity(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityDamage {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: EntityDamageCause,
    damage: f64,
    damage_source: DamageSource,
    entity: Entity,
    entity_type: EntityType,
    final_damage: f64,
    patch: v1::EntityDamagePatch,
}

impl EntityDamage {
    fn from_wire(message: v1::EntityDamage, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: EntityDamageCause::from_wire(message.cause)?,
            damage: message.damage,
            damage_source: DamageSource::from_wire(
                required(message.damage_source, "damage_source")?,
                link,
            )?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            final_damage: message.final_damage,
            patch: v1::EntityDamagePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityDamageCause {
        self.cause
    }

    pub fn damage(&self) -> f64 {
        self.damage
    }

    pub fn damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn final_damage(&self) -> f64 {
        self.final_damage
    }

    pub fn set_damage(&mut self, value: f64) {
        self.patch.damage = Some(value);
        self.damage = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityDamage> for Verdict {
    fn from(event: EntityDamage) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityDamagePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityDamage(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityDamageByBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: EntityDamageCause,
    damage: f64,
    damage_source: DamageSource,
    damager: Option<Block>,
    damager_block_state: Option<BlockSnapshot>,
    entity: Entity,
    entity_type: EntityType,
    final_damage: f64,
    patch: v1::EntityDamageByBlockPatch,
}

impl EntityDamageByBlock {
    fn from_wire(message: v1::EntityDamageByBlock, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: EntityDamageCause::from_wire(message.cause)?,
            damage: message.damage,
            damage_source: DamageSource::from_wire(
                required(message.damage_source, "damage_source")?,
                link,
            )?,
            damager: message.damager.map(|value| block(value, link)),
            damager_block_state: message
                .damager_block_state
                .map(|value| BlockSnapshot::from_wire(value, link))
                .transpose()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            final_damage: message.final_damage,
            patch: v1::EntityDamageByBlockPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityDamageCause {
        self.cause
    }

    pub fn damage(&self) -> f64 {
        self.damage
    }

    pub fn damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    pub fn damager(&self) -> Option<&Block> {
        self.damager.as_ref()
    }

    pub fn damager_block_state(&self) -> Option<&BlockSnapshot> {
        self.damager_block_state.as_ref()
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn final_damage(&self) -> f64 {
        self.final_damage
    }

    pub fn set_damage(&mut self, value: f64) {
        self.patch.damage = Some(value);
        self.damage = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityDamageByBlock> for Verdict {
    fn from(event: EntityDamageByBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityDamageByBlockPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityDamageByBlock(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityDamageByEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: EntityDamageCause,
    damage: f64,
    damage_source: DamageSource,
    damager: Entity,
    entity: Entity,
    entity_type: EntityType,
    final_damage: f64,
    patch: v1::EntityDamageByEntityPatch,
}

impl EntityDamageByEntity {
    fn from_wire(
        message: v1::EntityDamageByEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: EntityDamageCause::from_wire(message.cause)?,
            damage: message.damage,
            damage_source: DamageSource::from_wire(
                required(message.damage_source, "damage_source")?,
                link,
            )?,
            damager: Entity::from_wire(required(message.damager, "damager")?, link)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            final_damage: message.final_damage,
            patch: v1::EntityDamageByEntityPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityDamageCause {
        self.cause
    }

    pub fn damage(&self) -> f64 {
        self.damage
    }

    pub fn damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    pub fn damager(&self) -> &Entity {
        &self.damager
    }

    pub fn player(&self) -> Option<&Player> {
        self.damager.player.as_ref()
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn final_damage(&self) -> f64 {
        self.final_damage
    }

    pub fn set_damage(&mut self, value: f64) {
        self.patch.damage = Some(value);
        self.damage = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityDamageByEntity> for Verdict {
    fn from(event: EntityDamageByEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityDamageByEntityPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityDamageByEntity(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityDeath {
    id: u64,
    damage_source: DamageSource,
    dropped_exp: i32,
    drops: Vec<ItemStack>,
    entity: Entity,
    entity_type: EntityType,
    drops_snapshot: Option<Vec<ItemStack>>,
    patch: v1::EntityDeathPatch,
}

impl EntityDeath {
    fn from_wire(message: v1::EntityDeath, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            damage_source: DamageSource::from_wire(
                required(message.damage_source, "damage_source")?,
                link,
            )?,
            dropped_exp: message.dropped_exp,
            drops: message
                .drops
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            drops_snapshot: None,
            patch: v1::EntityDeathPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    pub fn dropped_exp(&self) -> i32 {
        self.dropped_exp
    }

    pub fn drops(&self) -> &[ItemStack] {
        &self.drops
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn set_dropped_exp(&mut self, value: i32) {
        self.patch.dropped_exp = Some(value);
        self.dropped_exp = value;
    }

    pub fn set_drops(&mut self, value: Vec<ItemStack>) {
        let snapshot = self
            .drops_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.drops));
        self.patch.drops = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_item,
        )
        .map(|diff| v1::ItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::ItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.drops = value;
    }
}

impl From<EntityDeath> for Verdict {
    fn from(event: EntityDeath) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::EntityDeathPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityDeath(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityDismount {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    dismounted: Entity,
    entity: Entity,
    entity_type: EntityType,
}

impl EntityDismount {
    fn from_wire(message: v1::EntityDismount, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            dismounted: Entity::from_wire(required(message.dismounted, "dismounted")?, link)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn dismounted(&self) -> &Entity {
        &self.dismounted
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityDismount> for Verdict {
    fn from(event: EntityDismount) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityDropItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    item_drop: Entity,
}

impl EntityDropItem {
    fn from_wire(message: v1::EntityDropItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            item_drop: Entity::from_wire(required(message.item_drop, "item_drop")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn item_drop(&self) -> &Entity {
        &self.item_drop
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityDropItem> for Verdict {
    fn from(event: EntityDropItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityEnterBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Entity,
    entity_type: EntityType,
}

impl EntityEnterBlock {
    fn from_wire(message: v1::EntityEnterBlock, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityEnterBlock> for Verdict {
    fn from(event: EntityEnterBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityEnterLoveMode {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    human_entity: Option<Player>,
    ticks_in_love: i32,
    patch: v1::EntityEnterLoveModePatch,
}

impl EntityEnterLoveMode {
    fn from_wire(message: v1::EntityEnterLoveMode, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            human_entity: message
                .human_entity
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
            ticks_in_love: message.ticks_in_love,
            patch: v1::EntityEnterLoveModePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn player(&self) -> Option<&Player> {
        self.human_entity.as_ref()
    }

    pub fn ticks_in_love(&self) -> i32 {
        self.ticks_in_love
    }

    pub fn set_ticks_in_love(&mut self, value: i32) {
        self.patch.ticks_in_love = Some(value);
        self.ticks_in_love = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityEnterLoveMode> for Verdict {
    fn from(event: EntityEnterLoveMode) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityEnterLoveModePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityEnterLoveMode(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityExhaustion {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Player,
    entity_type: EntityType,
    exhaustion: f32,
    exhaustion_reason: EntityExhaustionReason,
    patch: v1::EntityExhaustionPatch,
}

impl EntityExhaustion {
    fn from_wire(message: v1::EntityExhaustion, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Player::from_snapshot(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            exhaustion: message.exhaustion,
            exhaustion_reason: EntityExhaustionReason::from_wire(message.exhaustion_reason)?,
            patch: v1::EntityExhaustionPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn exhaustion(&self) -> f32 {
        self.exhaustion
    }

    pub fn exhaustion_reason(&self) -> EntityExhaustionReason {
        self.exhaustion_reason
    }

    pub fn set_exhaustion(&mut self, value: f32) {
        self.patch.exhaustion = Some(value);
        self.exhaustion = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityExhaustion> for Verdict {
    fn from(event: EntityExhaustion) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityExhaustionPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityExhaustion(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityExplode {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block_list: Vec<Block>,
    entity: Entity,
    entity_type: EntityType,
    explosion_result: ExplosionResult,
    location: Location,
    explosion_yield: f32,
    block_list_snapshot: Option<Vec<Block>>,
    patch: v1::EntityExplodePatch,
}

impl EntityExplode {
    fn from_wire(message: v1::EntityExplode, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block_list: message
                .block_list
                .into_iter()
                .map(|value| block(value, link))
                .collect(),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            explosion_result: ExplosionResult::from_wire(message.explosion_result)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            explosion_yield: message.explosion_yield,
            block_list_snapshot: None,
            patch: v1::EntityExplodePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block_list(&self) -> &[Block] {
        &self.block_list
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn explosion_result(&self) -> ExplosionResult {
        self.explosion_result
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn explosion_yield(&self) -> f32 {
        self.explosion_yield
    }

    pub fn set_block_list(&mut self, value: Vec<Block>) {
        let snapshot = self
            .block_list_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.block_list));
        self.patch.block_list = diff::list(
            &snapshot
                .iter()
                .map(|value| value.pos.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.pos.to_wire()).collect(),
        )
        .map(|diff| v1::BlockPosListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::BlockPosReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.block_list = value;
    }

    pub fn set_yield(&mut self, value: f32) {
        self.patch.explosion_yield = Some(value);
        self.explosion_yield = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityExplode> for Verdict {
    fn from(event: EntityExplode) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityExplodePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityExplode(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityInteract {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    entity: Entity,
    entity_type: EntityType,
}

impl EntityInteract {
    fn from_wire(message: v1::EntityInteract, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityInteract> for Verdict {
    fn from(event: EntityInteract) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityKnockback {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: EntityKnockbackCause,
    entity: Entity,
    entity_type: EntityType,
    final_knockback: Vector,
    force: f64,
    knockback: Vector,
    patch: v1::EntityKnockbackPatch,
}

impl EntityKnockback {
    fn from_wire(message: v1::EntityKnockback, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: EntityKnockbackCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            final_knockback: vector(required(message.final_knockback, "final_knockback")?),
            force: message.force,
            knockback: vector(required(message.knockback, "knockback")?),
            patch: v1::EntityKnockbackPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityKnockbackCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn final_knockback(&self) -> Vector {
        self.final_knockback
    }

    pub fn force(&self) -> f64 {
        self.force
    }

    pub fn knockback(&self) -> Vector {
        self.knockback
    }

    pub fn set_final_knockback(&mut self, value: Vector) {
        self.patch.final_knockback = Some(value.to_wire());
        self.final_knockback = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityKnockback> for Verdict {
    fn from(event: EntityKnockback) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityKnockbackPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityKnockback(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityKnockbackByEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: EntityKnockbackCause,
    entity: Entity,
    entity_type: EntityType,
    final_knockback: Vector,
    force: f64,
    knockback: Vector,
    source_entity: Entity,
    patch: v1::EntityKnockbackByEntityPatch,
}

impl EntityKnockbackByEntity {
    fn from_wire(
        message: v1::EntityKnockbackByEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: EntityKnockbackCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            final_knockback: vector(required(message.final_knockback, "final_knockback")?),
            force: message.force,
            knockback: vector(required(message.knockback, "knockback")?),
            source_entity: Entity::from_wire(
                required(message.source_entity, "source_entity")?,
                link,
            )?,
            patch: v1::EntityKnockbackByEntityPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityKnockbackCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn final_knockback(&self) -> Vector {
        self.final_knockback
    }

    pub fn force(&self) -> f64 {
        self.force
    }

    pub fn knockback(&self) -> Vector {
        self.knockback
    }

    pub fn source_entity(&self) -> &Entity {
        &self.source_entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.source_entity.player.as_ref()
    }

    pub fn set_final_knockback(&mut self, value: Vector) {
        self.patch.final_knockback = Some(value.to_wire());
        self.final_knockback = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityKnockbackByEntity> for Verdict {
    fn from(event: EntityKnockbackByEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityKnockbackByEntityPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityKnockbackByEntity(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityMount {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    mount: Entity,
}

impl EntityMount {
    fn from_wire(message: v1::EntityMount, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            mount: Entity::from_wire(required(message.mount, "mount")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn mount(&self) -> &Entity {
        &self.mount
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityMount> for Verdict {
    fn from(event: EntityMount) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityPickupItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    item: Entity,
    remaining: i32,
}

impl EntityPickupItem {
    fn from_wire(message: v1::EntityPickupItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            item: Entity::from_wire(required(message.item, "item")?, link)?,
            remaining: message.remaining,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn item(&self) -> &Entity {
        &self.item
    }

    pub fn remaining(&self) -> i32 {
        self.remaining
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityPickupItem> for Verdict {
    fn from(event: EntityPickupItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityPlace {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_face: Face,
    entity: Entity,
    entity_type: EntityType,
    hand: EquipmentSlot,
    player: Option<Player>,
}

impl EntityPlace {
    fn from_wire(message: v1::EntityPlace, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_face: Face::from_wire(message.block_face)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_face(&self) -> Face {
        self.block_face
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityPlace> for Verdict {
    fn from(event: EntityPlace) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityPortal {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    can_create_portal: bool,
    creation_radius: i32,
    entity: Entity,
    entity_type: EntityType,
    from: Location,
    search_radius: i32,
    to: Option<Location>,
    patch: v1::EntityPortalPatch,
}

impl EntityPortal {
    fn from_wire(message: v1::EntityPortal, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            can_create_portal: message.can_create_portal,
            creation_radius: message.creation_radius,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            search_radius: message.search_radius,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::EntityPortalPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn can_create_portal(&self) -> bool {
        self.can_create_portal
    }

    pub fn creation_radius(&self) -> i32 {
        self.creation_radius
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn search_radius(&self) -> i32 {
        self.search_radius
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_can_create_portal(&mut self, value: bool) {
        self.patch.can_create_portal = Some(value);
        self.can_create_portal = value;
    }

    pub fn set_creation_radius(&mut self, value: i32) {
        self.patch.creation_radius = Some(value);
        self.creation_radius = value;
    }

    pub fn set_search_radius(&mut self, value: i32) {
        self.patch.search_radius = Some(value);
        self.search_radius = value;
    }

    pub fn set_to(&mut self, value: Option<Location>) {
        self.patch.to = Some(v1::NullableLocation {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.to = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityPortal> for Verdict {
    fn from(event: EntityPortal) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityPortalPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityPortal(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityPortalEnter {
    id: u64,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
}

impl EntityPortalEnter {
    fn from_wire(message: v1::EntityPortalEnter, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }
}

#[derive(Debug)]
pub struct EntityPortalExit {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    after: Vector,
    before: Vector,
    entity: Entity,
    entity_type: EntityType,
    from: Location,
    to: Option<Location>,
    patch: v1::EntityPortalExitPatch,
}

impl EntityPortalExit {
    fn from_wire(message: v1::EntityPortalExit, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            after: vector(required(message.after, "after")?),
            before: vector(required(message.before, "before")?),
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::EntityPortalExitPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn after(&self) -> Vector {
        self.after
    }

    pub fn before(&self) -> Vector {
        self.before
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_after(&mut self, value: Vector) {
        self.patch.after = Some(value.to_wire());
        self.after = value;
    }

    pub fn set_to(&mut self, value: Option<Location>) {
        self.patch.to = Some(v1::NullableLocation {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.to = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityPortalExit> for Verdict {
    fn from(event: EntityPortalExit) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityPortalExitPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityPortalExit(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityPoseChange {
    id: u64,
    entity: Entity,
    entity_type: EntityType,
    pose: Pose,
}

impl EntityPoseChange {
    fn from_wire(message: v1::EntityPoseChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            pose: Pose::from_wire(message.pose)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn pose(&self) -> Pose {
        self.pose
    }
}

#[derive(Debug)]
pub struct EntityPotionEffect {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    action: EntityPotionEffectAction,
    cause: EntityPotionEffectCause,
    entity: Entity,
    entity_type: EntityType,
    modified_type: EffectType,
    new_effect: Option<Effect>,
    old_effect: Option<Effect>,
    overrides: bool,
    patch: v1::EntityPotionEffectPatch,
}

impl EntityPotionEffect {
    fn from_wire(message: v1::EntityPotionEffect, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            action: EntityPotionEffectAction::from_wire(message.action)?,
            cause: EntityPotionEffectCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            modified_type: effect_type(message.modified_type)?,
            new_effect: message.new_effect.map(Effect::from_wire).transpose()?,
            old_effect: message.old_effect.map(Effect::from_wire).transpose()?,
            overrides: message.overrides,
            patch: v1::EntityPotionEffectPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> EntityPotionEffectAction {
        self.action
    }

    pub fn cause(&self) -> EntityPotionEffectCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn modified_type(&self) -> EffectType {
        self.modified_type
    }

    pub fn new_effect(&self) -> Option<Effect> {
        self.new_effect
    }

    pub fn old_effect(&self) -> Option<Effect> {
        self.old_effect
    }

    pub fn overrides(&self) -> bool {
        self.overrides
    }

    pub fn set_override(&mut self, value: bool) {
        self.patch.overrides = Some(value);
        self.overrides = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityPotionEffect> for Verdict {
    fn from(event: EntityPotionEffect) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityPotionEffectPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityPotionEffect(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityRegainHealth {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    amount: f64,
    entity: Entity,
    entity_type: EntityType,
    regain_reason: EntityRegainHealthRegainReason,
    patch: v1::EntityRegainHealthPatch,
}

impl EntityRegainHealth {
    fn from_wire(message: v1::EntityRegainHealth, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            amount: message.amount,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            regain_reason: EntityRegainHealthRegainReason::from_wire(message.regain_reason)?,
            patch: v1::EntityRegainHealthPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn amount(&self) -> f64 {
        self.amount
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn regain_reason(&self) -> EntityRegainHealthRegainReason {
        self.regain_reason
    }

    pub fn set_amount(&mut self, value: f64) {
        self.patch.amount = Some(value);
        self.amount = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityRegainHealth> for Verdict {
    fn from(event: EntityRegainHealth) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityRegainHealthPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityRegainHealth(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityRemove {
    id: u64,
    cause: EntityRemoveCause,
    entity: Entity,
    entity_type: EntityType,
}

impl EntityRemove {
    fn from_wire(message: v1::EntityRemove, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cause: EntityRemoveCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> EntityRemoveCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }
}

#[derive(Debug)]
pub struct EntityResurrect {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    hand: Option<EquipmentSlot>,
}

impl EntityResurrect {
    fn from_wire(message: v1::EntityResurrect, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            hand: message.hand.map(EquipmentSlot::from_wire).transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn hand(&self) -> Option<EquipmentSlot> {
        self.hand
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityResurrect> for Verdict {
    fn from(event: EntityResurrect) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityShootBow {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    bow: Option<ItemStack>,
    consumable: Option<ItemStack>,
    consume_item: bool,
    entity: Entity,
    entity_type: EntityType,
    force: f32,
    hand: EquipmentSlot,
    projectile: Entity,
    patch: v1::EntityShootBowPatch,
}

impl EntityShootBow {
    fn from_wire(message: v1::EntityShootBow, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            bow: message.bow.map(ItemStack::from_wire).transpose()?,
            consumable: message.consumable.map(ItemStack::from_wire).transpose()?,
            consume_item: message.consume_item,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            force: message.force,
            hand: EquipmentSlot::from_wire(message.hand)?,
            projectile: Entity::from_wire(required(message.projectile, "projectile")?, link)?,
            patch: v1::EntityShootBowPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bow(&self) -> Option<&ItemStack> {
        self.bow.as_ref()
    }

    pub fn consumable(&self) -> Option<&ItemStack> {
        self.consumable.as_ref()
    }

    pub fn should_consume_item(&self) -> bool {
        self.consume_item
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn force(&self) -> f32 {
        self.force
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn projectile(&self) -> &Entity {
        &self.projectile
    }

    pub fn set_projectile(&mut self, value: Entity) {
        self.patch.projectile = Some(value.to_wire());
        self.projectile = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityShootBow> for Verdict {
    fn from(event: EntityShootBow) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityShootBowPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityShootBow(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntitySpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
}

impl EntitySpawn {
    fn from_wire(message: v1::EntitySpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntitySpawn> for Verdict {
    fn from(event: EntitySpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntitySpellCast {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    spell: SpellcasterSpell,
}

impl EntitySpellCast {
    fn from_wire(message: v1::EntitySpellCast, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            spell: SpellcasterSpell::from_wire(message.spell)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn spell(&self) -> SpellcasterSpell {
        self.spell
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntitySpellCast> for Verdict {
    fn from(event: EntitySpellCast) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityTame {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    owner: OwnerRef,
}

impl EntityTame {
    fn from_wire(message: v1::EntityTame, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            owner: OwnerRef::from_wire(required(message.owner, "owner")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn owner(&self) -> &OwnerRef {
        &self.owner
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTame> for Verdict {
    fn from(event: EntityTame) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityTarget {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    reason: EntityTargetReason,
    target: Option<Entity>,
    patch: v1::EntityTargetPatch,
}

impl EntityTarget {
    fn from_wire(message: v1::EntityTarget, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            reason: EntityTargetReason::from_wire(message.reason)?,
            target: message
                .target
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            patch: v1::EntityTargetPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn reason(&self) -> EntityTargetReason {
        self.reason
    }

    pub fn target(&self) -> Option<&Entity> {
        self.target.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.target
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub fn set_target(&mut self, value: Option<Entity>) {
        self.patch.target = Some(v1::NullableEntitySnapshot {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.target = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTarget> for Verdict {
    fn from(event: EntityTarget) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityTargetPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityTarget(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityTargetBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    target: Option<Block>,
    patch: v1::EntityTargetBlockPatch,
}

impl EntityTargetBlock {
    fn from_wire(message: v1::EntityTargetBlock, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            target: message.target.map(|value| block(value, link)),
            patch: v1::EntityTargetBlockPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn target(&self) -> Option<&Block> {
        self.target.as_ref()
    }

    pub fn set_target(&mut self, value: Option<Block>) {
        self.patch.target = Some(v1::NullableBlockPos {
            value: value.as_ref().map(|value| value.pos.to_wire()),
        });
        self.target = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTargetBlock> for Verdict {
    fn from(event: EntityTargetBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityTargetBlockPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityTargetBlock(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityTargetLivingEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    reason: EntityTargetReason,
    target: Option<Entity>,
    patch: v1::EntityTargetLivingEntityPatch,
}

impl EntityTargetLivingEntity {
    fn from_wire(
        message: v1::EntityTargetLivingEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            reason: EntityTargetReason::from_wire(message.reason)?,
            target: message
                .target
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            patch: v1::EntityTargetLivingEntityPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn reason(&self) -> EntityTargetReason {
        self.reason
    }

    pub fn target(&self) -> Option<&Entity> {
        self.target.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.target
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub fn set_target(&mut self, value: Option<Entity>) {
        self.patch.target = Some(v1::NullableEntitySnapshot {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.target = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTargetLivingEntity> for Verdict {
    fn from(event: EntityTargetLivingEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityTargetLivingEntityPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityTargetLivingEntity(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityTeleport {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    from: Location,
    to: Option<Location>,
    patch: v1::EntityTeleportPatch,
}

impl EntityTeleport {
    fn from_wire(message: v1::EntityTeleport, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::EntityTeleportPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_to(&mut self, value: Option<Location>) {
        self.patch.to = Some(v1::NullableLocation {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.to = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTeleport> for Verdict {
    fn from(event: EntityTeleport) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::EntityTeleportPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::EntityTeleport(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct EntityToggleGlide {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    gliding: bool,
}

impl EntityToggleGlide {
    fn from_wire(message: v1::EntityToggleGlide, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            gliding: message.gliding,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn is_gliding(&self) -> bool {
        self.gliding
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityToggleGlide> for Verdict {
    fn from(event: EntityToggleGlide) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityToggleSwim {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    swimming: bool,
}

impl EntityToggleSwim {
    fn from_wire(message: v1::EntityToggleSwim, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            swimming: message.swimming,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn is_swimming(&self) -> bool {
        self.swimming
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityToggleSwim> for Verdict {
    fn from(event: EntityToggleSwim) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct EntityTransform {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    transform_reason: EntityTransformReason,
    transformed_entities: Vec<Entity>,
    transformed_entity: Entity,
}

impl EntityTransform {
    fn from_wire(message: v1::EntityTransform, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            transform_reason: EntityTransformReason::from_wire(message.transform_reason)?,
            transformed_entities: message
                .transformed_entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            transformed_entity: Entity::from_wire(
                required(message.transformed_entity, "transformed_entity")?,
                link,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn transform_reason(&self) -> EntityTransformReason {
        self.transform_reason
    }

    pub fn transformed_entities(&self) -> &[Entity] {
        &self.transformed_entities
    }

    pub fn transformed_entity(&self) -> &Entity {
        &self.transformed_entity
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<EntityTransform> for Verdict {
    fn from(event: EntityTransform) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityUnleash {
    id: u64,
    entity: Entity,
    entity_type: EntityType,
    reason: EntityUnleashReason,
}

impl EntityUnleash {
    fn from_wire(message: v1::EntityUnleash, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            reason: EntityUnleashReason::from_wire(message.reason)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn reason(&self) -> EntityUnleashReason {
        self.reason
    }
}

#[derive(Debug)]
pub struct ExpBottle {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    experience: i32,
    hit_block: Option<Block>,
    hit_block_face: Option<Face>,
    hit_entity: Option<Entity>,
    show_effect: bool,
    patch: v1::ExpBottlePatch,
}

impl ExpBottle {
    fn from_wire(message: v1::ExpBottle, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            experience: message.experience,
            hit_block: message.hit_block.map(|value| block(value, link)),
            hit_block_face: message.hit_block_face.map(Face::from_wire).transpose()?,
            hit_entity: message
                .hit_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            show_effect: message.show_effect,
            patch: v1::ExpBottlePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn experience(&self) -> i32 {
        self.experience
    }

    pub fn hit_block(&self) -> Option<&Block> {
        self.hit_block.as_ref()
    }

    pub fn hit_block_face(&self) -> Option<Face> {
        self.hit_block_face
    }

    pub fn hit_entity(&self) -> Option<&Entity> {
        self.hit_entity.as_ref()
    }

    pub fn show_effect(&self) -> bool {
        self.show_effect
    }

    pub fn set_experience(&mut self, value: i32) {
        self.patch.experience = Some(value);
        self.experience = value;
    }

    pub fn set_show_effect(&mut self, value: bool) {
        self.patch.show_effect = Some(value);
        self.show_effect = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ExpBottle> for Verdict {
    fn from(event: ExpBottle) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::ExpBottlePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ExpBottle(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct ExplosionPrime {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    fire: bool,
    radius: f32,
    patch: v1::ExplosionPrimePatch,
}

impl ExplosionPrime {
    fn from_wire(message: v1::ExplosionPrime, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            fire: message.fire,
            radius: message.radius,
            patch: v1::ExplosionPrimePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn fire(&self) -> bool {
        self.fire
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn set_fire(&mut self, value: bool) {
        self.patch.fire = Some(value);
        self.fire = value;
    }

    pub fn set_radius(&mut self, value: f32) {
        self.patch.radius = Some(value);
        self.radius = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ExplosionPrime> for Verdict {
    fn from(event: ExplosionPrime) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::ExplosionPrimePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ExplosionPrime(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FireworkExplode {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
}

impl FireworkExplode {
    fn from_wire(message: v1::FireworkExplode, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<FireworkExplode> for Verdict {
    fn from(event: FireworkExplode) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct FluidLevelChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_data: BlockState,
    patch: v1::FluidLevelChangePatch,
}

impl FluidLevelChange {
    fn from_wire(message: v1::FluidLevelChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_data: BlockState::from_wire(required(message.new_data, "new_data")?)?,
            patch: v1::FluidLevelChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_data(&self) -> &BlockState {
        &self.new_data
    }

    pub fn set_new_data(&mut self, value: BlockState) {
        self.patch.new_data = Some(value.to_wire());
        self.new_data = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<FluidLevelChange> for Verdict {
    fn from(event: FluidLevelChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::FluidLevelChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FluidLevelChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FoodLevelChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Player,
    entity_type: EntityType,
    food_level: i32,
    item: Option<ItemStack>,
    patch: v1::FoodLevelChangePatch,
}

impl FoodLevelChange {
    fn from_wire(message: v1::FoodLevelChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Player::from_snapshot(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            food_level: message.food_level,
            item: message.item.map(ItemStack::from_wire).transpose()?,
            patch: v1::FoodLevelChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn food_level(&self) -> i32 {
        self.food_level
    }

    pub fn item(&self) -> Option<&ItemStack> {
        self.item.as_ref()
    }

    pub fn set_food_level(&mut self, value: i32) {
        self.patch.food_level = Some(value);
        self.food_level = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<FoodLevelChange> for Verdict {
    fn from(event: FoodLevelChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::FoodLevelChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FoodLevelChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FurnaceBurn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    burn_time: i32,
    burning: bool,
    fuel: ItemStack,
    patch: v1::FurnaceBurnPatch,
}

impl FurnaceBurn {
    fn from_wire(message: v1::FurnaceBurn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            burn_time: message.burn_time,
            burning: message.burning,
            fuel: ItemStack::from_wire(required(message.fuel, "fuel")?)?,
            patch: v1::FurnaceBurnPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn burn_time(&self) -> i32 {
        self.burn_time
    }

    pub fn is_burning(&self) -> bool {
        self.burning
    }

    pub fn fuel(&self) -> &ItemStack {
        &self.fuel
    }

    pub fn set_burn_time(&mut self, value: i32) {
        self.patch.burn_time = Some(value);
        self.burn_time = value;
    }

    pub fn set_burning(&mut self, value: bool) {
        self.patch.burning = Some(value);
        self.burning = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<FurnaceBurn> for Verdict {
    fn from(event: FurnaceBurn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::FurnaceBurnPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FurnaceBurn(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FurnaceExtract {
    id: u64,
    block: Block,
    exp_to_drop: i32,
    item_amount: i32,
    item_type: Material,
    player: Player,
    patch: v1::FurnaceExtractPatch,
}

impl FurnaceExtract {
    fn from_wire(message: v1::FurnaceExtract, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            exp_to_drop: message.exp_to_drop,
            item_amount: message.item_amount,
            item_type: material(message.item_type)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::FurnaceExtractPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn exp_to_drop(&self) -> i32 {
        self.exp_to_drop
    }

    pub fn item_amount(&self) -> i32 {
        self.item_amount
    }

    pub fn item_type(&self) -> Material {
        self.item_type
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_exp_to_drop(&mut self, value: i32) {
        self.patch.exp_to_drop = Some(value);
        self.exp_to_drop = value;
    }
}

impl From<FurnaceExtract> for Verdict {
    fn from(event: FurnaceExtract) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::FurnaceExtractPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FurnaceExtract(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FurnaceSmelt {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    result: ItemStack,
    source: ItemStack,
    patch: v1::FurnaceSmeltPatch,
}

impl FurnaceSmelt {
    fn from_wire(message: v1::FurnaceSmelt, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            result: ItemStack::from_wire(required(message.result, "result")?)?,
            source: ItemStack::from_wire(required(message.source, "source")?)?,
            patch: v1::FurnaceSmeltPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn result(&self) -> &ItemStack {
        &self.result
    }

    pub fn source(&self) -> &ItemStack {
        &self.source
    }

    pub fn set_result(&mut self, value: ItemStack) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<FurnaceSmelt> for Verdict {
    fn from(event: FurnaceSmelt) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::FurnaceSmeltPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FurnaceSmelt(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct FurnaceStartSmelt {
    id: u64,
    block: Block,
    recipe: String,
    source: ItemStack,
    total_cook_time: i32,
    patch: v1::FurnaceStartSmeltPatch,
}

impl FurnaceStartSmelt {
    fn from_wire(message: v1::FurnaceStartSmelt, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            recipe: message.recipe,
            source: ItemStack::from_wire(required(message.source, "source")?)?,
            total_cook_time: message.total_cook_time,
            patch: v1::FurnaceStartSmeltPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub fn source(&self) -> &ItemStack {
        &self.source
    }

    pub fn total_cook_time(&self) -> i32 {
        self.total_cook_time
    }

    pub fn set_total_cook_time(&mut self, value: i32) {
        self.patch.total_cook_time = Some(value);
        self.total_cook_time = value;
    }
}

impl From<FurnaceStartSmelt> for Verdict {
    fn from(event: FurnaceStartSmelt) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::FurnaceStartSmeltPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::FurnaceStartSmelt(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct GenericGame {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Option<Entity>,
    event: String,
    location: Location,
    radius: i32,
    world: World,
    patch: v1::GenericGamePatch,
}

impl GenericGame {
    fn from_wire(message: v1::GenericGame, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            event: message.event,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            radius: message.radius,
            world: world(message.world, link),
            patch: v1::GenericGamePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub fn event(&self) -> &str {
        &self.event
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn radius(&self) -> i32 {
        self.radius
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn set_radius(&mut self, value: i32) {
        self.patch.radius = Some(value);
        self.radius = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<GenericGame> for Verdict {
    fn from(event: GenericGame) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::GenericGamePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::GenericGame(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct HangingBreak {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: HangingBreakRemoveCause,
    entity: Entity,
}

impl HangingBreak {
    fn from_wire(message: v1::HangingBreak, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: HangingBreakRemoveCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> HangingBreakRemoveCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<HangingBreak> for Verdict {
    fn from(event: HangingBreak) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct HangingBreakByEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: HangingBreakRemoveCause,
    entity: Entity,
    remover: Option<Entity>,
}

impl HangingBreakByEntity {
    fn from_wire(
        message: v1::HangingBreakByEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: HangingBreakRemoveCause::from_wire(message.cause)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            remover: message
                .remover
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> HangingBreakRemoveCause {
        self.cause
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn remover(&self) -> Option<&Entity> {
        self.remover.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.remover
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<HangingBreakByEntity> for Verdict {
    fn from(event: HangingBreakByEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct HangingPlace {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_face: Face,
    entity: Entity,
    hand: Option<EquipmentSlot>,
    item_stack: Option<ItemStack>,
    player: Option<Player>,
}

impl HangingPlace {
    fn from_wire(message: v1::HangingPlace, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_face: Face::from_wire(message.block_face)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            hand: message.hand.map(EquipmentSlot::from_wire).transpose()?,
            item_stack: message.item_stack.map(ItemStack::from_wire).transpose()?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_face(&self) -> Face {
        self.block_face
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn hand(&self) -> Option<EquipmentSlot> {
        self.hand
    }

    pub fn item_stack(&self) -> Option<&ItemStack> {
        self.item_stack.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<HangingPlace> for Verdict {
    fn from(event: HangingPlace) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct HopperInventorySearch {
    id: u64,
    block: Block,
    container_type: HopperInventorySearchContainerType,
    inventory: Option<InventoryRef>,
    search_block: Block,
    patch: v1::HopperInventorySearchPatch,
}

impl HopperInventorySearch {
    fn from_wire(
        message: v1::HopperInventorySearch,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            container_type: HopperInventorySearchContainerType::from_wire(message.container_type)?,
            inventory: message
                .inventory
                .map(|value| InventoryRef::from_wire(value, link))
                .transpose()?,
            search_block: block(required(message.search_block, "search_block")?, link),
            patch: v1::HopperInventorySearchPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn container_type(&self) -> HopperInventorySearchContainerType {
        self.container_type
    }

    pub fn inventory(&self) -> Option<&InventoryRef> {
        self.inventory.as_ref()
    }

    pub fn search_block(&self) -> &Block {
        &self.search_block
    }

    pub fn set_inventory(&mut self, value: Option<InventoryRef>) {
        self.patch.inventory = Some(v1::NullableInventoryRef {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.inventory = value;
    }
}

impl From<HopperInventorySearch> for Verdict {
    fn from(event: HopperInventorySearch) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::HopperInventorySearchPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::HopperInventorySearch(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct HorseJump {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    power: f32,
}

impl HorseJump {
    fn from_wire(message: v1::HorseJump, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            power: message.power,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn power(&self) -> f32 {
        self.power
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<HorseJump> for Verdict {
    fn from(event: HorseJump) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct InventoryClick {
    id: u64,
    rolled_back: bool,
    action: InventoryAction,
    click: ClickKind,
    clicked_inventory: Option<InventoryRef>,
    current_item: Option<ItemStack>,
    cursor: Option<ItemStack>,
    hotbar_button: i32,
    inventory: InventoryRef,
    left_click: bool,
    raw_slot: i32,
    result: EventResult,
    right_click: bool,
    shift_click: bool,
    slot: i32,
    slot_type: InventoryTypeSlotType,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::InventoryClickPatch,
}

impl InventoryClick {
    fn from_wire(message: v1::InventoryClick, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            action: InventoryAction::from_wire(message.action)?,
            click: ClickKind::from_wire(message.click)?,
            clicked_inventory: message
                .clicked_inventory
                .map(|value| InventoryRef::from_wire(value, link))
                .transpose()?,
            current_item: message.current_item.map(ItemStack::from_wire).transpose()?,
            cursor: message.cursor.map(ItemStack::from_wire).transpose()?,
            hotbar_button: message.hotbar_button,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            left_click: message.left_click,
            raw_slot: message.raw_slot,
            result: EventResult::from_wire(message.result)?,
            right_click: message.right_click,
            shift_click: message.shift_click,
            slot: message.slot,
            slot_type: InventoryTypeSlotType::from_wire(message.slot_type)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::InventoryClickPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> InventoryAction {
        self.action
    }

    pub fn click(&self) -> ClickKind {
        self.click
    }

    pub fn clicked_inventory(&self) -> Option<&InventoryRef> {
        self.clicked_inventory.as_ref()
    }

    pub fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }

    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    pub fn hotbar_button(&self) -> i32 {
        self.hotbar_button
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn is_left_click(&self) -> bool {
        self.left_click
    }

    pub fn raw_slot(&self) -> i32 {
        self.raw_slot
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn is_right_click(&self) -> bool {
        self.right_click
    }

    pub fn is_shift_click(&self) -> bool {
        self.shift_click
    }

    pub fn slot(&self) -> i32 {
        self.slot
    }

    pub fn slot_type(&self) -> InventoryTypeSlotType {
        self.slot_type
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_current_item(&mut self, value: Option<ItemStack>) {
        self.patch.current_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.current_item = value;
    }

    pub fn set_cursor(&mut self, value: Option<ItemStack>) {
        self.patch.cursor = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.cursor = value;
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryClick> for Verdict {
    fn from(event: InventoryClick) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::InventoryClickPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::InventoryClick(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InventoryClose {
    id: u64,
    inventory: InventoryRef,
    player: Player,
    view: InventoryView,
    viewers: Vec<Player>,
    by_player: bool,
}

impl InventoryClose {
    fn from_wire(message: v1::InventoryClose, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            by_player: message.by_player,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn by_player(&self) -> bool {
        self.by_player
    }
}

#[derive(Debug)]
pub struct InventoryCreative {
    id: u64,
    rolled_back: bool,
    action: InventoryAction,
    click: ClickKind,
    clicked_inventory: Option<InventoryRef>,
    current_item: Option<ItemStack>,
    cursor: Option<ItemStack>,
    hotbar_button: i32,
    inventory: InventoryRef,
    left_click: bool,
    raw_slot: i32,
    result: EventResult,
    right_click: bool,
    shift_click: bool,
    slot: i32,
    slot_type: InventoryTypeSlotType,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::InventoryCreativePatch,
}

impl InventoryCreative {
    fn from_wire(message: v1::InventoryCreative, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            action: InventoryAction::from_wire(message.action)?,
            click: ClickKind::from_wire(message.click)?,
            clicked_inventory: message
                .clicked_inventory
                .map(|value| InventoryRef::from_wire(value, link))
                .transpose()?,
            current_item: message.current_item.map(ItemStack::from_wire).transpose()?,
            cursor: message.cursor.map(ItemStack::from_wire).transpose()?,
            hotbar_button: message.hotbar_button,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            left_click: message.left_click,
            raw_slot: message.raw_slot,
            result: EventResult::from_wire(message.result)?,
            right_click: message.right_click,
            shift_click: message.shift_click,
            slot: message.slot,
            slot_type: InventoryTypeSlotType::from_wire(message.slot_type)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::InventoryCreativePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> InventoryAction {
        self.action
    }

    pub fn click(&self) -> ClickKind {
        self.click
    }

    pub fn clicked_inventory(&self) -> Option<&InventoryRef> {
        self.clicked_inventory.as_ref()
    }

    pub fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }

    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    pub fn hotbar_button(&self) -> i32 {
        self.hotbar_button
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn is_left_click(&self) -> bool {
        self.left_click
    }

    pub fn raw_slot(&self) -> i32 {
        self.raw_slot
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn is_right_click(&self) -> bool {
        self.right_click
    }

    pub fn is_shift_click(&self) -> bool {
        self.shift_click
    }

    pub fn slot(&self) -> i32 {
        self.slot
    }

    pub fn slot_type(&self) -> InventoryTypeSlotType {
        self.slot_type
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_current_item(&mut self, value: Option<ItemStack>) {
        self.patch.current_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.current_item = value;
    }

    pub fn set_cursor(&mut self, value: Option<ItemStack>) {
        self.patch.cursor = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.cursor = value;
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryCreative> for Verdict {
    fn from(event: InventoryCreative) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::InventoryCreativePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::InventoryCreative(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct InventoryDrag {
    id: u64,
    rolled_back: bool,
    cursor: Option<ItemStack>,
    inventory: InventoryRef,
    inventory_slots: Vec<i32>,
    new_items: Vec<(i32, ItemStack)>,
    old_cursor: ItemStack,
    raw_slots: Vec<i32>,
    result: EventResult,
    drag_type: DragType,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::InventoryDragPatch,
}

impl InventoryDrag {
    fn from_wire(message: v1::InventoryDrag, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            cursor: message.cursor.map(ItemStack::from_wire).transpose()?,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            inventory_slots: message.inventory_slots,
            new_items: message
                .new_items
                .into_iter()
                .map(slot)
                .collect::<Result<Vec<_>, _>>()?,
            old_cursor: ItemStack::from_wire(required(message.old_cursor, "old_cursor")?)?,
            raw_slots: message.raw_slots,
            result: EventResult::from_wire(message.result)?,
            drag_type: DragType::from_wire(message.drag_type)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::InventoryDragPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn inventory_slots(&self) -> &[i32] {
        &self.inventory_slots
    }

    pub fn new_items(&self) -> &[(i32, ItemStack)] {
        &self.new_items
    }

    pub fn old_cursor(&self) -> &ItemStack {
        &self.old_cursor
    }

    pub fn raw_slots(&self) -> &[i32] {
        &self.raw_slots
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn drag_type(&self) -> DragType {
        self.drag_type
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_cursor(&mut self, value: Option<ItemStack>) {
        self.patch.cursor = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.cursor = value;
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryDrag> for Verdict {
    fn from(event: InventoryDrag) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::InventoryDragPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::InventoryDrag(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct InventoryMoveItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    destination: InventoryRef,
    initiator: InventoryRef,
    item: ItemStack,
    source: InventoryRef,
    patch: v1::InventoryMoveItemPatch,
}

impl InventoryMoveItem {
    fn from_wire(message: v1::InventoryMoveItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            destination: InventoryRef::from_wire(
                required(message.destination, "destination")?,
                link,
            )?,
            initiator: InventoryRef::from_wire(required(message.initiator, "initiator")?, link)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            source: InventoryRef::from_wire(required(message.source, "source")?, link)?,
            patch: v1::InventoryMoveItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn destination(&self) -> &InventoryRef {
        &self.destination
    }

    pub fn initiator(&self) -> &InventoryRef {
        &self.initiator
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn source(&self) -> &InventoryRef {
        &self.source
    }

    pub fn set_item(&mut self, value: ItemStack) {
        self.patch.item = Some(value.to_wire());
        self.item = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryMoveItem> for Verdict {
    fn from(event: InventoryMoveItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::InventoryMoveItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::InventoryMoveItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct InventoryOpen {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    inventory: InventoryRef,
    player: Player,
    view: InventoryView,
    viewers: Vec<Player>,
}

impl InventoryOpen {
    fn from_wire(message: v1::InventoryOpen, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryOpen> for Verdict {
    fn from(event: InventoryOpen) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct InventoryPickupItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    inventory: InventoryRef,
    item: Entity,
}

impl InventoryPickupItem {
    fn from_wire(message: v1::InventoryPickupItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            item: Entity::from_wire(required(message.item, "item")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn item(&self) -> &Entity {
        &self.item
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<InventoryPickupItem> for Verdict {
    fn from(event: InventoryPickupItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct ItemDespawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
}

impl ItemDespawn {
    fn from_wire(message: v1::ItemDespawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ItemDespawn> for Verdict {
    fn from(event: ItemDespawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct ItemMerge {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    target: Entity,
}

impl ItemMerge {
    fn from_wire(message: v1::ItemMerge, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            target: Entity::from_wire(required(message.target, "target")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn target(&self) -> &Entity {
        &self.target
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ItemMerge> for Verdict {
    fn from(event: ItemMerge) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct ItemSpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
}

impl ItemSpawn {
    fn from_wire(message: v1::ItemSpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ItemSpawn> for Verdict {
    fn from(event: ItemSpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct LeavesDecay {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
}

impl LeavesDecay {
    fn from_wire(message: v1::LeavesDecay, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<LeavesDecay> for Verdict {
    fn from(event: LeavesDecay) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct LightningStrike {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: LightningStrikeCause,
    lightning: Entity,
    world: World,
}

impl LightningStrike {
    fn from_wire(message: v1::LightningStrike, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: LightningStrikeCause::from_wire(message.cause)?,
            lightning: Entity::from_wire(required(message.lightning, "lightning")?, link)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> LightningStrikeCause {
        self.cause
    }

    pub fn lightning(&self) -> &Entity {
        &self.lightning
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<LightningStrike> for Verdict {
    fn from(event: LightningStrike) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct LingeringPotionSplash {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    area_effect_cloud: Entity,
    entity: Entity,
    entity_type: EntityType,
    hit_block: Option<Block>,
    hit_block_face: Option<Face>,
    hit_entity: Option<Entity>,
}

impl LingeringPotionSplash {
    fn from_wire(
        message: v1::LingeringPotionSplash,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            area_effect_cloud: Entity::from_wire(
                required(message.area_effect_cloud, "area_effect_cloud")?,
                link,
            )?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            hit_block: message.hit_block.map(|value| block(value, link)),
            hit_block_face: message.hit_block_face.map(Face::from_wire).transpose()?,
            hit_entity: message
                .hit_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn area_effect_cloud(&self) -> &Entity {
        &self.area_effect_cloud
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn hit_block(&self) -> Option<&Block> {
        self.hit_block.as_ref()
    }

    pub fn hit_block_face(&self) -> Option<Face> {
        self.hit_block_face
    }

    pub fn hit_entity(&self) -> Option<&Entity> {
        self.hit_entity.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<LingeringPotionSplash> for Verdict {
    fn from(event: LingeringPotionSplash) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct LootGenerate {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Option<Entity>,
    inventory_holder: Option<HolderRef>,
    loot: Vec<ItemStack>,
    loot_context: LootContext,
    loot_table: String,
    plugin: bool,
    world: World,
    loot_snapshot: Option<Vec<ItemStack>>,
    patch: v1::LootGeneratePatch,
}

impl LootGenerate {
    fn from_wire(message: v1::LootGenerate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            inventory_holder: message
                .inventory_holder
                .map(|value| HolderRef::from_wire(value, link))
                .transpose()?,
            loot: message
                .loot
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            loot_context: LootContext::from_wire(
                required(message.loot_context, "loot_context")?,
                link,
            )?,
            loot_table: message.loot_table,
            plugin: message.plugin,
            world: world(message.world, link),
            loot_snapshot: None,
            patch: v1::LootGeneratePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub fn inventory_holder(&self) -> Option<&HolderRef> {
        self.inventory_holder.as_ref()
    }

    pub fn loot(&self) -> &[ItemStack] {
        &self.loot
    }

    pub fn loot_context(&self) -> &LootContext {
        &self.loot_context
    }

    pub fn loot_table(&self) -> &str {
        &self.loot_table
    }

    pub fn is_plugin(&self) -> bool {
        self.plugin
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn set_loot(&mut self, value: Vec<ItemStack>) {
        let snapshot = self
            .loot_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.loot));
        self.patch.loot = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_item,
        )
        .map(|diff| v1::ItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::ItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.loot = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<LootGenerate> for Verdict {
    fn from(event: LootGenerate) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::LootGeneratePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::LootGenerate(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapInitialize {
    id: u64,
    map_id: i32,
}

impl MapInitialize {
    fn from_wire(message: v1::MapInitialize, meta: Meta, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            map_id: message.map_id,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn map_id(&self) -> i32 {
        self.map_id
    }
}

#[derive(Debug)]
pub struct MoistureChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    new_state: BlockSnapshot,
}

impl MoistureChange {
    fn from_wire(message: v1::MoistureChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            new_state: BlockSnapshot::from_wire(required(message.new_state, "new_state")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn new_state(&self) -> &BlockSnapshot {
        &self.new_state
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<MoistureChange> for Verdict {
    fn from(event: MoistureChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct NotePlay {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    instrument: Instrument,
    note: u8,
}

impl NotePlay {
    fn from_wire(message: v1::NotePlay, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            instrument: Instrument::from_wire(message.instrument)?,
            note: note(message.note)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn instrument(&self) -> Instrument {
        self.instrument
    }

    pub fn note(&self) -> u8 {
        self.note
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<NotePlay> for Verdict {
    fn from(event: NotePlay) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PigZap {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    lightning: Entity,
    transform_reason: EntityTransformReason,
    transformed_entities: Vec<Entity>,
    transformed_entity: Entity,
}

impl PigZap {
    fn from_wire(message: v1::PigZap, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            lightning: Entity::from_wire(required(message.lightning, "lightning")?, link)?,
            transform_reason: EntityTransformReason::from_wire(message.transform_reason)?,
            transformed_entities: message
                .transformed_entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            transformed_entity: Entity::from_wire(
                required(message.transformed_entity, "transformed_entity")?,
                link,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn lightning(&self) -> &Entity {
        &self.lightning
    }

    pub fn transform_reason(&self) -> EntityTransformReason {
        self.transform_reason
    }

    pub fn transformed_entities(&self) -> &[Entity] {
        &self.transformed_entities
    }

    pub fn transformed_entity(&self) -> &Entity {
        &self.transformed_entity
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PigZap> for Verdict {
    fn from(event: PigZap) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PigZombieAnger {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    new_anger: i32,
    target: Option<Entity>,
    patch: v1::PigZombieAngerPatch,
}

impl PigZombieAnger {
    fn from_wire(message: v1::PigZombieAnger, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            new_anger: message.new_anger,
            target: message
                .target
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            patch: v1::PigZombieAngerPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn new_anger(&self) -> i32 {
        self.new_anger
    }

    pub fn target(&self) -> Option<&Entity> {
        self.target.as_ref()
    }

    pub fn set_new_anger(&mut self, value: i32) {
        self.patch.new_anger = Some(value);
        self.new_anger = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PigZombieAnger> for Verdict {
    fn from(event: PigZombieAnger) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PigZombieAngerPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PigZombieAnger(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PiglinBarter {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    input: ItemStack,
    outcome: Vec<ItemStack>,
}

impl PiglinBarter {
    fn from_wire(message: v1::PiglinBarter, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            input: ItemStack::from_wire(required(message.input, "input")?)?,
            outcome: message
                .outcome
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn input(&self) -> &ItemStack {
        &self.input
    }

    pub fn outcome(&self) -> &[ItemStack] {
        &self.outcome
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PiglinBarter> for Verdict {
    fn from(event: PiglinBarter) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerAdvancementCriterionGrant {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    advancement: String,
    criterion: String,
    player: Player,
}

impl PlayerAdvancementCriterionGrant {
    fn from_wire(
        message: v1::PlayerAdvancementCriterionGrant,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            advancement: message.advancement,
            criterion: message.criterion,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn advancement(&self) -> &str {
        &self.advancement
    }

    pub fn criterion(&self) -> &str {
        &self.criterion
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerAdvancementCriterionGrant> for Verdict {
    fn from(event: PlayerAdvancementCriterionGrant) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerAdvancementDone {
    id: u64,
    advancement: String,
    player: Player,
}

impl PlayerAdvancementDone {
    fn from_wire(
        message: v1::PlayerAdvancementDone,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            advancement: message.advancement,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn advancement(&self) -> &str {
        &self.advancement
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerAnimation {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    animation_type: PlayerAnimationType,
    player: Player,
}

impl PlayerAnimation {
    fn from_wire(message: v1::PlayerAnimation, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            animation_type: PlayerAnimationType::from_wire(message.animation_type)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn animation_type(&self) -> PlayerAnimationType {
        self.animation_type
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerAnimation> for Verdict {
    fn from(event: PlayerAnimation) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerArmorStandManipulate {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    armor_stand_item: ItemStack,
    hand: EquipmentSlot,
    player: Player,
    player_item: ItemStack,
    right_clicked: Entity,
    slot: EquipmentSlot,
}

impl PlayerArmorStandManipulate {
    fn from_wire(
        message: v1::PlayerArmorStandManipulate,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            armor_stand_item: ItemStack::from_wire(required(
                message.armor_stand_item,
                "armor_stand_item",
            )?)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            player_item: ItemStack::from_wire(required(message.player_item, "player_item")?)?,
            right_clicked: Entity::from_wire(
                required(message.right_clicked, "right_clicked")?,
                link,
            )?,
            slot: EquipmentSlot::from_wire(message.slot)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn armor_stand_item(&self) -> &ItemStack {
        &self.armor_stand_item
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn player_item(&self) -> &ItemStack {
        &self.player_item
    }

    pub fn right_clicked(&self) -> &Entity {
        &self.right_clicked
    }

    pub fn slot(&self) -> EquipmentSlot {
        self.slot
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerArmorStandManipulate> for Verdict {
    fn from(event: PlayerArmorStandManipulate) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerBedEnter {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    bed: Block,
    bed_enter_result: PlayerBedEnterResult,
    player: Player,
    use_bed: EventResult,
    patch: v1::PlayerBedEnterPatch,
}

impl PlayerBedEnter {
    fn from_wire(message: v1::PlayerBedEnter, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            bed: block(required(message.bed, "bed")?, link),
            bed_enter_result: PlayerBedEnterResult::from_wire(message.bed_enter_result)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            use_bed: EventResult::from_wire(message.use_bed)?,
            patch: v1::PlayerBedEnterPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bed(&self) -> &Block {
        &self.bed
    }

    pub fn bed_enter_result(&self) -> PlayerBedEnterResult {
        self.bed_enter_result
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn use_bed(&self) -> EventResult {
        self.use_bed
    }

    pub fn set_use_bed(&mut self, value: EventResult) {
        self.patch.use_bed = Some(value.to_wire());
        self.use_bed = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerBedEnter> for Verdict {
    fn from(event: PlayerBedEnter) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerBedEnterPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerBedEnter(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerBedLeave {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    bed: Block,
    player: Player,
    set_spawn_location: bool,
    patch: v1::PlayerBedLeavePatch,
}

impl PlayerBedLeave {
    fn from_wire(message: v1::PlayerBedLeave, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            bed: block(required(message.bed, "bed")?, link),
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            set_spawn_location: message.set_spawn_location,
            patch: v1::PlayerBedLeavePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn bed(&self) -> &Block {
        &self.bed
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn should_set_spawn_location(&self) -> bool {
        self.set_spawn_location
    }

    pub fn set_spawn_location(&mut self, value: bool) {
        self.patch.set_spawn_location = Some(value);
        self.set_spawn_location = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerBedLeave> for Verdict {
    fn from(event: PlayerBedLeave) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerBedLeavePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerBedLeave(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerBucketEmpty {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_clicked: Block,
    block_face: Face,
    bucket: Material,
    hand: EquipmentSlot,
    item_stack: Option<ItemStack>,
    player: Player,
    patch: v1::PlayerBucketEmptyPatch,
}

impl PlayerBucketEmpty {
    fn from_wire(message: v1::PlayerBucketEmpty, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_clicked: block(required(message.block_clicked, "block_clicked")?, link),
            block_face: Face::from_wire(message.block_face)?,
            bucket: material(message.bucket)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item_stack: message.item_stack.map(ItemStack::from_wire).transpose()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerBucketEmptyPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_clicked(&self) -> &Block {
        &self.block_clicked
    }

    pub fn block_face(&self) -> Face {
        self.block_face
    }

    pub fn bucket(&self) -> Material {
        self.bucket
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item_stack(&self) -> Option<&ItemStack> {
        self.item_stack.as_ref()
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_item_stack(&mut self, value: Option<ItemStack>) {
        self.patch.item_stack = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.item_stack = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerBucketEmpty> for Verdict {
    fn from(event: PlayerBucketEmpty) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerBucketEmptyPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerBucketEmpty(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerBucketEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_bucket: ItemStack,
    hand: EquipmentSlot,
    original_bucket: ItemStack,
    player: Player,
}

impl PlayerBucketEntity {
    fn from_wire(message: v1::PlayerBucketEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_bucket: ItemStack::from_wire(required(message.entity_bucket, "entity_bucket")?)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            original_bucket: ItemStack::from_wire(required(
                message.original_bucket,
                "original_bucket",
            )?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_bucket(&self) -> &ItemStack {
        &self.entity_bucket
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn original_bucket(&self) -> &ItemStack {
        &self.original_bucket
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerBucketEntity> for Verdict {
    fn from(event: PlayerBucketEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerBucketFill {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    block_clicked: Block,
    block_face: Face,
    bucket: Material,
    hand: EquipmentSlot,
    item_stack: Option<ItemStack>,
    player: Player,
    patch: v1::PlayerBucketFillPatch,
}

impl PlayerBucketFill {
    fn from_wire(message: v1::PlayerBucketFill, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            block_clicked: block(required(message.block_clicked, "block_clicked")?, link),
            block_face: Face::from_wire(message.block_face)?,
            bucket: material(message.bucket)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item_stack: message.item_stack.map(ItemStack::from_wire).transpose()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerBucketFillPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_clicked(&self) -> &Block {
        &self.block_clicked
    }

    pub fn block_face(&self) -> Face {
        self.block_face
    }

    pub fn bucket(&self) -> Material {
        self.bucket
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item_stack(&self) -> Option<&ItemStack> {
        self.item_stack.as_ref()
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_item_stack(&mut self, value: Option<ItemStack>) {
        self.patch.item_stack = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.item_stack = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerBucketFill> for Verdict {
    fn from(event: PlayerBucketFill) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerBucketFillPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerBucketFill(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerChangedMainHand {
    id: u64,
    main_hand: MainHand,
    player: Player,
}

impl PlayerChangedMainHand {
    fn from_wire(
        message: v1::PlayerChangedMainHand,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            main_hand: MainHand::from_wire(message.main_hand)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn main_hand(&self) -> MainHand {
        self.main_hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug, Clone)]
pub struct PlayerChangedWorld {
    id: u64,
    from: World,
    player: Player,
}

impl PlayerChangedWorld {
    fn from_wire(message: v1::PlayerChangedWorld, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            from: world(message.from, link),
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn from(&self) -> &World {
        &self.from
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerCommandPreprocess {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    message: String,
    player: Player,
    patch: v1::PlayerCommandPreprocessPatch,
}

impl PlayerCommandPreprocess {
    fn from_wire(
        message: v1::PlayerCommandPreprocess,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            message: message.message,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerCommandPreprocessPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_message(&mut self, value: String) {
        self.patch.message = Some(value.clone());
        self.message = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerCommandPreprocess> for Verdict {
    fn from(event: PlayerCommandPreprocess) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerCommandPreprocessPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerCommandPreprocess(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerCommandSend {
    id: u64,
    commands: Vec<String>,
    player: Player,
    commands_snapshot: Option<Vec<String>>,
    patch: v1::PlayerCommandSendPatch,
}

impl PlayerCommandSend {
    fn from_wire(message: v1::PlayerCommandSend, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            commands: message.commands,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            commands_snapshot: None,
            patch: v1::PlayerCommandSendPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn commands(&self) -> &[String] {
        &self.commands
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_commands(&mut self, value: Vec<String>) {
        let snapshot = self
            .commands_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.commands));
        self.patch.commands = diff::list(snapshot, value.clone()).map(|diff| v1::StringListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::StringReplacement { index, value })
                .collect(),
            added: diff.added,
        });
        self.commands = value;
    }
}

impl From<PlayerCommandSend> for Verdict {
    fn from(event: PlayerCommandSend) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerCommandSendPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerCommandSend(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerCustomClick {
    id: u64,
    data: Option<String>,
    click_id: String,
    player: Player,
}

impl PlayerCustomClick {
    fn from_wire(message: v1::PlayerCustomClick, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            data: message.data,
            click_id: message.click_id,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    pub fn click_id(&self) -> &str {
        &self.click_id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerDeath {
    id: u64,
    damage_source: DamageSource,
    death_message: Option<String>,
    dropped_exp: i32,
    drops: Vec<ItemStack>,
    entity: Player,
    entity_type: EntityType,
    keep_inventory: bool,
    keep_level: bool,
    new_exp: i32,
    new_level: i32,
    new_total_exp: i32,
    drops_snapshot: Option<Vec<ItemStack>>,
    patch: v1::PlayerDeathPatch,
}

impl PlayerDeath {
    fn from_wire(message: v1::PlayerDeath, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            damage_source: DamageSource::from_wire(
                required(message.damage_source, "damage_source")?,
                link,
            )?,
            death_message: message.death_message,
            dropped_exp: message.dropped_exp,
            drops: message
                .drops
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            entity: Player::from_snapshot(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            keep_inventory: message.keep_inventory,
            keep_level: message.keep_level,
            new_exp: message.new_exp,
            new_level: message.new_level,
            new_total_exp: message.new_total_exp,
            drops_snapshot: None,
            patch: v1::PlayerDeathPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn damage_source(&self) -> &DamageSource {
        &self.damage_source
    }

    pub fn death_message(&self) -> Option<&str> {
        self.death_message.as_deref()
    }

    pub fn dropped_exp(&self) -> i32 {
        self.dropped_exp
    }

    pub fn drops(&self) -> &[ItemStack] {
        &self.drops
    }

    pub fn player(&self) -> &Player {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn keep_inventory(&self) -> bool {
        self.keep_inventory
    }

    pub fn keep_level(&self) -> bool {
        self.keep_level
    }

    pub fn new_exp(&self) -> i32 {
        self.new_exp
    }

    pub fn new_level(&self) -> i32 {
        self.new_level
    }

    pub fn new_total_exp(&self) -> i32 {
        self.new_total_exp
    }

    pub fn set_death_message(&mut self, value: Option<String>) {
        self.patch.death_message = Some(v1::NullableString {
            value: value.clone(),
        });
        self.death_message = value;
    }

    pub fn set_dropped_exp(&mut self, value: i32) {
        self.patch.dropped_exp = Some(value);
        self.dropped_exp = value;
    }

    pub fn set_drops(&mut self, value: Vec<ItemStack>) {
        let snapshot = self
            .drops_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.drops));
        self.patch.drops = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_item,
        )
        .map(|diff| v1::ItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::ItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.drops = value;
    }

    pub fn set_keep_inventory(&mut self, value: bool) {
        self.patch.keep_inventory = Some(value);
        self.keep_inventory = value;
    }

    pub fn set_keep_level(&mut self, value: bool) {
        self.patch.keep_level = Some(value);
        self.keep_level = value;
    }

    pub fn set_new_exp(&mut self, value: i32) {
        self.patch.new_exp = Some(value);
        self.new_exp = value;
    }

    pub fn set_new_level(&mut self, value: i32) {
        self.patch.new_level = Some(value);
        self.new_level = value;
    }

    pub fn set_new_total_exp(&mut self, value: i32) {
        self.patch.new_total_exp = Some(value);
        self.new_total_exp = value;
    }
}

impl From<PlayerDeath> for Verdict {
    fn from(event: PlayerDeath) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerDeathPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerDeath(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerDropItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    item_drop: Entity,
    player: Player,
    item: ItemStack,
    patch: v1::PlayerDropItemPatch,
}

impl PlayerDropItem {
    fn from_wire(message: v1::PlayerDropItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            item_drop: Entity::from_wire(required(message.item_drop, "item_drop")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            patch: v1::PlayerDropItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn item_drop(&self) -> &Entity {
        &self.item_drop
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn set_item(&mut self, value: ItemStack) {
        self.patch.item = Some(value.to_wire());
        self.item = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerDropItem> for Verdict {
    fn from(event: PlayerDropItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerDropItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerDropItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerEditBook {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    new_book_meta: BookMeta,
    player: Player,
    previous_book_meta: BookMeta,
    signing: bool,
    patch: v1::PlayerEditBookPatch,
}

impl PlayerEditBook {
    fn from_wire(message: v1::PlayerEditBook, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            new_book_meta: BookMeta::from_wire(
                required(message.new_book_meta, "new_book_meta")?,
                link,
            )?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            previous_book_meta: BookMeta::from_wire(
                required(message.previous_book_meta, "previous_book_meta")?,
                link,
            )?,
            signing: message.signing,
            patch: v1::PlayerEditBookPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn new_book_meta(&self) -> &BookMeta {
        &self.new_book_meta
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn previous_book_meta(&self) -> &BookMeta {
        &self.previous_book_meta
    }

    pub fn is_signing(&self) -> bool {
        self.signing
    }

    pub fn set_new_book_meta(&mut self, value: BookMeta) {
        self.patch.new_book_meta = Some(value.to_wire());
        self.new_book_meta = value;
    }

    pub fn set_signing(&mut self, value: bool) {
        self.patch.signing = Some(value);
        self.signing = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerEditBook> for Verdict {
    fn from(event: PlayerEditBook) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerEditBookPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerEditBook(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerEggThrow {
    id: u64,
    egg: Entity,
    hatching: bool,
    hatching_type: EntityType,
    num_hatches: i8,
    player: Player,
    patch: v1::PlayerEggThrowPatch,
}

impl PlayerEggThrow {
    fn from_wire(message: v1::PlayerEggThrow, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            egg: Entity::from_wire(required(message.egg, "egg")?, link)?,
            hatching: message.hatching,
            hatching_type: entity_type(message.hatching_type)?,
            num_hatches: byte(message.num_hatches)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerEggThrowPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn egg(&self) -> &Entity {
        &self.egg
    }

    pub fn is_hatching(&self) -> bool {
        self.hatching
    }

    pub fn hatching_type(&self) -> EntityType {
        self.hatching_type
    }

    pub fn num_hatches(&self) -> i8 {
        self.num_hatches
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_hatching(&mut self, value: bool) {
        self.patch.hatching = Some(value);
        self.hatching = value;
    }

    pub fn set_hatching_type(&mut self, value: EntityType) {
        self.patch.hatching_type = Some(value.key().to_owned());
        self.hatching_type = value;
    }

    pub fn set_num_hatches(&mut self, value: i8) {
        self.patch.num_hatches = Some(i32::from(value));
        self.num_hatches = value;
    }
}

impl From<PlayerEggThrow> for Verdict {
    fn from(event: PlayerEggThrow) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerEggThrowPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerEggThrow(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerExpChange {
    id: u64,
    amount: i32,
    player: Player,
    patch: v1::PlayerExpChangePatch,
}

impl PlayerExpChange {
    fn from_wire(message: v1::PlayerExpChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            amount: message.amount,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerExpChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn amount(&self) -> i32 {
        self.amount
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_amount(&mut self, value: i32) {
        self.patch.amount = Some(value);
        self.amount = value;
    }
}

impl From<PlayerExpChange> for Verdict {
    fn from(event: PlayerExpChange) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerExpChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerExpChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerExpCooldownChange {
    id: u64,
    new_cooldown: i32,
    player: Player,
    reason: PlayerExpCooldownChangeReason,
    patch: v1::PlayerExpCooldownChangePatch,
}

impl PlayerExpCooldownChange {
    fn from_wire(
        message: v1::PlayerExpCooldownChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            new_cooldown: message.new_cooldown,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            reason: PlayerExpCooldownChangeReason::from_wire(message.reason)?,
            patch: v1::PlayerExpCooldownChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn new_cooldown(&self) -> i32 {
        self.new_cooldown
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn reason(&self) -> PlayerExpCooldownChangeReason {
        self.reason
    }

    pub fn set_new_cooldown(&mut self, value: i32) {
        self.patch.new_cooldown = Some(value);
        self.new_cooldown = value;
    }
}

impl From<PlayerExpCooldownChange> for Verdict {
    fn from(event: PlayerExpCooldownChange) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerExpCooldownChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerExpCooldownChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerFish {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    caught: Option<Entity>,
    exp_to_drop: i32,
    hand: Option<EquipmentSlot>,
    hook: Entity,
    player: Player,
    state: PlayerFishState,
    patch: v1::PlayerFishPatch,
}

impl PlayerFish {
    fn from_wire(message: v1::PlayerFish, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            caught: message
                .caught
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            exp_to_drop: message.exp_to_drop,
            hand: message.hand.map(EquipmentSlot::from_wire).transpose()?,
            hook: Entity::from_wire(required(message.hook, "hook")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            state: PlayerFishState::from_wire(message.state)?,
            patch: v1::PlayerFishPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn caught(&self) -> Option<&Entity> {
        self.caught.as_ref()
    }

    pub fn exp_to_drop(&self) -> i32 {
        self.exp_to_drop
    }

    pub fn hand(&self) -> Option<EquipmentSlot> {
        self.hand
    }

    pub fn hook(&self) -> &Entity {
        &self.hook
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn state(&self) -> PlayerFishState {
        self.state
    }

    pub fn set_exp_to_drop(&mut self, value: i32) {
        self.patch.exp_to_drop = Some(value);
        self.exp_to_drop = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerFish> for Verdict {
    fn from(event: PlayerFish) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerFishPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerFish(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerGameModeChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    new_game_mode: GameMode,
    player: Player,
}

impl PlayerGameModeChange {
    fn from_wire(
        message: v1::PlayerGameModeChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            new_game_mode: GameMode::from_wire(message.new_game_mode)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn new_game_mode(&self) -> GameMode {
        self.new_game_mode
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerGameModeChange> for Verdict {
    fn from(event: PlayerGameModeChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerHarvestBlock {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    hand: EquipmentSlot,
    harvested_block: Block,
    items_harvested: Vec<ItemStack>,
    player: Player,
    items_harvested_snapshot: Option<Vec<ItemStack>>,
    patch: v1::PlayerHarvestBlockPatch,
}

impl PlayerHarvestBlock {
    fn from_wire(message: v1::PlayerHarvestBlock, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            hand: EquipmentSlot::from_wire(message.hand)?,
            harvested_block: block(required(message.harvested_block, "harvested_block")?, link),
            items_harvested: message
                .items_harvested
                .into_iter()
                .map(ItemStack::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            items_harvested_snapshot: None,
            patch: v1::PlayerHarvestBlockPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn harvested_block(&self) -> &Block {
        &self.harvested_block
    }

    pub fn items_harvested(&self) -> &[ItemStack] {
        &self.items_harvested
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_items_harvested(&mut self, value: Vec<ItemStack>) {
        let snapshot = self
            .items_harvested_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.items_harvested));
        self.patch.items_harvested = diff::list_by(
            &snapshot
                .iter()
                .map(|value| value.to_wire())
                .collect::<Vec<_>>(),
            value.iter().map(|value| value.to_wire()).collect(),
            diff::same_item,
        )
        .map(|diff| v1::ItemListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::ItemReplacement {
                    index,
                    value: Some(value),
                })
                .collect(),
            added: diff.added,
        });
        self.items_harvested = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerHarvestBlock> for Verdict {
    fn from(event: PlayerHarvestBlock) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerHarvestBlockPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerHarvestBlock(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerHideEntity {
    id: u64,
    entity: Entity,
    player: Player,
}

impl PlayerHideEntity {
    fn from_wire(message: v1::PlayerHideEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug, Clone)]
pub struct PlayerInput {
    id: u64,
    input: Input,
    player: Player,
}

impl PlayerInput {
    fn from_wire(message: v1::PlayerInput, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            input: Input::from_wire(required(message.input, "input")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn input(&self) -> Input {
        self.input
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerInteract {
    id: u64,
    rolled_back: bool,
    action: Action,
    block: bool,
    block_face: Face,
    block_in_hand: bool,
    clicked_block: Option<Block>,
    clicked_position: Option<Vector>,
    hand: Option<EquipmentSlot>,
    has_item: bool,
    item: Option<ItemStack>,
    material: Material,
    player: Player,
    use_interacted_block: EventResult,
    use_item_in_hand: EventResult,
    patch: v1::PlayerInteractPatch,
}

impl PlayerInteract {
    fn from_wire(message: v1::PlayerInteract, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            action: Action::from_wire(message.action)?,
            block: message.block,
            block_face: Face::from_wire(message.block_face)?,
            block_in_hand: message.block_in_hand,
            clicked_block: message.clicked_block.map(|value| block(value, link)),
            clicked_position: message.clicked_position.map(vector),
            hand: message.hand.map(EquipmentSlot::from_wire).transpose()?,
            has_item: message.has_item,
            item: message.item.map(ItemStack::from_wire).transpose()?,
            material: material(message.material)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            use_interacted_block: EventResult::from_wire(message.use_interacted_block)?,
            use_item_in_hand: EventResult::from_wire(message.use_item_in_hand)?,
            patch: v1::PlayerInteractPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn has_block(&self) -> bool {
        self.block
    }

    pub fn block_face(&self) -> Face {
        self.block_face
    }

    pub fn is_block_in_hand(&self) -> bool {
        self.block_in_hand
    }

    pub fn clicked_block(&self) -> Option<&Block> {
        self.clicked_block.as_ref()
    }

    pub fn clicked_position(&self) -> Option<Vector> {
        self.clicked_position
    }

    pub fn hand(&self) -> Option<EquipmentSlot> {
        self.hand
    }

    pub fn has_item(&self) -> bool {
        self.has_item
    }

    pub fn item(&self) -> Option<&ItemStack> {
        self.item.as_ref()
    }

    pub fn material(&self) -> Material {
        self.material
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn use_interacted_block(&self) -> EventResult {
        self.use_interacted_block
    }

    pub fn use_item_in_hand(&self) -> EventResult {
        self.use_item_in_hand
    }

    pub fn set_use_interacted_block(&mut self, value: EventResult) {
        self.patch.use_interacted_block = Some(value.to_wire());
        self.use_interacted_block = value;
    }

    pub fn set_use_item_in_hand(&mut self, value: EventResult) {
        self.patch.use_item_in_hand = Some(value.to_wire());
        self.use_item_in_hand = value;
    }

    pub fn cancel(&mut self) {
        self.set_use_interacted_block(EventResult::Deny);
        self.set_use_item_in_hand(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.use_interacted_block == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerInteract> for Verdict {
    fn from(event: PlayerInteract) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerInteractPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerInteract(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerInteractAtEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    clicked_position: Vector,
    hand: EquipmentSlot,
    player: Player,
    right_clicked: Entity,
}

impl PlayerInteractAtEntity {
    fn from_wire(
        message: v1::PlayerInteractAtEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            clicked_position: vector(required(message.clicked_position, "clicked_position")?),
            hand: EquipmentSlot::from_wire(message.hand)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            right_clicked: Entity::from_wire(
                required(message.right_clicked, "right_clicked")?,
                link,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn clicked_position(&self) -> Vector {
        self.clicked_position
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn right_clicked(&self) -> &Entity {
        &self.right_clicked
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerInteractAtEntity> for Verdict {
    fn from(event: PlayerInteractAtEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerInteractEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    hand: EquipmentSlot,
    player: Player,
    right_clicked: Entity,
}

impl PlayerInteractEntity {
    fn from_wire(
        message: v1::PlayerInteractEntity,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            hand: EquipmentSlot::from_wire(message.hand)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            right_clicked: Entity::from_wire(
                required(message.right_clicked, "right_clicked")?,
                link,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn right_clicked(&self) -> &Entity {
        &self.right_clicked
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerInteractEntity> for Verdict {
    fn from(event: PlayerInteractEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerItemBreak {
    id: u64,
    broken_item: ItemStack,
    player: Player,
}

impl PlayerItemBreak {
    fn from_wire(message: v1::PlayerItemBreak, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            broken_item: ItemStack::from_wire(required(message.broken_item, "broken_item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn broken_item(&self) -> &ItemStack {
        &self.broken_item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerItemConsume {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    hand: EquipmentSlot,
    item: ItemStack,
    player: Player,
    patch: v1::PlayerItemConsumePatch,
}

impl PlayerItemConsume {
    fn from_wire(message: v1::PlayerItemConsume, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerItemConsumePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_item(&mut self, value: ItemStack) {
        self.patch.item = Some(value.to_wire());
        self.item = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerItemConsume> for Verdict {
    fn from(event: PlayerItemConsume) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerItemConsumePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerItemConsume(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerItemDamage {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    damage: i32,
    item: ItemStack,
    player: Player,
    patch: v1::PlayerItemDamagePatch,
}

impl PlayerItemDamage {
    fn from_wire(message: v1::PlayerItemDamage, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            damage: message.damage,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerItemDamagePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn damage(&self) -> i32 {
        self.damage
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_damage(&mut self, value: i32) {
        self.patch.damage = Some(value);
        self.damage = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerItemDamage> for Verdict {
    fn from(event: PlayerItemDamage) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerItemDamagePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerItemDamage(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerItemHeld {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    new_slot: i32,
    player: Player,
    previous_slot: i32,
}

impl PlayerItemHeld {
    fn from_wire(message: v1::PlayerItemHeld, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            new_slot: message.new_slot,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            previous_slot: message.previous_slot,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn new_slot(&self) -> i32 {
        self.new_slot
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn previous_slot(&self) -> i32 {
        self.previous_slot
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerItemHeld> for Verdict {
    fn from(event: PlayerItemHeld) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerItemMend {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    experience_orb: Entity,
    item: ItemStack,
    player: Player,
    repair_amount: i32,
    slot: EquipmentSlot,
    patch: v1::PlayerItemMendPatch,
}

impl PlayerItemMend {
    fn from_wire(message: v1::PlayerItemMend, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            experience_orb: Entity::from_wire(
                required(message.experience_orb, "experience_orb")?,
                link,
            )?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            repair_amount: message.repair_amount,
            slot: EquipmentSlot::from_wire(message.slot)?,
            patch: v1::PlayerItemMendPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn experience_orb(&self) -> &Entity {
        &self.experience_orb
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn repair_amount(&self) -> i32 {
        self.repair_amount
    }

    pub fn slot(&self) -> EquipmentSlot {
        self.slot
    }

    pub fn set_repair_amount(&mut self, value: i32) {
        self.patch.repair_amount = Some(value);
        self.repair_amount = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerItemMend> for Verdict {
    fn from(event: PlayerItemMend) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerItemMendPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerItemMend(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerJoin {
    id: u64,
    join_message: Option<String>,
    player: Player,
    patch: v1::PlayerJoinPatch,
}

impl PlayerJoin {
    fn from_wire(message: v1::PlayerJoin, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            join_message: message.join_message,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerJoinPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn join_message(&self) -> Option<&str> {
        self.join_message.as_deref()
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_join_message(&mut self, value: Option<String>) {
        self.patch.join_message = Some(v1::NullableString {
            value: value.clone(),
        });
        self.join_message = value;
    }
}

impl From<PlayerJoin> for Verdict {
    fn from(event: PlayerJoin) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerJoinPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerJoin(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerKick {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    leave_message: String,
    player: Player,
    reason: String,
    patch: v1::PlayerKickPatch,
}

impl PlayerKick {
    fn from_wire(message: v1::PlayerKick, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            leave_message: message.leave_message,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            reason: message.reason,
            patch: v1::PlayerKickPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn leave_message(&self) -> &str {
        &self.leave_message
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn set_leave_message(&mut self, value: String) {
        self.patch.leave_message = Some(value.clone());
        self.leave_message = value;
    }

    pub fn set_reason(&mut self, value: String) {
        self.patch.reason = Some(value.clone());
        self.reason = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerKick> for Verdict {
    fn from(event: PlayerKick) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerKickPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerKick(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerLeashEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    hand: EquipmentSlot,
    leash_holder: Entity,
    player: Player,
}

impl PlayerLeashEntity {
    fn from_wire(message: v1::PlayerLeashEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            leash_holder: Entity::from_wire(required(message.leash_holder, "leash_holder")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn leash_holder(&self) -> &Entity {
        &self.leash_holder
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerLeashEntity> for Verdict {
    fn from(event: PlayerLeashEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerLeave {
    id: u64,
    player: Player,
    quit_message: Option<String>,
    patch: v1::PlayerLeavePatch,
}

impl PlayerLeave {
    fn from_wire(message: v1::PlayerLeave, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            quit_message: message.quit_message,
            patch: v1::PlayerLeavePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn quit_message(&self) -> Option<&str> {
        self.quit_message.as_deref()
    }

    pub fn set_quit_message(&mut self, value: Option<String>) {
        self.patch.quit_message = Some(v1::NullableString {
            value: value.clone(),
        });
        self.quit_message = value;
    }
}

impl From<PlayerLeave> for Verdict {
    fn from(event: PlayerLeave) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerLeavePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerLeave(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLevelChange {
    id: u64,
    new_level: i32,
    old_level: i32,
    player: Player,
}

impl PlayerLevelChange {
    fn from_wire(message: v1::PlayerLevelChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            new_level: message.new_level,
            old_level: message.old_level,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn new_level(&self) -> i32 {
        self.new_level
    }

    pub fn old_level(&self) -> i32 {
        self.old_level
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLinksSend {
    id: u64,
    links: ServerLinks,
    player: Player,
}

impl PlayerLinksSend {
    fn from_wire(message: v1::PlayerLinksSend, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            links: ServerLinks::from_wire(required(message.links, "links")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn links(&self) -> &ServerLinks {
        &self.links
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLocaleChange {
    id: u64,
    locale: String,
    player: Player,
}

impl PlayerLocaleChange {
    fn from_wire(message: v1::PlayerLocaleChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            locale: message.locale,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerLogin {
    id: u64,
    address: IpAddr,
    hostname: String,
    kick_message: String,
    player: Player,
    real_address: IpAddr,
    result: PlayerLoginResult,
    patch: v1::PlayerLoginPatch,
}

impl PlayerLogin {
    fn from_wire(message: v1::PlayerLogin, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            address: address(message.address)?,
            hostname: message.hostname,
            kick_message: message.kick_message,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            real_address: address(message.real_address)?,
            result: PlayerLoginResult::from_wire(message.result)?,
            patch: v1::PlayerLoginPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn kick_message(&self) -> &str {
        &self.kick_message
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn real_address(&self) -> IpAddr {
        self.real_address
    }

    pub fn result(&self) -> PlayerLoginResult {
        self.result
    }

    pub fn set_kick_message(&mut self, value: String) {
        self.patch.kick_message = Some(value.clone());
        self.kick_message = value;
    }

    pub fn set_result(&mut self, value: PlayerLoginResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }
}

impl From<PlayerLogin> for Verdict {
    fn from(event: PlayerLogin) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerLoginPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerLogin(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerMove {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    from: Location,
    player: Player,
    to: Option<Location>,
    patch: v1::PlayerMovePatch,
}

impl PlayerMove {
    fn from_wire(message: v1::PlayerMove, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::PlayerMovePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_to(&mut self, value: Location) {
        self.patch.to = Some(value.to_wire());
        self.to = Some(value);
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerMove> for Verdict {
    fn from(event: PlayerMove) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerMovePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerMove(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerPickupArrow {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    arrow: Entity,
    player: Player,
    remaining: i32,
    item: ItemStack,
}

impl PlayerPickupArrow {
    fn from_wire(message: v1::PlayerPickupArrow, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            arrow: Entity::from_wire(required(message.arrow, "arrow")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            remaining: message.remaining,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn arrow(&self) -> &Entity {
        &self.arrow
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn remaining(&self) -> i32 {
        self.remaining
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerPickupArrow> for Verdict {
    fn from(event: PlayerPickupArrow) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerPortal {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    can_create_portal: bool,
    cause: PlayerTeleportCause,
    creation_radius: i32,
    from: Location,
    player: Player,
    search_radius: i32,
    to: Option<Location>,
    patch: v1::PlayerPortalPatch,
}

impl PlayerPortal {
    fn from_wire(message: v1::PlayerPortal, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            can_create_portal: message.can_create_portal,
            cause: PlayerTeleportCause::from_wire(message.cause)?,
            creation_radius: message.creation_radius,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            search_radius: message.search_radius,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::PlayerPortalPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn can_create_portal(&self) -> bool {
        self.can_create_portal
    }

    pub fn cause(&self) -> PlayerTeleportCause {
        self.cause
    }

    pub fn creation_radius(&self) -> i32 {
        self.creation_radius
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn search_radius(&self) -> i32 {
        self.search_radius
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_can_create_portal(&mut self, value: bool) {
        self.patch.can_create_portal = Some(value);
        self.can_create_portal = value;
    }

    pub fn set_creation_radius(&mut self, value: i32) {
        self.patch.creation_radius = Some(value);
        self.creation_radius = value;
    }

    pub fn set_search_radius(&mut self, value: i32) {
        self.patch.search_radius = Some(value);
        self.search_radius = value;
    }

    pub fn set_to(&mut self, value: Location) {
        self.patch.to = Some(value.to_wire());
        self.to = Some(value);
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerPortal> for Verdict {
    fn from(event: PlayerPortal) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerPortalPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerPortal(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerRecipeBookClick {
    id: u64,
    original_recipe: String,
    player: Player,
    recipe: String,
    shift_click: bool,
    patch: v1::PlayerRecipeBookClickPatch,
}

impl PlayerRecipeBookClick {
    fn from_wire(
        message: v1::PlayerRecipeBookClick,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            original_recipe: message.original_recipe,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            recipe: message.recipe,
            shift_click: message.shift_click,
            patch: v1::PlayerRecipeBookClickPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn original_recipe(&self) -> &str {
        &self.original_recipe
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub fn is_shift_click(&self) -> bool {
        self.shift_click
    }

    pub fn set_recipe(&mut self, value: String) {
        self.patch.recipe = Some(value.clone());
        self.recipe = value;
    }

    pub fn set_shift_click(&mut self, value: bool) {
        self.patch.shift_click = Some(value);
        self.shift_click = value;
    }
}

impl From<PlayerRecipeBookClick> for Verdict {
    fn from(event: PlayerRecipeBookClick) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerRecipeBookClickPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerRecipeBookClick(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerRecipeBookSettingsChange {
    id: u64,
    filtering: bool,
    open: bool,
    player: Player,
    recipe_book_type: PlayerRecipeBookSettingsChangeRecipeBookType,
}

impl PlayerRecipeBookSettingsChange {
    fn from_wire(
        message: v1::PlayerRecipeBookSettingsChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            filtering: message.filtering,
            open: message.open,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            recipe_book_type: PlayerRecipeBookSettingsChangeRecipeBookType::from_wire(
                message.recipe_book_type,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn recipe_book_type(&self) -> PlayerRecipeBookSettingsChangeRecipeBookType {
        self.recipe_book_type
    }
}

#[derive(Debug)]
pub struct PlayerRecipeDiscover {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    player: Player,
    recipe: String,
}

impl PlayerRecipeDiscover {
    fn from_wire(
        message: v1::PlayerRecipeDiscover,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            recipe: message.recipe,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn recipe(&self) -> &str {
        &self.recipe
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerRecipeDiscover> for Verdict {
    fn from(event: PlayerRecipeDiscover) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerResourcePackStatus {
    id: u64,
    pack_id: Uuid,
    player: Player,
    status: ResourcePackStatus,
}

impl PlayerResourcePackStatus {
    fn from_wire(
        message: v1::PlayerResourcePackStatus,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            pack_id: uuid(message.pack_id)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            status: ResourcePackStatus::from_wire(message.status)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn pack_id(&self) -> Uuid {
        self.pack_id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn status(&self) -> ResourcePackStatus {
        self.status
    }
}

#[derive(Debug)]
pub struct PlayerRespawn {
    id: u64,
    anchor_spawn: bool,
    bed_spawn: bool,
    player: Player,
    respawn_location: Location,
    respawn_reason: PlayerRespawnReason,
    patch: v1::PlayerRespawnPatch,
}

impl PlayerRespawn {
    fn from_wire(message: v1::PlayerRespawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            anchor_spawn: message.anchor_spawn,
            bed_spawn: message.bed_spawn,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            respawn_location: Location::from_wire(
                required(message.respawn_location, "respawn_location")?,
                link,
            )?,
            respawn_reason: PlayerRespawnReason::from_wire(message.respawn_reason)?,
            patch: v1::PlayerRespawnPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn is_anchor_spawn(&self) -> bool {
        self.anchor_spawn
    }

    pub fn is_bed_spawn(&self) -> bool {
        self.bed_spawn
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn respawn_location(&self) -> &Location {
        &self.respawn_location
    }

    pub fn respawn_reason(&self) -> PlayerRespawnReason {
        self.respawn_reason
    }

    pub fn set_respawn_location(&mut self, value: Location) {
        self.patch.respawn_location = Some(value.to_wire());
        self.respawn_location = value;
    }
}

impl From<PlayerRespawn> for Verdict {
    fn from(event: PlayerRespawn) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PlayerRespawnPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerRespawn(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerRiptide {
    id: u64,
    item: ItemStack,
    player: Player,
    velocity: Vector,
}

impl PlayerRiptide {
    fn from_wire(message: v1::PlayerRiptide, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            velocity: vector(required(message.velocity, "velocity")?),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn velocity(&self) -> Vector {
        self.velocity
    }
}

#[derive(Debug)]
pub struct PlayerShearEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    hand: EquipmentSlot,
    item: ItemStack,
    player: Player,
}

impl PlayerShearEntity {
    fn from_wire(message: v1::PlayerShearEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerShearEntity> for Verdict {
    fn from(event: PlayerShearEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerShowEntity {
    id: u64,
    entity: Entity,
    player: Player,
}

impl PlayerShowEntity {
    fn from_wire(message: v1::PlayerShowEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> &Player {
        &self.player
    }
}

#[derive(Debug)]
pub struct PlayerSignOpen {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: PlayerSignOpenCause,
    player: Player,
    side: Side,
    sign: SignData,
}

impl PlayerSignOpen {
    fn from_wire(message: v1::PlayerSignOpen, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: PlayerSignOpenCause::from_wire(message.cause)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            side: Side::from_wire(message.side)?,
            sign: SignData::from_wire(required(message.sign, "sign")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> PlayerSignOpenCause {
        self.cause
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn sign(&self) -> &SignData {
        &self.sign
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerSignOpen> for Verdict {
    fn from(event: PlayerSignOpen) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerSpawnChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: PlayerSpawnChangeCause,
    forced: bool,
    new_spawn: Option<Location>,
    player: Player,
    patch: v1::PlayerSpawnChangePatch,
}

impl PlayerSpawnChange {
    fn from_wire(message: v1::PlayerSpawnChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: PlayerSpawnChangeCause::from_wire(message.cause)?,
            forced: message.forced,
            new_spawn: message
                .new_spawn
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerSpawnChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> PlayerSpawnChangeCause {
        self.cause
    }

    pub fn is_forced(&self) -> bool {
        self.forced
    }

    pub fn new_spawn(&self) -> Option<&Location> {
        self.new_spawn.as_ref()
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_forced(&mut self, value: bool) {
        self.patch.forced = Some(value);
        self.forced = value;
    }

    pub fn set_new_spawn(&mut self, value: Option<Location>) {
        self.patch.new_spawn = Some(v1::NullableLocation {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.new_spawn = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerSpawnChange> for Verdict {
    fn from(event: PlayerSpawnChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerSpawnChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerSpawnChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerStatisticIncrement {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity_type: Option<EntityType>,
    material: Option<Material>,
    new_value: i32,
    player: Player,
    previous_value: i32,
    statistic: Statistic,
}

impl PlayerStatisticIncrement {
    fn from_wire(
        message: v1::PlayerStatisticIncrement,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity_type: message.entity_type.map(entity_type).transpose()?,
            material: message.material.map(material).transpose()?,
            new_value: message.new_value,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            previous_value: message.previous_value,
            statistic: Statistic::from_wire(message.statistic)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity_type(&self) -> Option<EntityType> {
        self.entity_type
    }

    pub fn material(&self) -> Option<Material> {
        self.material
    }

    pub fn new_value(&self) -> i32 {
        self.new_value
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn previous_value(&self) -> i32 {
        self.previous_value
    }

    pub fn statistic(&self) -> Statistic {
        self.statistic
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerStatisticIncrement> for Verdict {
    fn from(event: PlayerStatisticIncrement) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerSwapHandItems {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    main_hand_item: Option<ItemStack>,
    off_hand_item: Option<ItemStack>,
    player: Player,
    patch: v1::PlayerSwapHandItemsPatch,
}

impl PlayerSwapHandItems {
    fn from_wire(message: v1::PlayerSwapHandItems, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            main_hand_item: message
                .main_hand_item
                .map(ItemStack::from_wire)
                .transpose()?,
            off_hand_item: message
                .off_hand_item
                .map(ItemStack::from_wire)
                .transpose()?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            patch: v1::PlayerSwapHandItemsPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn main_hand_item(&self) -> Option<&ItemStack> {
        self.main_hand_item.as_ref()
    }

    pub fn off_hand_item(&self) -> Option<&ItemStack> {
        self.off_hand_item.as_ref()
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn set_main_hand_item(&mut self, value: Option<ItemStack>) {
        self.patch.main_hand_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.main_hand_item = value;
    }

    pub fn set_off_hand_item(&mut self, value: Option<ItemStack>) {
        self.patch.off_hand_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.off_hand_item = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerSwapHandItems> for Verdict {
    fn from(event: PlayerSwapHandItems) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerSwapHandItemsPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerSwapHandItems(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerTakeLecternBook {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    book: Option<ItemStack>,
    lectern: LecternData,
    player: Player,
}

impl PlayerTakeLecternBook {
    fn from_wire(
        message: v1::PlayerTakeLecternBook,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            book: message.book.map(ItemStack::from_wire).transpose()?,
            lectern: LecternData::from_wire(required(message.lectern, "lectern")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn book(&self) -> Option<&ItemStack> {
        self.book.as_ref()
    }

    pub fn lectern(&self) -> &LecternData {
        &self.lectern
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerTakeLecternBook> for Verdict {
    fn from(event: PlayerTakeLecternBook) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerTeleport {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    cause: PlayerTeleportCause,
    from: Location,
    player: Player,
    to: Option<Location>,
    patch: v1::PlayerTeleportPatch,
}

impl PlayerTeleport {
    fn from_wire(message: v1::PlayerTeleport, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            cause: PlayerTeleportCause::from_wire(message.cause)?,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            to: message
                .to
                .map(|value| Location::from_wire(value, link))
                .transpose()?,
            patch: v1::PlayerTeleportPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cause(&self) -> PlayerTeleportCause {
        self.cause
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn to(&self) -> Option<&Location> {
        self.to.as_ref()
    }

    pub fn set_from(&mut self, value: Location) {
        self.patch.from = Some(value.to_wire());
        self.from = value;
    }

    pub fn set_to(&mut self, value: Location) {
        self.patch.to = Some(value.to_wire());
        self.to = Some(value);
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerTeleport> for Verdict {
    fn from(event: PlayerTeleport) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerTeleportPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerTeleport(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PlayerToggleFlight {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    flying: bool,
    player: Player,
}

impl PlayerToggleFlight {
    fn from_wire(message: v1::PlayerToggleFlight, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            flying: message.flying,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn is_flying(&self) -> bool {
        self.flying
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerToggleFlight> for Verdict {
    fn from(event: PlayerToggleFlight) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerToggleSneak {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    player: Player,
    sneaking: bool,
}

impl PlayerToggleSneak {
    fn from_wire(message: v1::PlayerToggleSneak, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            sneaking: message.sneaking,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn is_sneaking(&self) -> bool {
        self.sneaking
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerToggleSneak> for Verdict {
    fn from(event: PlayerToggleSneak) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerToggleSprint {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    player: Player,
    sprinting: bool,
}

impl PlayerToggleSprint {
    fn from_wire(message: v1::PlayerToggleSprint, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            sprinting: message.sprinting,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn is_sprinting(&self) -> bool {
        self.sprinting
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerToggleSprint> for Verdict {
    fn from(event: PlayerToggleSprint) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerUnleashEntity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    hand: EquipmentSlot,
    player: Player,
    reason: EntityUnleashReason,
}

impl PlayerUnleashEntity {
    fn from_wire(message: v1::PlayerUnleashEntity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            hand: EquipmentSlot::from_wire(message.hand)?,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            reason: EntityUnleashReason::from_wire(message.reason)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn hand(&self) -> EquipmentSlot {
        self.hand
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn reason(&self) -> EntityUnleashReason {
        self.reason
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerUnleashEntity> for Verdict {
    fn from(event: PlayerUnleashEntity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PlayerVelocity {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    player: Player,
    velocity: Vector,
    patch: v1::PlayerVelocityPatch,
}

impl PlayerVelocity {
    fn from_wire(message: v1::PlayerVelocity, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            velocity: vector(required(message.velocity, "velocity")?),
            patch: v1::PlayerVelocityPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn velocity(&self) -> Vector {
        self.velocity
    }

    pub fn set_velocity(&mut self, value: Vector) {
        self.patch.velocity = Some(value.to_wire());
        self.velocity = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PlayerVelocity> for Verdict {
    fn from(event: PlayerVelocity) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PlayerVelocityPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PlayerVelocity(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginDisable {
    id: u64,
    plugin: PluginInfo,
}

impl PluginDisable {
    fn from_wire(message: v1::PluginDisable, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            plugin: PluginInfo::from_wire(required(message.plugin, "plugin")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn plugin(&self) -> &PluginInfo {
        &self.plugin
    }
}

#[derive(Debug, Clone)]
pub struct PluginEnable {
    id: u64,
    plugin: PluginInfo,
}

impl PluginEnable {
    fn from_wire(message: v1::PluginEnable, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            plugin: PluginInfo::from_wire(required(message.plugin, "plugin")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn plugin(&self) -> &PluginInfo {
        &self.plugin
    }
}

#[derive(Debug)]
pub struct PortalCreate {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    blocks: Vec<BlockSnapshot>,
    entity: Option<Entity>,
    reason: PortalCreateReason,
    world: World,
}

impl PortalCreate {
    fn from_wire(message: v1::PortalCreate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            blocks: message
                .blocks
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            entity: message
                .entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            reason: PortalCreateReason::from_wire(message.reason)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn blocks(&self) -> &[BlockSnapshot] {
        &self.blocks
    }

    pub fn entity(&self) -> Option<&Entity> {
        self.entity.as_ref()
    }

    pub fn reason(&self) -> PortalCreateReason {
        self.reason
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PortalCreate> for Verdict {
    fn from(event: PortalCreate) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PotionSplash {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    affected_entities: Vec<Entity>,
    entity: Entity,
    entity_type: EntityType,
    intensities: Vec<(Entity, f64)>,
    hit_block: Option<Block>,
    hit_block_face: Option<Face>,
    hit_entity: Option<Entity>,
    potion: Entity,
    patch: v1::PotionSplashPatch,
}

impl PotionSplash {
    fn from_wire(message: v1::PotionSplash, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            affected_entities: message
                .affected_entities
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            intensities: message
                .intensities
                .into_iter()
                .map(|entry| {
                    Ok((
                        Entity::from_wire(required(entry.key, "intensities")?, link)?,
                        entry.value,
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?,
            hit_block: message.hit_block.map(|value| block(value, link)),
            hit_block_face: message.hit_block_face.map(Face::from_wire).transpose()?,
            hit_entity: message
                .hit_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            potion: Entity::from_wire(required(message.potion, "potion")?, link)?,
            patch: v1::PotionSplashPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn affected_entities(&self) -> &[Entity] {
        &self.affected_entities
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn intensity(&self, key: &Entity) -> f64 {
        self.intensities
            .iter()
            .find(|(known, _)| known.entity_id == key.entity_id)
            .map(|(_, value)| *value)
            .unwrap_or_default()
    }

    pub fn hit_block(&self) -> Option<&Block> {
        self.hit_block.as_ref()
    }

    pub fn hit_block_face(&self) -> Option<Face> {
        self.hit_block_face
    }

    pub fn hit_entity(&self) -> Option<&Entity> {
        self.hit_entity.as_ref()
    }

    pub fn potion(&self) -> &Entity {
        &self.potion
    }

    pub fn set_intensity(&mut self, key: &Entity, value: f64) {
        let entry = v1::EntitySnapshotDoubleEntry {
            key: Some(key.to_wire()),
            value,
        };
        match self.patch.intensities.iter_mut().find(|known| {
            known
                .key
                .as_ref()
                .is_some_and(|known| known.entity_id == key.entity_id)
        }) {
            Some(known) => *known = entry,
            None => self.patch.intensities.push(entry),
        }
        match self
            .intensities
            .iter_mut()
            .find(|(known, _)| known.entity_id == key.entity_id)
        {
            Some((_, known)) => *known = value,
            None => self.intensities.push((key.clone(), value)),
        }
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PotionSplash> for Verdict {
    fn from(event: PotionSplash) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::PotionSplashPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PotionSplash(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PrepareAnvil {
    id: u64,
    inventory: InventoryRef,
    result: Option<ItemStack>,
    view: InventoryView,
    viewers: Vec<Player>,
    patch: v1::PrepareAnvilPatch,
}

impl PrepareAnvil {
    fn from_wire(message: v1::PrepareAnvil, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            result: message.result.map(ItemStack::from_wire).transpose()?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            patch: v1::PrepareAnvilPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn result(&self) -> Option<&ItemStack> {
        self.result.as_ref()
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn set_result(&mut self, value: Option<ItemStack>) {
        self.patch.result = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.result = value;
    }
}

impl From<PrepareAnvil> for Verdict {
    fn from(event: PrepareAnvil) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PrepareAnvilPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PrepareAnvil(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PrepareGrindstone {
    id: u64,
    inventory: InventoryRef,
    result: Option<ItemStack>,
    view: InventoryView,
    viewers: Vec<Player>,
    patch: v1::PrepareGrindstonePatch,
}

impl PrepareGrindstone {
    fn from_wire(message: v1::PrepareGrindstone, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            result: message.result.map(ItemStack::from_wire).transpose()?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            patch: v1::PrepareGrindstonePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn result(&self) -> Option<&ItemStack> {
        self.result.as_ref()
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn set_result(&mut self, value: Option<ItemStack>) {
        self.patch.result = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.result = value;
    }
}

impl From<PrepareGrindstone> for Verdict {
    fn from(event: PrepareGrindstone) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PrepareGrindstonePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PrepareGrindstone(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PrepareItemCraft {
    id: u64,
    inventory: InventoryRef,
    recipe: Option<String>,
    repair: bool,
    view: InventoryView,
    viewers: Vec<Player>,
    result: Option<ItemStack>,
    patch: v1::PrepareItemCraftPatch,
}

impl PrepareItemCraft {
    fn from_wire(message: v1::PrepareItemCraft, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            recipe: message.recipe,
            repair: message.repair,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            result: message.result.map(ItemStack::from_wire).transpose()?,
            patch: v1::PrepareItemCraftPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn recipe(&self) -> Option<&str> {
        self.recipe.as_deref()
    }

    pub fn is_repair(&self) -> bool {
        self.repair
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn result(&self) -> Option<&ItemStack> {
        self.result.as_ref()
    }

    pub fn set_result(&mut self, value: Option<ItemStack>) {
        self.patch.result = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.result = value;
    }
}

impl From<PrepareItemCraft> for Verdict {
    fn from(event: PrepareItemCraft) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PrepareItemCraftPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PrepareItemCraft(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct PrepareItemEnchant {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    enchant_block: Block,
    enchanter: Player,
    enchantment_bonus: i32,
    inventory: InventoryRef,
    item: ItemStack,
    offers: Vec<EnchantOffer>,
    view: InventoryView,
    viewers: Vec<Player>,
}

impl PrepareItemEnchant {
    fn from_wire(message: v1::PrepareItemEnchant, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            enchant_block: block(required(message.enchant_block, "enchant_block")?, link),
            enchanter: Player::from_snapshot(required(message.enchanter, "enchanter")?, link)?,
            enchantment_bonus: message.enchantment_bonus,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            item: ItemStack::from_wire(required(message.item, "item")?)?,
            offers: message
                .offers
                .into_iter()
                .map(|value| EnchantOffer::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn enchant_block(&self) -> &Block {
        &self.enchant_block
    }

    pub fn player(&self) -> &Player {
        &self.enchanter
    }

    pub fn enchantment_bonus(&self) -> i32 {
        self.enchantment_bonus
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn item(&self) -> &ItemStack {
        &self.item
    }

    pub fn offers(&self) -> &[EnchantOffer] {
        &self.offers
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<PrepareItemEnchant> for Verdict {
    fn from(event: PrepareItemEnchant) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct PrepareSmithing {
    id: u64,
    inventory: InventoryRef,
    result: Option<ItemStack>,
    view: InventoryView,
    viewers: Vec<Player>,
    patch: v1::PrepareSmithingPatch,
}

impl PrepareSmithing {
    fn from_wire(message: v1::PrepareSmithing, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            result: message.result.map(ItemStack::from_wire).transpose()?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            patch: v1::PrepareSmithingPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn result(&self) -> Option<&ItemStack> {
        self.result.as_ref()
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn set_result(&mut self, value: Option<ItemStack>) {
        self.patch.result = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.result = value;
    }
}

impl From<PrepareSmithing> for Verdict {
    fn from(event: PrepareSmithing) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::PrepareSmithingPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::PrepareSmithing(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct ProjectileHit {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    hit_block: Option<Block>,
    hit_block_face: Option<Face>,
    hit_entity: Option<Entity>,
}

impl ProjectileHit {
    fn from_wire(message: v1::ProjectileHit, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            hit_block: message.hit_block.map(|value| block(value, link)),
            hit_block_face: message.hit_block_face.map(Face::from_wire).transpose()?,
            hit_entity: message
                .hit_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn hit_block(&self) -> Option<&Block> {
        self.hit_block.as_ref()
    }

    pub fn hit_block_face(&self) -> Option<Face> {
        self.hit_block_face
    }

    pub fn hit_entity(&self) -> Option<&Entity> {
        self.hit_entity.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ProjectileHit> for Verdict {
    fn from(event: ProjectileHit) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct ProjectileLaunch {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
}

impl ProjectileLaunch {
    fn from_wire(message: v1::ProjectileLaunch, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ProjectileLaunch> for Verdict {
    fn from(event: ProjectileLaunch) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RaidFinish {
    id: u64,
    raid: RaidRef,
    winners: Vec<Player>,
    world: World,
}

impl RaidFinish {
    fn from_wire(message: v1::RaidFinish, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            raid: RaidRef::from_wire(required(message.raid, "raid")?, link)?,
            winners: message
                .winners
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn raid(&self) -> &RaidRef {
        &self.raid
    }

    pub fn winners(&self) -> &[Player] {
        &self.winners
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct RaidSpawnWave {
    id: u64,
    patrol_leader: Option<Entity>,
    raid: RaidRef,
    raiders: Vec<Entity>,
    world: World,
}

impl RaidSpawnWave {
    fn from_wire(message: v1::RaidSpawnWave, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            patrol_leader: message
                .patrol_leader
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            raid: RaidRef::from_wire(required(message.raid, "raid")?, link)?,
            raiders: message
                .raiders
                .into_iter()
                .map(|value| Entity::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn patrol_leader(&self) -> Option<&Entity> {
        self.patrol_leader.as_ref()
    }

    pub fn raid(&self) -> &RaidRef {
        &self.raid
    }

    pub fn raiders(&self) -> &[Entity] {
        &self.raiders
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct RaidStop {
    id: u64,
    raid: RaidRef,
    reason: RaidStopReason,
    world: World,
}

impl RaidStop {
    fn from_wire(message: v1::RaidStop, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            raid: RaidRef::from_wire(required(message.raid, "raid")?, link)?,
            reason: RaidStopReason::from_wire(message.reason)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn raid(&self) -> &RaidRef {
        &self.raid
    }

    pub fn reason(&self) -> RaidStopReason {
        self.reason
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct RaidTrigger {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    player: Player,
    raid: RaidRef,
    world: World,
}

impl RaidTrigger {
    fn from_wire(message: v1::RaidTrigger, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            raid: RaidRef::from_wire(required(message.raid, "raid")?, link)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn raid(&self) -> &RaidRef {
        &self.raid
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<RaidTrigger> for Verdict {
    fn from(event: RaidTrigger) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct RemoteServerCommand {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    command: String,
    sender: CommandSender,
    patch: v1::RemoteServerCommandPatch,
}

impl RemoteServerCommand {
    fn from_wire(message: v1::RemoteServerCommand, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            command: message.command,
            sender: CommandSender::from_wire(required(message.sender, "sender")?, link)?,
            patch: v1::RemoteServerCommandPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn sender(&self) -> &CommandSender {
        &self.sender
    }

    pub fn set_command(&mut self, value: String) {
        self.patch.command = Some(value.clone());
        self.command = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<RemoteServerCommand> for Verdict {
    fn from(event: RemoteServerCommand) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::RemoteServerCommandPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::RemoteServerCommand(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct SculkBloom {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    charge: i32,
    patch: v1::SculkBloomPatch,
}

impl SculkBloom {
    fn from_wire(message: v1::SculkBloom, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            charge: message.charge,
            patch: v1::SculkBloomPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn charge(&self) -> i32 {
        self.charge
    }

    pub fn set_charge(&mut self, value: i32) {
        self.patch.charge = Some(value);
        self.charge = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SculkBloom> for Verdict {
    fn from(event: SculkBloom) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::SculkBloomPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::SculkBloom(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct ServerCommand {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    command: String,
    sender: CommandSender,
    patch: v1::ServerCommandPatch,
}

impl ServerCommand {
    fn from_wire(message: v1::ServerCommand, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            command: message.command,
            sender: CommandSender::from_wire(required(message.sender, "sender")?, link)?,
            patch: v1::ServerCommandPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn sender(&self) -> &CommandSender {
        &self.sender
    }

    pub fn set_command(&mut self, value: String) {
        self.patch.command = Some(value.clone());
        self.command = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ServerCommand> for Verdict {
    fn from(event: ServerCommand) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::ServerCommandPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ServerCommand(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct ServerListPing {
    id: u64,
    address: IpAddr,
    hostname: String,
    players: Vec<Player>,
    max_players: i32,
    motd: String,
    num_players: i32,
    hidden_players: Vec<Uuid>,
    hidden_players_snapshot: Option<Vec<Uuid>>,
    patch: v1::ServerListPingPatch,
}

impl ServerListPing {
    fn from_wire(message: v1::ServerListPing, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            address: address(message.address)?,
            hostname: message.hostname,
            players: message
                .players
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            max_players: message.max_players,
            motd: message.motd,
            num_players: message.num_players,
            hidden_players: message
                .hidden_players
                .into_iter()
                .map(uuid)
                .collect::<Result<Vec<_>, _>>()?,
            hidden_players_snapshot: None,
            patch: v1::ServerListPingPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn players(&self) -> &[Player] {
        &self.players
    }

    pub fn max_players(&self) -> i32 {
        self.max_players
    }

    pub fn motd(&self) -> &str {
        &self.motd
    }

    pub fn num_players(&self) -> i32 {
        self.num_players
    }

    pub fn hidden_players(&self) -> &[Uuid] {
        &self.hidden_players
    }

    pub fn set_max_players(&mut self, value: i32) {
        self.patch.max_players = Some(value);
        self.max_players = value;
    }

    pub fn set_motd(&mut self, value: String) {
        self.patch.motd = Some(value.clone());
        self.motd = value;
    }

    pub fn set_server_icon(&mut self, value: Vec<u8>) {
        self.patch.server_icon = Some(value.clone());
    }

    pub fn set_hidden_players(&mut self, value: Vec<Uuid>) {
        let snapshot = self
            .hidden_players_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.hidden_players));
        self.patch.hidden_players = diff::list(
            &snapshot
                .iter()
                .copied()
                .map(|value| value.hyphenated().to_string())
                .collect::<Vec<_>>(),
            value
                .iter()
                .copied()
                .map(|value| value.hyphenated().to_string())
                .collect(),
        )
        .map(|diff| v1::StringListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::StringReplacement { index, value })
                .collect(),
            added: diff.added,
        });
        self.hidden_players = value;
    }
}

impl From<ServerListPing> for Verdict {
    fn from(event: ServerListPing) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::ServerListPingPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::ServerListPing(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerLoad {
    id: u64,
    load_type: ServerLoadType,
}

impl ServerLoad {
    fn from_wire(message: v1::ServerLoad, meta: Meta, _link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            load_type: ServerLoadType::from_wire(message.load_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn load_type(&self) -> ServerLoadType {
        self.load_type
    }
}

#[derive(Debug)]
pub struct SheepDyeWool {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    color: DyeColor,
    entity: Entity,
    entity_type: EntityType,
    player: Option<Player>,
    patch: v1::SheepDyeWoolPatch,
}

impl SheepDyeWool {
    fn from_wire(message: v1::SheepDyeWool, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            color: DyeColor::from_wire(message.color)?,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
            patch: v1::SheepDyeWoolPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn color(&self) -> DyeColor {
        self.color
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub fn set_color(&mut self, value: DyeColor) {
        self.patch.color = Some(value.to_wire());
        self.color = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SheepDyeWool> for Verdict {
    fn from(event: SheepDyeWool) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::SheepDyeWoolPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::SheepDyeWool(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct SheepRegrowWool {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
}

impl SheepRegrowWool {
    fn from_wire(message: v1::SheepRegrowWool, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SheepRegrowWool> for Verdict {
    fn from(event: SheepRegrowWool) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct SignChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    lines: Vec<String>,
    player: Player,
    side: Side,
    lines_snapshot: Option<Vec<String>>,
    patch: v1::SignChangePatch,
}

impl SignChange {
    fn from_wire(message: v1::SignChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            lines: message.lines,
            player: Player::from_snapshot(required(message.player, "player")?, link)?,
            side: Side::from_wire(message.side)?,
            lines_snapshot: None,
            patch: v1::SignChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn player(&self) -> &Player {
        &self.player
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn set_lines(&mut self, value: Vec<String>) {
        let snapshot = self
            .lines_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.lines));
        self.patch.lines = diff::slots(snapshot, value.clone()).map(|diff| v1::StringListDiff {
            removed: diff.removed,
            replaced: diff
                .replaced
                .into_iter()
                .map(|(index, value)| v1::StringReplacement { index, value })
                .collect(),
            added: diff.added,
        });
        self.lines = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SignChange> for Verdict {
    fn from(event: SignChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::SignChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::SignChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct SmithItem {
    id: u64,
    rolled_back: bool,
    action: InventoryAction,
    click: ClickKind,
    clicked_inventory: Option<InventoryRef>,
    current_item: Option<ItemStack>,
    cursor: Option<ItemStack>,
    hotbar_button: i32,
    inventory: InventoryRef,
    left_click: bool,
    raw_slot: i32,
    result: EventResult,
    right_click: bool,
    shift_click: bool,
    slot: i32,
    slot_type: InventoryTypeSlotType,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::SmithItemPatch,
}

impl SmithItem {
    fn from_wire(message: v1::SmithItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            action: InventoryAction::from_wire(message.action)?,
            click: ClickKind::from_wire(message.click)?,
            clicked_inventory: message
                .clicked_inventory
                .map(|value| InventoryRef::from_wire(value, link))
                .transpose()?,
            current_item: message.current_item.map(ItemStack::from_wire).transpose()?,
            cursor: message.cursor.map(ItemStack::from_wire).transpose()?,
            hotbar_button: message.hotbar_button,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            left_click: message.left_click,
            raw_slot: message.raw_slot,
            result: EventResult::from_wire(message.result)?,
            right_click: message.right_click,
            shift_click: message.shift_click,
            slot: message.slot,
            slot_type: InventoryTypeSlotType::from_wire(message.slot_type)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::SmithItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn action(&self) -> InventoryAction {
        self.action
    }

    pub fn click(&self) -> ClickKind {
        self.click
    }

    pub fn clicked_inventory(&self) -> Option<&InventoryRef> {
        self.clicked_inventory.as_ref()
    }

    pub fn current_item(&self) -> Option<&ItemStack> {
        self.current_item.as_ref()
    }

    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    pub fn hotbar_button(&self) -> i32 {
        self.hotbar_button
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn is_left_click(&self) -> bool {
        self.left_click
    }

    pub fn raw_slot(&self) -> i32 {
        self.raw_slot
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn is_right_click(&self) -> bool {
        self.right_click
    }

    pub fn is_shift_click(&self) -> bool {
        self.shift_click
    }

    pub fn slot(&self) -> i32 {
        self.slot
    }

    pub fn slot_type(&self) -> InventoryTypeSlotType {
        self.slot_type
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_current_item(&mut self, value: Option<ItemStack>) {
        self.patch.current_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.current_item = value;
    }

    pub fn set_cursor(&mut self, value: Option<ItemStack>) {
        self.patch.cursor = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.cursor = value;
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SmithItem> for Verdict {
    fn from(event: SmithItem) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::SmithItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::SmithItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpawnChange {
    id: u64,
    previous_location: Location,
    world: World,
}

impl SpawnChange {
    fn from_wire(message: v1::SpawnChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            previous_location: Location::from_wire(
                required(message.previous_location, "previous_location")?,
                link,
            )?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn previous_location(&self) -> &Location {
        &self.previous_location
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct SpawnerSpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
    spawner: SpawnerData,
}

impl SpawnerSpawn {
    fn from_wire(message: v1::SpawnerSpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            spawner: SpawnerData::from_wire(required(message.spawner, "spawner")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn spawner(&self) -> &SpawnerData {
        &self.spawner
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SpawnerSpawn> for Verdict {
    fn from(event: SpawnerSpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct SpongeAbsorb {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    blocks: Vec<BlockSnapshot>,
}

impl SpongeAbsorb {
    fn from_wire(message: v1::SpongeAbsorb, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            blocks: message
                .blocks
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn blocks(&self) -> &[BlockSnapshot] {
        &self.blocks
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<SpongeAbsorb> for Verdict {
    fn from(event: SpongeAbsorb) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct StriderTemperatureChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    shivering: bool,
}

impl StriderTemperatureChange {
    fn from_wire(
        message: v1::StriderTemperatureChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            shivering: message.shivering,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn is_shivering(&self) -> bool {
        self.shivering
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<StriderTemperatureChange> for Verdict {
    fn from(event: StriderTemperatureChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct StructureGrow {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    blocks: Vec<BlockSnapshot>,
    from_bonemeal: bool,
    location: Location,
    player: Option<Player>,
    species: TreeType,
    world: World,
}

impl StructureGrow {
    fn from_wire(message: v1::StructureGrow, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            blocks: message
                .blocks
                .into_iter()
                .map(|value| BlockSnapshot::from_wire(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            from_bonemeal: message.from_bonemeal,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            player: message
                .player
                .map(|value| Player::from_snapshot(value, link))
                .transpose()?,
            species: TreeType::from_wire(message.species)?,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn blocks(&self) -> &[BlockSnapshot] {
        &self.blocks
    }

    pub fn is_from_bonemeal(&self) -> bool {
        self.from_bonemeal
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    pub fn species(&self) -> TreeType {
        self.species
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<StructureGrow> for Verdict {
    fn from(event: StructureGrow) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct TNTPrime {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    cause: TNTPrimeCause,
    priming_block: Option<Block>,
    priming_entity: Option<Entity>,
}

impl TNTPrime {
    fn from_wire(message: v1::TntPrime, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            cause: TNTPrimeCause::from_wire(message.cause)?,
            priming_block: message.priming_block.map(|value| block(value, link)),
            priming_entity: message
                .priming_entity
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn cause(&self) -> TNTPrimeCause {
        self.cause
    }

    pub fn priming_block(&self) -> Option<&Block> {
        self.priming_block.as_ref()
    }

    pub fn priming_entity(&self) -> Option<&Entity> {
        self.priming_entity.as_ref()
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<TNTPrime> for Verdict {
    fn from(event: TNTPrime) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct TabComplete {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    buffer: String,
    completions: Vec<String>,
    sender: CommandSender,
    completions_snapshot: Option<Vec<String>>,
    patch: v1::TabCompletePatch,
}

impl TabComplete {
    fn from_wire(message: v1::TabComplete, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            buffer: message.buffer,
            completions: message.completions,
            sender: CommandSender::from_wire(required(message.sender, "sender")?, link)?,
            completions_snapshot: None,
            patch: v1::TabCompletePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn completions(&self) -> &[String] {
        &self.completions
    }

    pub fn sender(&self) -> &CommandSender {
        &self.sender
    }

    pub fn set_completions(&mut self, value: Vec<String>) {
        let snapshot = self
            .completions_snapshot
            .get_or_insert_with(|| std::mem::take(&mut self.completions));
        self.patch.completions =
            diff::list(snapshot, value.clone()).map(|diff| v1::StringListDiff {
                removed: diff.removed,
                replaced: diff
                    .replaced
                    .into_iter()
                    .map(|(index, value)| v1::StringReplacement { index, value })
                    .collect(),
                added: diff.added,
            });
        self.completions = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<TabComplete> for Verdict {
    fn from(event: TabComplete) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::TabCompletePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::TabComplete(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct ThunderChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    to_thunder_state: bool,
    world: World,
}

impl ThunderChange {
    fn from_wire(message: v1::ThunderChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            to_thunder_state: message.to_thunder_state,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn to_thunder_state(&self) -> bool {
        self.to_thunder_state
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<ThunderChange> for Verdict {
    fn from(event: ThunderChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct TimeSkip {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    skip_amount: i64,
    skip_reason: TimeSkipReason,
    world: World,
    patch: v1::TimeSkipPatch,
}

impl TimeSkip {
    fn from_wire(message: v1::TimeSkip, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            skip_amount: message.skip_amount,
            skip_reason: TimeSkipReason::from_wire(message.skip_reason)?,
            world: world(message.world, link),
            patch: v1::TimeSkipPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn skip_amount(&self) -> i64 {
        self.skip_amount
    }

    pub fn skip_reason(&self) -> TimeSkipReason {
        self.skip_reason
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn set_skip_amount(&mut self, value: i64) {
        self.patch.skip_amount = Some(value);
        self.skip_amount = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<TimeSkip> for Verdict {
    fn from(event: TimeSkip) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::TimeSkipPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::TimeSkip(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct TradeSelect {
    id: u64,
    rolled_back: bool,
    index: i32,
    inventory: InventoryRef,
    merchant: MerchantRef,
    result: EventResult,
    view: InventoryView,
    viewers: Vec<Player>,
    who_clicked: Player,
    patch: v1::TradeSelectPatch,
}

impl TradeSelect {
    fn from_wire(message: v1::TradeSelect, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            rolled_back: meta.rolled_back,
            index: message.index,
            inventory: InventoryRef::from_wire(required(message.inventory, "inventory")?, link)?,
            merchant: MerchantRef::from_wire(required(message.merchant, "merchant")?, link)?,
            result: EventResult::from_wire(message.result)?,
            view: InventoryView::from_wire(required(message.view, "view")?, link)?,
            viewers: message
                .viewers
                .into_iter()
                .map(|value| Player::from_snapshot(value, link))
                .collect::<Result<Vec<_>, _>>()?,
            who_clicked: Player::from_snapshot(
                required(message.who_clicked, "who_clicked")?,
                link,
            )?,
            patch: v1::TradeSelectPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn inventory(&self) -> &InventoryRef {
        &self.inventory
    }

    pub fn merchant(&self) -> &MerchantRef {
        &self.merchant
    }

    pub fn result(&self) -> EventResult {
        self.result
    }

    pub fn view(&self) -> &InventoryView {
        &self.view
    }

    pub fn viewers(&self) -> &[Player] {
        &self.viewers
    }

    pub fn player(&self) -> &Player {
        &self.who_clicked
    }

    pub fn set_result(&mut self, value: EventResult) {
        self.patch.result = Some(value.to_wire());
        self.result = value;
    }

    pub fn cancel(&mut self) {
        self.set_result(EventResult::Deny);
    }

    pub fn is_cancelled(&self) -> bool {
        self.result == EventResult::Deny
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<TradeSelect> for Verdict {
    fn from(event: TradeSelect) -> Self {
        Self {
            cancel: false,
            patch: (event.patch != v1::TradeSelectPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::TradeSelect(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct TrialSpawnerSpawn {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    location: Location,
    trial_spawner: TrialSpawnerData,
}

impl TrialSpawnerSpawn {
    fn from_wire(message: v1::TrialSpawnerSpawn, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            location: Location::from_wire(required(message.location, "location")?, link)?,
            trial_spawner: TrialSpawnerData::from_wire(
                required(message.trial_spawner, "trial_spawner")?,
                link,
            )?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn trial_spawner(&self) -> &TrialSpawnerData {
        &self.trial_spawner
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<TrialSpawnerSpawn> for Verdict {
    fn from(event: TrialSpawnerSpawn) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct VaultDisplayItem {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    block: Block,
    display_item: Option<ItemStack>,
    patch: v1::VaultDisplayItemPatch,
}

impl VaultDisplayItem {
    fn from_wire(message: v1::VaultDisplayItem, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            block: block(required(message.block, "block")?, link),
            display_item: message.display_item.map(ItemStack::from_wire).transpose()?,
            patch: v1::VaultDisplayItemPatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn display_item(&self) -> Option<&ItemStack> {
        self.display_item.as_ref()
    }

    pub fn set_display_item(&mut self, value: Option<ItemStack>) {
        self.patch.display_item = Some(v1::NullableItem {
            value: value.as_ref().map(|value| value.to_wire()),
        });
        self.display_item = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VaultDisplayItem> for Verdict {
    fn from(event: VaultDisplayItem) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VaultDisplayItemPatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VaultDisplayItem(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VehicleBlockCollision {
    id: u64,
    block: Block,
    vehicle: Entity,
}

impl VehicleBlockCollision {
    fn from_wire(
        message: v1::VehicleBlockCollision,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            block: block(required(message.block, "block")?, link),
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }
}

#[derive(Debug)]
pub struct VehicleCreate {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    vehicle: Entity,
}

impl VehicleCreate {
    fn from_wire(message: v1::VehicleCreate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleCreate> for Verdict {
    fn from(event: VehicleCreate) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct VehicleDamage {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    attacker: Option<Entity>,
    damage: f64,
    vehicle: Entity,
    patch: v1::VehicleDamagePatch,
}

impl VehicleDamage {
    fn from_wire(message: v1::VehicleDamage, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            attacker: message
                .attacker
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            damage: message.damage,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
            patch: v1::VehicleDamagePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn attacker(&self) -> Option<&Entity> {
        self.attacker.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.attacker
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub fn damage(&self) -> f64 {
        self.damage
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub fn set_damage(&mut self, value: f64) {
        self.patch.damage = Some(value);
        self.damage = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleDamage> for Verdict {
    fn from(event: VehicleDamage) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VehicleDamagePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VehicleDamage(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct VehicleDestroy {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    attacker: Option<Entity>,
    vehicle: Entity,
}

impl VehicleDestroy {
    fn from_wire(message: v1::VehicleDestroy, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            attacker: message
                .attacker
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn attacker(&self) -> Option<&Entity> {
        self.attacker.as_ref()
    }

    pub fn player(&self) -> Option<&Player> {
        self.attacker
            .as_ref()
            .and_then(|entity| entity.player.as_ref())
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleDestroy> for Verdict {
    fn from(event: VehicleDestroy) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct VehicleEnter {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entered: Entity,
    vehicle: Entity,
}

impl VehicleEnter {
    fn from_wire(message: v1::VehicleEnter, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entered: Entity::from_wire(required(message.entered, "entered")?, link)?,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entered(&self) -> &Entity {
        &self.entered
    }

    pub fn player(&self) -> Option<&Player> {
        self.entered.player.as_ref()
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleEnter> for Verdict {
    fn from(event: VehicleEnter) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct VehicleEntityCollision {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    collision_cancelled: bool,
    entity: Entity,
    pickup_cancelled: bool,
    vehicle: Entity,
}

impl VehicleEntityCollision {
    fn from_wire(
        message: v1::VehicleEntityCollision,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            collision_cancelled: message.collision_cancelled,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            pickup_cancelled: message.pickup_cancelled,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn is_collision_cancelled(&self) -> bool {
        self.collision_cancelled
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn player(&self) -> Option<&Player> {
        self.entity.player.as_ref()
    }

    pub fn is_pickup_cancelled(&self) -> bool {
        self.pickup_cancelled
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleEntityCollision> for Verdict {
    fn from(event: VehicleEntityCollision) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug)]
pub struct VehicleExit {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    exited: Entity,
    vehicle: Entity,
}

impl VehicleExit {
    fn from_wire(message: v1::VehicleExit, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            exited: Entity::from_wire(required(message.exited, "exited")?, link)?,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn exited(&self) -> &Entity {
        &self.exited
    }

    pub fn player(&self) -> Option<&Player> {
        self.exited.player.as_ref()
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VehicleExit> for Verdict {
    fn from(event: VehicleExit) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VehicleMove {
    id: u64,
    from: Location,
    to: Location,
    vehicle: Entity,
}

impl VehicleMove {
    fn from_wire(message: v1::VehicleMove, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            from: Location::from_wire(required(message.from, "from")?, link)?,
            to: Location::from_wire(required(message.to, "to")?, link)?,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn from(&self) -> &Location {
        &self.from
    }

    pub fn to(&self) -> &Location {
        &self.to
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }
}

#[derive(Debug, Clone)]
pub struct VehicleUpdate {
    id: u64,
    vehicle: Entity,
}

impl VehicleUpdate {
    fn from_wire(message: v1::VehicleUpdate, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            vehicle: Entity::from_wire(required(message.vehicle, "vehicle")?, link)?,
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn vehicle(&self) -> &Entity {
        &self.vehicle
    }
}

#[derive(Debug)]
pub struct VillagerAcquireTrade {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    recipe: Trade,
    patch: v1::VillagerAcquireTradePatch,
}

impl VillagerAcquireTrade {
    fn from_wire(
        message: v1::VillagerAcquireTrade,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            recipe: Trade::from_wire(required(message.recipe, "recipe")?, link)?,
            patch: v1::VillagerAcquireTradePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn recipe(&self) -> &Trade {
        &self.recipe
    }

    pub fn set_recipe(&mut self, value: Trade) {
        self.patch.recipe = Some(value.to_wire());
        self.recipe = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VillagerAcquireTrade> for Verdict {
    fn from(event: VillagerAcquireTrade) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VillagerAcquireTradePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VillagerAcquireTrade(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct VillagerCareerChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    profession: String,
    reason: VillagerCareerChangeReason,
    patch: v1::VillagerCareerChangePatch,
}

impl VillagerCareerChange {
    fn from_wire(
        message: v1::VillagerCareerChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            profession: message.profession,
            reason: VillagerCareerChangeReason::from_wire(message.reason)?,
            patch: v1::VillagerCareerChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn profession(&self) -> &str {
        &self.profession
    }

    pub fn reason(&self) -> VillagerCareerChangeReason {
        self.reason
    }

    pub fn set_profession(&mut self, value: String) {
        self.patch.profession = Some(value.clone());
        self.profession = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VillagerCareerChange> for Verdict {
    fn from(event: VillagerCareerChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VillagerCareerChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VillagerCareerChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct VillagerReplenishTrade {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    recipe: Trade,
    patch: v1::VillagerReplenishTradePatch,
}

impl VillagerReplenishTrade {
    fn from_wire(
        message: v1::VillagerReplenishTrade,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            recipe: Trade::from_wire(required(message.recipe, "recipe")?, link)?,
            patch: v1::VillagerReplenishTradePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn recipe(&self) -> &Trade {
        &self.recipe
    }

    pub fn set_recipe(&mut self, value: Trade) {
        self.patch.recipe = Some(value.to_wire());
        self.recipe = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VillagerReplenishTrade> for Verdict {
    fn from(event: VillagerReplenishTrade) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VillagerReplenishTradePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VillagerReplenishTrade(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct VillagerReputationChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    entity: Entity,
    entity_type: EntityType,
    max_value: i32,
    new_value: i32,
    old_value: i32,
    reason: String,
    reputation_type: String,
    target: Option<Entity>,
    target_uuid: Uuid,
    patch: v1::VillagerReputationChangePatch,
}

impl VillagerReputationChange {
    fn from_wire(
        message: v1::VillagerReputationChange,
        meta: Meta,
        link: &Link,
    ) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            entity: Entity::from_wire(required(message.entity, "entity")?, link)?,
            entity_type: entity_type(message.entity_type)?,
            max_value: message.max_value,
            new_value: message.new_value,
            old_value: message.old_value,
            reason: message.reason,
            reputation_type: message.reputation_type,
            target: message
                .target
                .map(|value| Entity::from_wire(value, link))
                .transpose()?,
            target_uuid: uuid(message.target_uuid)?,
            patch: v1::VillagerReputationChangePatch::default(),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn entity(&self) -> &Entity {
        &self.entity
    }

    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    pub fn max_value(&self) -> i32 {
        self.max_value
    }

    pub fn new_value(&self) -> i32 {
        self.new_value
    }

    pub fn old_value(&self) -> i32 {
        self.old_value
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn reputation_type(&self) -> &str {
        &self.reputation_type
    }

    pub fn target(&self) -> Option<&Entity> {
        self.target.as_ref()
    }

    pub fn target_uuid(&self) -> Uuid {
        self.target_uuid
    }

    pub fn set_new_value(&mut self, value: i32) {
        self.patch.new_value = Some(value);
        self.new_value = value;
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<VillagerReputationChange> for Verdict {
    fn from(event: VillagerReputationChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: (event.patch != v1::VillagerReputationChangePatch::default()).then(|| {
                Box::new(v1::Patch {
                    change: Some(patch::Change::VillagerReputationChange(event.patch)),
                })
            }),
        }
    }
}

#[derive(Debug)]
pub struct WeatherChange {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    to_weather_state: bool,
    world: World,
}

impl WeatherChange {
    fn from_wire(message: v1::WeatherChange, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            to_weather_state: message.to_weather_state,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn to_weather_state(&self) -> bool {
        self.to_weather_state
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<WeatherChange> for Verdict {
    fn from(event: WeatherChange) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorldInit {
    id: u64,
    world: World,
}

impl WorldInit {
    fn from_wire(message: v1::WorldInit, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct WorldLoad {
    id: u64,
    world: World,
}

impl WorldLoad {
    fn from_wire(message: v1::WorldLoad, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug, Clone)]
pub struct WorldSave {
    id: u64,
    world: World,
}

impl WorldSave {
    fn from_wire(message: v1::WorldSave, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn world(&self) -> &World {
        &self.world
    }
}

#[derive(Debug)]
pub struct WorldUnload {
    id: u64,
    cancelled: bool,
    rolled_back: bool,
    world: World,
}

impl WorldUnload {
    fn from_wire(message: v1::WorldUnload, meta: Meta, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            id: meta.id,
            cancelled: meta.cancelled,
            rolled_back: meta.rolled_back,
            world: world(message.world, link),
        })
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub const fn is_rolled_back(&self) -> bool {
        self.rolled_back
    }
}

impl From<WorldUnload> for Verdict {
    fn from(event: WorldUnload) -> Self {
        Self {
            cancel: event.cancelled,
            patch: None,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Call {
    AreaEffectCloudApply {
        handler: u32,
        event: AreaEffectCloudApply,
    },
    ArrowBodyCountChange {
        handler: u32,
        event: ArrowBodyCountChange,
    },
    AsyncPlayerChat {
        handler: u32,
        event: AsyncPlayerChat,
    },
    AsyncPlayerPreLogin {
        handler: u32,
        event: AsyncPlayerPreLogin,
    },
    AsyncStructureGenerate {
        handler: u32,
        event: AsyncStructureGenerate,
    },
    AsyncStructureSpawn {
        handler: u32,
        event: AsyncStructureSpawn,
    },
    BatToggleSleep {
        handler: u32,
        event: BatToggleSleep,
    },
    BellResonate {
        handler: u32,
        event: BellResonate,
    },
    BellRing {
        handler: u32,
        event: BellRing,
    },
    BlockBreak {
        handler: u32,
        event: BlockBreak,
    },
    BlockBrush {
        handler: u32,
        event: BlockBrush,
    },
    BlockBurn {
        handler: u32,
        event: BlockBurn,
    },
    BlockCanBuild {
        handler: u32,
        event: BlockCanBuild,
    },
    BlockCook {
        handler: u32,
        event: BlockCook,
    },
    BlockDamage {
        handler: u32,
        event: BlockDamage,
    },
    BlockDamageAbort {
        handler: u32,
        event: BlockDamageAbort,
    },
    BlockDispense {
        handler: u32,
        event: BlockDispense,
    },
    BlockDispenseArmor {
        handler: u32,
        event: BlockDispenseArmor,
    },
    BlockDispenseLoot {
        handler: u32,
        event: BlockDispenseLoot,
    },
    BlockDropItem {
        handler: u32,
        event: BlockDropItem,
    },
    BlockExp {
        handler: u32,
        event: BlockExp,
    },
    BlockExplode {
        handler: u32,
        event: BlockExplode,
    },
    BlockFade {
        handler: u32,
        event: BlockFade,
    },
    BlockFertilize {
        handler: u32,
        event: BlockFertilize,
    },
    BlockForm {
        handler: u32,
        event: BlockForm,
    },
    BlockFromTo {
        handler: u32,
        event: BlockFromTo,
    },
    BlockGrow {
        handler: u32,
        event: BlockGrow,
    },
    BlockIgnite {
        handler: u32,
        event: BlockIgnite,
    },
    BlockMultiPlace {
        handler: u32,
        event: BlockMultiPlace,
    },
    BlockPhysics {
        handler: u32,
        event: BlockPhysics,
    },
    BlockPistonExtend {
        handler: u32,
        event: BlockPistonExtend,
    },
    BlockPistonRetract {
        handler: u32,
        event: BlockPistonRetract,
    },
    BlockPlace {
        handler: u32,
        event: BlockPlace,
    },
    BlockReceiveGame {
        handler: u32,
        event: BlockReceiveGame,
    },
    BlockRedstone {
        handler: u32,
        event: BlockRedstone,
    },
    BlockShearEntity {
        handler: u32,
        event: BlockShearEntity,
    },
    BlockSpread {
        handler: u32,
        event: BlockSpread,
    },
    Brew {
        handler: u32,
        event: Brew,
    },
    BrewingStandFuel {
        handler: u32,
        event: BrewingStandFuel,
    },
    BrewingStart {
        handler: u32,
        event: BrewingStart,
    },
    BroadcastMessage {
        handler: u32,
        event: BroadcastMessage,
    },
    CampfireStart {
        handler: u32,
        event: CampfireStart,
    },
    CauldronLevelChange {
        handler: u32,
        event: CauldronLevelChange,
    },
    ChunkLoad {
        handler: u32,
        event: ChunkLoad,
    },
    ChunkPopulate {
        handler: u32,
        event: ChunkPopulate,
    },
    ChunkUnload {
        handler: u32,
        event: ChunkUnload,
    },
    CraftItem {
        handler: u32,
        event: CraftItem,
    },
    CrafterCraft {
        handler: u32,
        event: CrafterCraft,
    },
    CreatureSpawn {
        handler: u32,
        event: CreatureSpawn,
    },
    CreeperPower {
        handler: u32,
        event: CreeperPower,
    },
    CubeMobSplit {
        handler: u32,
        event: CubeMobSplit,
    },
    EnchantItem {
        handler: u32,
        event: EnchantItem,
    },
    EnderDragonChangePhase {
        handler: u32,
        event: EnderDragonChangePhase,
    },
    EntitiesLoad {
        handler: u32,
        event: EntitiesLoad,
    },
    EntitiesUnload {
        handler: u32,
        event: EntitiesUnload,
    },
    EntityAirChange {
        handler: u32,
        event: EntityAirChange,
    },
    EntityBlockForm {
        handler: u32,
        event: EntityBlockForm,
    },
    EntityBreakDoor {
        handler: u32,
        event: EntityBreakDoor,
    },
    EntityBreed {
        handler: u32,
        event: EntityBreed,
    },
    EntityChangeBlock {
        handler: u32,
        event: EntityChangeBlock,
    },
    EntityCombust {
        handler: u32,
        event: EntityCombust,
    },
    EntityCombustByBlock {
        handler: u32,
        event: EntityCombustByBlock,
    },
    EntityCombustByEntity {
        handler: u32,
        event: EntityCombustByEntity,
    },
    EntityDamage {
        handler: u32,
        event: EntityDamage,
    },
    EntityDamageByBlock {
        handler: u32,
        event: EntityDamageByBlock,
    },
    EntityDamageByEntity {
        handler: u32,
        event: EntityDamageByEntity,
    },
    EntityDeath {
        handler: u32,
        event: EntityDeath,
    },
    EntityDismount {
        handler: u32,
        event: EntityDismount,
    },
    EntityDropItem {
        handler: u32,
        event: EntityDropItem,
    },
    EntityEnterBlock {
        handler: u32,
        event: EntityEnterBlock,
    },
    EntityEnterLoveMode {
        handler: u32,
        event: EntityEnterLoveMode,
    },
    EntityExhaustion {
        handler: u32,
        event: EntityExhaustion,
    },
    EntityExplode {
        handler: u32,
        event: EntityExplode,
    },
    EntityInteract {
        handler: u32,
        event: EntityInteract,
    },
    EntityKnockback {
        handler: u32,
        event: EntityKnockback,
    },
    EntityKnockbackByEntity {
        handler: u32,
        event: EntityKnockbackByEntity,
    },
    EntityMount {
        handler: u32,
        event: EntityMount,
    },
    EntityPickupItem {
        handler: u32,
        event: EntityPickupItem,
    },
    EntityPlace {
        handler: u32,
        event: EntityPlace,
    },
    EntityPortal {
        handler: u32,
        event: EntityPortal,
    },
    EntityPortalEnter {
        handler: u32,
        event: EntityPortalEnter,
    },
    EntityPortalExit {
        handler: u32,
        event: EntityPortalExit,
    },
    EntityPoseChange {
        handler: u32,
        event: EntityPoseChange,
    },
    EntityPotionEffect {
        handler: u32,
        event: EntityPotionEffect,
    },
    EntityRegainHealth {
        handler: u32,
        event: EntityRegainHealth,
    },
    EntityRemove {
        handler: u32,
        event: EntityRemove,
    },
    EntityResurrect {
        handler: u32,
        event: EntityResurrect,
    },
    EntityShootBow {
        handler: u32,
        event: EntityShootBow,
    },
    EntitySpawn {
        handler: u32,
        event: EntitySpawn,
    },
    EntitySpellCast {
        handler: u32,
        event: EntitySpellCast,
    },
    EntityTame {
        handler: u32,
        event: EntityTame,
    },
    EntityTarget {
        handler: u32,
        event: EntityTarget,
    },
    EntityTargetBlock {
        handler: u32,
        event: EntityTargetBlock,
    },
    EntityTargetLivingEntity {
        handler: u32,
        event: EntityTargetLivingEntity,
    },
    EntityTeleport {
        handler: u32,
        event: EntityTeleport,
    },
    EntityToggleGlide {
        handler: u32,
        event: EntityToggleGlide,
    },
    EntityToggleSwim {
        handler: u32,
        event: EntityToggleSwim,
    },
    EntityTransform {
        handler: u32,
        event: EntityTransform,
    },
    EntityUnleash {
        handler: u32,
        event: EntityUnleash,
    },
    ExpBottle {
        handler: u32,
        event: ExpBottle,
    },
    ExplosionPrime {
        handler: u32,
        event: ExplosionPrime,
    },
    FireworkExplode {
        handler: u32,
        event: FireworkExplode,
    },
    FluidLevelChange {
        handler: u32,
        event: FluidLevelChange,
    },
    FoodLevelChange {
        handler: u32,
        event: FoodLevelChange,
    },
    FurnaceBurn {
        handler: u32,
        event: FurnaceBurn,
    },
    FurnaceExtract {
        handler: u32,
        event: FurnaceExtract,
    },
    FurnaceSmelt {
        handler: u32,
        event: FurnaceSmelt,
    },
    FurnaceStartSmelt {
        handler: u32,
        event: FurnaceStartSmelt,
    },
    GenericGame {
        handler: u32,
        event: GenericGame,
    },
    HangingBreak {
        handler: u32,
        event: HangingBreak,
    },
    HangingBreakByEntity {
        handler: u32,
        event: HangingBreakByEntity,
    },
    HangingPlace {
        handler: u32,
        event: HangingPlace,
    },
    HopperInventorySearch {
        handler: u32,
        event: HopperInventorySearch,
    },
    HorseJump {
        handler: u32,
        event: HorseJump,
    },
    InventoryClick {
        handler: u32,
        event: InventoryClick,
    },
    InventoryClose {
        handler: u32,
        event: InventoryClose,
    },
    InventoryCreative {
        handler: u32,
        event: InventoryCreative,
    },
    InventoryDrag {
        handler: u32,
        event: InventoryDrag,
    },
    InventoryMoveItem {
        handler: u32,
        event: InventoryMoveItem,
    },
    InventoryOpen {
        handler: u32,
        event: InventoryOpen,
    },
    InventoryPickupItem {
        handler: u32,
        event: InventoryPickupItem,
    },
    ItemDespawn {
        handler: u32,
        event: ItemDespawn,
    },
    ItemMerge {
        handler: u32,
        event: ItemMerge,
    },
    ItemSpawn {
        handler: u32,
        event: ItemSpawn,
    },
    LeavesDecay {
        handler: u32,
        event: LeavesDecay,
    },
    LightningStrike {
        handler: u32,
        event: LightningStrike,
    },
    LingeringPotionSplash {
        handler: u32,
        event: LingeringPotionSplash,
    },
    LootGenerate {
        handler: u32,
        event: LootGenerate,
    },
    MapInitialize {
        handler: u32,
        event: MapInitialize,
    },
    MoistureChange {
        handler: u32,
        event: MoistureChange,
    },
    NotePlay {
        handler: u32,
        event: NotePlay,
    },
    PigZap {
        handler: u32,
        event: PigZap,
    },
    PigZombieAnger {
        handler: u32,
        event: PigZombieAnger,
    },
    PiglinBarter {
        handler: u32,
        event: PiglinBarter,
    },
    PlayerAdvancementCriterionGrant {
        handler: u32,
        event: PlayerAdvancementCriterionGrant,
    },
    PlayerAdvancementDone {
        handler: u32,
        event: PlayerAdvancementDone,
    },
    PlayerAnimation {
        handler: u32,
        event: PlayerAnimation,
    },
    PlayerArmorStandManipulate {
        handler: u32,
        event: PlayerArmorStandManipulate,
    },
    PlayerBedEnter {
        handler: u32,
        event: PlayerBedEnter,
    },
    PlayerBedLeave {
        handler: u32,
        event: PlayerBedLeave,
    },
    PlayerBucketEmpty {
        handler: u32,
        event: PlayerBucketEmpty,
    },
    PlayerBucketEntity {
        handler: u32,
        event: PlayerBucketEntity,
    },
    PlayerBucketFill {
        handler: u32,
        event: PlayerBucketFill,
    },
    PlayerChangedMainHand {
        handler: u32,
        event: PlayerChangedMainHand,
    },
    PlayerChangedWorld {
        handler: u32,
        event: PlayerChangedWorld,
    },
    PlayerCommandPreprocess {
        handler: u32,
        event: PlayerCommandPreprocess,
    },
    PlayerCommandSend {
        handler: u32,
        event: PlayerCommandSend,
    },
    PlayerCustomClick {
        handler: u32,
        event: PlayerCustomClick,
    },
    PlayerDeath {
        handler: u32,
        event: PlayerDeath,
    },
    PlayerDropItem {
        handler: u32,
        event: PlayerDropItem,
    },
    PlayerEditBook {
        handler: u32,
        event: PlayerEditBook,
    },
    PlayerEggThrow {
        handler: u32,
        event: PlayerEggThrow,
    },
    PlayerExpChange {
        handler: u32,
        event: PlayerExpChange,
    },
    PlayerExpCooldownChange {
        handler: u32,
        event: PlayerExpCooldownChange,
    },
    PlayerFish {
        handler: u32,
        event: PlayerFish,
    },
    PlayerGameModeChange {
        handler: u32,
        event: PlayerGameModeChange,
    },
    PlayerHarvestBlock {
        handler: u32,
        event: PlayerHarvestBlock,
    },
    PlayerHideEntity {
        handler: u32,
        event: PlayerHideEntity,
    },
    PlayerInput {
        handler: u32,
        event: PlayerInput,
    },
    PlayerInteract {
        handler: u32,
        event: PlayerInteract,
    },
    PlayerInteractAtEntity {
        handler: u32,
        event: PlayerInteractAtEntity,
    },
    PlayerInteractEntity {
        handler: u32,
        event: PlayerInteractEntity,
    },
    PlayerItemBreak {
        handler: u32,
        event: PlayerItemBreak,
    },
    PlayerItemConsume {
        handler: u32,
        event: PlayerItemConsume,
    },
    PlayerItemDamage {
        handler: u32,
        event: PlayerItemDamage,
    },
    PlayerItemHeld {
        handler: u32,
        event: PlayerItemHeld,
    },
    PlayerItemMend {
        handler: u32,
        event: PlayerItemMend,
    },
    PlayerJoin {
        handler: u32,
        event: PlayerJoin,
    },
    PlayerKick {
        handler: u32,
        event: PlayerKick,
    },
    PlayerLeashEntity {
        handler: u32,
        event: PlayerLeashEntity,
    },
    PlayerLeave {
        handler: u32,
        event: PlayerLeave,
    },
    PlayerLevelChange {
        handler: u32,
        event: PlayerLevelChange,
    },
    PlayerLinksSend {
        handler: u32,
        event: PlayerLinksSend,
    },
    PlayerLocaleChange {
        handler: u32,
        event: PlayerLocaleChange,
    },
    PlayerLogin {
        handler: u32,
        event: PlayerLogin,
    },
    PlayerMove {
        handler: u32,
        event: PlayerMove,
    },
    PlayerPickupArrow {
        handler: u32,
        event: PlayerPickupArrow,
    },
    PlayerPortal {
        handler: u32,
        event: PlayerPortal,
    },
    PlayerRecipeBookClick {
        handler: u32,
        event: PlayerRecipeBookClick,
    },
    PlayerRecipeBookSettingsChange {
        handler: u32,
        event: PlayerRecipeBookSettingsChange,
    },
    PlayerRecipeDiscover {
        handler: u32,
        event: PlayerRecipeDiscover,
    },
    PlayerResourcePackStatus {
        handler: u32,
        event: PlayerResourcePackStatus,
    },
    PlayerRespawn {
        handler: u32,
        event: PlayerRespawn,
    },
    PlayerRiptide {
        handler: u32,
        event: PlayerRiptide,
    },
    PlayerShearEntity {
        handler: u32,
        event: PlayerShearEntity,
    },
    PlayerShowEntity {
        handler: u32,
        event: PlayerShowEntity,
    },
    PlayerSignOpen {
        handler: u32,
        event: PlayerSignOpen,
    },
    PlayerSpawnChange {
        handler: u32,
        event: PlayerSpawnChange,
    },
    PlayerStatisticIncrement {
        handler: u32,
        event: PlayerStatisticIncrement,
    },
    PlayerSwapHandItems {
        handler: u32,
        event: PlayerSwapHandItems,
    },
    PlayerTakeLecternBook {
        handler: u32,
        event: PlayerTakeLecternBook,
    },
    PlayerTeleport {
        handler: u32,
        event: PlayerTeleport,
    },
    PlayerToggleFlight {
        handler: u32,
        event: PlayerToggleFlight,
    },
    PlayerToggleSneak {
        handler: u32,
        event: PlayerToggleSneak,
    },
    PlayerToggleSprint {
        handler: u32,
        event: PlayerToggleSprint,
    },
    PlayerUnleashEntity {
        handler: u32,
        event: PlayerUnleashEntity,
    },
    PlayerVelocity {
        handler: u32,
        event: PlayerVelocity,
    },
    PluginDisable {
        handler: u32,
        event: PluginDisable,
    },
    PluginEnable {
        handler: u32,
        event: PluginEnable,
    },
    PortalCreate {
        handler: u32,
        event: PortalCreate,
    },
    PotionSplash {
        handler: u32,
        event: PotionSplash,
    },
    PrepareAnvil {
        handler: u32,
        event: PrepareAnvil,
    },
    PrepareGrindstone {
        handler: u32,
        event: PrepareGrindstone,
    },
    PrepareItemCraft {
        handler: u32,
        event: PrepareItemCraft,
    },
    PrepareItemEnchant {
        handler: u32,
        event: PrepareItemEnchant,
    },
    PrepareSmithing {
        handler: u32,
        event: PrepareSmithing,
    },
    ProjectileHit {
        handler: u32,
        event: ProjectileHit,
    },
    ProjectileLaunch {
        handler: u32,
        event: ProjectileLaunch,
    },
    RaidFinish {
        handler: u32,
        event: RaidFinish,
    },
    RaidSpawnWave {
        handler: u32,
        event: RaidSpawnWave,
    },
    RaidStop {
        handler: u32,
        event: RaidStop,
    },
    RaidTrigger {
        handler: u32,
        event: RaidTrigger,
    },
    RemoteServerCommand {
        handler: u32,
        event: RemoteServerCommand,
    },
    SculkBloom {
        handler: u32,
        event: SculkBloom,
    },
    ServerCommand {
        handler: u32,
        event: ServerCommand,
    },
    ServerListPing {
        handler: u32,
        event: ServerListPing,
    },
    ServerLoad {
        handler: u32,
        event: ServerLoad,
    },
    SheepDyeWool {
        handler: u32,
        event: SheepDyeWool,
    },
    SheepRegrowWool {
        handler: u32,
        event: SheepRegrowWool,
    },
    SignChange {
        handler: u32,
        event: SignChange,
    },
    SmithItem {
        handler: u32,
        event: SmithItem,
    },
    SpawnChange {
        handler: u32,
        event: SpawnChange,
    },
    SpawnerSpawn {
        handler: u32,
        event: SpawnerSpawn,
    },
    SpongeAbsorb {
        handler: u32,
        event: SpongeAbsorb,
    },
    StriderTemperatureChange {
        handler: u32,
        event: StriderTemperatureChange,
    },
    StructureGrow {
        handler: u32,
        event: StructureGrow,
    },
    TNTPrime {
        handler: u32,
        event: TNTPrime,
    },
    TabComplete {
        handler: u32,
        event: TabComplete,
    },
    ThunderChange {
        handler: u32,
        event: ThunderChange,
    },
    TimeSkip {
        handler: u32,
        event: TimeSkip,
    },
    TradeSelect {
        handler: u32,
        event: TradeSelect,
    },
    TrialSpawnerSpawn {
        handler: u32,
        event: TrialSpawnerSpawn,
    },
    VaultDisplayItem {
        handler: u32,
        event: VaultDisplayItem,
    },
    VehicleBlockCollision {
        handler: u32,
        event: VehicleBlockCollision,
    },
    VehicleCreate {
        handler: u32,
        event: VehicleCreate,
    },
    VehicleDamage {
        handler: u32,
        event: VehicleDamage,
    },
    VehicleDestroy {
        handler: u32,
        event: VehicleDestroy,
    },
    VehicleEnter {
        handler: u32,
        event: VehicleEnter,
    },
    VehicleEntityCollision {
        handler: u32,
        event: VehicleEntityCollision,
    },
    VehicleExit {
        handler: u32,
        event: VehicleExit,
    },
    VehicleMove {
        handler: u32,
        event: VehicleMove,
    },
    VehicleUpdate {
        handler: u32,
        event: VehicleUpdate,
    },
    VillagerAcquireTrade {
        handler: u32,
        event: VillagerAcquireTrade,
    },
    VillagerCareerChange {
        handler: u32,
        event: VillagerCareerChange,
    },
    VillagerReplenishTrade {
        handler: u32,
        event: VillagerReplenishTrade,
    },
    VillagerReputationChange {
        handler: u32,
        event: VillagerReputationChange,
    },
    WeatherChange {
        handler: u32,
        event: WeatherChange,
    },
    WorldInit {
        handler: u32,
        event: WorldInit,
    },
    WorldLoad {
        handler: u32,
        event: WorldLoad,
    },
    WorldSave {
        handler: u32,
        event: WorldSave,
    },
    WorldUnload {
        handler: u32,
        event: WorldUnload,
    },
}

impl Received {
    pub(crate) const fn writes_when_cancelled(&self) -> bool {
        matches!(
            self.payload,
            event::Payload::CraftItem(_)
                | event::Payload::InventoryClick(_)
                | event::Payload::InventoryCreative(_)
                | event::Payload::SmithItem(_)
        )
    }

    pub(crate) fn call(
        &self,
        handler: u32,
        kind: EventKind,
        link: &Link,
    ) -> Option<Result<Call, Error>> {
        let meta = self.meta;
        Some(match (&self.payload, kind) {
            (event::Payload::AreaEffectCloudApply(message), EventKind::AreaEffectCloudApply) => {
                AreaEffectCloudApply::from_wire(message.clone(), meta, link)
                    .map(|event| Call::AreaEffectCloudApply { handler, event })
            }
            (event::Payload::ArrowBodyCountChange(message), EventKind::ArrowBodyCountChange) => {
                ArrowBodyCountChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ArrowBodyCountChange { handler, event })
            }
            (event::Payload::AsyncPlayerChat(message), EventKind::AsyncPlayerChat) => {
                AsyncPlayerChat::from_wire(message.clone(), meta, link)
                    .map(|event| Call::AsyncPlayerChat { handler, event })
            }
            (event::Payload::AsyncPlayerPreLogin(message), EventKind::AsyncPlayerPreLogin) => {
                AsyncPlayerPreLogin::from_wire(message.clone(), meta, link)
                    .map(|event| Call::AsyncPlayerPreLogin { handler, event })
            }
            (
                event::Payload::AsyncStructureGenerate(message),
                EventKind::AsyncStructureGenerate,
            ) => AsyncStructureGenerate::from_wire(message.clone(), meta, link)
                .map(|event| Call::AsyncStructureGenerate { handler, event }),
            (event::Payload::AsyncStructureSpawn(message), EventKind::AsyncStructureSpawn) => {
                AsyncStructureSpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::AsyncStructureSpawn { handler, event })
            }
            (event::Payload::BatToggleSleep(message), EventKind::BatToggleSleep) => {
                BatToggleSleep::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BatToggleSleep { handler, event })
            }
            (event::Payload::BellResonate(message), EventKind::BellResonate) => {
                BellResonate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BellResonate { handler, event })
            }
            (event::Payload::BellRing(message), EventKind::BellRing) => {
                BellRing::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BellRing { handler, event })
            }
            (event::Payload::BlockBreak(message), EventKind::BlockBreak) => {
                BlockBreak::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockBreak { handler, event })
            }
            (event::Payload::BlockBrush(message), EventKind::BlockBrush) => {
                BlockBrush::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockBrush { handler, event })
            }
            (event::Payload::BlockBurn(message), EventKind::BlockBurn) => {
                BlockBurn::from_wire(*message, meta, link)
                    .map(|event| Call::BlockBurn { handler, event })
            }
            (event::Payload::BlockCanBuild(message), EventKind::BlockCanBuild) => {
                BlockCanBuild::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockCanBuild { handler, event })
            }
            (event::Payload::BlockCook(message), EventKind::BlockCook) => {
                BlockCook::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockCook { handler, event })
            }
            (event::Payload::BlockDamage(message), EventKind::BlockDamage) => {
                BlockDamage::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDamage { handler, event })
            }
            (event::Payload::BlockDamageAbort(message), EventKind::BlockDamageAbort) => {
                BlockDamageAbort::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDamageAbort { handler, event })
            }
            (event::Payload::BlockDispense(message), EventKind::BlockDispense) => {
                BlockDispense::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDispense { handler, event })
            }
            (event::Payload::BlockDispenseArmor(message), EventKind::BlockDispenseArmor) => {
                BlockDispenseArmor::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDispenseArmor { handler, event })
            }
            (event::Payload::BlockDispenseLoot(message), EventKind::BlockDispenseLoot) => {
                BlockDispenseLoot::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDispenseLoot { handler, event })
            }
            (event::Payload::BlockDropItem(message), EventKind::BlockDropItem) => {
                BlockDropItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockDropItem { handler, event })
            }
            (event::Payload::BlockExp(message), EventKind::BlockExp) => {
                BlockExp::from_wire(*message, meta, link)
                    .map(|event| Call::BlockExp { handler, event })
            }
            (event::Payload::BlockExplode(message), EventKind::BlockExplode) => {
                BlockExplode::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockExplode { handler, event })
            }
            (event::Payload::BlockFade(message), EventKind::BlockFade) => {
                BlockFade::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockFade { handler, event })
            }
            (event::Payload::BlockFertilize(message), EventKind::BlockFertilize) => {
                BlockFertilize::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockFertilize { handler, event })
            }
            (event::Payload::BlockForm(message), EventKind::BlockForm) => {
                BlockForm::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockForm { handler, event })
            }
            (event::Payload::BlockFromTo(message), EventKind::BlockFromTo) => {
                BlockFromTo::from_wire(*message, meta, link)
                    .map(|event| Call::BlockFromTo { handler, event })
            }
            (event::Payload::BlockGrow(message), EventKind::BlockGrow) => {
                BlockGrow::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockGrow { handler, event })
            }
            (event::Payload::BlockIgnite(message), EventKind::BlockIgnite) => {
                BlockIgnite::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockIgnite { handler, event })
            }
            (event::Payload::BlockMultiPlace(message), EventKind::BlockMultiPlace) => {
                BlockMultiPlace::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockMultiPlace { handler, event })
            }
            (event::Payload::BlockPhysics(message), EventKind::BlockPhysics) => {
                BlockPhysics::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockPhysics { handler, event })
            }
            (event::Payload::BlockPistonExtend(message), EventKind::BlockPistonExtend) => {
                BlockPistonExtend::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockPistonExtend { handler, event })
            }
            (event::Payload::BlockPistonRetract(message), EventKind::BlockPistonRetract) => {
                BlockPistonRetract::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockPistonRetract { handler, event })
            }
            (event::Payload::BlockPlace(message), EventKind::BlockPlace) => {
                BlockPlace::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockPlace { handler, event })
            }
            (event::Payload::BlockReceiveGame(message), EventKind::BlockReceiveGame) => {
                BlockReceiveGame::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockReceiveGame { handler, event })
            }
            (event::Payload::BlockRedstone(message), EventKind::BlockRedstone) => {
                BlockRedstone::from_wire(*message, meta, link)
                    .map(|event| Call::BlockRedstone { handler, event })
            }
            (event::Payload::BlockShearEntity(message), EventKind::BlockShearEntity) => {
                BlockShearEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockShearEntity { handler, event })
            }
            (event::Payload::BlockSpread(message), EventKind::BlockSpread) => {
                BlockSpread::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BlockSpread { handler, event })
            }
            (event::Payload::Brew(message), EventKind::Brew) => {
                Brew::from_wire(message.clone(), meta, link)
                    .map(|event| Call::Brew { handler, event })
            }
            (event::Payload::BrewingStandFuel(message), EventKind::BrewingStandFuel) => {
                BrewingStandFuel::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BrewingStandFuel { handler, event })
            }
            (event::Payload::BrewingStart(message), EventKind::BrewingStart) => {
                BrewingStart::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BrewingStart { handler, event })
            }
            (event::Payload::BroadcastMessage(message), EventKind::BroadcastMessage) => {
                BroadcastMessage::from_wire(message.clone(), meta, link)
                    .map(|event| Call::BroadcastMessage { handler, event })
            }
            (event::Payload::CampfireStart(message), EventKind::CampfireStart) => {
                CampfireStart::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CampfireStart { handler, event })
            }
            (event::Payload::CauldronLevelChange(message), EventKind::CauldronLevelChange) => {
                CauldronLevelChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CauldronLevelChange { handler, event })
            }
            (event::Payload::ChunkLoad(message), EventKind::ChunkLoad) => {
                ChunkLoad::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ChunkLoad { handler, event })
            }
            (event::Payload::ChunkPopulate(message), EventKind::ChunkPopulate) => {
                ChunkPopulate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ChunkPopulate { handler, event })
            }
            (event::Payload::ChunkUnload(message), EventKind::ChunkUnload) => {
                ChunkUnload::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ChunkUnload { handler, event })
            }
            (event::Payload::CraftItem(message), EventKind::CraftItem) => {
                CraftItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CraftItem { handler, event })
            }
            (event::Payload::CrafterCraft(message), EventKind::CrafterCraft) => {
                CrafterCraft::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CrafterCraft { handler, event })
            }
            (event::Payload::CreatureSpawn(message), EventKind::CreatureSpawn) => {
                CreatureSpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CreatureSpawn { handler, event })
            }
            (event::Payload::CreeperPower(message), EventKind::CreeperPower) => {
                CreeperPower::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CreeperPower { handler, event })
            }
            (event::Payload::CubeMobSplit(message), EventKind::CubeMobSplit) => {
                CubeMobSplit::from_wire(message.clone(), meta, link)
                    .map(|event| Call::CubeMobSplit { handler, event })
            }
            (event::Payload::EnchantItem(message), EventKind::EnchantItem) => {
                EnchantItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EnchantItem { handler, event })
            }
            (
                event::Payload::EnderDragonChangePhase(message),
                EventKind::EnderDragonChangePhase,
            ) => EnderDragonChangePhase::from_wire(message.clone(), meta, link)
                .map(|event| Call::EnderDragonChangePhase { handler, event }),
            (event::Payload::EntitiesLoad(message), EventKind::EntitiesLoad) => {
                EntitiesLoad::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntitiesLoad { handler, event })
            }
            (event::Payload::EntitiesUnload(message), EventKind::EntitiesUnload) => {
                EntitiesUnload::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntitiesUnload { handler, event })
            }
            (event::Payload::EntityAirChange(message), EventKind::EntityAirChange) => {
                EntityAirChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityAirChange { handler, event })
            }
            (event::Payload::EntityBlockForm(message), EventKind::EntityBlockForm) => {
                EntityBlockForm::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityBlockForm { handler, event })
            }
            (event::Payload::EntityBreakDoor(message), EventKind::EntityBreakDoor) => {
                EntityBreakDoor::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityBreakDoor { handler, event })
            }
            (event::Payload::EntityBreed(message), EventKind::EntityBreed) => {
                EntityBreed::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityBreed { handler, event })
            }
            (event::Payload::EntityChangeBlock(message), EventKind::EntityChangeBlock) => {
                EntityChangeBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityChangeBlock { handler, event })
            }
            (event::Payload::EntityCombust(message), EventKind::EntityCombust) => {
                EntityCombust::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityCombust { handler, event })
            }
            (event::Payload::EntityCombustByBlock(message), EventKind::EntityCombustByBlock) => {
                EntityCombustByBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityCombustByBlock { handler, event })
            }
            (event::Payload::EntityCombustByEntity(message), EventKind::EntityCombustByEntity) => {
                EntityCombustByEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityCombustByEntity { handler, event })
            }
            (event::Payload::EntityDamage(message), EventKind::EntityDamage) => {
                EntityDamage::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDamage { handler, event })
            }
            (event::Payload::EntityDamageByBlock(message), EventKind::EntityDamageByBlock) => {
                EntityDamageByBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDamageByBlock { handler, event })
            }
            (event::Payload::EntityDamageByEntity(message), EventKind::EntityDamageByEntity) => {
                EntityDamageByEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDamageByEntity { handler, event })
            }
            (event::Payload::EntityDeath(message), EventKind::EntityDeath) => {
                EntityDeath::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDeath { handler, event })
            }
            (event::Payload::EntityDismount(message), EventKind::EntityDismount) => {
                EntityDismount::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDismount { handler, event })
            }
            (event::Payload::EntityDropItem(message), EventKind::EntityDropItem) => {
                EntityDropItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityDropItem { handler, event })
            }
            (event::Payload::EntityEnterBlock(message), EventKind::EntityEnterBlock) => {
                EntityEnterBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityEnterBlock { handler, event })
            }
            (event::Payload::EntityEnterLoveMode(message), EventKind::EntityEnterLoveMode) => {
                EntityEnterLoveMode::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityEnterLoveMode { handler, event })
            }
            (event::Payload::EntityExhaustion(message), EventKind::EntityExhaustion) => {
                EntityExhaustion::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityExhaustion { handler, event })
            }
            (event::Payload::EntityExplode(message), EventKind::EntityExplode) => {
                EntityExplode::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityExplode { handler, event })
            }
            (event::Payload::EntityInteract(message), EventKind::EntityInteract) => {
                EntityInteract::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityInteract { handler, event })
            }
            (event::Payload::EntityKnockback(message), EventKind::EntityKnockback) => {
                EntityKnockback::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityKnockback { handler, event })
            }
            (
                event::Payload::EntityKnockbackByEntity(message),
                EventKind::EntityKnockbackByEntity,
            ) => EntityKnockbackByEntity::from_wire(message.clone(), meta, link)
                .map(|event| Call::EntityKnockbackByEntity { handler, event }),
            (event::Payload::EntityMount(message), EventKind::EntityMount) => {
                EntityMount::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityMount { handler, event })
            }
            (event::Payload::EntityPickupItem(message), EventKind::EntityPickupItem) => {
                EntityPickupItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPickupItem { handler, event })
            }
            (event::Payload::EntityPlace(message), EventKind::EntityPlace) => {
                EntityPlace::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPlace { handler, event })
            }
            (event::Payload::EntityPortal(message), EventKind::EntityPortal) => {
                EntityPortal::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPortal { handler, event })
            }
            (event::Payload::EntityPortalEnter(message), EventKind::EntityPortalEnter) => {
                EntityPortalEnter::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPortalEnter { handler, event })
            }
            (event::Payload::EntityPortalExit(message), EventKind::EntityPortalExit) => {
                EntityPortalExit::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPortalExit { handler, event })
            }
            (event::Payload::EntityPoseChange(message), EventKind::EntityPoseChange) => {
                EntityPoseChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPoseChange { handler, event })
            }
            (event::Payload::EntityPotionEffect(message), EventKind::EntityPotionEffect) => {
                EntityPotionEffect::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityPotionEffect { handler, event })
            }
            (event::Payload::EntityRegainHealth(message), EventKind::EntityRegainHealth) => {
                EntityRegainHealth::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityRegainHealth { handler, event })
            }
            (event::Payload::EntityRemove(message), EventKind::EntityRemove) => {
                EntityRemove::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityRemove { handler, event })
            }
            (event::Payload::EntityResurrect(message), EventKind::EntityResurrect) => {
                EntityResurrect::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityResurrect { handler, event })
            }
            (event::Payload::EntityShootBow(message), EventKind::EntityShootBow) => {
                EntityShootBow::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityShootBow { handler, event })
            }
            (event::Payload::EntitySpawn(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntitySpawn { handler, event })
            }
            (event::Payload::EntitySpellCast(message), EventKind::EntitySpellCast) => {
                EntitySpellCast::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntitySpellCast { handler, event })
            }
            (event::Payload::EntityTame(message), EventKind::EntityTame) => {
                EntityTame::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityTame { handler, event })
            }
            (event::Payload::EntityTarget(message), EventKind::EntityTarget) => {
                EntityTarget::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityTarget { handler, event })
            }
            (event::Payload::EntityTargetBlock(message), EventKind::EntityTargetBlock) => {
                EntityTargetBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityTargetBlock { handler, event })
            }
            (
                event::Payload::EntityTargetLivingEntity(message),
                EventKind::EntityTargetLivingEntity,
            ) => EntityTargetLivingEntity::from_wire(message.clone(), meta, link)
                .map(|event| Call::EntityTargetLivingEntity { handler, event }),
            (event::Payload::EntityTeleport(message), EventKind::EntityTeleport) => {
                EntityTeleport::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityTeleport { handler, event })
            }
            (event::Payload::EntityToggleGlide(message), EventKind::EntityToggleGlide) => {
                EntityToggleGlide::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityToggleGlide { handler, event })
            }
            (event::Payload::EntityToggleSwim(message), EventKind::EntityToggleSwim) => {
                EntityToggleSwim::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityToggleSwim { handler, event })
            }
            (event::Payload::EntityTransform(message), EventKind::EntityTransform) => {
                EntityTransform::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityTransform { handler, event })
            }
            (event::Payload::EntityUnleash(message), EventKind::EntityUnleash) => {
                EntityUnleash::from_wire(message.clone(), meta, link)
                    .map(|event| Call::EntityUnleash { handler, event })
            }
            (event::Payload::ExpBottle(message), EventKind::ExpBottle) => {
                ExpBottle::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ExpBottle { handler, event })
            }
            (event::Payload::ExplosionPrime(message), EventKind::ExplosionPrime) => {
                ExplosionPrime::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ExplosionPrime { handler, event })
            }
            (event::Payload::FireworkExplode(message), EventKind::FireworkExplode) => {
                FireworkExplode::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FireworkExplode { handler, event })
            }
            (event::Payload::FluidLevelChange(message), EventKind::FluidLevelChange) => {
                FluidLevelChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FluidLevelChange { handler, event })
            }
            (event::Payload::FoodLevelChange(message), EventKind::FoodLevelChange) => {
                FoodLevelChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FoodLevelChange { handler, event })
            }
            (event::Payload::FurnaceBurn(message), EventKind::FurnaceBurn) => {
                FurnaceBurn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FurnaceBurn { handler, event })
            }
            (event::Payload::FurnaceExtract(message), EventKind::FurnaceExtract) => {
                FurnaceExtract::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FurnaceExtract { handler, event })
            }
            (event::Payload::FurnaceSmelt(message), EventKind::FurnaceSmelt) => {
                FurnaceSmelt::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FurnaceSmelt { handler, event })
            }
            (event::Payload::FurnaceStartSmelt(message), EventKind::FurnaceStartSmelt) => {
                FurnaceStartSmelt::from_wire(message.clone(), meta, link)
                    .map(|event| Call::FurnaceStartSmelt { handler, event })
            }
            (event::Payload::GenericGame(message), EventKind::GenericGame) => {
                GenericGame::from_wire(message.clone(), meta, link)
                    .map(|event| Call::GenericGame { handler, event })
            }
            (event::Payload::HangingBreak(message), EventKind::HangingBreak) => {
                HangingBreak::from_wire(message.clone(), meta, link)
                    .map(|event| Call::HangingBreak { handler, event })
            }
            (event::Payload::HangingBreakByEntity(message), EventKind::HangingBreakByEntity) => {
                HangingBreakByEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::HangingBreakByEntity { handler, event })
            }
            (event::Payload::HangingPlace(message), EventKind::HangingPlace) => {
                HangingPlace::from_wire(message.clone(), meta, link)
                    .map(|event| Call::HangingPlace { handler, event })
            }
            (event::Payload::HopperInventorySearch(message), EventKind::HopperInventorySearch) => {
                HopperInventorySearch::from_wire(message.clone(), meta, link)
                    .map(|event| Call::HopperInventorySearch { handler, event })
            }
            (event::Payload::HorseJump(message), EventKind::HorseJump) => {
                HorseJump::from_wire(message.clone(), meta, link)
                    .map(|event| Call::HorseJump { handler, event })
            }
            (event::Payload::InventoryClick(message), EventKind::InventoryClick) => {
                InventoryClick::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryClick { handler, event })
            }
            (event::Payload::InventoryClose(message), EventKind::InventoryClose) => {
                InventoryClose::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryClose { handler, event })
            }
            (event::Payload::InventoryCreative(message), EventKind::InventoryCreative) => {
                InventoryCreative::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryCreative { handler, event })
            }
            (event::Payload::InventoryDrag(message), EventKind::InventoryDrag) => {
                InventoryDrag::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryDrag { handler, event })
            }
            (event::Payload::InventoryMoveItem(message), EventKind::InventoryMoveItem) => {
                InventoryMoveItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryMoveItem { handler, event })
            }
            (event::Payload::InventoryOpen(message), EventKind::InventoryOpen) => {
                InventoryOpen::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryOpen { handler, event })
            }
            (event::Payload::InventoryPickupItem(message), EventKind::InventoryPickupItem) => {
                InventoryPickupItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::InventoryPickupItem { handler, event })
            }
            (event::Payload::ItemDespawn(message), EventKind::ItemDespawn) => {
                ItemDespawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ItemDespawn { handler, event })
            }
            (event::Payload::ItemMerge(message), EventKind::ItemMerge) => {
                ItemMerge::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ItemMerge { handler, event })
            }
            (event::Payload::ItemSpawn(message), EventKind::ItemSpawn) => {
                ItemSpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ItemSpawn { handler, event })
            }
            (event::Payload::LeavesDecay(message), EventKind::LeavesDecay) => {
                LeavesDecay::from_wire(*message, meta, link)
                    .map(|event| Call::LeavesDecay { handler, event })
            }
            (event::Payload::LightningStrike(message), EventKind::LightningStrike) => {
                LightningStrike::from_wire(message.clone(), meta, link)
                    .map(|event| Call::LightningStrike { handler, event })
            }
            (event::Payload::LingeringPotionSplash(message), EventKind::LingeringPotionSplash) => {
                LingeringPotionSplash::from_wire(message.clone(), meta, link)
                    .map(|event| Call::LingeringPotionSplash { handler, event })
            }
            (event::Payload::LootGenerate(message), EventKind::LootGenerate) => {
                LootGenerate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::LootGenerate { handler, event })
            }
            (event::Payload::MapInitialize(message), EventKind::MapInitialize) => {
                MapInitialize::from_wire(*message, meta, link)
                    .map(|event| Call::MapInitialize { handler, event })
            }
            (event::Payload::MoistureChange(message), EventKind::MoistureChange) => {
                MoistureChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::MoistureChange { handler, event })
            }
            (event::Payload::NotePlay(message), EventKind::NotePlay) => {
                NotePlay::from_wire(*message, meta, link)
                    .map(|event| Call::NotePlay { handler, event })
            }
            (event::Payload::PigZap(message), EventKind::PigZap) => {
                PigZap::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PigZap { handler, event })
            }
            (event::Payload::PigZombieAnger(message), EventKind::PigZombieAnger) => {
                PigZombieAnger::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PigZombieAnger { handler, event })
            }
            (event::Payload::PiglinBarter(message), EventKind::PiglinBarter) => {
                PiglinBarter::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PiglinBarter { handler, event })
            }
            (
                event::Payload::PlayerAdvancementCriterionGrant(message),
                EventKind::PlayerAdvancementCriterionGrant,
            ) => PlayerAdvancementCriterionGrant::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerAdvancementCriterionGrant { handler, event }),
            (event::Payload::PlayerAdvancementDone(message), EventKind::PlayerAdvancementDone) => {
                PlayerAdvancementDone::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerAdvancementDone { handler, event })
            }
            (event::Payload::PlayerAnimation(message), EventKind::PlayerAnimation) => {
                PlayerAnimation::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerAnimation { handler, event })
            }
            (
                event::Payload::PlayerArmorStandManipulate(message),
                EventKind::PlayerArmorStandManipulate,
            ) => PlayerArmorStandManipulate::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerArmorStandManipulate { handler, event }),
            (event::Payload::PlayerBedEnter(message), EventKind::PlayerBedEnter) => {
                PlayerBedEnter::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerBedEnter { handler, event })
            }
            (event::Payload::PlayerBedLeave(message), EventKind::PlayerBedLeave) => {
                PlayerBedLeave::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerBedLeave { handler, event })
            }
            (event::Payload::PlayerBucketEmpty(message), EventKind::PlayerBucketEmpty) => {
                PlayerBucketEmpty::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerBucketEmpty { handler, event })
            }
            (event::Payload::PlayerBucketEntity(message), EventKind::PlayerBucketEntity) => {
                PlayerBucketEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerBucketEntity { handler, event })
            }
            (event::Payload::PlayerBucketFill(message), EventKind::PlayerBucketFill) => {
                PlayerBucketFill::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerBucketFill { handler, event })
            }
            (event::Payload::PlayerChangedMainHand(message), EventKind::PlayerChangedMainHand) => {
                PlayerChangedMainHand::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerChangedMainHand { handler, event })
            }
            (event::Payload::PlayerChangedWorld(message), EventKind::PlayerChangedWorld) => {
                PlayerChangedWorld::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerChangedWorld { handler, event })
            }
            (
                event::Payload::PlayerCommandPreprocess(message),
                EventKind::PlayerCommandPreprocess,
            ) => PlayerCommandPreprocess::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerCommandPreprocess { handler, event }),
            (event::Payload::PlayerCommandSend(message), EventKind::PlayerCommandSend) => {
                PlayerCommandSend::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerCommandSend { handler, event })
            }
            (event::Payload::PlayerCustomClick(message), EventKind::PlayerCustomClick) => {
                PlayerCustomClick::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerCustomClick { handler, event })
            }
            (event::Payload::PlayerDeath(message), EventKind::PlayerDeath) => {
                PlayerDeath::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerDeath { handler, event })
            }
            (event::Payload::PlayerDropItem(message), EventKind::PlayerDropItem) => {
                PlayerDropItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerDropItem { handler, event })
            }
            (event::Payload::PlayerEditBook(message), EventKind::PlayerEditBook) => {
                PlayerEditBook::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerEditBook { handler, event })
            }
            (event::Payload::PlayerEggThrow(message), EventKind::PlayerEggThrow) => {
                PlayerEggThrow::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerEggThrow { handler, event })
            }
            (event::Payload::PlayerExpChange(message), EventKind::PlayerExpChange) => {
                PlayerExpChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerExpChange { handler, event })
            }
            (
                event::Payload::PlayerExpCooldownChange(message),
                EventKind::PlayerExpCooldownChange,
            ) => PlayerExpCooldownChange::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerExpCooldownChange { handler, event }),
            (event::Payload::PlayerFish(message), EventKind::PlayerFish) => {
                PlayerFish::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerFish { handler, event })
            }
            (event::Payload::PlayerGameModeChange(message), EventKind::PlayerGameModeChange) => {
                PlayerGameModeChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerGameModeChange { handler, event })
            }
            (event::Payload::PlayerHarvestBlock(message), EventKind::PlayerHarvestBlock) => {
                PlayerHarvestBlock::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerHarvestBlock { handler, event })
            }
            (event::Payload::PlayerHideEntity(message), EventKind::PlayerHideEntity) => {
                PlayerHideEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerHideEntity { handler, event })
            }
            (event::Payload::PlayerInput(message), EventKind::PlayerInput) => {
                PlayerInput::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerInput { handler, event })
            }
            (event::Payload::PlayerInteract(message), EventKind::PlayerInteract) => {
                PlayerInteract::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerInteract { handler, event })
            }
            (
                event::Payload::PlayerInteractAtEntity(message),
                EventKind::PlayerInteractAtEntity,
            ) => PlayerInteractAtEntity::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerInteractAtEntity { handler, event }),
            (event::Payload::PlayerInteractEntity(message), EventKind::PlayerInteractEntity) => {
                PlayerInteractEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerInteractEntity { handler, event })
            }
            (event::Payload::PlayerItemBreak(message), EventKind::PlayerItemBreak) => {
                PlayerItemBreak::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerItemBreak { handler, event })
            }
            (event::Payload::PlayerItemConsume(message), EventKind::PlayerItemConsume) => {
                PlayerItemConsume::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerItemConsume { handler, event })
            }
            (event::Payload::PlayerItemDamage(message), EventKind::PlayerItemDamage) => {
                PlayerItemDamage::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerItemDamage { handler, event })
            }
            (event::Payload::PlayerItemHeld(message), EventKind::PlayerItemHeld) => {
                PlayerItemHeld::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerItemHeld { handler, event })
            }
            (event::Payload::PlayerItemMend(message), EventKind::PlayerItemMend) => {
                PlayerItemMend::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerItemMend { handler, event })
            }
            (event::Payload::PlayerJoin(message), EventKind::PlayerJoin) => {
                PlayerJoin::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerJoin { handler, event })
            }
            (event::Payload::PlayerKick(message), EventKind::PlayerKick) => {
                PlayerKick::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerKick { handler, event })
            }
            (event::Payload::PlayerLeashEntity(message), EventKind::PlayerLeashEntity) => {
                PlayerLeashEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLeashEntity { handler, event })
            }
            (event::Payload::PlayerLeave(message), EventKind::PlayerLeave) => {
                PlayerLeave::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLeave { handler, event })
            }
            (event::Payload::PlayerLevelChange(message), EventKind::PlayerLevelChange) => {
                PlayerLevelChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLevelChange { handler, event })
            }
            (event::Payload::PlayerLinksSend(message), EventKind::PlayerLinksSend) => {
                PlayerLinksSend::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLinksSend { handler, event })
            }
            (event::Payload::PlayerLocaleChange(message), EventKind::PlayerLocaleChange) => {
                PlayerLocaleChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLocaleChange { handler, event })
            }
            (event::Payload::PlayerLogin(message), EventKind::PlayerLogin) => {
                PlayerLogin::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerLogin { handler, event })
            }
            (event::Payload::PlayerMove(message), EventKind::PlayerMove) => {
                PlayerMove::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerMove { handler, event })
            }
            (event::Payload::PlayerPickupArrow(message), EventKind::PlayerPickupArrow) => {
                PlayerPickupArrow::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerPickupArrow { handler, event })
            }
            (event::Payload::PlayerPortal(message), EventKind::PlayerPortal) => {
                PlayerPortal::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerPortal { handler, event })
            }
            (event::Payload::PlayerRecipeBookClick(message), EventKind::PlayerRecipeBookClick) => {
                PlayerRecipeBookClick::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerRecipeBookClick { handler, event })
            }
            (
                event::Payload::PlayerRecipeBookSettingsChange(message),
                EventKind::PlayerRecipeBookSettingsChange,
            ) => PlayerRecipeBookSettingsChange::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerRecipeBookSettingsChange { handler, event }),
            (event::Payload::PlayerRecipeDiscover(message), EventKind::PlayerRecipeDiscover) => {
                PlayerRecipeDiscover::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerRecipeDiscover { handler, event })
            }
            (
                event::Payload::PlayerResourcePackStatus(message),
                EventKind::PlayerResourcePackStatus,
            ) => PlayerResourcePackStatus::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerResourcePackStatus { handler, event }),
            (event::Payload::PlayerRespawn(message), EventKind::PlayerRespawn) => {
                PlayerRespawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerRespawn { handler, event })
            }
            (event::Payload::PlayerRiptide(message), EventKind::PlayerRiptide) => {
                PlayerRiptide::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerRiptide { handler, event })
            }
            (event::Payload::PlayerShearEntity(message), EventKind::PlayerShearEntity) => {
                PlayerShearEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerShearEntity { handler, event })
            }
            (event::Payload::PlayerShowEntity(message), EventKind::PlayerShowEntity) => {
                PlayerShowEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerShowEntity { handler, event })
            }
            (event::Payload::PlayerSignOpen(message), EventKind::PlayerSignOpen) => {
                PlayerSignOpen::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerSignOpen { handler, event })
            }
            (event::Payload::PlayerSpawnChange(message), EventKind::PlayerSpawnChange) => {
                PlayerSpawnChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerSpawnChange { handler, event })
            }
            (
                event::Payload::PlayerStatisticIncrement(message),
                EventKind::PlayerStatisticIncrement,
            ) => PlayerStatisticIncrement::from_wire(message.clone(), meta, link)
                .map(|event| Call::PlayerStatisticIncrement { handler, event }),
            (event::Payload::PlayerSwapHandItems(message), EventKind::PlayerSwapHandItems) => {
                PlayerSwapHandItems::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerSwapHandItems { handler, event })
            }
            (event::Payload::PlayerTakeLecternBook(message), EventKind::PlayerTakeLecternBook) => {
                PlayerTakeLecternBook::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerTakeLecternBook { handler, event })
            }
            (event::Payload::PlayerTeleport(message), EventKind::PlayerTeleport) => {
                PlayerTeleport::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerTeleport { handler, event })
            }
            (event::Payload::PlayerToggleFlight(message), EventKind::PlayerToggleFlight) => {
                PlayerToggleFlight::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerToggleFlight { handler, event })
            }
            (event::Payload::PlayerToggleSneak(message), EventKind::PlayerToggleSneak) => {
                PlayerToggleSneak::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerToggleSneak { handler, event })
            }
            (event::Payload::PlayerToggleSprint(message), EventKind::PlayerToggleSprint) => {
                PlayerToggleSprint::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerToggleSprint { handler, event })
            }
            (event::Payload::PlayerUnleashEntity(message), EventKind::PlayerUnleashEntity) => {
                PlayerUnleashEntity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerUnleashEntity { handler, event })
            }
            (event::Payload::PlayerVelocity(message), EventKind::PlayerVelocity) => {
                PlayerVelocity::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PlayerVelocity { handler, event })
            }
            (event::Payload::PluginDisable(message), EventKind::PluginDisable) => {
                PluginDisable::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PluginDisable { handler, event })
            }
            (event::Payload::PluginEnable(message), EventKind::PluginEnable) => {
                PluginEnable::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PluginEnable { handler, event })
            }
            (event::Payload::PortalCreate(message), EventKind::PortalCreate) => {
                PortalCreate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PortalCreate { handler, event })
            }
            (event::Payload::PotionSplash(message), EventKind::PotionSplash) => {
                PotionSplash::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PotionSplash { handler, event })
            }
            (event::Payload::PrepareAnvil(message), EventKind::PrepareAnvil) => {
                PrepareAnvil::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PrepareAnvil { handler, event })
            }
            (event::Payload::PrepareGrindstone(message), EventKind::PrepareGrindstone) => {
                PrepareGrindstone::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PrepareGrindstone { handler, event })
            }
            (event::Payload::PrepareItemCraft(message), EventKind::PrepareItemCraft) => {
                PrepareItemCraft::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PrepareItemCraft { handler, event })
            }
            (event::Payload::PrepareItemEnchant(message), EventKind::PrepareItemEnchant) => {
                PrepareItemEnchant::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PrepareItemEnchant { handler, event })
            }
            (event::Payload::PrepareSmithing(message), EventKind::PrepareSmithing) => {
                PrepareSmithing::from_wire(message.clone(), meta, link)
                    .map(|event| Call::PrepareSmithing { handler, event })
            }
            (event::Payload::ProjectileHit(message), EventKind::ProjectileHit) => {
                ProjectileHit::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ProjectileHit { handler, event })
            }
            (event::Payload::ProjectileLaunch(message), EventKind::ProjectileLaunch) => {
                ProjectileLaunch::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ProjectileLaunch { handler, event })
            }
            (event::Payload::RaidFinish(message), EventKind::RaidFinish) => {
                RaidFinish::from_wire(message.clone(), meta, link)
                    .map(|event| Call::RaidFinish { handler, event })
            }
            (event::Payload::RaidSpawnWave(message), EventKind::RaidSpawnWave) => {
                RaidSpawnWave::from_wire(message.clone(), meta, link)
                    .map(|event| Call::RaidSpawnWave { handler, event })
            }
            (event::Payload::RaidStop(message), EventKind::RaidStop) => {
                RaidStop::from_wire(message.clone(), meta, link)
                    .map(|event| Call::RaidStop { handler, event })
            }
            (event::Payload::RaidTrigger(message), EventKind::RaidTrigger) => {
                RaidTrigger::from_wire(message.clone(), meta, link)
                    .map(|event| Call::RaidTrigger { handler, event })
            }
            (event::Payload::RemoteServerCommand(message), EventKind::RemoteServerCommand) => {
                RemoteServerCommand::from_wire(message.clone(), meta, link)
                    .map(|event| Call::RemoteServerCommand { handler, event })
            }
            (event::Payload::SculkBloom(message), EventKind::SculkBloom) => {
                SculkBloom::from_wire(*message, meta, link)
                    .map(|event| Call::SculkBloom { handler, event })
            }
            (event::Payload::ServerCommand(message), EventKind::ServerCommand) => {
                ServerCommand::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ServerCommand { handler, event })
            }
            (event::Payload::ServerListPing(message), EventKind::ServerListPing) => {
                ServerListPing::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ServerListPing { handler, event })
            }
            (event::Payload::ServerLoad(message), EventKind::ServerLoad) => {
                ServerLoad::from_wire(*message, meta, link)
                    .map(|event| Call::ServerLoad { handler, event })
            }
            (event::Payload::SheepDyeWool(message), EventKind::SheepDyeWool) => {
                SheepDyeWool::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SheepDyeWool { handler, event })
            }
            (event::Payload::SheepRegrowWool(message), EventKind::SheepRegrowWool) => {
                SheepRegrowWool::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SheepRegrowWool { handler, event })
            }
            (event::Payload::SignChange(message), EventKind::SignChange) => {
                SignChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SignChange { handler, event })
            }
            (event::Payload::SmithItem(message), EventKind::SmithItem) => {
                SmithItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SmithItem { handler, event })
            }
            (event::Payload::SpawnChange(message), EventKind::SpawnChange) => {
                SpawnChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SpawnChange { handler, event })
            }
            (event::Payload::SpawnerSpawn(message), EventKind::SpawnerSpawn) => {
                SpawnerSpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SpawnerSpawn { handler, event })
            }
            (event::Payload::SpongeAbsorb(message), EventKind::SpongeAbsorb) => {
                SpongeAbsorb::from_wire(message.clone(), meta, link)
                    .map(|event| Call::SpongeAbsorb { handler, event })
            }
            (
                event::Payload::StriderTemperatureChange(message),
                EventKind::StriderTemperatureChange,
            ) => StriderTemperatureChange::from_wire(message.clone(), meta, link)
                .map(|event| Call::StriderTemperatureChange { handler, event }),
            (event::Payload::StructureGrow(message), EventKind::StructureGrow) => {
                StructureGrow::from_wire(message.clone(), meta, link)
                    .map(|event| Call::StructureGrow { handler, event })
            }
            (event::Payload::TntPrime(message), EventKind::TNTPrime) => {
                TNTPrime::from_wire(message.clone(), meta, link)
                    .map(|event| Call::TNTPrime { handler, event })
            }
            (event::Payload::TabComplete(message), EventKind::TabComplete) => {
                TabComplete::from_wire(message.clone(), meta, link)
                    .map(|event| Call::TabComplete { handler, event })
            }
            (event::Payload::ThunderChange(message), EventKind::ThunderChange) => {
                ThunderChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::ThunderChange { handler, event })
            }
            (event::Payload::TimeSkip(message), EventKind::TimeSkip) => {
                TimeSkip::from_wire(message.clone(), meta, link)
                    .map(|event| Call::TimeSkip { handler, event })
            }
            (event::Payload::TradeSelect(message), EventKind::TradeSelect) => {
                TradeSelect::from_wire(message.clone(), meta, link)
                    .map(|event| Call::TradeSelect { handler, event })
            }
            (event::Payload::TrialSpawnerSpawn(message), EventKind::TrialSpawnerSpawn) => {
                TrialSpawnerSpawn::from_wire(message.clone(), meta, link)
                    .map(|event| Call::TrialSpawnerSpawn { handler, event })
            }
            (event::Payload::VaultDisplayItem(message), EventKind::VaultDisplayItem) => {
                VaultDisplayItem::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VaultDisplayItem { handler, event })
            }
            (event::Payload::VehicleBlockCollision(message), EventKind::VehicleBlockCollision) => {
                VehicleBlockCollision::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleBlockCollision { handler, event })
            }
            (event::Payload::VehicleCreate(message), EventKind::VehicleCreate) => {
                VehicleCreate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleCreate { handler, event })
            }
            (event::Payload::VehicleDamage(message), EventKind::VehicleDamage) => {
                VehicleDamage::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleDamage { handler, event })
            }
            (event::Payload::VehicleDestroy(message), EventKind::VehicleDestroy) => {
                VehicleDestroy::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleDestroy { handler, event })
            }
            (event::Payload::VehicleEnter(message), EventKind::VehicleEnter) => {
                VehicleEnter::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleEnter { handler, event })
            }
            (
                event::Payload::VehicleEntityCollision(message),
                EventKind::VehicleEntityCollision,
            ) => VehicleEntityCollision::from_wire(message.clone(), meta, link)
                .map(|event| Call::VehicleEntityCollision { handler, event }),
            (event::Payload::VehicleExit(message), EventKind::VehicleExit) => {
                VehicleExit::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleExit { handler, event })
            }
            (event::Payload::VehicleMove(message), EventKind::VehicleMove) => {
                VehicleMove::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleMove { handler, event })
            }
            (event::Payload::VehicleUpdate(message), EventKind::VehicleUpdate) => {
                VehicleUpdate::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VehicleUpdate { handler, event })
            }
            (event::Payload::VillagerAcquireTrade(message), EventKind::VillagerAcquireTrade) => {
                VillagerAcquireTrade::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VillagerAcquireTrade { handler, event })
            }
            (event::Payload::VillagerCareerChange(message), EventKind::VillagerCareerChange) => {
                VillagerCareerChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::VillagerCareerChange { handler, event })
            }
            (
                event::Payload::VillagerReplenishTrade(message),
                EventKind::VillagerReplenishTrade,
            ) => VillagerReplenishTrade::from_wire(message.clone(), meta, link)
                .map(|event| Call::VillagerReplenishTrade { handler, event }),
            (
                event::Payload::VillagerReputationChange(message),
                EventKind::VillagerReputationChange,
            ) => VillagerReputationChange::from_wire(message.clone(), meta, link)
                .map(|event| Call::VillagerReputationChange { handler, event }),
            (event::Payload::WeatherChange(message), EventKind::WeatherChange) => {
                WeatherChange::from_wire(message.clone(), meta, link)
                    .map(|event| Call::WeatherChange { handler, event })
            }
            (event::Payload::WorldInit(message), EventKind::WorldInit) => {
                WorldInit::from_wire(message.clone(), meta, link)
                    .map(|event| Call::WorldInit { handler, event })
            }
            (event::Payload::WorldLoad(message), EventKind::WorldLoad) => {
                WorldLoad::from_wire(message.clone(), meta, link)
                    .map(|event| Call::WorldLoad { handler, event })
            }
            (event::Payload::WorldSave(message), EventKind::WorldSave) => {
                WorldSave::from_wire(message.clone(), meta, link)
                    .map(|event| Call::WorldSave { handler, event })
            }
            (event::Payload::WorldUnload(message), EventKind::WorldUnload) => {
                WorldUnload::from_wire(message.clone(), meta, link)
                    .map(|event| Call::WorldUnload { handler, event })
            }
            (event::Payload::BlockBreak(message), EventKind::BlockExp) => {
                BlockExp::from_wire(project_block_break_block_exp(message), meta, link)
                    .map(|event| Call::BlockExp { handler, event })
            }
            (event::Payload::BlockDispenseArmor(message), EventKind::BlockDispense) => {
                BlockDispense::from_wire(
                    project_block_dispense_armor_block_dispense(message),
                    meta,
                    link,
                )
                .map(|event| Call::BlockDispense { handler, event })
            }
            (event::Payload::BlockMultiPlace(message), EventKind::BlockPlace) => {
                BlockPlace::from_wire(project_block_multi_place_block_place(message), meta, link)
                    .map(|event| Call::BlockPlace { handler, event })
            }
            (event::Payload::CraftItem(message), EventKind::InventoryClick) => {
                InventoryClick::from_wire(project_craft_item_inventory_click(message), meta, link)
                    .map(|event| Call::InventoryClick { handler, event })
            }
            (event::Payload::CreatureSpawn(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(project_creature_spawn_entity_spawn(message), meta, link)
                    .map(|event| Call::EntitySpawn { handler, event })
            }
            (event::Payload::EntityBlockForm(message), EventKind::BlockForm) => {
                BlockForm::from_wire(project_entity_block_form_block_form(message), meta, link)
                    .map(|event| Call::BlockForm { handler, event })
            }
            (event::Payload::EntityBreakDoor(message), EventKind::EntityChangeBlock) => {
                EntityChangeBlock::from_wire(
                    project_entity_break_door_entity_change_block(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityChangeBlock { handler, event })
            }
            (event::Payload::EntityCombustByBlock(message), EventKind::EntityCombust) => {
                EntityCombust::from_wire(
                    project_entity_combust_by_block_entity_combust(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityCombust { handler, event })
            }
            (event::Payload::EntityCombustByEntity(message), EventKind::EntityCombust) => {
                EntityCombust::from_wire(
                    project_entity_combust_by_entity_entity_combust(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityCombust { handler, event })
            }
            (event::Payload::EntityDamageByBlock(message), EventKind::EntityDamage) => {
                EntityDamage::from_wire(
                    project_entity_damage_by_block_entity_damage(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityDamage { handler, event })
            }
            (event::Payload::EntityDamageByEntity(message), EventKind::EntityDamage) => {
                EntityDamage::from_wire(
                    project_entity_damage_by_entity_entity_damage(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityDamage { handler, event })
            }
            (event::Payload::EntityKnockbackByEntity(message), EventKind::EntityKnockback) => {
                EntityKnockback::from_wire(
                    project_entity_knockback_by_entity_entity_knockback(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityKnockback { handler, event })
            }
            (event::Payload::EntityTargetLivingEntity(message), EventKind::EntityTarget) => {
                EntityTarget::from_wire(
                    project_entity_target_living_entity_entity_target(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityTarget { handler, event })
            }
            (event::Payload::FurnaceExtract(message), EventKind::BlockExp) => {
                BlockExp::from_wire(project_furnace_extract_block_exp(message), meta, link)
                    .map(|event| Call::BlockExp { handler, event })
            }
            (event::Payload::FurnaceSmelt(message), EventKind::BlockCook) => {
                BlockCook::from_wire(project_furnace_smelt_block_cook(message), meta, link)
                    .map(|event| Call::BlockCook { handler, event })
            }
            (event::Payload::HangingBreakByEntity(message), EventKind::HangingBreak) => {
                HangingBreak::from_wire(
                    project_hanging_break_by_entity_hanging_break(message),
                    meta,
                    link,
                )
                .map(|event| Call::HangingBreak { handler, event })
            }
            (event::Payload::InventoryCreative(message), EventKind::InventoryClick) => {
                InventoryClick::from_wire(
                    project_inventory_creative_inventory_click(message),
                    meta,
                    link,
                )
                .map(|event| Call::InventoryClick { handler, event })
            }
            (event::Payload::ItemSpawn(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(project_item_spawn_entity_spawn(message), meta, link)
                    .map(|event| Call::EntitySpawn { handler, event })
            }
            (event::Payload::PlayerDeath(message), EventKind::EntityDeath) => {
                EntityDeath::from_wire(project_player_death_entity_death(message), meta, link)
                    .map(|event| Call::EntityDeath { handler, event })
            }
            (event::Payload::PlayerInteractAtEntity(message), EventKind::PlayerInteractEntity) => {
                PlayerInteractEntity::from_wire(
                    project_player_interact_at_entity_player_interact_entity(message),
                    meta,
                    link,
                )
                .map(|event| Call::PlayerInteractEntity { handler, event })
            }
            (event::Payload::PlayerUnleashEntity(message), EventKind::EntityUnleash) => {
                EntityUnleash::from_wire(
                    project_player_unleash_entity_entity_unleash(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntityUnleash { handler, event })
            }
            (event::Payload::ProjectileLaunch(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(project_projectile_launch_entity_spawn(message), meta, link)
                    .map(|event| Call::EntitySpawn { handler, event })
            }
            (event::Payload::SmithItem(message), EventKind::InventoryClick) => {
                InventoryClick::from_wire(project_smith_item_inventory_click(message), meta, link)
                    .map(|event| Call::InventoryClick { handler, event })
            }
            (event::Payload::SpawnerSpawn(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(project_spawner_spawn_entity_spawn(message), meta, link)
                    .map(|event| Call::EntitySpawn { handler, event })
            }
            (event::Payload::TrialSpawnerSpawn(message), EventKind::EntitySpawn) => {
                EntitySpawn::from_wire(
                    project_trial_spawner_spawn_entity_spawn(message),
                    meta,
                    link,
                )
                .map(|event| Call::EntitySpawn { handler, event })
            }
            _ => return None,
        })
    }
}

fn project_block_break_block_exp(message: &v1::BlockBreak) -> v1::BlockExp {
    v1::BlockExp {
        block: message.block,
        exp_to_drop: message.exp_to_drop,
    }
}

fn project_block_dispense_armor_block_dispense(
    message: &v1::BlockDispenseArmor,
) -> v1::BlockDispense {
    v1::BlockDispense {
        block: message.block,
        item: message.item.clone(),
        velocity: message.velocity,
    }
}

fn project_block_multi_place_block_place(message: &v1::BlockMultiPlace) -> v1::BlockPlace {
    v1::BlockPlace {
        block: message.block,
        block_against: message.block_against,
        block_placed: message.block_placed,
        block_replaced_state: message.block_replaced_state.clone(),
        build: message.build,
        hand: message.hand,
        item_in_hand: message.item_in_hand.clone(),
        player: message.player.clone(),
        placed_state: message.placed_state.clone(),
    }
}

fn project_craft_item_inventory_click(message: &v1::CraftItem) -> v1::InventoryClick {
    v1::InventoryClick {
        action: message.action,
        click: message.click,
        clicked_inventory: message.clicked_inventory.clone(),
        current_item: message.current_item.clone(),
        cursor: message.cursor.clone(),
        hotbar_button: message.hotbar_button,
        inventory: message.inventory.clone(),
        left_click: message.left_click,
        raw_slot: message.raw_slot,
        result: message.result,
        right_click: message.right_click,
        shift_click: message.shift_click,
        slot: message.slot,
        slot_type: message.slot_type,
        view: message.view.clone(),
        viewers: message.viewers.clone(),
        who_clicked: message.who_clicked.clone(),
    }
}

fn project_creature_spawn_entity_spawn(message: &v1::CreatureSpawn) -> v1::EntitySpawn {
    v1::EntitySpawn {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        location: message.location.clone(),
    }
}

fn project_entity_block_form_block_form(message: &v1::EntityBlockForm) -> v1::BlockForm {
    v1::BlockForm {
        block: message.block,
        new_state: message.new_state.clone(),
    }
}

fn project_entity_break_door_entity_change_block(
    message: &v1::EntityBreakDoor,
) -> v1::EntityChangeBlock {
    v1::EntityChangeBlock {
        block: message.block,
        block_data: message.block_data.clone(),
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        to: message.to.clone(),
    }
}

fn project_entity_combust_by_block_entity_combust(
    message: &v1::EntityCombustByBlock,
) -> v1::EntityCombust {
    v1::EntityCombust {
        duration: message.duration,
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
    }
}

fn project_entity_combust_by_entity_entity_combust(
    message: &v1::EntityCombustByEntity,
) -> v1::EntityCombust {
    v1::EntityCombust {
        duration: message.duration,
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
    }
}

fn project_entity_damage_by_block_entity_damage(
    message: &v1::EntityDamageByBlock,
) -> v1::EntityDamage {
    v1::EntityDamage {
        cause: message.cause,
        damage: message.damage,
        damage_source: message.damage_source.clone(),
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        final_damage: message.final_damage,
    }
}

fn project_entity_damage_by_entity_entity_damage(
    message: &v1::EntityDamageByEntity,
) -> v1::EntityDamage {
    v1::EntityDamage {
        cause: message.cause,
        damage: message.damage,
        damage_source: message.damage_source.clone(),
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        final_damage: message.final_damage,
    }
}

fn project_entity_knockback_by_entity_entity_knockback(
    message: &v1::EntityKnockbackByEntity,
) -> v1::EntityKnockback {
    v1::EntityKnockback {
        cause: message.cause,
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        final_knockback: message.final_knockback,
        force: message.force,
        knockback: message.knockback,
    }
}

fn project_entity_target_living_entity_entity_target(
    message: &v1::EntityTargetLivingEntity,
) -> v1::EntityTarget {
    v1::EntityTarget {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        reason: message.reason,
        target: message.target.clone(),
    }
}

fn project_furnace_extract_block_exp(message: &v1::FurnaceExtract) -> v1::BlockExp {
    v1::BlockExp {
        block: message.block,
        exp_to_drop: message.exp_to_drop,
    }
}

fn project_furnace_smelt_block_cook(message: &v1::FurnaceSmelt) -> v1::BlockCook {
    v1::BlockCook {
        block: message.block,
        result: message.result.clone(),
        source: message.source.clone(),
    }
}

fn project_hanging_break_by_entity_hanging_break(
    message: &v1::HangingBreakByEntity,
) -> v1::HangingBreak {
    v1::HangingBreak {
        cause: message.cause,
        entity: message.entity.clone(),
    }
}

fn project_inventory_creative_inventory_click(
    message: &v1::InventoryCreative,
) -> v1::InventoryClick {
    v1::InventoryClick {
        action: message.action,
        click: message.click,
        clicked_inventory: message.clicked_inventory.clone(),
        current_item: message.current_item.clone(),
        cursor: message.cursor.clone(),
        hotbar_button: message.hotbar_button,
        inventory: message.inventory.clone(),
        left_click: message.left_click,
        raw_slot: message.raw_slot,
        result: message.result,
        right_click: message.right_click,
        shift_click: message.shift_click,
        slot: message.slot,
        slot_type: message.slot_type,
        view: message.view.clone(),
        viewers: message.viewers.clone(),
        who_clicked: message.who_clicked.clone(),
    }
}

fn project_item_spawn_entity_spawn(message: &v1::ItemSpawn) -> v1::EntitySpawn {
    v1::EntitySpawn {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        location: message.location.clone(),
    }
}

fn project_player_death_entity_death(message: &v1::PlayerDeath) -> v1::EntityDeath {
    v1::EntityDeath {
        damage_source: message.damage_source.clone(),
        dropped_exp: message.dropped_exp,
        drops: message.drops.clone(),
        entity: message.entity.as_ref().map(player_entity),
        entity_type: message.entity_type.clone(),
    }
}

fn project_player_interact_at_entity_player_interact_entity(
    message: &v1::PlayerInteractAtEntity,
) -> v1::PlayerInteractEntity {
    v1::PlayerInteractEntity {
        hand: message.hand,
        player: message.player.clone(),
        right_clicked: message.right_clicked.clone(),
    }
}

fn project_player_unleash_entity_entity_unleash(
    message: &v1::PlayerUnleashEntity,
) -> v1::EntityUnleash {
    v1::EntityUnleash {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        reason: message.reason,
    }
}

fn project_projectile_launch_entity_spawn(message: &v1::ProjectileLaunch) -> v1::EntitySpawn {
    v1::EntitySpawn {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        location: message.location.clone(),
    }
}

fn project_smith_item_inventory_click(message: &v1::SmithItem) -> v1::InventoryClick {
    v1::InventoryClick {
        action: message.action,
        click: message.click,
        clicked_inventory: message.clicked_inventory.clone(),
        current_item: message.current_item.clone(),
        cursor: message.cursor.clone(),
        hotbar_button: message.hotbar_button,
        inventory: message.inventory.clone(),
        left_click: message.left_click,
        raw_slot: message.raw_slot,
        result: message.result,
        right_click: message.right_click,
        shift_click: message.shift_click,
        slot: message.slot,
        slot_type: message.slot_type,
        view: message.view.clone(),
        viewers: message.viewers.clone(),
        who_clicked: message.who_clicked.clone(),
    }
}

fn project_spawner_spawn_entity_spawn(message: &v1::SpawnerSpawn) -> v1::EntitySpawn {
    v1::EntitySpawn {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        location: message.location.clone(),
    }
}

fn project_trial_spawner_spawn_entity_spawn(message: &v1::TrialSpawnerSpawn) -> v1::EntitySpawn {
    v1::EntitySpawn {
        entity: message.entity.clone(),
        entity_type: message.entity_type.clone(),
        location: message.location.clone(),
    }
}

impl Action {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Action::try_from(value) {
            Ok(v1::Action::LeftClickBlock) => Ok(Self::LeftClickBlock),
            Ok(v1::Action::RightClickBlock) => Ok(Self::RightClickBlock),
            Ok(v1::Action::LeftClickAir) => Ok(Self::LeftClickAir),
            Ok(v1::Action::RightClickAir) => Ok(Self::RightClickAir),
            Ok(v1::Action::Physical) => Ok(Self::Physical),
            Ok(v1::Action::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown action {value}")))
            }
        }
    }
}

impl AsyncPlayerPreLoginResult {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::AsyncPlayerPreLoginResult::try_from(value) {
            Ok(v1::AsyncPlayerPreLoginResult::Allowed) => Ok(Self::Allowed),
            Ok(v1::AsyncPlayerPreLoginResult::KickFull) => Ok(Self::KickFull),
            Ok(v1::AsyncPlayerPreLoginResult::KickBanned) => Ok(Self::KickBanned),
            Ok(v1::AsyncPlayerPreLoginResult::KickWhitelist) => Ok(Self::KickWhitelist),
            Ok(v1::AsyncPlayerPreLoginResult::KickOther) => Ok(Self::KickOther),
            Ok(v1::AsyncPlayerPreLoginResult::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown async player pre login result {value}"),
            )),
        }
    }

    pub(crate) const fn to_wire(self) -> i32 {
        (match self {
            Self::Allowed => v1::AsyncPlayerPreLoginResult::Allowed,
            Self::KickFull => v1::AsyncPlayerPreLoginResult::KickFull,
            Self::KickBanned => v1::AsyncPlayerPreLoginResult::KickBanned,
            Self::KickWhitelist => v1::AsyncPlayerPreLoginResult::KickWhitelist,
            Self::KickOther => v1::AsyncPlayerPreLoginResult::KickOther,
        }) as i32
    }
}

impl AsyncStructureGenerateCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::AsyncStructureGenerateCause::try_from(value) {
            Ok(v1::AsyncStructureGenerateCause::Command) => Ok(Self::Command),
            Ok(v1::AsyncStructureGenerateCause::WorldGeneration) => Ok(Self::WorldGeneration),
            Ok(v1::AsyncStructureGenerateCause::Custom) => Ok(Self::Custom),
            Ok(v1::AsyncStructureGenerateCause::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown async structure generate cause {value}"),
            )),
        }
    }
}

impl BlockIgniteCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::BlockIgniteCause::try_from(value) {
            Ok(v1::BlockIgniteCause::Lava) => Ok(Self::Lava),
            Ok(v1::BlockIgniteCause::FlintAndSteel) => Ok(Self::FlintAndSteel),
            Ok(v1::BlockIgniteCause::Spread) => Ok(Self::Spread),
            Ok(v1::BlockIgniteCause::Lightning) => Ok(Self::Lightning),
            Ok(v1::BlockIgniteCause::Fireball) => Ok(Self::Fireball),
            Ok(v1::BlockIgniteCause::EnderCrystal) => Ok(Self::EnderCrystal),
            Ok(v1::BlockIgniteCause::Explosion) => Ok(Self::Explosion),
            Ok(v1::BlockIgniteCause::Arrow) => Ok(Self::Arrow),
            Ok(v1::BlockIgniteCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown block ignite cause {value}"
            ))),
        }
    }
}

impl CauldronLevelChangeReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::CauldronLevelChangeReason::try_from(value) {
            Ok(v1::CauldronLevelChangeReason::BucketFill) => Ok(Self::BucketFill),
            Ok(v1::CauldronLevelChangeReason::BucketEmpty) => Ok(Self::BucketEmpty),
            Ok(v1::CauldronLevelChangeReason::BottleFill) => Ok(Self::BottleFill),
            Ok(v1::CauldronLevelChangeReason::BottleEmpty) => Ok(Self::BottleEmpty),
            Ok(v1::CauldronLevelChangeReason::BannerWash) => Ok(Self::BannerWash),
            Ok(v1::CauldronLevelChangeReason::ArmorWash) => Ok(Self::ArmorWash),
            Ok(v1::CauldronLevelChangeReason::ShulkerWash) => Ok(Self::ShulkerWash),
            Ok(v1::CauldronLevelChangeReason::Extinguish) => Ok(Self::Extinguish),
            Ok(v1::CauldronLevelChangeReason::Evaporate) => Ok(Self::Evaporate),
            Ok(v1::CauldronLevelChangeReason::NaturalFill) => Ok(Self::NaturalFill),
            Ok(v1::CauldronLevelChangeReason::Unknown) => Ok(Self::Unknown),
            Ok(v1::CauldronLevelChangeReason::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown cauldron level change reason {value}"),
            )),
        }
    }
}

impl ClickKind {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::ClickKind::try_from(value) {
            Ok(v1::ClickKind::Left) => Ok(Self::Left),
            Ok(v1::ClickKind::Right) => Ok(Self::Right),
            Ok(v1::ClickKind::ShiftLeft) => Ok(Self::ShiftLeft),
            Ok(v1::ClickKind::ShiftRight) => Ok(Self::ShiftRight),
            Ok(v1::ClickKind::NumberKey) => Ok(Self::NumberKey),
            Ok(v1::ClickKind::SwapOffhand) => Ok(Self::SwapOffhand),
            Ok(v1::ClickKind::Drop) => Ok(Self::Drop),
            Ok(v1::ClickKind::ControlDrop) => Ok(Self::ControlDrop),
            Ok(v1::ClickKind::DoubleClick) => Ok(Self::DoubleClick),
            Ok(v1::ClickKind::Middle) => Ok(Self::Middle),
            Ok(v1::ClickKind::WindowBorderLeft) => Ok(Self::WindowBorderLeft),
            Ok(v1::ClickKind::WindowBorderRight) => Ok(Self::WindowBorderRight),
            Ok(v1::ClickKind::Creative) => Ok(Self::Creative),
            Ok(v1::ClickKind::Unknown) => Ok(Self::Unknown),
            Ok(v1::ClickKind::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown click kind {value}")))
            }
        }
    }
}

impl CreatureSpawnReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::CreatureSpawnReason::try_from(value) {
            Ok(v1::CreatureSpawnReason::Natural) => Ok(Self::Natural),
            Ok(v1::CreatureSpawnReason::Jockey) => Ok(Self::Jockey),
            Ok(v1::CreatureSpawnReason::ChunkGen) => Ok(Self::ChunkGen),
            Ok(v1::CreatureSpawnReason::Spawner) => Ok(Self::Spawner),
            Ok(v1::CreatureSpawnReason::TrialSpawner) => Ok(Self::TrialSpawner),
            Ok(v1::CreatureSpawnReason::Egg) => Ok(Self::Egg),
            Ok(v1::CreatureSpawnReason::SpawnerEgg) => Ok(Self::SpawnerEgg),
            Ok(v1::CreatureSpawnReason::Bucket) => Ok(Self::Bucket),
            Ok(v1::CreatureSpawnReason::Lightning) => Ok(Self::Lightning),
            Ok(v1::CreatureSpawnReason::BuildSnowman) => Ok(Self::BuildSnowman),
            Ok(v1::CreatureSpawnReason::BuildIrongolem) => Ok(Self::BuildIrongolem),
            Ok(v1::CreatureSpawnReason::BuildCoppergolem) => Ok(Self::BuildCoppergolem),
            Ok(v1::CreatureSpawnReason::BuildWither) => Ok(Self::BuildWither),
            Ok(v1::CreatureSpawnReason::VillageDefense) => Ok(Self::VillageDefense),
            Ok(v1::CreatureSpawnReason::VillageInvasion) => Ok(Self::VillageInvasion),
            Ok(v1::CreatureSpawnReason::Breeding) => Ok(Self::Breeding),
            Ok(v1::CreatureSpawnReason::SlimeSplit) => Ok(Self::SlimeSplit),
            Ok(v1::CreatureSpawnReason::Reinforcements) => Ok(Self::Reinforcements),
            Ok(v1::CreatureSpawnReason::NetherPortal) => Ok(Self::NetherPortal),
            Ok(v1::CreatureSpawnReason::DispenseEgg) => Ok(Self::DispenseEgg),
            Ok(v1::CreatureSpawnReason::Infection) => Ok(Self::Infection),
            Ok(v1::CreatureSpawnReason::Cured) => Ok(Self::Cured),
            Ok(v1::CreatureSpawnReason::OcelotBaby) => Ok(Self::OcelotBaby),
            Ok(v1::CreatureSpawnReason::SilverfishBlock) => Ok(Self::SilverfishBlock),
            Ok(v1::CreatureSpawnReason::Mount) => Ok(Self::Mount),
            Ok(v1::CreatureSpawnReason::Trap) => Ok(Self::Trap),
            Ok(v1::CreatureSpawnReason::EnderPearl) => Ok(Self::EnderPearl),
            Ok(v1::CreatureSpawnReason::ShoulderEntity) => Ok(Self::ShoulderEntity),
            Ok(v1::CreatureSpawnReason::Drowned) => Ok(Self::Drowned),
            Ok(v1::CreatureSpawnReason::Sheared) => Ok(Self::Sheared),
            Ok(v1::CreatureSpawnReason::Explosion) => Ok(Self::Explosion),
            Ok(v1::CreatureSpawnReason::Raid) => Ok(Self::Raid),
            Ok(v1::CreatureSpawnReason::Patrol) => Ok(Self::Patrol),
            Ok(v1::CreatureSpawnReason::Beehive) => Ok(Self::Beehive),
            Ok(v1::CreatureSpawnReason::PiglinZombified) => Ok(Self::PiglinZombified),
            Ok(v1::CreatureSpawnReason::Spell) => Ok(Self::Spell),
            Ok(v1::CreatureSpawnReason::Frozen) => Ok(Self::Frozen),
            Ok(v1::CreatureSpawnReason::Metamorphosis) => Ok(Self::Metamorphosis),
            Ok(v1::CreatureSpawnReason::Duplication) => Ok(Self::Duplication),
            Ok(v1::CreatureSpawnReason::Command) => Ok(Self::Command),
            Ok(v1::CreatureSpawnReason::Enchantment) => Ok(Self::Enchantment),
            Ok(v1::CreatureSpawnReason::PotionEffect) => Ok(Self::PotionEffect),
            Ok(v1::CreatureSpawnReason::Reanimate) => Ok(Self::Reanimate),
            Ok(v1::CreatureSpawnReason::Custom) => Ok(Self::Custom),
            Ok(v1::CreatureSpawnReason::Default) => Ok(Self::Default),
            Ok(v1::CreatureSpawnReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown creature spawn reason {value}"
            ))),
        }
    }
}

impl CreeperPowerCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::CreeperPowerCause::try_from(value) {
            Ok(v1::CreeperPowerCause::Lightning) => Ok(Self::Lightning),
            Ok(v1::CreeperPowerCause::SetOn) => Ok(Self::SetOn),
            Ok(v1::CreeperPowerCause::SetOff) => Ok(Self::SetOff),
            Ok(v1::CreeperPowerCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown creeper power cause {value}"
            ))),
        }
    }
}

impl DragType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::DragType::try_from(value) {
            Ok(v1::DragType::Single) => Ok(Self::Single),
            Ok(v1::DragType::Even) => Ok(Self::Even),
            Ok(v1::DragType::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown drag type {value}")))
            }
        }
    }
}

impl DyeColor {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::DyeColor::try_from(value) {
            Ok(v1::DyeColor::White) => Ok(Self::White),
            Ok(v1::DyeColor::Orange) => Ok(Self::Orange),
            Ok(v1::DyeColor::Magenta) => Ok(Self::Magenta),
            Ok(v1::DyeColor::LightBlue) => Ok(Self::LightBlue),
            Ok(v1::DyeColor::Yellow) => Ok(Self::Yellow),
            Ok(v1::DyeColor::Lime) => Ok(Self::Lime),
            Ok(v1::DyeColor::Pink) => Ok(Self::Pink),
            Ok(v1::DyeColor::Gray) => Ok(Self::Gray),
            Ok(v1::DyeColor::LightGray) => Ok(Self::LightGray),
            Ok(v1::DyeColor::Cyan) => Ok(Self::Cyan),
            Ok(v1::DyeColor::Purple) => Ok(Self::Purple),
            Ok(v1::DyeColor::Blue) => Ok(Self::Blue),
            Ok(v1::DyeColor::Brown) => Ok(Self::Brown),
            Ok(v1::DyeColor::Green) => Ok(Self::Green),
            Ok(v1::DyeColor::Red) => Ok(Self::Red),
            Ok(v1::DyeColor::Black) => Ok(Self::Black),
            Ok(v1::DyeColor::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown dye color {value}")))
            }
        }
    }

    pub(crate) const fn to_wire(self) -> i32 {
        (match self {
            Self::White => v1::DyeColor::White,
            Self::Orange => v1::DyeColor::Orange,
            Self::Magenta => v1::DyeColor::Magenta,
            Self::LightBlue => v1::DyeColor::LightBlue,
            Self::Yellow => v1::DyeColor::Yellow,
            Self::Lime => v1::DyeColor::Lime,
            Self::Pink => v1::DyeColor::Pink,
            Self::Gray => v1::DyeColor::Gray,
            Self::LightGray => v1::DyeColor::LightGray,
            Self::Cyan => v1::DyeColor::Cyan,
            Self::Purple => v1::DyeColor::Purple,
            Self::Blue => v1::DyeColor::Blue,
            Self::Brown => v1::DyeColor::Brown,
            Self::Green => v1::DyeColor::Green,
            Self::Red => v1::DyeColor::Red,
            Self::Black => v1::DyeColor::Black,
        }) as i32
    }
}

impl EnderDragonPhase {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EnderDragonPhase::try_from(value) {
            Ok(v1::EnderDragonPhase::Circling) => Ok(Self::Circling),
            Ok(v1::EnderDragonPhase::Strafing) => Ok(Self::Strafing),
            Ok(v1::EnderDragonPhase::FlyToPortal) => Ok(Self::FlyToPortal),
            Ok(v1::EnderDragonPhase::LandOnPortal) => Ok(Self::LandOnPortal),
            Ok(v1::EnderDragonPhase::LeavePortal) => Ok(Self::LeavePortal),
            Ok(v1::EnderDragonPhase::BreathAttack) => Ok(Self::BreathAttack),
            Ok(v1::EnderDragonPhase::SearchForBreathAttackTarget) => {
                Ok(Self::SearchForBreathAttackTarget)
            }
            Ok(v1::EnderDragonPhase::RoarBeforeAttack) => Ok(Self::RoarBeforeAttack),
            Ok(v1::EnderDragonPhase::ChargePlayer) => Ok(Self::ChargePlayer),
            Ok(v1::EnderDragonPhase::Dying) => Ok(Self::Dying),
            Ok(v1::EnderDragonPhase::Hover) => Ok(Self::Hover),
            Ok(v1::EnderDragonPhase::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown ender dragon phase {value}"
            ))),
        }
    }

    pub(crate) const fn to_wire(self) -> i32 {
        (match self {
            Self::Circling => v1::EnderDragonPhase::Circling,
            Self::Strafing => v1::EnderDragonPhase::Strafing,
            Self::FlyToPortal => v1::EnderDragonPhase::FlyToPortal,
            Self::LandOnPortal => v1::EnderDragonPhase::LandOnPortal,
            Self::LeavePortal => v1::EnderDragonPhase::LeavePortal,
            Self::BreathAttack => v1::EnderDragonPhase::BreathAttack,
            Self::SearchForBreathAttackTarget => v1::EnderDragonPhase::SearchForBreathAttackTarget,
            Self::RoarBeforeAttack => v1::EnderDragonPhase::RoarBeforeAttack,
            Self::ChargePlayer => v1::EnderDragonPhase::ChargePlayer,
            Self::Dying => v1::EnderDragonPhase::Dying,
            Self::Hover => v1::EnderDragonPhase::Hover,
        }) as i32
    }
}

impl EntityDamageCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityDamageCause::try_from(value) {
            Ok(v1::EntityDamageCause::Kill) => Ok(Self::Kill),
            Ok(v1::EntityDamageCause::WorldBorder) => Ok(Self::WorldBorder),
            Ok(v1::EntityDamageCause::Contact) => Ok(Self::Contact),
            Ok(v1::EntityDamageCause::EntityAttack) => Ok(Self::EntityAttack),
            Ok(v1::EntityDamageCause::EntitySweepAttack) => Ok(Self::EntitySweepAttack),
            Ok(v1::EntityDamageCause::Projectile) => Ok(Self::Projectile),
            Ok(v1::EntityDamageCause::Suffocation) => Ok(Self::Suffocation),
            Ok(v1::EntityDamageCause::Fall) => Ok(Self::Fall),
            Ok(v1::EntityDamageCause::Fire) => Ok(Self::Fire),
            Ok(v1::EntityDamageCause::FireTick) => Ok(Self::FireTick),
            Ok(v1::EntityDamageCause::Melting) => Ok(Self::Melting),
            Ok(v1::EntityDamageCause::Lava) => Ok(Self::Lava),
            Ok(v1::EntityDamageCause::Drowning) => Ok(Self::Drowning),
            Ok(v1::EntityDamageCause::BlockExplosion) => Ok(Self::BlockExplosion),
            Ok(v1::EntityDamageCause::EntityExplosion) => Ok(Self::EntityExplosion),
            Ok(v1::EntityDamageCause::Void) => Ok(Self::Void),
            Ok(v1::EntityDamageCause::Lightning) => Ok(Self::Lightning),
            Ok(v1::EntityDamageCause::Suicide) => Ok(Self::Suicide),
            Ok(v1::EntityDamageCause::Starvation) => Ok(Self::Starvation),
            Ok(v1::EntityDamageCause::Poison) => Ok(Self::Poison),
            Ok(v1::EntityDamageCause::Magic) => Ok(Self::Magic),
            Ok(v1::EntityDamageCause::Wither) => Ok(Self::Wither),
            Ok(v1::EntityDamageCause::FallingBlock) => Ok(Self::FallingBlock),
            Ok(v1::EntityDamageCause::Thorns) => Ok(Self::Thorns),
            Ok(v1::EntityDamageCause::DragonBreath) => Ok(Self::DragonBreath),
            Ok(v1::EntityDamageCause::Custom) => Ok(Self::Custom),
            Ok(v1::EntityDamageCause::FlyIntoWall) => Ok(Self::FlyIntoWall),
            Ok(v1::EntityDamageCause::HotFloor) => Ok(Self::HotFloor),
            Ok(v1::EntityDamageCause::Campfire) => Ok(Self::Campfire),
            Ok(v1::EntityDamageCause::Cramming) => Ok(Self::Cramming),
            Ok(v1::EntityDamageCause::Dryout) => Ok(Self::Dryout),
            Ok(v1::EntityDamageCause::Freeze) => Ok(Self::Freeze),
            Ok(v1::EntityDamageCause::SonicBoom) => Ok(Self::SonicBoom),
            Ok(v1::EntityDamageCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity damage cause {value}"
            ))),
        }
    }
}

impl EntityExhaustionReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityExhaustionReason::try_from(value) {
            Ok(v1::EntityExhaustionReason::BlockMined) => Ok(Self::BlockMined),
            Ok(v1::EntityExhaustionReason::HungerEffect) => Ok(Self::HungerEffect),
            Ok(v1::EntityExhaustionReason::Damaged) => Ok(Self::Damaged),
            Ok(v1::EntityExhaustionReason::Attack) => Ok(Self::Attack),
            Ok(v1::EntityExhaustionReason::JumpSprint) => Ok(Self::JumpSprint),
            Ok(v1::EntityExhaustionReason::Jump) => Ok(Self::Jump),
            Ok(v1::EntityExhaustionReason::Swim) => Ok(Self::Swim),
            Ok(v1::EntityExhaustionReason::WalkUnderwater) => Ok(Self::WalkUnderwater),
            Ok(v1::EntityExhaustionReason::WalkOnWater) => Ok(Self::WalkOnWater),
            Ok(v1::EntityExhaustionReason::Sprint) => Ok(Self::Sprint),
            Ok(v1::EntityExhaustionReason::Crouch) => Ok(Self::Crouch),
            Ok(v1::EntityExhaustionReason::Walk) => Ok(Self::Walk),
            Ok(v1::EntityExhaustionReason::Regen) => Ok(Self::Regen),
            Ok(v1::EntityExhaustionReason::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityExhaustionReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity exhaustion reason {value}"
            ))),
        }
    }
}

impl EntityKnockbackCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityKnockbackCause::try_from(value) {
            Ok(v1::EntityKnockbackCause::Damage) => Ok(Self::Damage),
            Ok(v1::EntityKnockbackCause::EntityAttack) => Ok(Self::EntityAttack),
            Ok(v1::EntityKnockbackCause::Explosion) => Ok(Self::Explosion),
            Ok(v1::EntityKnockbackCause::ShieldBlock) => Ok(Self::ShieldBlock),
            Ok(v1::EntityKnockbackCause::SweepAttack) => Ok(Self::SweepAttack),
            Ok(v1::EntityKnockbackCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityKnockbackCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity knockback cause {value}"
            ))),
        }
    }
}

impl EntityPotionEffectAction {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityPotionEffectAction::try_from(value) {
            Ok(v1::EntityPotionEffectAction::Added) => Ok(Self::Added),
            Ok(v1::EntityPotionEffectAction::Changed) => Ok(Self::Changed),
            Ok(v1::EntityPotionEffectAction::Cleared) => Ok(Self::Cleared),
            Ok(v1::EntityPotionEffectAction::Removed) => Ok(Self::Removed),
            Ok(v1::EntityPotionEffectAction::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown entity potion effect action {value}"),
            )),
        }
    }
}

impl EntityPotionEffectCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityPotionEffectCause::try_from(value) {
            Ok(v1::EntityPotionEffectCause::AreaEffectCloud) => Ok(Self::AreaEffectCloud),
            Ok(v1::EntityPotionEffectCause::Arrow) => Ok(Self::Arrow),
            Ok(v1::EntityPotionEffectCause::Attack) => Ok(Self::Attack),
            Ok(v1::EntityPotionEffectCause::Axolotl) => Ok(Self::Axolotl),
            Ok(v1::EntityPotionEffectCause::Beacon) => Ok(Self::Beacon),
            Ok(v1::EntityPotionEffectCause::Command) => Ok(Self::Command),
            Ok(v1::EntityPotionEffectCause::Conduit) => Ok(Self::Conduit),
            Ok(v1::EntityPotionEffectCause::Conversion) => Ok(Self::Conversion),
            Ok(v1::EntityPotionEffectCause::Death) => Ok(Self::Death),
            Ok(v1::EntityPotionEffectCause::Dolphin) => Ok(Self::Dolphin),
            Ok(v1::EntityPotionEffectCause::Expiration) => Ok(Self::Expiration),
            Ok(v1::EntityPotionEffectCause::Food) => Ok(Self::Food),
            Ok(v1::EntityPotionEffectCause::Illusion) => Ok(Self::Illusion),
            Ok(v1::EntityPotionEffectCause::Milk) => Ok(Self::Milk),
            Ok(v1::EntityPotionEffectCause::PatrolCaptain) => Ok(Self::PatrolCaptain),
            Ok(v1::EntityPotionEffectCause::Plugin) => Ok(Self::Plugin),
            Ok(v1::EntityPotionEffectCause::PotionDrink) => Ok(Self::PotionDrink),
            Ok(v1::EntityPotionEffectCause::PotionSplash) => Ok(Self::PotionSplash),
            Ok(v1::EntityPotionEffectCause::SpiderSpawn) => Ok(Self::SpiderSpawn),
            Ok(v1::EntityPotionEffectCause::Totem) => Ok(Self::Totem),
            Ok(v1::EntityPotionEffectCause::TurtleHelmet) => Ok(Self::TurtleHelmet),
            Ok(v1::EntityPotionEffectCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityPotionEffectCause::VillagerTrade) => Ok(Self::VillagerTrade),
            Ok(v1::EntityPotionEffectCause::Warden) => Ok(Self::Warden),
            Ok(v1::EntityPotionEffectCause::WitherRose) => Ok(Self::WitherRose),
            Ok(v1::EntityPotionEffectCause::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown entity potion effect cause {value}"),
            )),
        }
    }
}

impl EntityRegainHealthRegainReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityRegainHealthRegainReason::try_from(value) {
            Ok(v1::EntityRegainHealthRegainReason::Regen) => Ok(Self::Regen),
            Ok(v1::EntityRegainHealthRegainReason::Satiated) => Ok(Self::Satiated),
            Ok(v1::EntityRegainHealthRegainReason::Eating) => Ok(Self::Eating),
            Ok(v1::EntityRegainHealthRegainReason::EnderCrystal) => Ok(Self::EnderCrystal),
            Ok(v1::EntityRegainHealthRegainReason::Magic) => Ok(Self::Magic),
            Ok(v1::EntityRegainHealthRegainReason::MagicRegen) => Ok(Self::MagicRegen),
            Ok(v1::EntityRegainHealthRegainReason::WitherSpawn) => Ok(Self::WitherSpawn),
            Ok(v1::EntityRegainHealthRegainReason::Wither) => Ok(Self::Wither),
            Ok(v1::EntityRegainHealthRegainReason::Custom) => Ok(Self::Custom),
            Ok(v1::EntityRegainHealthRegainReason::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown entity regain health regain reason {value}"),
            )),
        }
    }
}

impl EntityRemoveCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityRemoveCause::try_from(value) {
            Ok(v1::EntityRemoveCause::Death) => Ok(Self::Death),
            Ok(v1::EntityRemoveCause::Despawn) => Ok(Self::Despawn),
            Ok(v1::EntityRemoveCause::Drop) => Ok(Self::Drop),
            Ok(v1::EntityRemoveCause::EnterBlock) => Ok(Self::EnterBlock),
            Ok(v1::EntityRemoveCause::Explode) => Ok(Self::Explode),
            Ok(v1::EntityRemoveCause::Hit) => Ok(Self::Hit),
            Ok(v1::EntityRemoveCause::Merge) => Ok(Self::Merge),
            Ok(v1::EntityRemoveCause::OutOfWorld) => Ok(Self::OutOfWorld),
            Ok(v1::EntityRemoveCause::Pickup) => Ok(Self::Pickup),
            Ok(v1::EntityRemoveCause::PlayerQuit) => Ok(Self::PlayerQuit),
            Ok(v1::EntityRemoveCause::Plugin) => Ok(Self::Plugin),
            Ok(v1::EntityRemoveCause::Transformation) => Ok(Self::Transformation),
            Ok(v1::EntityRemoveCause::Unload) => Ok(Self::Unload),
            Ok(v1::EntityRemoveCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity remove cause {value}"
            ))),
        }
    }
}

impl EntityTargetReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityTargetReason::try_from(value) {
            Ok(v1::EntityTargetReason::TargetDied) => Ok(Self::TargetDied),
            Ok(v1::EntityTargetReason::ClosestPlayer) => Ok(Self::ClosestPlayer),
            Ok(v1::EntityTargetReason::TargetAttackedEntity) => Ok(Self::TargetAttackedEntity),
            Ok(v1::EntityTargetReason::PigZombieTarget) => Ok(Self::PigZombieTarget),
            Ok(v1::EntityTargetReason::ForgotTarget) => Ok(Self::ForgotTarget),
            Ok(v1::EntityTargetReason::TargetAttackedOwner) => Ok(Self::TargetAttackedOwner),
            Ok(v1::EntityTargetReason::OwnerAttackedTarget) => Ok(Self::OwnerAttackedTarget),
            Ok(v1::EntityTargetReason::RandomTarget) => Ok(Self::RandomTarget),
            Ok(v1::EntityTargetReason::DefendVillage) => Ok(Self::DefendVillage),
            Ok(v1::EntityTargetReason::TargetAttackedNearbyEntity) => {
                Ok(Self::TargetAttackedNearbyEntity)
            }
            Ok(v1::EntityTargetReason::ReinforcementTarget) => Ok(Self::ReinforcementTarget),
            Ok(v1::EntityTargetReason::Collision) => Ok(Self::Collision),
            Ok(v1::EntityTargetReason::Custom) => Ok(Self::Custom),
            Ok(v1::EntityTargetReason::ClosestEntity) => Ok(Self::ClosestEntity),
            Ok(v1::EntityTargetReason::FollowLeader) => Ok(Self::FollowLeader),
            Ok(v1::EntityTargetReason::Tempt) => Ok(Self::Tempt),
            Ok(v1::EntityTargetReason::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityTargetReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity target reason {value}"
            ))),
        }
    }
}

impl EntityTransformReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityTransformReason::try_from(value) {
            Ok(v1::EntityTransformReason::Cured) => Ok(Self::Cured),
            Ok(v1::EntityTransformReason::Frozen) => Ok(Self::Frozen),
            Ok(v1::EntityTransformReason::Infection) => Ok(Self::Infection),
            Ok(v1::EntityTransformReason::Drowned) => Ok(Self::Drowned),
            Ok(v1::EntityTransformReason::Sheared) => Ok(Self::Sheared),
            Ok(v1::EntityTransformReason::Lightning) => Ok(Self::Lightning),
            Ok(v1::EntityTransformReason::Split) => Ok(Self::Split),
            Ok(v1::EntityTransformReason::PiglinZombified) => Ok(Self::PiglinZombified),
            Ok(v1::EntityTransformReason::Metamorphosis) => Ok(Self::Metamorphosis),
            Ok(v1::EntityTransformReason::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityTransformReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity transform reason {value}"
            ))),
        }
    }
}

impl EntityUnleashReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EntityUnleashReason::try_from(value) {
            Ok(v1::EntityUnleashReason::HolderGone) => Ok(Self::HolderGone),
            Ok(v1::EntityUnleashReason::PlayerUnleash) => Ok(Self::PlayerUnleash),
            Ok(v1::EntityUnleashReason::Distance) => Ok(Self::Distance),
            Ok(v1::EntityUnleashReason::Shear) => Ok(Self::Shear),
            Ok(v1::EntityUnleashReason::Firework) => Ok(Self::Firework),
            Ok(v1::EntityUnleashReason::Unknown) => Ok(Self::Unknown),
            Ok(v1::EntityUnleashReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown entity unleash reason {value}"
            ))),
        }
    }
}

impl EquipmentSlot {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EquipmentSlot::try_from(value) {
            Ok(v1::EquipmentSlot::Hand) => Ok(Self::Hand),
            Ok(v1::EquipmentSlot::OffHand) => Ok(Self::OffHand),
            Ok(v1::EquipmentSlot::Feet) => Ok(Self::Feet),
            Ok(v1::EquipmentSlot::Legs) => Ok(Self::Legs),
            Ok(v1::EquipmentSlot::Chest) => Ok(Self::Chest),
            Ok(v1::EquipmentSlot::Head) => Ok(Self::Head),
            Ok(v1::EquipmentSlot::Body) => Ok(Self::Body),
            Ok(v1::EquipmentSlot::Saddle) => Ok(Self::Saddle),
            Ok(v1::EquipmentSlot::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown equipment slot {value}"
            ))),
        }
    }
}

impl EventResult {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::EventResult::try_from(value) {
            Ok(v1::EventResult::Deny) => Ok(Self::Deny),
            Ok(v1::EventResult::Default) => Ok(Self::Default),
            Ok(v1::EventResult::Allow) => Ok(Self::Allow),
            Ok(v1::EventResult::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown event result {value}")))
            }
        }
    }

    pub(crate) const fn to_wire(self) -> i32 {
        (match self {
            Self::Deny => v1::EventResult::Deny,
            Self::Default => v1::EventResult::Default,
            Self::Allow => v1::EventResult::Allow,
        }) as i32
    }
}

impl ExplosionResult {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::ExplosionResult::try_from(value) {
            Ok(v1::ExplosionResult::Keep) => Ok(Self::Keep),
            Ok(v1::ExplosionResult::Destroy) => Ok(Self::Destroy),
            Ok(v1::ExplosionResult::DestroyWithDecay) => Ok(Self::DestroyWithDecay),
            Ok(v1::ExplosionResult::TriggerBlock) => Ok(Self::TriggerBlock),
            Ok(v1::ExplosionResult::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown explosion result {value}"
            ))),
        }
    }
}

impl Face {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Face::try_from(value) {
            Ok(v1::Face::Down) => Ok(Self::Down),
            Ok(v1::Face::Up) => Ok(Self::Up),
            Ok(v1::Face::North) => Ok(Self::North),
            Ok(v1::Face::South) => Ok(Self::South),
            Ok(v1::Face::West) => Ok(Self::West),
            Ok(v1::Face::East) => Ok(Self::East),
            Ok(v1::Face::NorthEast) => Ok(Self::NorthEast),
            Ok(v1::Face::NorthWest) => Ok(Self::NorthWest),
            Ok(v1::Face::SouthEast) => Ok(Self::SouthEast),
            Ok(v1::Face::SouthWest) => Ok(Self::SouthWest),
            Ok(v1::Face::WestNorthWest) => Ok(Self::WestNorthWest),
            Ok(v1::Face::NorthNorthWest) => Ok(Self::NorthNorthWest),
            Ok(v1::Face::NorthNorthEast) => Ok(Self::NorthNorthEast),
            Ok(v1::Face::EastNorthEast) => Ok(Self::EastNorthEast),
            Ok(v1::Face::EastSouthEast) => Ok(Self::EastSouthEast),
            Ok(v1::Face::SouthSouthEast) => Ok(Self::SouthSouthEast),
            Ok(v1::Face::SouthSouthWest) => Ok(Self::SouthSouthWest),
            Ok(v1::Face::WestSouthWest) => Ok(Self::WestSouthWest),
            Ok(v1::Face::Self_) => Ok(Self::Itself),
            Ok(v1::Face::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown face {value}")))
            }
        }
    }
}

impl HangingBreakRemoveCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::HangingBreakRemoveCause::try_from(value) {
            Ok(v1::HangingBreakRemoveCause::Entity) => Ok(Self::Entity),
            Ok(v1::HangingBreakRemoveCause::Explosion) => Ok(Self::Explosion),
            Ok(v1::HangingBreakRemoveCause::Obstruction) => Ok(Self::Obstruction),
            Ok(v1::HangingBreakRemoveCause::Physics) => Ok(Self::Physics),
            Ok(v1::HangingBreakRemoveCause::Default) => Ok(Self::Default),
            Ok(v1::HangingBreakRemoveCause::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown hanging break remove cause {value}"),
            )),
        }
    }
}

impl HopperInventorySearchContainerType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::HopperInventorySearchContainerType::try_from(value) {
            Ok(v1::HopperInventorySearchContainerType::Source) => Ok(Self::Source),
            Ok(v1::HopperInventorySearchContainerType::Destination) => Ok(Self::Destination),
            Ok(v1::HopperInventorySearchContainerType::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!(
                    "an unknown hopper inventory search container type {value}"
                )))
            }
        }
    }
}

impl Instrument {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Instrument::try_from(value) {
            Ok(v1::Instrument::Piano) => Ok(Self::Piano),
            Ok(v1::Instrument::BassDrum) => Ok(Self::BassDrum),
            Ok(v1::Instrument::SnareDrum) => Ok(Self::SnareDrum),
            Ok(v1::Instrument::Sticks) => Ok(Self::Sticks),
            Ok(v1::Instrument::BassGuitar) => Ok(Self::BassGuitar),
            Ok(v1::Instrument::Flute) => Ok(Self::Flute),
            Ok(v1::Instrument::Bell) => Ok(Self::Bell),
            Ok(v1::Instrument::Guitar) => Ok(Self::Guitar),
            Ok(v1::Instrument::Chime) => Ok(Self::Chime),
            Ok(v1::Instrument::Xylophone) => Ok(Self::Xylophone),
            Ok(v1::Instrument::IronXylophone) => Ok(Self::IronXylophone),
            Ok(v1::Instrument::CowBell) => Ok(Self::CowBell),
            Ok(v1::Instrument::Didgeridoo) => Ok(Self::Didgeridoo),
            Ok(v1::Instrument::Bit) => Ok(Self::Bit),
            Ok(v1::Instrument::Banjo) => Ok(Self::Banjo),
            Ok(v1::Instrument::Pling) => Ok(Self::Pling),
            Ok(v1::Instrument::Zombie) => Ok(Self::Zombie),
            Ok(v1::Instrument::Trumpet) => Ok(Self::Trumpet),
            Ok(v1::Instrument::TrumpetExposed) => Ok(Self::TrumpetExposed),
            Ok(v1::Instrument::TrumpetOxidized) => Ok(Self::TrumpetOxidized),
            Ok(v1::Instrument::TrumpetWeathered) => Ok(Self::TrumpetWeathered),
            Ok(v1::Instrument::Skeleton) => Ok(Self::Skeleton),
            Ok(v1::Instrument::Creeper) => Ok(Self::Creeper),
            Ok(v1::Instrument::Dragon) => Ok(Self::Dragon),
            Ok(v1::Instrument::WitherSkeleton) => Ok(Self::WitherSkeleton),
            Ok(v1::Instrument::Piglin) => Ok(Self::Piglin),
            Ok(v1::Instrument::CustomHead) => Ok(Self::CustomHead),
            Ok(v1::Instrument::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown instrument {value}")))
            }
        }
    }
}

impl InventoryAction {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::InventoryAction::try_from(value) {
            Ok(v1::InventoryAction::Nothing) => Ok(Self::Nothing),
            Ok(v1::InventoryAction::PickupAll) => Ok(Self::PickupAll),
            Ok(v1::InventoryAction::PickupSome) => Ok(Self::PickupSome),
            Ok(v1::InventoryAction::PickupHalf) => Ok(Self::PickupHalf),
            Ok(v1::InventoryAction::PickupOne) => Ok(Self::PickupOne),
            Ok(v1::InventoryAction::PlaceAll) => Ok(Self::PlaceAll),
            Ok(v1::InventoryAction::PlaceSome) => Ok(Self::PlaceSome),
            Ok(v1::InventoryAction::PlaceOne) => Ok(Self::PlaceOne),
            Ok(v1::InventoryAction::SwapWithCursor) => Ok(Self::SwapWithCursor),
            Ok(v1::InventoryAction::DropAllCursor) => Ok(Self::DropAllCursor),
            Ok(v1::InventoryAction::DropOneCursor) => Ok(Self::DropOneCursor),
            Ok(v1::InventoryAction::DropAllSlot) => Ok(Self::DropAllSlot),
            Ok(v1::InventoryAction::DropOneSlot) => Ok(Self::DropOneSlot),
            Ok(v1::InventoryAction::MoveToOtherInventory) => Ok(Self::MoveToOtherInventory),
            Ok(v1::InventoryAction::HotbarMoveAndReadd) => Ok(Self::HotbarMoveAndReadd),
            Ok(v1::InventoryAction::HotbarSwap) => Ok(Self::HotbarSwap),
            Ok(v1::InventoryAction::CloneStack) => Ok(Self::CloneStack),
            Ok(v1::InventoryAction::CollectToCursor) => Ok(Self::CollectToCursor),
            Ok(v1::InventoryAction::Unknown) => Ok(Self::Unknown),
            Ok(v1::InventoryAction::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown inventory action {value}"
            ))),
        }
    }
}

impl InventoryTypeSlotType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::InventoryTypeSlotType::try_from(value) {
            Ok(v1::InventoryTypeSlotType::Result) => Ok(Self::Result),
            Ok(v1::InventoryTypeSlotType::Crafting) => Ok(Self::Crafting),
            Ok(v1::InventoryTypeSlotType::Armor) => Ok(Self::Armor),
            Ok(v1::InventoryTypeSlotType::Container) => Ok(Self::Container),
            Ok(v1::InventoryTypeSlotType::Quickbar) => Ok(Self::Quickbar),
            Ok(v1::InventoryTypeSlotType::Outside) => Ok(Self::Outside),
            Ok(v1::InventoryTypeSlotType::Fuel) => Ok(Self::Fuel),
            Ok(v1::InventoryTypeSlotType::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown inventory type slot type {value}"
            ))),
        }
    }
}

impl LightningStrikeCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::LightningStrikeCause::try_from(value) {
            Ok(v1::LightningStrikeCause::Command) => Ok(Self::Command),
            Ok(v1::LightningStrikeCause::Custom) => Ok(Self::Custom),
            Ok(v1::LightningStrikeCause::Spawner) => Ok(Self::Spawner),
            Ok(v1::LightningStrikeCause::Trident) => Ok(Self::Trident),
            Ok(v1::LightningStrikeCause::Trap) => Ok(Self::Trap),
            Ok(v1::LightningStrikeCause::Weather) => Ok(Self::Weather),
            Ok(v1::LightningStrikeCause::Enchantment) => Ok(Self::Enchantment),
            Ok(v1::LightningStrikeCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::LightningStrikeCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown lightning strike cause {value}"
            ))),
        }
    }
}

impl MainHand {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::MainHand::try_from(value) {
            Ok(v1::MainHand::Left) => Ok(Self::Left),
            Ok(v1::MainHand::Right) => Ok(Self::Right),
            Ok(v1::MainHand::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown main hand {value}")))
            }
        }
    }
}

impl PlayerAnimationType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerAnimationType::try_from(value) {
            Ok(v1::PlayerAnimationType::ArmSwing) => Ok(Self::ArmSwing),
            Ok(v1::PlayerAnimationType::OffArmSwing) => Ok(Self::OffArmSwing),
            Ok(v1::PlayerAnimationType::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player animation type {value}"
            ))),
        }
    }
}

impl PlayerBedEnterResult {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerBedEnterResult::try_from(value) {
            Ok(v1::PlayerBedEnterResult::Ok) => Ok(Self::Ok),
            Ok(v1::PlayerBedEnterResult::NotPossibleHere) => Ok(Self::NotPossibleHere),
            Ok(v1::PlayerBedEnterResult::NotPossibleNow) => Ok(Self::NotPossibleNow),
            Ok(v1::PlayerBedEnterResult::TooFarAway) => Ok(Self::TooFarAway),
            Ok(v1::PlayerBedEnterResult::NotSafe) => Ok(Self::NotSafe),
            Ok(v1::PlayerBedEnterResult::OtherProblem) => Ok(Self::OtherProblem),
            Ok(v1::PlayerBedEnterResult::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player bed enter result {value}"
            ))),
        }
    }
}

impl PlayerExpCooldownChangeReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerExpCooldownChangeReason::try_from(value) {
            Ok(v1::PlayerExpCooldownChangeReason::PickupOrb) => Ok(Self::PickupOrb),
            Ok(v1::PlayerExpCooldownChangeReason::Plugin) => Ok(Self::Plugin),
            Ok(v1::PlayerExpCooldownChangeReason::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown player exp cooldown change reason {value}"),
            )),
        }
    }
}

impl PlayerFishState {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerFishState::try_from(value) {
            Ok(v1::PlayerFishState::Fishing) => Ok(Self::Fishing),
            Ok(v1::PlayerFishState::CaughtFish) => Ok(Self::CaughtFish),
            Ok(v1::PlayerFishState::CaughtEntity) => Ok(Self::CaughtEntity),
            Ok(v1::PlayerFishState::InGround) => Ok(Self::InGround),
            Ok(v1::PlayerFishState::FailedAttempt) => Ok(Self::FailedAttempt),
            Ok(v1::PlayerFishState::ReelIn) => Ok(Self::ReelIn),
            Ok(v1::PlayerFishState::Bite) => Ok(Self::Bite),
            Ok(v1::PlayerFishState::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player fish state {value}"
            ))),
        }
    }
}

impl PlayerLoginResult {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerLoginResult::try_from(value) {
            Ok(v1::PlayerLoginResult::Allowed) => Ok(Self::Allowed),
            Ok(v1::PlayerLoginResult::KickFull) => Ok(Self::KickFull),
            Ok(v1::PlayerLoginResult::KickBanned) => Ok(Self::KickBanned),
            Ok(v1::PlayerLoginResult::KickWhitelist) => Ok(Self::KickWhitelist),
            Ok(v1::PlayerLoginResult::KickOther) => Ok(Self::KickOther),
            Ok(v1::PlayerLoginResult::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player login result {value}"
            ))),
        }
    }

    pub(crate) const fn to_wire(self) -> i32 {
        (match self {
            Self::Allowed => v1::PlayerLoginResult::Allowed,
            Self::KickFull => v1::PlayerLoginResult::KickFull,
            Self::KickBanned => v1::PlayerLoginResult::KickBanned,
            Self::KickWhitelist => v1::PlayerLoginResult::KickWhitelist,
            Self::KickOther => v1::PlayerLoginResult::KickOther,
        }) as i32
    }
}

impl PlayerRecipeBookSettingsChangeRecipeBookType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerRecipeBookSettingsChangeRecipeBookType::try_from(value) {
            Ok(v1::PlayerRecipeBookSettingsChangeRecipeBookType::Crafting) => Ok(Self::Crafting),
            Ok(v1::PlayerRecipeBookSettingsChangeRecipeBookType::Furnace) => Ok(Self::Furnace),
            Ok(v1::PlayerRecipeBookSettingsChangeRecipeBookType::BlastFurnace) => {
                Ok(Self::BlastFurnace)
            }
            Ok(v1::PlayerRecipeBookSettingsChangeRecipeBookType::Smoker) => Ok(Self::Smoker),
            Ok(v1::PlayerRecipeBookSettingsChangeRecipeBookType::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!(
                    "an unknown player recipe book settings change recipe book type {value}"
                )))
            }
        }
    }
}

impl PlayerRespawnReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerRespawnReason::try_from(value) {
            Ok(v1::PlayerRespawnReason::Death) => Ok(Self::Death),
            Ok(v1::PlayerRespawnReason::EndPortal) => Ok(Self::EndPortal),
            Ok(v1::PlayerRespawnReason::Plugin) => Ok(Self::Plugin),
            Ok(v1::PlayerRespawnReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player respawn reason {value}"
            ))),
        }
    }
}

impl PlayerSignOpenCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerSignOpenCause::try_from(value) {
            Ok(v1::PlayerSignOpenCause::Interact) => Ok(Self::Interact),
            Ok(v1::PlayerSignOpenCause::Place) => Ok(Self::Place),
            Ok(v1::PlayerSignOpenCause::Plugin) => Ok(Self::Plugin),
            Ok(v1::PlayerSignOpenCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::PlayerSignOpenCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player sign open cause {value}"
            ))),
        }
    }
}

impl PlayerSpawnChangeCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerSpawnChangeCause::try_from(value) {
            Ok(v1::PlayerSpawnChangeCause::Command) => Ok(Self::Command),
            Ok(v1::PlayerSpawnChangeCause::Bed) => Ok(Self::Bed),
            Ok(v1::PlayerSpawnChangeCause::RespawnAnchor) => Ok(Self::RespawnAnchor),
            Ok(v1::PlayerSpawnChangeCause::Plugin) => Ok(Self::Plugin),
            Ok(v1::PlayerSpawnChangeCause::Reset) => Ok(Self::Reset),
            Ok(v1::PlayerSpawnChangeCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::PlayerSpawnChangeCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player spawn change cause {value}"
            ))),
        }
    }
}

impl PlayerTeleportCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PlayerTeleportCause::try_from(value) {
            Ok(v1::PlayerTeleportCause::EnderPearl) => Ok(Self::EnderPearl),
            Ok(v1::PlayerTeleportCause::Command) => Ok(Self::Command),
            Ok(v1::PlayerTeleportCause::Plugin) => Ok(Self::Plugin),
            Ok(v1::PlayerTeleportCause::NetherPortal) => Ok(Self::NetherPortal),
            Ok(v1::PlayerTeleportCause::EndPortal) => Ok(Self::EndPortal),
            Ok(v1::PlayerTeleportCause::Spectate) => Ok(Self::Spectate),
            Ok(v1::PlayerTeleportCause::EndGateway) => Ok(Self::EndGateway),
            Ok(v1::PlayerTeleportCause::ChorusFruit) => Ok(Self::ChorusFruit),
            Ok(v1::PlayerTeleportCause::Dismount) => Ok(Self::Dismount),
            Ok(v1::PlayerTeleportCause::ExitBed) => Ok(Self::ExitBed),
            Ok(v1::PlayerTeleportCause::Unknown) => Ok(Self::Unknown),
            Ok(v1::PlayerTeleportCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown player teleport cause {value}"
            ))),
        }
    }
}

impl PortalCreateReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::PortalCreateReason::try_from(value) {
            Ok(v1::PortalCreateReason::Fire) => Ok(Self::Fire),
            Ok(v1::PortalCreateReason::NetherPair) => Ok(Self::NetherPair),
            Ok(v1::PortalCreateReason::EndPlatform) => Ok(Self::EndPlatform),
            Ok(v1::PortalCreateReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown portal create reason {value}"
            ))),
        }
    }
}

impl Pose {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Pose::try_from(value) {
            Ok(v1::Pose::Standing) => Ok(Self::Standing),
            Ok(v1::Pose::FallFlying) => Ok(Self::FallFlying),
            Ok(v1::Pose::Sleeping) => Ok(Self::Sleeping),
            Ok(v1::Pose::Swimming) => Ok(Self::Swimming),
            Ok(v1::Pose::SpinAttack) => Ok(Self::SpinAttack),
            Ok(v1::Pose::Sneaking) => Ok(Self::Sneaking),
            Ok(v1::Pose::LongJumping) => Ok(Self::LongJumping),
            Ok(v1::Pose::Dying) => Ok(Self::Dying),
            Ok(v1::Pose::Croaking) => Ok(Self::Croaking),
            Ok(v1::Pose::UsingTongue) => Ok(Self::UsingTongue),
            Ok(v1::Pose::Sitting) => Ok(Self::Sitting),
            Ok(v1::Pose::Roaring) => Ok(Self::Roaring),
            Ok(v1::Pose::Sniffing) => Ok(Self::Sniffing),
            Ok(v1::Pose::Emerging) => Ok(Self::Emerging),
            Ok(v1::Pose::Digging) => Ok(Self::Digging),
            Ok(v1::Pose::Sliding) => Ok(Self::Sliding),
            Ok(v1::Pose::Shooting) => Ok(Self::Shooting),
            Ok(v1::Pose::Inhaling) => Ok(Self::Inhaling),
            Ok(v1::Pose::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown pose {value}")))
            }
        }
    }
}

impl RaidStopReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::RaidStopReason::try_from(value) {
            Ok(v1::RaidStopReason::Peace) => Ok(Self::Peace),
            Ok(v1::RaidStopReason::Timeout) => Ok(Self::Timeout),
            Ok(v1::RaidStopReason::Finished) => Ok(Self::Finished),
            Ok(v1::RaidStopReason::Unspawnable) => Ok(Self::Unspawnable),
            Ok(v1::RaidStopReason::NotInVillage) => Ok(Self::NotInVillage),
            Ok(v1::RaidStopReason::Plugin) => Ok(Self::Plugin),
            Ok(v1::RaidStopReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown raid stop reason {value}"
            ))),
        }
    }
}

impl ResourcePackStatus {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::ResourcePackStatus::try_from(value) {
            Ok(v1::ResourcePackStatus::SuccessfullyLoaded) => Ok(Self::SuccessfullyLoaded),
            Ok(v1::ResourcePackStatus::Declined) => Ok(Self::Declined),
            Ok(v1::ResourcePackStatus::FailedDownload) => Ok(Self::FailedDownload),
            Ok(v1::ResourcePackStatus::Accepted) => Ok(Self::Accepted),
            Ok(v1::ResourcePackStatus::Downloaded) => Ok(Self::Downloaded),
            Ok(v1::ResourcePackStatus::InvalidUrl) => Ok(Self::InvalidUrl),
            Ok(v1::ResourcePackStatus::FailedReload) => Ok(Self::FailedReload),
            Ok(v1::ResourcePackStatus::Discarded) => Ok(Self::Discarded),
            Ok(v1::ResourcePackStatus::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown resource pack status {value}"
            ))),
        }
    }
}

impl ServerLoadType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::ServerLoadType::try_from(value) {
            Ok(v1::ServerLoadType::Startup) => Ok(Self::Startup),
            Ok(v1::ServerLoadType::Reload) => Ok(Self::Reload),
            Ok(v1::ServerLoadType::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown server load type {value}"
            ))),
        }
    }
}

impl Side {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Side::try_from(value) {
            Ok(v1::Side::Front) => Ok(Self::Front),
            Ok(v1::Side::Back) => Ok(Self::Back),
            Ok(v1::Side::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown side {value}")))
            }
        }
    }
}

impl SpellcasterSpell {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::SpellcasterSpell::try_from(value) {
            Ok(v1::SpellcasterSpell::None) => Ok(Self::None),
            Ok(v1::SpellcasterSpell::SummonVex) => Ok(Self::SummonVex),
            Ok(v1::SpellcasterSpell::Fangs) => Ok(Self::Fangs),
            Ok(v1::SpellcasterSpell::Wololo) => Ok(Self::Wololo),
            Ok(v1::SpellcasterSpell::Disappear) => Ok(Self::Disappear),
            Ok(v1::SpellcasterSpell::Blindness) => Ok(Self::Blindness),
            Ok(v1::SpellcasterSpell::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown spellcaster spell {value}"
            ))),
        }
    }
}

impl Statistic {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Statistic::try_from(value) {
            Ok(v1::Statistic::DamageDealt) => Ok(Self::DamageDealt),
            Ok(v1::Statistic::DamageTaken) => Ok(Self::DamageTaken),
            Ok(v1::Statistic::Deaths) => Ok(Self::Deaths),
            Ok(v1::Statistic::MobKills) => Ok(Self::MobKills),
            Ok(v1::Statistic::PlayerKills) => Ok(Self::PlayerKills),
            Ok(v1::Statistic::FishCaught) => Ok(Self::FishCaught),
            Ok(v1::Statistic::AnimalsBred) => Ok(Self::AnimalsBred),
            Ok(v1::Statistic::LeaveGame) => Ok(Self::LeaveGame),
            Ok(v1::Statistic::Jump) => Ok(Self::Jump),
            Ok(v1::Statistic::DropCount) => Ok(Self::DropCount),
            Ok(v1::Statistic::Drop) => Ok(Self::Drop),
            Ok(v1::Statistic::Pickup) => Ok(Self::Pickup),
            Ok(v1::Statistic::PlayOneMinute) => Ok(Self::PlayOneMinute),
            Ok(v1::Statistic::TotalWorldTime) => Ok(Self::TotalWorldTime),
            Ok(v1::Statistic::WalkOneCm) => Ok(Self::WalkOneCm),
            Ok(v1::Statistic::WalkOnWaterOneCm) => Ok(Self::WalkOnWaterOneCm),
            Ok(v1::Statistic::FallOneCm) => Ok(Self::FallOneCm),
            Ok(v1::Statistic::SneakTime) => Ok(Self::SneakTime),
            Ok(v1::Statistic::ClimbOneCm) => Ok(Self::ClimbOneCm),
            Ok(v1::Statistic::FlyOneCm) => Ok(Self::FlyOneCm),
            Ok(v1::Statistic::WalkUnderWaterOneCm) => Ok(Self::WalkUnderWaterOneCm),
            Ok(v1::Statistic::MinecartOneCm) => Ok(Self::MinecartOneCm),
            Ok(v1::Statistic::BoatOneCm) => Ok(Self::BoatOneCm),
            Ok(v1::Statistic::PigOneCm) => Ok(Self::PigOneCm),
            Ok(v1::Statistic::HappyGhastOneCm) => Ok(Self::HappyGhastOneCm),
            Ok(v1::Statistic::HorseOneCm) => Ok(Self::HorseOneCm),
            Ok(v1::Statistic::SprintOneCm) => Ok(Self::SprintOneCm),
            Ok(v1::Statistic::CrouchOneCm) => Ok(Self::CrouchOneCm),
            Ok(v1::Statistic::AviateOneCm) => Ok(Self::AviateOneCm),
            Ok(v1::Statistic::MineBlock) => Ok(Self::MineBlock),
            Ok(v1::Statistic::UseItem) => Ok(Self::UseItem),
            Ok(v1::Statistic::BreakItem) => Ok(Self::BreakItem),
            Ok(v1::Statistic::CraftItem) => Ok(Self::CraftItem),
            Ok(v1::Statistic::KillEntity) => Ok(Self::KillEntity),
            Ok(v1::Statistic::EntityKilledBy) => Ok(Self::EntityKilledBy),
            Ok(v1::Statistic::TimeSinceDeath) => Ok(Self::TimeSinceDeath),
            Ok(v1::Statistic::TalkedToVillager) => Ok(Self::TalkedToVillager),
            Ok(v1::Statistic::TradedWithVillager) => Ok(Self::TradedWithVillager),
            Ok(v1::Statistic::CakeSlicesEaten) => Ok(Self::CakeSlicesEaten),
            Ok(v1::Statistic::CauldronFilled) => Ok(Self::CauldronFilled),
            Ok(v1::Statistic::CauldronUsed) => Ok(Self::CauldronUsed),
            Ok(v1::Statistic::ArmorCleaned) => Ok(Self::ArmorCleaned),
            Ok(v1::Statistic::BannerCleaned) => Ok(Self::BannerCleaned),
            Ok(v1::Statistic::BrewingstandInteraction) => Ok(Self::BrewingstandInteraction),
            Ok(v1::Statistic::BeaconInteraction) => Ok(Self::BeaconInteraction),
            Ok(v1::Statistic::DropperInspected) => Ok(Self::DropperInspected),
            Ok(v1::Statistic::HopperInspected) => Ok(Self::HopperInspected),
            Ok(v1::Statistic::DispenserInspected) => Ok(Self::DispenserInspected),
            Ok(v1::Statistic::NoteblockPlayed) => Ok(Self::NoteblockPlayed),
            Ok(v1::Statistic::NoteblockTuned) => Ok(Self::NoteblockTuned),
            Ok(v1::Statistic::FlowerPotted) => Ok(Self::FlowerPotted),
            Ok(v1::Statistic::TrappedChestTriggered) => Ok(Self::TrappedChestTriggered),
            Ok(v1::Statistic::EnderchestOpened) => Ok(Self::EnderchestOpened),
            Ok(v1::Statistic::ItemEnchanted) => Ok(Self::ItemEnchanted),
            Ok(v1::Statistic::RecordPlayed) => Ok(Self::RecordPlayed),
            Ok(v1::Statistic::FurnaceInteraction) => Ok(Self::FurnaceInteraction),
            Ok(v1::Statistic::CraftingTableInteraction) => Ok(Self::CraftingTableInteraction),
            Ok(v1::Statistic::ChestOpened) => Ok(Self::ChestOpened),
            Ok(v1::Statistic::SleepInBed) => Ok(Self::SleepInBed),
            Ok(v1::Statistic::SleepInStrawBed) => Ok(Self::SleepInStrawBed),
            Ok(v1::Statistic::ShulkerBoxOpened) => Ok(Self::ShulkerBoxOpened),
            Ok(v1::Statistic::TimeSinceRest) => Ok(Self::TimeSinceRest),
            Ok(v1::Statistic::SwimOneCm) => Ok(Self::SwimOneCm),
            Ok(v1::Statistic::DamageDealtAbsorbed) => Ok(Self::DamageDealtAbsorbed),
            Ok(v1::Statistic::DamageDealtResisted) => Ok(Self::DamageDealtResisted),
            Ok(v1::Statistic::DamageBlockedByShield) => Ok(Self::DamageBlockedByShield),
            Ok(v1::Statistic::DamageAbsorbed) => Ok(Self::DamageAbsorbed),
            Ok(v1::Statistic::DamageResisted) => Ok(Self::DamageResisted),
            Ok(v1::Statistic::CleanShulkerBox) => Ok(Self::CleanShulkerBox),
            Ok(v1::Statistic::OpenBarrel) => Ok(Self::OpenBarrel),
            Ok(v1::Statistic::InteractWithBlastFurnace) => Ok(Self::InteractWithBlastFurnace),
            Ok(v1::Statistic::InteractWithSmoker) => Ok(Self::InteractWithSmoker),
            Ok(v1::Statistic::InteractWithLectern) => Ok(Self::InteractWithLectern),
            Ok(v1::Statistic::InteractWithCampfire) => Ok(Self::InteractWithCampfire),
            Ok(v1::Statistic::InteractWithCartographyTable) => {
                Ok(Self::InteractWithCartographyTable)
            }
            Ok(v1::Statistic::InteractWithLoom) => Ok(Self::InteractWithLoom),
            Ok(v1::Statistic::InteractWithStonecutter) => Ok(Self::InteractWithStonecutter),
            Ok(v1::Statistic::BellRing) => Ok(Self::BellRing),
            Ok(v1::Statistic::RaidTrigger) => Ok(Self::RaidTrigger),
            Ok(v1::Statistic::RaidWin) => Ok(Self::RaidWin),
            Ok(v1::Statistic::InteractWithAnvil) => Ok(Self::InteractWithAnvil),
            Ok(v1::Statistic::InteractWithGrindstone) => Ok(Self::InteractWithGrindstone),
            Ok(v1::Statistic::TargetHit) => Ok(Self::TargetHit),
            Ok(v1::Statistic::InteractWithSmithingTable) => Ok(Self::InteractWithSmithingTable),
            Ok(v1::Statistic::StriderOneCm) => Ok(Self::StriderOneCm),
            Ok(v1::Statistic::NautilusOneCm) => Ok(Self::NautilusOneCm),
            Ok(v1::Statistic::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown statistic {value}")))
            }
        }
    }
}

impl TNTPrimeCause {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::TntPrimeCause::try_from(value) {
            Ok(v1::TntPrimeCause::Fire) => Ok(Self::Fire),
            Ok(v1::TntPrimeCause::Redstone) => Ok(Self::Redstone),
            Ok(v1::TntPrimeCause::Player) => Ok(Self::Player),
            Ok(v1::TntPrimeCause::Explosion) => Ok(Self::Explosion),
            Ok(v1::TntPrimeCause::Projectile) => Ok(Self::Projectile),
            Ok(v1::TntPrimeCause::BlockBreak) => Ok(Self::BlockBreak),
            Ok(v1::TntPrimeCause::Dispenser) => Ok(Self::Dispenser),
            Ok(v1::TntPrimeCause::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown tnt prime cause {value}"
            ))),
        }
    }
}

impl TimeSkipReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::TimeSkipReason::try_from(value) {
            Ok(v1::TimeSkipReason::Command) => Ok(Self::Command),
            Ok(v1::TimeSkipReason::Custom) => Ok(Self::Custom),
            Ok(v1::TimeSkipReason::NightSkip) => Ok(Self::NightSkip),
            Ok(v1::TimeSkipReason::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown time skip reason {value}"
            ))),
        }
    }
}

impl TreeType {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::TreeType::try_from(value) {
            Ok(v1::TreeType::Tree) => Ok(Self::Tree),
            Ok(v1::TreeType::BigTree) => Ok(Self::BigTree),
            Ok(v1::TreeType::Redwood) => Ok(Self::Redwood),
            Ok(v1::TreeType::TallRedwood) => Ok(Self::TallRedwood),
            Ok(v1::TreeType::Birch) => Ok(Self::Birch),
            Ok(v1::TreeType::Jungle) => Ok(Self::Jungle),
            Ok(v1::TreeType::SmallJungle) => Ok(Self::SmallJungle),
            Ok(v1::TreeType::CocoaTree) => Ok(Self::CocoaTree),
            Ok(v1::TreeType::JungleBush) => Ok(Self::JungleBush),
            Ok(v1::TreeType::RedMushroom) => Ok(Self::RedMushroom),
            Ok(v1::TreeType::BrownMushroom) => Ok(Self::BrownMushroom),
            Ok(v1::TreeType::Swamp) => Ok(Self::Swamp),
            Ok(v1::TreeType::Acacia) => Ok(Self::Acacia),
            Ok(v1::TreeType::DarkOak) => Ok(Self::DarkOak),
            Ok(v1::TreeType::MegaRedwood) => Ok(Self::MegaRedwood),
            Ok(v1::TreeType::MegaPine) => Ok(Self::MegaPine),
            Ok(v1::TreeType::TallBirch) => Ok(Self::TallBirch),
            Ok(v1::TreeType::ChorusPlant) => Ok(Self::ChorusPlant),
            Ok(v1::TreeType::CrimsonFungus) => Ok(Self::CrimsonFungus),
            Ok(v1::TreeType::WarpedFungus) => Ok(Self::WarpedFungus),
            Ok(v1::TreeType::Azalea) => Ok(Self::Azalea),
            Ok(v1::TreeType::Mangrove) => Ok(Self::Mangrove),
            Ok(v1::TreeType::TallMangrove) => Ok(Self::TallMangrove),
            Ok(v1::TreeType::Cherry) => Ok(Self::Cherry),
            Ok(v1::TreeType::PaleOak) => Ok(Self::PaleOak),
            Ok(v1::TreeType::PaleOakCreaking) => Ok(Self::PaleOakCreaking),
            Ok(v1::TreeType::RedPoplar) => Ok(Self::RedPoplar),
            Ok(v1::TreeType::OrangePoplar) => Ok(Self::OrangePoplar),
            Ok(v1::TreeType::YellowPoplar) => Ok(Self::YellowPoplar),
            Ok(v1::TreeType::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown tree type {value}")))
            }
        }
    }
}

impl VillagerCareerChangeReason {
    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::VillagerCareerChangeReason::try_from(value) {
            Ok(v1::VillagerCareerChangeReason::LosingJob) => Ok(Self::LosingJob),
            Ok(v1::VillagerCareerChangeReason::Employed) => Ok(Self::Employed),
            Ok(v1::VillagerCareerChangeReason::Unspecified) | Err(_) => Err(Error::Malformed(
                format!("an unknown villager career change reason {value}"),
            )),
        }
    }
}
