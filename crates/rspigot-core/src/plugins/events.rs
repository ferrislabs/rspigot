use crate::plugins::BlockSnapshot;
use crate::plugins::BlockState;
use crate::plugins::BookMeta;
use crate::plugins::BoundingBox;
use crate::plugins::ChunkRef;
use crate::plugins::DamageSourceView;
use crate::plugins::DroppedItem;
use crate::plugins::EffectSpec;
use crate::plugins::EnchantOffer;
use crate::plugins::EntityView;
use crate::plugins::HolderRef;
use crate::plugins::Input;
use crate::plugins::InventoryRef;
use crate::plugins::InventoryView;
use crate::plugins::ItemSpec;
use crate::plugins::LecternData;
use crate::plugins::Location;
use crate::plugins::LootContext;
use crate::plugins::MerchantRef;
use crate::plugins::OwnerRef;
use crate::plugins::PlayerSnapshot;
use crate::plugins::PluginInfo;
use crate::plugins::SenderView;
use crate::plugins::SignData;
use crate::plugins::SpawnerData;
use crate::plugins::Trade;
use crate::plugins::TrialSpawnerData;
use crate::plugins::patch::{
    ListDiff, SetDiff, icon_refusal, location_refusal, merge_indexed, number_refusal, text_refusal,
    vector_refusal,
};
use rspigot_domain::BlockPos;
use rspigot_domain::Vec3;
use rspigot_domain::player::GameMode;
use std::net::IpAddr;
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub const ALL: [Self; 251] = [
        Self::AreaEffectCloudApply,
        Self::ArrowBodyCountChange,
        Self::AsyncPlayerChat,
        Self::AsyncPlayerPreLogin,
        Self::AsyncStructureGenerate,
        Self::AsyncStructureSpawn,
        Self::BatToggleSleep,
        Self::BellResonate,
        Self::BellRing,
        Self::BlockBreak,
        Self::BlockBrush,
        Self::BlockBurn,
        Self::BlockCanBuild,
        Self::BlockCook,
        Self::BlockDamage,
        Self::BlockDamageAbort,
        Self::BlockDispense,
        Self::BlockDispenseArmor,
        Self::BlockDispenseLoot,
        Self::BlockDropItem,
        Self::BlockExp,
        Self::BlockExplode,
        Self::BlockFade,
        Self::BlockFertilize,
        Self::BlockForm,
        Self::BlockFromTo,
        Self::BlockGrow,
        Self::BlockIgnite,
        Self::BlockMultiPlace,
        Self::BlockPhysics,
        Self::BlockPistonExtend,
        Self::BlockPistonRetract,
        Self::BlockPlace,
        Self::BlockReceiveGame,
        Self::BlockRedstone,
        Self::BlockShearEntity,
        Self::BlockSpread,
        Self::Brew,
        Self::BrewingStandFuel,
        Self::BrewingStart,
        Self::BroadcastMessage,
        Self::CampfireStart,
        Self::CauldronLevelChange,
        Self::ChunkLoad,
        Self::ChunkPopulate,
        Self::ChunkUnload,
        Self::CraftItem,
        Self::CrafterCraft,
        Self::CreatureSpawn,
        Self::CreeperPower,
        Self::CubeMobSplit,
        Self::EnchantItem,
        Self::EnderDragonChangePhase,
        Self::EntitiesLoad,
        Self::EntitiesUnload,
        Self::EntityAirChange,
        Self::EntityBlockForm,
        Self::EntityBreakDoor,
        Self::EntityBreed,
        Self::EntityChangeBlock,
        Self::EntityCombust,
        Self::EntityCombustByBlock,
        Self::EntityCombustByEntity,
        Self::EntityDamage,
        Self::EntityDamageByBlock,
        Self::EntityDamageByEntity,
        Self::EntityDeath,
        Self::EntityDismount,
        Self::EntityDropItem,
        Self::EntityEnterBlock,
        Self::EntityEnterLoveMode,
        Self::EntityExhaustion,
        Self::EntityExplode,
        Self::EntityInteract,
        Self::EntityKnockback,
        Self::EntityKnockbackByEntity,
        Self::EntityMount,
        Self::EntityPickupItem,
        Self::EntityPlace,
        Self::EntityPortal,
        Self::EntityPortalEnter,
        Self::EntityPortalExit,
        Self::EntityPoseChange,
        Self::EntityPotionEffect,
        Self::EntityRegainHealth,
        Self::EntityRemove,
        Self::EntityResurrect,
        Self::EntityShootBow,
        Self::EntitySpawn,
        Self::EntitySpellCast,
        Self::EntityTame,
        Self::EntityTarget,
        Self::EntityTargetBlock,
        Self::EntityTargetLivingEntity,
        Self::EntityTeleport,
        Self::EntityToggleGlide,
        Self::EntityToggleSwim,
        Self::EntityTransform,
        Self::EntityUnleash,
        Self::ExpBottle,
        Self::ExplosionPrime,
        Self::FireworkExplode,
        Self::FluidLevelChange,
        Self::FoodLevelChange,
        Self::FurnaceBurn,
        Self::FurnaceExtract,
        Self::FurnaceSmelt,
        Self::FurnaceStartSmelt,
        Self::GenericGame,
        Self::HangingBreak,
        Self::HangingBreakByEntity,
        Self::HangingPlace,
        Self::HopperInventorySearch,
        Self::HorseJump,
        Self::InventoryClick,
        Self::InventoryClose,
        Self::InventoryCreative,
        Self::InventoryDrag,
        Self::InventoryMoveItem,
        Self::InventoryOpen,
        Self::InventoryPickupItem,
        Self::ItemDespawn,
        Self::ItemMerge,
        Self::ItemSpawn,
        Self::LeavesDecay,
        Self::LightningStrike,
        Self::LingeringPotionSplash,
        Self::LootGenerate,
        Self::MapInitialize,
        Self::MoistureChange,
        Self::NotePlay,
        Self::PigZap,
        Self::PigZombieAnger,
        Self::PiglinBarter,
        Self::PlayerAdvancementCriterionGrant,
        Self::PlayerAdvancementDone,
        Self::PlayerAnimation,
        Self::PlayerArmorStandManipulate,
        Self::PlayerBedEnter,
        Self::PlayerBedLeave,
        Self::PlayerBucketEmpty,
        Self::PlayerBucketEntity,
        Self::PlayerBucketFill,
        Self::PlayerChangedMainHand,
        Self::PlayerChangedWorld,
        Self::PlayerCommandPreprocess,
        Self::PlayerCommandSend,
        Self::PlayerCustomClick,
        Self::PlayerDeath,
        Self::PlayerDropItem,
        Self::PlayerEditBook,
        Self::PlayerEggThrow,
        Self::PlayerExpChange,
        Self::PlayerExpCooldownChange,
        Self::PlayerFish,
        Self::PlayerGameModeChange,
        Self::PlayerHarvestBlock,
        Self::PlayerHideEntity,
        Self::PlayerInput,
        Self::PlayerInteract,
        Self::PlayerInteractAtEntity,
        Self::PlayerInteractEntity,
        Self::PlayerItemBreak,
        Self::PlayerItemConsume,
        Self::PlayerItemDamage,
        Self::PlayerItemHeld,
        Self::PlayerItemMend,
        Self::PlayerJoin,
        Self::PlayerKick,
        Self::PlayerLeashEntity,
        Self::PlayerLeave,
        Self::PlayerLevelChange,
        Self::PlayerLinksSend,
        Self::PlayerLocaleChange,
        Self::PlayerLogin,
        Self::PlayerMove,
        Self::PlayerPickupArrow,
        Self::PlayerPortal,
        Self::PlayerRecipeBookClick,
        Self::PlayerRecipeBookSettingsChange,
        Self::PlayerRecipeDiscover,
        Self::PlayerResourcePackStatus,
        Self::PlayerRespawn,
        Self::PlayerRiptide,
        Self::PlayerShearEntity,
        Self::PlayerShowEntity,
        Self::PlayerSignOpen,
        Self::PlayerSpawnChange,
        Self::PlayerStatisticIncrement,
        Self::PlayerSwapHandItems,
        Self::PlayerTakeLecternBook,
        Self::PlayerTeleport,
        Self::PlayerToggleFlight,
        Self::PlayerToggleSneak,
        Self::PlayerToggleSprint,
        Self::PlayerUnleashEntity,
        Self::PlayerVelocity,
        Self::PluginDisable,
        Self::PluginEnable,
        Self::PortalCreate,
        Self::PotionSplash,
        Self::PrepareAnvil,
        Self::PrepareGrindstone,
        Self::PrepareItemCraft,
        Self::PrepareItemEnchant,
        Self::PrepareSmithing,
        Self::ProjectileHit,
        Self::ProjectileLaunch,
        Self::RaidFinish,
        Self::RaidSpawnWave,
        Self::RaidStop,
        Self::RaidTrigger,
        Self::RemoteServerCommand,
        Self::SculkBloom,
        Self::ServerCommand,
        Self::ServerListPing,
        Self::ServerLoad,
        Self::SheepDyeWool,
        Self::SheepRegrowWool,
        Self::SignChange,
        Self::SmithItem,
        Self::SpawnChange,
        Self::SpawnerSpawn,
        Self::SpongeAbsorb,
        Self::StriderTemperatureChange,
        Self::StructureGrow,
        Self::TNTPrime,
        Self::TabComplete,
        Self::ThunderChange,
        Self::TimeSkip,
        Self::TradeSelect,
        Self::TrialSpawnerSpawn,
        Self::VaultDisplayItem,
        Self::VehicleBlockCollision,
        Self::VehicleCreate,
        Self::VehicleDamage,
        Self::VehicleDestroy,
        Self::VehicleEnter,
        Self::VehicleEntityCollision,
        Self::VehicleExit,
        Self::VehicleMove,
        Self::VehicleUpdate,
        Self::VillagerAcquireTrade,
        Self::VillagerCareerChange,
        Self::VillagerReplenishTrade,
        Self::VillagerReputationChange,
        Self::WeatherChange,
        Self::WorldInit,
        Self::WorldLoad,
        Self::WorldSave,
        Self::WorldUnload,
    ];

    pub const fn cancellable(self) -> bool {
        matches!(
            self,
            Self::AreaEffectCloudApply
                | Self::ArrowBodyCountChange
                | Self::AsyncPlayerChat
                | Self::AsyncStructureSpawn
                | Self::BatToggleSleep
                | Self::BellRing
                | Self::BlockBreak
                | Self::BlockBrush
                | Self::BlockBurn
                | Self::BlockCook
                | Self::BlockDamage
                | Self::BlockDispense
                | Self::BlockDispenseArmor
                | Self::BlockDispenseLoot
                | Self::BlockDropItem
                | Self::BlockExplode
                | Self::BlockFade
                | Self::BlockFertilize
                | Self::BlockForm
                | Self::BlockFromTo
                | Self::BlockGrow
                | Self::BlockIgnite
                | Self::BlockMultiPlace
                | Self::BlockPhysics
                | Self::BlockPistonExtend
                | Self::BlockPistonRetract
                | Self::BlockPlace
                | Self::BlockReceiveGame
                | Self::BlockShearEntity
                | Self::BlockSpread
                | Self::Brew
                | Self::BrewingStandFuel
                | Self::BroadcastMessage
                | Self::CauldronLevelChange
                | Self::CraftItem
                | Self::CrafterCraft
                | Self::CreatureSpawn
                | Self::CreeperPower
                | Self::CubeMobSplit
                | Self::EnchantItem
                | Self::EnderDragonChangePhase
                | Self::EntityAirChange
                | Self::EntityBlockForm
                | Self::EntityBreakDoor
                | Self::EntityBreed
                | Self::EntityChangeBlock
                | Self::EntityCombust
                | Self::EntityCombustByBlock
                | Self::EntityCombustByEntity
                | Self::EntityDamage
                | Self::EntityDamageByBlock
                | Self::EntityDamageByEntity
                | Self::EntityDismount
                | Self::EntityDropItem
                | Self::EntityEnterBlock
                | Self::EntityEnterLoveMode
                | Self::EntityExhaustion
                | Self::EntityExplode
                | Self::EntityInteract
                | Self::EntityKnockback
                | Self::EntityKnockbackByEntity
                | Self::EntityMount
                | Self::EntityPickupItem
                | Self::EntityPlace
                | Self::EntityPortal
                | Self::EntityPortalExit
                | Self::EntityPotionEffect
                | Self::EntityRegainHealth
                | Self::EntityResurrect
                | Self::EntityShootBow
                | Self::EntitySpawn
                | Self::EntitySpellCast
                | Self::EntityTame
                | Self::EntityTarget
                | Self::EntityTargetBlock
                | Self::EntityTargetLivingEntity
                | Self::EntityTeleport
                | Self::EntityToggleGlide
                | Self::EntityToggleSwim
                | Self::EntityTransform
                | Self::ExpBottle
                | Self::ExplosionPrime
                | Self::FireworkExplode
                | Self::FluidLevelChange
                | Self::FoodLevelChange
                | Self::FurnaceBurn
                | Self::FurnaceSmelt
                | Self::GenericGame
                | Self::HangingBreak
                | Self::HangingBreakByEntity
                | Self::HangingPlace
                | Self::HorseJump
                | Self::InventoryClick
                | Self::InventoryCreative
                | Self::InventoryDrag
                | Self::InventoryMoveItem
                | Self::InventoryOpen
                | Self::InventoryPickupItem
                | Self::ItemDespawn
                | Self::ItemMerge
                | Self::ItemSpawn
                | Self::LeavesDecay
                | Self::LightningStrike
                | Self::LingeringPotionSplash
                | Self::LootGenerate
                | Self::MoistureChange
                | Self::NotePlay
                | Self::PigZap
                | Self::PigZombieAnger
                | Self::PiglinBarter
                | Self::PlayerAdvancementCriterionGrant
                | Self::PlayerAnimation
                | Self::PlayerArmorStandManipulate
                | Self::PlayerBedEnter
                | Self::PlayerBedLeave
                | Self::PlayerBucketEmpty
                | Self::PlayerBucketEntity
                | Self::PlayerBucketFill
                | Self::PlayerCommandPreprocess
                | Self::PlayerDropItem
                | Self::PlayerEditBook
                | Self::PlayerFish
                | Self::PlayerGameModeChange
                | Self::PlayerHarvestBlock
                | Self::PlayerInteract
                | Self::PlayerInteractAtEntity
                | Self::PlayerInteractEntity
                | Self::PlayerItemConsume
                | Self::PlayerItemDamage
                | Self::PlayerItemHeld
                | Self::PlayerItemMend
                | Self::PlayerKick
                | Self::PlayerLeashEntity
                | Self::PlayerMove
                | Self::PlayerPickupArrow
                | Self::PlayerPortal
                | Self::PlayerRecipeDiscover
                | Self::PlayerShearEntity
                | Self::PlayerSignOpen
                | Self::PlayerSpawnChange
                | Self::PlayerStatisticIncrement
                | Self::PlayerSwapHandItems
                | Self::PlayerTakeLecternBook
                | Self::PlayerTeleport
                | Self::PlayerToggleFlight
                | Self::PlayerToggleSneak
                | Self::PlayerToggleSprint
                | Self::PlayerUnleashEntity
                | Self::PlayerVelocity
                | Self::PortalCreate
                | Self::PotionSplash
                | Self::PrepareItemEnchant
                | Self::ProjectileHit
                | Self::ProjectileLaunch
                | Self::RaidTrigger
                | Self::RemoteServerCommand
                | Self::SculkBloom
                | Self::ServerCommand
                | Self::SheepDyeWool
                | Self::SheepRegrowWool
                | Self::SignChange
                | Self::SmithItem
                | Self::SpawnerSpawn
                | Self::SpongeAbsorb
                | Self::StriderTemperatureChange
                | Self::StructureGrow
                | Self::TNTPrime
                | Self::TabComplete
                | Self::ThunderChange
                | Self::TimeSkip
                | Self::TradeSelect
                | Self::TrialSpawnerSpawn
                | Self::VaultDisplayItem
                | Self::VehicleCreate
                | Self::VehicleDamage
                | Self::VehicleDestroy
                | Self::VehicleEnter
                | Self::VehicleEntityCollision
                | Self::VehicleExit
                | Self::VillagerAcquireTrade
                | Self::VillagerCareerChange
                | Self::VillagerReplenishTrade
                | Self::VillagerReputationChange
                | Self::WeatherChange
                | Self::WorldUnload
        )
    }

    pub const fn decidable(self) -> bool {
        matches!(
            self,
            Self::AreaEffectCloudApply
                | Self::ArrowBodyCountChange
                | Self::AsyncPlayerChat
                | Self::AsyncPlayerPreLogin
                | Self::AsyncStructureSpawn
                | Self::BatToggleSleep
                | Self::BellRing
                | Self::BlockBreak
                | Self::BlockBrush
                | Self::BlockBurn
                | Self::BlockCanBuild
                | Self::BlockCook
                | Self::BlockDamage
                | Self::BlockDispense
                | Self::BlockDispenseArmor
                | Self::BlockDispenseLoot
                | Self::BlockDropItem
                | Self::BlockExp
                | Self::BlockExplode
                | Self::BlockFade
                | Self::BlockFertilize
                | Self::BlockForm
                | Self::BlockFromTo
                | Self::BlockGrow
                | Self::BlockIgnite
                | Self::BlockMultiPlace
                | Self::BlockPhysics
                | Self::BlockPistonExtend
                | Self::BlockPistonRetract
                | Self::BlockPlace
                | Self::BlockReceiveGame
                | Self::BlockRedstone
                | Self::BlockShearEntity
                | Self::BlockSpread
                | Self::Brew
                | Self::BrewingStandFuel
                | Self::BrewingStart
                | Self::BroadcastMessage
                | Self::CampfireStart
                | Self::CauldronLevelChange
                | Self::ChunkUnload
                | Self::CraftItem
                | Self::CrafterCraft
                | Self::CreatureSpawn
                | Self::CreeperPower
                | Self::CubeMobSplit
                | Self::EnchantItem
                | Self::EnderDragonChangePhase
                | Self::EntityAirChange
                | Self::EntityBlockForm
                | Self::EntityBreakDoor
                | Self::EntityBreed
                | Self::EntityChangeBlock
                | Self::EntityCombust
                | Self::EntityCombustByBlock
                | Self::EntityCombustByEntity
                | Self::EntityDamage
                | Self::EntityDamageByBlock
                | Self::EntityDamageByEntity
                | Self::EntityDeath
                | Self::EntityDismount
                | Self::EntityDropItem
                | Self::EntityEnterBlock
                | Self::EntityEnterLoveMode
                | Self::EntityExhaustion
                | Self::EntityExplode
                | Self::EntityInteract
                | Self::EntityKnockback
                | Self::EntityKnockbackByEntity
                | Self::EntityMount
                | Self::EntityPickupItem
                | Self::EntityPlace
                | Self::EntityPortal
                | Self::EntityPortalExit
                | Self::EntityPotionEffect
                | Self::EntityRegainHealth
                | Self::EntityResurrect
                | Self::EntityShootBow
                | Self::EntitySpawn
                | Self::EntitySpellCast
                | Self::EntityTame
                | Self::EntityTarget
                | Self::EntityTargetBlock
                | Self::EntityTargetLivingEntity
                | Self::EntityTeleport
                | Self::EntityToggleGlide
                | Self::EntityToggleSwim
                | Self::EntityTransform
                | Self::ExpBottle
                | Self::ExplosionPrime
                | Self::FireworkExplode
                | Self::FluidLevelChange
                | Self::FoodLevelChange
                | Self::FurnaceBurn
                | Self::FurnaceExtract
                | Self::FurnaceSmelt
                | Self::FurnaceStartSmelt
                | Self::GenericGame
                | Self::HangingBreak
                | Self::HangingBreakByEntity
                | Self::HangingPlace
                | Self::HopperInventorySearch
                | Self::HorseJump
                | Self::InventoryClick
                | Self::InventoryCreative
                | Self::InventoryDrag
                | Self::InventoryMoveItem
                | Self::InventoryOpen
                | Self::InventoryPickupItem
                | Self::ItemDespawn
                | Self::ItemMerge
                | Self::ItemSpawn
                | Self::LeavesDecay
                | Self::LightningStrike
                | Self::LingeringPotionSplash
                | Self::LootGenerate
                | Self::MoistureChange
                | Self::NotePlay
                | Self::PigZap
                | Self::PigZombieAnger
                | Self::PiglinBarter
                | Self::PlayerAdvancementCriterionGrant
                | Self::PlayerAnimation
                | Self::PlayerArmorStandManipulate
                | Self::PlayerBedEnter
                | Self::PlayerBedLeave
                | Self::PlayerBucketEmpty
                | Self::PlayerBucketEntity
                | Self::PlayerBucketFill
                | Self::PlayerCommandPreprocess
                | Self::PlayerCommandSend
                | Self::PlayerDeath
                | Self::PlayerDropItem
                | Self::PlayerEditBook
                | Self::PlayerEggThrow
                | Self::PlayerExpChange
                | Self::PlayerExpCooldownChange
                | Self::PlayerFish
                | Self::PlayerGameModeChange
                | Self::PlayerHarvestBlock
                | Self::PlayerInteract
                | Self::PlayerInteractAtEntity
                | Self::PlayerInteractEntity
                | Self::PlayerItemConsume
                | Self::PlayerItemDamage
                | Self::PlayerItemHeld
                | Self::PlayerItemMend
                | Self::PlayerJoin
                | Self::PlayerKick
                | Self::PlayerLeashEntity
                | Self::PlayerLeave
                | Self::PlayerLogin
                | Self::PlayerMove
                | Self::PlayerPickupArrow
                | Self::PlayerPortal
                | Self::PlayerRecipeBookClick
                | Self::PlayerRecipeDiscover
                | Self::PlayerRespawn
                | Self::PlayerShearEntity
                | Self::PlayerSignOpen
                | Self::PlayerSpawnChange
                | Self::PlayerStatisticIncrement
                | Self::PlayerSwapHandItems
                | Self::PlayerTakeLecternBook
                | Self::PlayerTeleport
                | Self::PlayerToggleFlight
                | Self::PlayerToggleSneak
                | Self::PlayerToggleSprint
                | Self::PlayerUnleashEntity
                | Self::PlayerVelocity
                | Self::PortalCreate
                | Self::PotionSplash
                | Self::PrepareAnvil
                | Self::PrepareGrindstone
                | Self::PrepareItemCraft
                | Self::PrepareItemEnchant
                | Self::PrepareSmithing
                | Self::ProjectileHit
                | Self::ProjectileLaunch
                | Self::RaidTrigger
                | Self::RemoteServerCommand
                | Self::SculkBloom
                | Self::ServerCommand
                | Self::ServerListPing
                | Self::SheepDyeWool
                | Self::SheepRegrowWool
                | Self::SignChange
                | Self::SmithItem
                | Self::SpawnerSpawn
                | Self::SpongeAbsorb
                | Self::StriderTemperatureChange
                | Self::StructureGrow
                | Self::TNTPrime
                | Self::TabComplete
                | Self::ThunderChange
                | Self::TimeSkip
                | Self::TradeSelect
                | Self::TrialSpawnerSpawn
                | Self::VaultDisplayItem
                | Self::VehicleCreate
                | Self::VehicleDamage
                | Self::VehicleDestroy
                | Self::VehicleEnter
                | Self::VehicleEntityCollision
                | Self::VehicleExit
                | Self::VillagerAcquireTrade
                | Self::VillagerCareerChange
                | Self::VillagerReplenishTrade
                | Self::VillagerReputationChange
                | Self::WeatherChange
                | Self::WorldUnload
        )
    }

    pub const fn writes_when_cancelled(self) -> bool {
        matches!(
            self,
            Self::CraftItem | Self::InventoryClick | Self::InventoryCreative | Self::SmithItem
        )
    }

    pub const fn cancels_by_result(self) -> bool {
        matches!(
            self,
            Self::CraftItem
                | Self::InventoryClick
                | Self::InventoryCreative
                | Self::InventoryDrag
                | Self::PlayerInteract
                | Self::SmithItem
                | Self::TradeSelect
        )
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::AreaEffectCloudApply => "AreaEffectCloudApply",
            Self::ArrowBodyCountChange => "ArrowBodyCountChange",
            Self::AsyncPlayerChat => "AsyncPlayerChat",
            Self::AsyncPlayerPreLogin => "AsyncPlayerPreLogin",
            Self::AsyncStructureGenerate => "AsyncStructureGenerate",
            Self::AsyncStructureSpawn => "AsyncStructureSpawn",
            Self::BatToggleSleep => "BatToggleSleep",
            Self::BellResonate => "BellResonate",
            Self::BellRing => "BellRing",
            Self::BlockBreak => "BlockBreak",
            Self::BlockBrush => "BlockBrush",
            Self::BlockBurn => "BlockBurn",
            Self::BlockCanBuild => "BlockCanBuild",
            Self::BlockCook => "BlockCook",
            Self::BlockDamage => "BlockDamage",
            Self::BlockDamageAbort => "BlockDamageAbort",
            Self::BlockDispense => "BlockDispense",
            Self::BlockDispenseArmor => "BlockDispenseArmor",
            Self::BlockDispenseLoot => "BlockDispenseLoot",
            Self::BlockDropItem => "BlockDropItem",
            Self::BlockExp => "BlockExp",
            Self::BlockExplode => "BlockExplode",
            Self::BlockFade => "BlockFade",
            Self::BlockFertilize => "BlockFertilize",
            Self::BlockForm => "BlockForm",
            Self::BlockFromTo => "BlockFromTo",
            Self::BlockGrow => "BlockGrow",
            Self::BlockIgnite => "BlockIgnite",
            Self::BlockMultiPlace => "BlockMultiPlace",
            Self::BlockPhysics => "BlockPhysics",
            Self::BlockPistonExtend => "BlockPistonExtend",
            Self::BlockPistonRetract => "BlockPistonRetract",
            Self::BlockPlace => "BlockPlace",
            Self::BlockReceiveGame => "BlockReceiveGame",
            Self::BlockRedstone => "BlockRedstone",
            Self::BlockShearEntity => "BlockShearEntity",
            Self::BlockSpread => "BlockSpread",
            Self::Brew => "Brew",
            Self::BrewingStandFuel => "BrewingStandFuel",
            Self::BrewingStart => "BrewingStart",
            Self::BroadcastMessage => "BroadcastMessage",
            Self::CampfireStart => "CampfireStart",
            Self::CauldronLevelChange => "CauldronLevelChange",
            Self::ChunkLoad => "ChunkLoad",
            Self::ChunkPopulate => "ChunkPopulate",
            Self::ChunkUnload => "ChunkUnload",
            Self::CraftItem => "CraftItem",
            Self::CrafterCraft => "CrafterCraft",
            Self::CreatureSpawn => "CreatureSpawn",
            Self::CreeperPower => "CreeperPower",
            Self::CubeMobSplit => "CubeMobSplit",
            Self::EnchantItem => "EnchantItem",
            Self::EnderDragonChangePhase => "EnderDragonChangePhase",
            Self::EntitiesLoad => "EntitiesLoad",
            Self::EntitiesUnload => "EntitiesUnload",
            Self::EntityAirChange => "EntityAirChange",
            Self::EntityBlockForm => "EntityBlockForm",
            Self::EntityBreakDoor => "EntityBreakDoor",
            Self::EntityBreed => "EntityBreed",
            Self::EntityChangeBlock => "EntityChangeBlock",
            Self::EntityCombust => "EntityCombust",
            Self::EntityCombustByBlock => "EntityCombustByBlock",
            Self::EntityCombustByEntity => "EntityCombustByEntity",
            Self::EntityDamage => "EntityDamage",
            Self::EntityDamageByBlock => "EntityDamageByBlock",
            Self::EntityDamageByEntity => "EntityDamageByEntity",
            Self::EntityDeath => "EntityDeath",
            Self::EntityDismount => "EntityDismount",
            Self::EntityDropItem => "EntityDropItem",
            Self::EntityEnterBlock => "EntityEnterBlock",
            Self::EntityEnterLoveMode => "EntityEnterLoveMode",
            Self::EntityExhaustion => "EntityExhaustion",
            Self::EntityExplode => "EntityExplode",
            Self::EntityInteract => "EntityInteract",
            Self::EntityKnockback => "EntityKnockback",
            Self::EntityKnockbackByEntity => "EntityKnockbackByEntity",
            Self::EntityMount => "EntityMount",
            Self::EntityPickupItem => "EntityPickupItem",
            Self::EntityPlace => "EntityPlace",
            Self::EntityPortal => "EntityPortal",
            Self::EntityPortalEnter => "EntityPortalEnter",
            Self::EntityPortalExit => "EntityPortalExit",
            Self::EntityPoseChange => "EntityPoseChange",
            Self::EntityPotionEffect => "EntityPotionEffect",
            Self::EntityRegainHealth => "EntityRegainHealth",
            Self::EntityRemove => "EntityRemove",
            Self::EntityResurrect => "EntityResurrect",
            Self::EntityShootBow => "EntityShootBow",
            Self::EntitySpawn => "EntitySpawn",
            Self::EntitySpellCast => "EntitySpellCast",
            Self::EntityTame => "EntityTame",
            Self::EntityTarget => "EntityTarget",
            Self::EntityTargetBlock => "EntityTargetBlock",
            Self::EntityTargetLivingEntity => "EntityTargetLivingEntity",
            Self::EntityTeleport => "EntityTeleport",
            Self::EntityToggleGlide => "EntityToggleGlide",
            Self::EntityToggleSwim => "EntityToggleSwim",
            Self::EntityTransform => "EntityTransform",
            Self::EntityUnleash => "EntityUnleash",
            Self::ExpBottle => "ExpBottle",
            Self::ExplosionPrime => "ExplosionPrime",
            Self::FireworkExplode => "FireworkExplode",
            Self::FluidLevelChange => "FluidLevelChange",
            Self::FoodLevelChange => "FoodLevelChange",
            Self::FurnaceBurn => "FurnaceBurn",
            Self::FurnaceExtract => "FurnaceExtract",
            Self::FurnaceSmelt => "FurnaceSmelt",
            Self::FurnaceStartSmelt => "FurnaceStartSmelt",
            Self::GenericGame => "GenericGame",
            Self::HangingBreak => "HangingBreak",
            Self::HangingBreakByEntity => "HangingBreakByEntity",
            Self::HangingPlace => "HangingPlace",
            Self::HopperInventorySearch => "HopperInventorySearch",
            Self::HorseJump => "HorseJump",
            Self::InventoryClick => "InventoryClick",
            Self::InventoryClose => "InventoryClose",
            Self::InventoryCreative => "InventoryCreative",
            Self::InventoryDrag => "InventoryDrag",
            Self::InventoryMoveItem => "InventoryMoveItem",
            Self::InventoryOpen => "InventoryOpen",
            Self::InventoryPickupItem => "InventoryPickupItem",
            Self::ItemDespawn => "ItemDespawn",
            Self::ItemMerge => "ItemMerge",
            Self::ItemSpawn => "ItemSpawn",
            Self::LeavesDecay => "LeavesDecay",
            Self::LightningStrike => "LightningStrike",
            Self::LingeringPotionSplash => "LingeringPotionSplash",
            Self::LootGenerate => "LootGenerate",
            Self::MapInitialize => "MapInitialize",
            Self::MoistureChange => "MoistureChange",
            Self::NotePlay => "NotePlay",
            Self::PigZap => "PigZap",
            Self::PigZombieAnger => "PigZombieAnger",
            Self::PiglinBarter => "PiglinBarter",
            Self::PlayerAdvancementCriterionGrant => "PlayerAdvancementCriterionGrant",
            Self::PlayerAdvancementDone => "PlayerAdvancementDone",
            Self::PlayerAnimation => "PlayerAnimation",
            Self::PlayerArmorStandManipulate => "PlayerArmorStandManipulate",
            Self::PlayerBedEnter => "PlayerBedEnter",
            Self::PlayerBedLeave => "PlayerBedLeave",
            Self::PlayerBucketEmpty => "PlayerBucketEmpty",
            Self::PlayerBucketEntity => "PlayerBucketEntity",
            Self::PlayerBucketFill => "PlayerBucketFill",
            Self::PlayerChangedMainHand => "PlayerChangedMainHand",
            Self::PlayerChangedWorld => "PlayerChangedWorld",
            Self::PlayerCommandPreprocess => "PlayerCommandPreprocess",
            Self::PlayerCommandSend => "PlayerCommandSend",
            Self::PlayerCustomClick => "PlayerCustomClick",
            Self::PlayerDeath => "PlayerDeath",
            Self::PlayerDropItem => "PlayerDropItem",
            Self::PlayerEditBook => "PlayerEditBook",
            Self::PlayerEggThrow => "PlayerEggThrow",
            Self::PlayerExpChange => "PlayerExpChange",
            Self::PlayerExpCooldownChange => "PlayerExpCooldownChange",
            Self::PlayerFish => "PlayerFish",
            Self::PlayerGameModeChange => "PlayerGameModeChange",
            Self::PlayerHarvestBlock => "PlayerHarvestBlock",
            Self::PlayerHideEntity => "PlayerHideEntity",
            Self::PlayerInput => "PlayerInput",
            Self::PlayerInteract => "PlayerInteract",
            Self::PlayerInteractAtEntity => "PlayerInteractAtEntity",
            Self::PlayerInteractEntity => "PlayerInteractEntity",
            Self::PlayerItemBreak => "PlayerItemBreak",
            Self::PlayerItemConsume => "PlayerItemConsume",
            Self::PlayerItemDamage => "PlayerItemDamage",
            Self::PlayerItemHeld => "PlayerItemHeld",
            Self::PlayerItemMend => "PlayerItemMend",
            Self::PlayerJoin => "PlayerJoin",
            Self::PlayerKick => "PlayerKick",
            Self::PlayerLeashEntity => "PlayerLeashEntity",
            Self::PlayerLeave => "PlayerLeave",
            Self::PlayerLevelChange => "PlayerLevelChange",
            Self::PlayerLinksSend => "PlayerLinksSend",
            Self::PlayerLocaleChange => "PlayerLocaleChange",
            Self::PlayerLogin => "PlayerLogin",
            Self::PlayerMove => "PlayerMove",
            Self::PlayerPickupArrow => "PlayerPickupArrow",
            Self::PlayerPortal => "PlayerPortal",
            Self::PlayerRecipeBookClick => "PlayerRecipeBookClick",
            Self::PlayerRecipeBookSettingsChange => "PlayerRecipeBookSettingsChange",
            Self::PlayerRecipeDiscover => "PlayerRecipeDiscover",
            Self::PlayerResourcePackStatus => "PlayerResourcePackStatus",
            Self::PlayerRespawn => "PlayerRespawn",
            Self::PlayerRiptide => "PlayerRiptide",
            Self::PlayerShearEntity => "PlayerShearEntity",
            Self::PlayerShowEntity => "PlayerShowEntity",
            Self::PlayerSignOpen => "PlayerSignOpen",
            Self::PlayerSpawnChange => "PlayerSpawnChange",
            Self::PlayerStatisticIncrement => "PlayerStatisticIncrement",
            Self::PlayerSwapHandItems => "PlayerSwapHandItems",
            Self::PlayerTakeLecternBook => "PlayerTakeLecternBook",
            Self::PlayerTeleport => "PlayerTeleport",
            Self::PlayerToggleFlight => "PlayerToggleFlight",
            Self::PlayerToggleSneak => "PlayerToggleSneak",
            Self::PlayerToggleSprint => "PlayerToggleSprint",
            Self::PlayerUnleashEntity => "PlayerUnleashEntity",
            Self::PlayerVelocity => "PlayerVelocity",
            Self::PluginDisable => "PluginDisable",
            Self::PluginEnable => "PluginEnable",
            Self::PortalCreate => "PortalCreate",
            Self::PotionSplash => "PotionSplash",
            Self::PrepareAnvil => "PrepareAnvil",
            Self::PrepareGrindstone => "PrepareGrindstone",
            Self::PrepareItemCraft => "PrepareItemCraft",
            Self::PrepareItemEnchant => "PrepareItemEnchant",
            Self::PrepareSmithing => "PrepareSmithing",
            Self::ProjectileHit => "ProjectileHit",
            Self::ProjectileLaunch => "ProjectileLaunch",
            Self::RaidFinish => "RaidFinish",
            Self::RaidSpawnWave => "RaidSpawnWave",
            Self::RaidStop => "RaidStop",
            Self::RaidTrigger => "RaidTrigger",
            Self::RemoteServerCommand => "RemoteServerCommand",
            Self::SculkBloom => "SculkBloom",
            Self::ServerCommand => "ServerCommand",
            Self::ServerListPing => "ServerListPing",
            Self::ServerLoad => "ServerLoad",
            Self::SheepDyeWool => "SheepDyeWool",
            Self::SheepRegrowWool => "SheepRegrowWool",
            Self::SignChange => "SignChange",
            Self::SmithItem => "SmithItem",
            Self::SpawnChange => "SpawnChange",
            Self::SpawnerSpawn => "SpawnerSpawn",
            Self::SpongeAbsorb => "SpongeAbsorb",
            Self::StriderTemperatureChange => "StriderTemperatureChange",
            Self::StructureGrow => "StructureGrow",
            Self::TNTPrime => "TNTPrime",
            Self::TabComplete => "TabComplete",
            Self::ThunderChange => "ThunderChange",
            Self::TimeSkip => "TimeSkip",
            Self::TradeSelect => "TradeSelect",
            Self::TrialSpawnerSpawn => "TrialSpawnerSpawn",
            Self::VaultDisplayItem => "VaultDisplayItem",
            Self::VehicleBlockCollision => "VehicleBlockCollision",
            Self::VehicleCreate => "VehicleCreate",
            Self::VehicleDamage => "VehicleDamage",
            Self::VehicleDestroy => "VehicleDestroy",
            Self::VehicleEnter => "VehicleEnter",
            Self::VehicleEntityCollision => "VehicleEntityCollision",
            Self::VehicleExit => "VehicleExit",
            Self::VehicleMove => "VehicleMove",
            Self::VehicleUpdate => "VehicleUpdate",
            Self::VillagerAcquireTrade => "VillagerAcquireTrade",
            Self::VillagerCareerChange => "VillagerCareerChange",
            Self::VillagerReplenishTrade => "VillagerReplenishTrade",
            Self::VillagerReputationChange => "VillagerReputationChange",
            Self::WeatherChange => "WeatherChange",
            Self::WorldInit => "WorldInit",
            Self::WorldLoad => "WorldLoad",
            Self::WorldSave => "WorldSave",
            Self::WorldUnload => "WorldUnload",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::AreaEffectCloudApply => "area_effect_cloud_apply",
            Self::ArrowBodyCountChange => "arrow_body_count_change",
            Self::AsyncPlayerChat => "async_player_chat",
            Self::AsyncPlayerPreLogin => "async_player_pre_login",
            Self::AsyncStructureGenerate => "async_structure_generate",
            Self::AsyncStructureSpawn => "async_structure_spawn",
            Self::BatToggleSleep => "bat_toggle_sleep",
            Self::BellResonate => "bell_resonate",
            Self::BellRing => "bell_ring",
            Self::BlockBreak => "block_break",
            Self::BlockBrush => "block_brush",
            Self::BlockBurn => "block_burn",
            Self::BlockCanBuild => "block_can_build",
            Self::BlockCook => "block_cook",
            Self::BlockDamage => "block_damage",
            Self::BlockDamageAbort => "block_damage_abort",
            Self::BlockDispense => "block_dispense",
            Self::BlockDispenseArmor => "block_dispense_armor",
            Self::BlockDispenseLoot => "block_dispense_loot",
            Self::BlockDropItem => "block_drop_item",
            Self::BlockExp => "block_exp",
            Self::BlockExplode => "block_explode",
            Self::BlockFade => "block_fade",
            Self::BlockFertilize => "block_fertilize",
            Self::BlockForm => "block_form",
            Self::BlockFromTo => "block_from_to",
            Self::BlockGrow => "block_grow",
            Self::BlockIgnite => "block_ignite",
            Self::BlockMultiPlace => "block_multi_place",
            Self::BlockPhysics => "block_physics",
            Self::BlockPistonExtend => "block_piston_extend",
            Self::BlockPistonRetract => "block_piston_retract",
            Self::BlockPlace => "block_place",
            Self::BlockReceiveGame => "block_receive_game",
            Self::BlockRedstone => "block_redstone",
            Self::BlockShearEntity => "block_shear_entity",
            Self::BlockSpread => "block_spread",
            Self::Brew => "brew",
            Self::BrewingStandFuel => "brewing_stand_fuel",
            Self::BrewingStart => "brewing_start",
            Self::BroadcastMessage => "broadcast_message",
            Self::CampfireStart => "campfire_start",
            Self::CauldronLevelChange => "cauldron_level_change",
            Self::ChunkLoad => "chunk_load",
            Self::ChunkPopulate => "chunk_populate",
            Self::ChunkUnload => "chunk_unload",
            Self::CraftItem => "craft_item",
            Self::CrafterCraft => "crafter_craft",
            Self::CreatureSpawn => "creature_spawn",
            Self::CreeperPower => "creeper_power",
            Self::CubeMobSplit => "cube_mob_split",
            Self::EnchantItem => "enchant_item",
            Self::EnderDragonChangePhase => "ender_dragon_change_phase",
            Self::EntitiesLoad => "entities_load",
            Self::EntitiesUnload => "entities_unload",
            Self::EntityAirChange => "entity_air_change",
            Self::EntityBlockForm => "entity_block_form",
            Self::EntityBreakDoor => "entity_break_door",
            Self::EntityBreed => "entity_breed",
            Self::EntityChangeBlock => "entity_change_block",
            Self::EntityCombust => "entity_combust",
            Self::EntityCombustByBlock => "entity_combust_by_block",
            Self::EntityCombustByEntity => "entity_combust_by_entity",
            Self::EntityDamage => "entity_damage",
            Self::EntityDamageByBlock => "entity_damage_by_block",
            Self::EntityDamageByEntity => "entity_damage_by_entity",
            Self::EntityDeath => "entity_death",
            Self::EntityDismount => "entity_dismount",
            Self::EntityDropItem => "entity_drop_item",
            Self::EntityEnterBlock => "entity_enter_block",
            Self::EntityEnterLoveMode => "entity_enter_love_mode",
            Self::EntityExhaustion => "entity_exhaustion",
            Self::EntityExplode => "entity_explode",
            Self::EntityInteract => "entity_interact",
            Self::EntityKnockback => "entity_knockback",
            Self::EntityKnockbackByEntity => "entity_knockback_by_entity",
            Self::EntityMount => "entity_mount",
            Self::EntityPickupItem => "entity_pickup_item",
            Self::EntityPlace => "entity_place",
            Self::EntityPortal => "entity_portal",
            Self::EntityPortalEnter => "entity_portal_enter",
            Self::EntityPortalExit => "entity_portal_exit",
            Self::EntityPoseChange => "entity_pose_change",
            Self::EntityPotionEffect => "entity_potion_effect",
            Self::EntityRegainHealth => "entity_regain_health",
            Self::EntityRemove => "entity_remove",
            Self::EntityResurrect => "entity_resurrect",
            Self::EntityShootBow => "entity_shoot_bow",
            Self::EntitySpawn => "entity_spawn",
            Self::EntitySpellCast => "entity_spell_cast",
            Self::EntityTame => "entity_tame",
            Self::EntityTarget => "entity_target",
            Self::EntityTargetBlock => "entity_target_block",
            Self::EntityTargetLivingEntity => "entity_target_living_entity",
            Self::EntityTeleport => "entity_teleport",
            Self::EntityToggleGlide => "entity_toggle_glide",
            Self::EntityToggleSwim => "entity_toggle_swim",
            Self::EntityTransform => "entity_transform",
            Self::EntityUnleash => "entity_unleash",
            Self::ExpBottle => "exp_bottle",
            Self::ExplosionPrime => "explosion_prime",
            Self::FireworkExplode => "firework_explode",
            Self::FluidLevelChange => "fluid_level_change",
            Self::FoodLevelChange => "food_level_change",
            Self::FurnaceBurn => "furnace_burn",
            Self::FurnaceExtract => "furnace_extract",
            Self::FurnaceSmelt => "furnace_smelt",
            Self::FurnaceStartSmelt => "furnace_start_smelt",
            Self::GenericGame => "generic_game",
            Self::HangingBreak => "hanging_break",
            Self::HangingBreakByEntity => "hanging_break_by_entity",
            Self::HangingPlace => "hanging_place",
            Self::HopperInventorySearch => "hopper_inventory_search",
            Self::HorseJump => "horse_jump",
            Self::InventoryClick => "inventory_click",
            Self::InventoryClose => "inventory_close",
            Self::InventoryCreative => "inventory_creative",
            Self::InventoryDrag => "inventory_drag",
            Self::InventoryMoveItem => "inventory_move_item",
            Self::InventoryOpen => "inventory_open",
            Self::InventoryPickupItem => "inventory_pickup_item",
            Self::ItemDespawn => "item_despawn",
            Self::ItemMerge => "item_merge",
            Self::ItemSpawn => "item_spawn",
            Self::LeavesDecay => "leaves_decay",
            Self::LightningStrike => "lightning_strike",
            Self::LingeringPotionSplash => "lingering_potion_splash",
            Self::LootGenerate => "loot_generate",
            Self::MapInitialize => "map_initialize",
            Self::MoistureChange => "moisture_change",
            Self::NotePlay => "note_play",
            Self::PigZap => "pig_zap",
            Self::PigZombieAnger => "pig_zombie_anger",
            Self::PiglinBarter => "piglin_barter",
            Self::PlayerAdvancementCriterionGrant => "player_advancement_criterion_grant",
            Self::PlayerAdvancementDone => "player_advancement_done",
            Self::PlayerAnimation => "player_animation",
            Self::PlayerArmorStandManipulate => "player_armor_stand_manipulate",
            Self::PlayerBedEnter => "player_bed_enter",
            Self::PlayerBedLeave => "player_bed_leave",
            Self::PlayerBucketEmpty => "player_bucket_empty",
            Self::PlayerBucketEntity => "player_bucket_entity",
            Self::PlayerBucketFill => "player_bucket_fill",
            Self::PlayerChangedMainHand => "player_changed_main_hand",
            Self::PlayerChangedWorld => "player_changed_world",
            Self::PlayerCommandPreprocess => "player_command_preprocess",
            Self::PlayerCommandSend => "player_command_send",
            Self::PlayerCustomClick => "player_custom_click",
            Self::PlayerDeath => "player_death",
            Self::PlayerDropItem => "player_drop_item",
            Self::PlayerEditBook => "player_edit_book",
            Self::PlayerEggThrow => "player_egg_throw",
            Self::PlayerExpChange => "player_exp_change",
            Self::PlayerExpCooldownChange => "player_exp_cooldown_change",
            Self::PlayerFish => "player_fish",
            Self::PlayerGameModeChange => "player_game_mode_change",
            Self::PlayerHarvestBlock => "player_harvest_block",
            Self::PlayerHideEntity => "player_hide_entity",
            Self::PlayerInput => "player_input",
            Self::PlayerInteract => "player_interact",
            Self::PlayerInteractAtEntity => "player_interact_at_entity",
            Self::PlayerInteractEntity => "player_interact_entity",
            Self::PlayerItemBreak => "player_item_break",
            Self::PlayerItemConsume => "player_item_consume",
            Self::PlayerItemDamage => "player_item_damage",
            Self::PlayerItemHeld => "player_item_held",
            Self::PlayerItemMend => "player_item_mend",
            Self::PlayerJoin => "player_join",
            Self::PlayerKick => "player_kick",
            Self::PlayerLeashEntity => "player_leash_entity",
            Self::PlayerLeave => "player_leave",
            Self::PlayerLevelChange => "player_level_change",
            Self::PlayerLinksSend => "player_links_send",
            Self::PlayerLocaleChange => "player_locale_change",
            Self::PlayerLogin => "player_login",
            Self::PlayerMove => "player_move",
            Self::PlayerPickupArrow => "player_pickup_arrow",
            Self::PlayerPortal => "player_portal",
            Self::PlayerRecipeBookClick => "player_recipe_book_click",
            Self::PlayerRecipeBookSettingsChange => "player_recipe_book_settings_change",
            Self::PlayerRecipeDiscover => "player_recipe_discover",
            Self::PlayerResourcePackStatus => "player_resource_pack_status",
            Self::PlayerRespawn => "player_respawn",
            Self::PlayerRiptide => "player_riptide",
            Self::PlayerShearEntity => "player_shear_entity",
            Self::PlayerShowEntity => "player_show_entity",
            Self::PlayerSignOpen => "player_sign_open",
            Self::PlayerSpawnChange => "player_spawn_change",
            Self::PlayerStatisticIncrement => "player_statistic_increment",
            Self::PlayerSwapHandItems => "player_swap_hand_items",
            Self::PlayerTakeLecternBook => "player_take_lectern_book",
            Self::PlayerTeleport => "player_teleport",
            Self::PlayerToggleFlight => "player_toggle_flight",
            Self::PlayerToggleSneak => "player_toggle_sneak",
            Self::PlayerToggleSprint => "player_toggle_sprint",
            Self::PlayerUnleashEntity => "player_unleash_entity",
            Self::PlayerVelocity => "player_velocity",
            Self::PluginDisable => "plugin_disable",
            Self::PluginEnable => "plugin_enable",
            Self::PortalCreate => "portal_create",
            Self::PotionSplash => "potion_splash",
            Self::PrepareAnvil => "prepare_anvil",
            Self::PrepareGrindstone => "prepare_grindstone",
            Self::PrepareItemCraft => "prepare_item_craft",
            Self::PrepareItemEnchant => "prepare_item_enchant",
            Self::PrepareSmithing => "prepare_smithing",
            Self::ProjectileHit => "projectile_hit",
            Self::ProjectileLaunch => "projectile_launch",
            Self::RaidFinish => "raid_finish",
            Self::RaidSpawnWave => "raid_spawn_wave",
            Self::RaidStop => "raid_stop",
            Self::RaidTrigger => "raid_trigger",
            Self::RemoteServerCommand => "remote_server_command",
            Self::SculkBloom => "sculk_bloom",
            Self::ServerCommand => "server_command",
            Self::ServerListPing => "server_list_ping",
            Self::ServerLoad => "server_load",
            Self::SheepDyeWool => "sheep_dye_wool",
            Self::SheepRegrowWool => "sheep_regrow_wool",
            Self::SignChange => "sign_change",
            Self::SmithItem => "smith_item",
            Self::SpawnChange => "spawn_change",
            Self::SpawnerSpawn => "spawner_spawn",
            Self::SpongeAbsorb => "sponge_absorb",
            Self::StriderTemperatureChange => "strider_temperature_change",
            Self::StructureGrow => "structure_grow",
            Self::TNTPrime => "tnt_prime",
            Self::TabComplete => "tab_complete",
            Self::ThunderChange => "thunder_change",
            Self::TimeSkip => "time_skip",
            Self::TradeSelect => "trade_select",
            Self::TrialSpawnerSpawn => "trial_spawner_spawn",
            Self::VaultDisplayItem => "vault_display_item",
            Self::VehicleBlockCollision => "vehicle_block_collision",
            Self::VehicleCreate => "vehicle_create",
            Self::VehicleDamage => "vehicle_damage",
            Self::VehicleDestroy => "vehicle_destroy",
            Self::VehicleEnter => "vehicle_enter",
            Self::VehicleEntityCollision => "vehicle_entity_collision",
            Self::VehicleExit => "vehicle_exit",
            Self::VehicleMove => "vehicle_move",
            Self::VehicleUpdate => "vehicle_update",
            Self::VillagerAcquireTrade => "villager_acquire_trade",
            Self::VillagerCareerChange => "villager_career_change",
            Self::VillagerReplenishTrade => "villager_replenish_trade",
            Self::VillagerReputationChange => "villager_reputation_change",
            Self::WeatherChange => "weather_change",
            Self::WorldInit => "world_init",
            Self::WorldLoad => "world_load",
            Self::WorldSave => "world_save",
            Self::WorldUnload => "world_unload",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    pub const fn receivers(self) -> &'static [Self] {
        match self {
            Self::AreaEffectCloudApply => &[Self::AreaEffectCloudApply],
            Self::ArrowBodyCountChange => &[Self::ArrowBodyCountChange],
            Self::AsyncPlayerChat => &[Self::AsyncPlayerChat],
            Self::AsyncPlayerPreLogin => &[Self::AsyncPlayerPreLogin],
            Self::AsyncStructureGenerate => &[Self::AsyncStructureGenerate],
            Self::AsyncStructureSpawn => &[Self::AsyncStructureSpawn],
            Self::BatToggleSleep => &[Self::BatToggleSleep],
            Self::BellResonate => &[Self::BellResonate],
            Self::BellRing => &[Self::BellRing],
            Self::BlockBreak => &[Self::BlockBreak, Self::BlockExp],
            Self::BlockBrush => &[Self::BlockBrush],
            Self::BlockBurn => &[Self::BlockBurn],
            Self::BlockCanBuild => &[Self::BlockCanBuild],
            Self::BlockCook => &[Self::BlockCook],
            Self::BlockDamage => &[Self::BlockDamage],
            Self::BlockDamageAbort => &[Self::BlockDamageAbort],
            Self::BlockDispense => &[Self::BlockDispense],
            Self::BlockDispenseArmor => &[Self::BlockDispense, Self::BlockDispenseArmor],
            Self::BlockDispenseLoot => &[Self::BlockDispenseLoot],
            Self::BlockDropItem => &[Self::BlockDropItem],
            Self::BlockExp => &[Self::BlockExp],
            Self::BlockExplode => &[Self::BlockExplode],
            Self::BlockFade => &[Self::BlockFade],
            Self::BlockFertilize => &[Self::BlockFertilize],
            Self::BlockForm => &[Self::BlockForm],
            Self::BlockFromTo => &[Self::BlockFromTo],
            Self::BlockGrow => &[Self::BlockGrow],
            Self::BlockIgnite => &[Self::BlockIgnite],
            Self::BlockMultiPlace => &[Self::BlockMultiPlace, Self::BlockPlace],
            Self::BlockPhysics => &[Self::BlockPhysics],
            Self::BlockPistonExtend => &[Self::BlockPistonExtend],
            Self::BlockPistonRetract => &[Self::BlockPistonRetract],
            Self::BlockPlace => &[Self::BlockPlace],
            Self::BlockReceiveGame => &[Self::BlockReceiveGame],
            Self::BlockRedstone => &[Self::BlockRedstone],
            Self::BlockShearEntity => &[Self::BlockShearEntity],
            Self::BlockSpread => &[Self::BlockSpread],
            Self::Brew => &[Self::Brew],
            Self::BrewingStandFuel => &[Self::BrewingStandFuel],
            Self::BrewingStart => &[Self::BrewingStart],
            Self::BroadcastMessage => &[Self::BroadcastMessage],
            Self::CampfireStart => &[Self::CampfireStart],
            Self::CauldronLevelChange => &[Self::CauldronLevelChange],
            Self::ChunkLoad => &[Self::ChunkLoad],
            Self::ChunkPopulate => &[Self::ChunkPopulate],
            Self::ChunkUnload => &[Self::ChunkUnload],
            Self::CraftItem => &[Self::CraftItem, Self::InventoryClick],
            Self::CrafterCraft => &[Self::CrafterCraft],
            Self::CreatureSpawn => &[Self::CreatureSpawn, Self::EntitySpawn],
            Self::CreeperPower => &[Self::CreeperPower],
            Self::CubeMobSplit => &[Self::CubeMobSplit],
            Self::EnchantItem => &[Self::EnchantItem],
            Self::EnderDragonChangePhase => &[Self::EnderDragonChangePhase],
            Self::EntitiesLoad => &[Self::EntitiesLoad],
            Self::EntitiesUnload => &[Self::EntitiesUnload],
            Self::EntityAirChange => &[Self::EntityAirChange],
            Self::EntityBlockForm => &[Self::BlockForm, Self::EntityBlockForm],
            Self::EntityBreakDoor => &[Self::EntityBreakDoor, Self::EntityChangeBlock],
            Self::EntityBreed => &[Self::EntityBreed],
            Self::EntityChangeBlock => &[Self::EntityChangeBlock],
            Self::EntityCombust => &[Self::EntityCombust],
            Self::EntityCombustByBlock => &[Self::EntityCombust, Self::EntityCombustByBlock],
            Self::EntityCombustByEntity => &[Self::EntityCombust, Self::EntityCombustByEntity],
            Self::EntityDamage => &[Self::EntityDamage],
            Self::EntityDamageByBlock => &[Self::EntityDamage, Self::EntityDamageByBlock],
            Self::EntityDamageByEntity => &[Self::EntityDamage, Self::EntityDamageByEntity],
            Self::EntityDeath => &[Self::EntityDeath],
            Self::EntityDismount => &[Self::EntityDismount],
            Self::EntityDropItem => &[Self::EntityDropItem],
            Self::EntityEnterBlock => &[Self::EntityEnterBlock],
            Self::EntityEnterLoveMode => &[Self::EntityEnterLoveMode],
            Self::EntityExhaustion => &[Self::EntityExhaustion],
            Self::EntityExplode => &[Self::EntityExplode],
            Self::EntityInteract => &[Self::EntityInteract],
            Self::EntityKnockback => &[Self::EntityKnockback],
            Self::EntityKnockbackByEntity => {
                &[Self::EntityKnockback, Self::EntityKnockbackByEntity]
            }
            Self::EntityMount => &[Self::EntityMount],
            Self::EntityPickupItem => &[Self::EntityPickupItem],
            Self::EntityPlace => &[Self::EntityPlace],
            Self::EntityPortal => &[Self::EntityPortal],
            Self::EntityPortalEnter => &[Self::EntityPortalEnter],
            Self::EntityPortalExit => &[Self::EntityPortalExit],
            Self::EntityPoseChange => &[Self::EntityPoseChange],
            Self::EntityPotionEffect => &[Self::EntityPotionEffect],
            Self::EntityRegainHealth => &[Self::EntityRegainHealth],
            Self::EntityRemove => &[Self::EntityRemove],
            Self::EntityResurrect => &[Self::EntityResurrect],
            Self::EntityShootBow => &[Self::EntityShootBow],
            Self::EntitySpawn => &[Self::EntitySpawn],
            Self::EntitySpellCast => &[Self::EntitySpellCast],
            Self::EntityTame => &[Self::EntityTame],
            Self::EntityTarget => &[Self::EntityTarget],
            Self::EntityTargetBlock => &[Self::EntityTargetBlock],
            Self::EntityTargetLivingEntity => &[Self::EntityTarget, Self::EntityTargetLivingEntity],
            Self::EntityTeleport => &[Self::EntityTeleport],
            Self::EntityToggleGlide => &[Self::EntityToggleGlide],
            Self::EntityToggleSwim => &[Self::EntityToggleSwim],
            Self::EntityTransform => &[Self::EntityTransform],
            Self::EntityUnleash => &[Self::EntityUnleash],
            Self::ExpBottle => &[Self::ExpBottle],
            Self::ExplosionPrime => &[Self::ExplosionPrime],
            Self::FireworkExplode => &[Self::FireworkExplode],
            Self::FluidLevelChange => &[Self::FluidLevelChange],
            Self::FoodLevelChange => &[Self::FoodLevelChange],
            Self::FurnaceBurn => &[Self::FurnaceBurn],
            Self::FurnaceExtract => &[Self::BlockExp, Self::FurnaceExtract],
            Self::FurnaceSmelt => &[Self::BlockCook, Self::FurnaceSmelt],
            Self::FurnaceStartSmelt => &[Self::FurnaceStartSmelt],
            Self::GenericGame => &[Self::GenericGame],
            Self::HangingBreak => &[Self::HangingBreak],
            Self::HangingBreakByEntity => &[Self::HangingBreak, Self::HangingBreakByEntity],
            Self::HangingPlace => &[Self::HangingPlace],
            Self::HopperInventorySearch => &[Self::HopperInventorySearch],
            Self::HorseJump => &[Self::HorseJump],
            Self::InventoryClick => &[Self::InventoryClick],
            Self::InventoryClose => &[Self::InventoryClose],
            Self::InventoryCreative => &[Self::InventoryClick, Self::InventoryCreative],
            Self::InventoryDrag => &[Self::InventoryDrag],
            Self::InventoryMoveItem => &[Self::InventoryMoveItem],
            Self::InventoryOpen => &[Self::InventoryOpen],
            Self::InventoryPickupItem => &[Self::InventoryPickupItem],
            Self::ItemDespawn => &[Self::ItemDespawn],
            Self::ItemMerge => &[Self::ItemMerge],
            Self::ItemSpawn => &[Self::EntitySpawn, Self::ItemSpawn],
            Self::LeavesDecay => &[Self::LeavesDecay],
            Self::LightningStrike => &[Self::LightningStrike],
            Self::LingeringPotionSplash => &[Self::LingeringPotionSplash],
            Self::LootGenerate => &[Self::LootGenerate],
            Self::MapInitialize => &[Self::MapInitialize],
            Self::MoistureChange => &[Self::MoistureChange],
            Self::NotePlay => &[Self::NotePlay],
            Self::PigZap => &[Self::PigZap],
            Self::PigZombieAnger => &[Self::PigZombieAnger],
            Self::PiglinBarter => &[Self::PiglinBarter],
            Self::PlayerAdvancementCriterionGrant => &[Self::PlayerAdvancementCriterionGrant],
            Self::PlayerAdvancementDone => &[Self::PlayerAdvancementDone],
            Self::PlayerAnimation => &[Self::PlayerAnimation],
            Self::PlayerArmorStandManipulate => &[Self::PlayerArmorStandManipulate],
            Self::PlayerBedEnter => &[Self::PlayerBedEnter],
            Self::PlayerBedLeave => &[Self::PlayerBedLeave],
            Self::PlayerBucketEmpty => &[Self::PlayerBucketEmpty],
            Self::PlayerBucketEntity => &[Self::PlayerBucketEntity],
            Self::PlayerBucketFill => &[Self::PlayerBucketFill],
            Self::PlayerChangedMainHand => &[Self::PlayerChangedMainHand],
            Self::PlayerChangedWorld => &[Self::PlayerChangedWorld],
            Self::PlayerCommandPreprocess => &[Self::PlayerCommandPreprocess],
            Self::PlayerCommandSend => &[Self::PlayerCommandSend],
            Self::PlayerCustomClick => &[Self::PlayerCustomClick],
            Self::PlayerDeath => &[Self::EntityDeath, Self::PlayerDeath],
            Self::PlayerDropItem => &[Self::PlayerDropItem],
            Self::PlayerEditBook => &[Self::PlayerEditBook],
            Self::PlayerEggThrow => &[Self::PlayerEggThrow],
            Self::PlayerExpChange => &[Self::PlayerExpChange],
            Self::PlayerExpCooldownChange => &[Self::PlayerExpCooldownChange],
            Self::PlayerFish => &[Self::PlayerFish],
            Self::PlayerGameModeChange => &[Self::PlayerGameModeChange],
            Self::PlayerHarvestBlock => &[Self::PlayerHarvestBlock],
            Self::PlayerHideEntity => &[Self::PlayerHideEntity],
            Self::PlayerInput => &[Self::PlayerInput],
            Self::PlayerInteract => &[Self::PlayerInteract],
            Self::PlayerInteractAtEntity => {
                &[Self::PlayerInteractAtEntity, Self::PlayerInteractEntity]
            }
            Self::PlayerInteractEntity => &[Self::PlayerInteractEntity],
            Self::PlayerItemBreak => &[Self::PlayerItemBreak],
            Self::PlayerItemConsume => &[Self::PlayerItemConsume],
            Self::PlayerItemDamage => &[Self::PlayerItemDamage],
            Self::PlayerItemHeld => &[Self::PlayerItemHeld],
            Self::PlayerItemMend => &[Self::PlayerItemMend],
            Self::PlayerJoin => &[Self::PlayerJoin],
            Self::PlayerKick => &[Self::PlayerKick],
            Self::PlayerLeashEntity => &[Self::PlayerLeashEntity],
            Self::PlayerLeave => &[Self::PlayerLeave],
            Self::PlayerLevelChange => &[Self::PlayerLevelChange],
            Self::PlayerLinksSend => &[Self::PlayerLinksSend],
            Self::PlayerLocaleChange => &[Self::PlayerLocaleChange],
            Self::PlayerLogin => &[Self::PlayerLogin],
            Self::PlayerMove => &[Self::PlayerMove],
            Self::PlayerPickupArrow => &[Self::PlayerPickupArrow],
            Self::PlayerPortal => &[Self::PlayerPortal],
            Self::PlayerRecipeBookClick => &[Self::PlayerRecipeBookClick],
            Self::PlayerRecipeBookSettingsChange => &[Self::PlayerRecipeBookSettingsChange],
            Self::PlayerRecipeDiscover => &[Self::PlayerRecipeDiscover],
            Self::PlayerResourcePackStatus => &[Self::PlayerResourcePackStatus],
            Self::PlayerRespawn => &[Self::PlayerRespawn],
            Self::PlayerRiptide => &[Self::PlayerRiptide],
            Self::PlayerShearEntity => &[Self::PlayerShearEntity],
            Self::PlayerShowEntity => &[Self::PlayerShowEntity],
            Self::PlayerSignOpen => &[Self::PlayerSignOpen],
            Self::PlayerSpawnChange => &[Self::PlayerSpawnChange],
            Self::PlayerStatisticIncrement => &[Self::PlayerStatisticIncrement],
            Self::PlayerSwapHandItems => &[Self::PlayerSwapHandItems],
            Self::PlayerTakeLecternBook => &[Self::PlayerTakeLecternBook],
            Self::PlayerTeleport => &[Self::PlayerTeleport],
            Self::PlayerToggleFlight => &[Self::PlayerToggleFlight],
            Self::PlayerToggleSneak => &[Self::PlayerToggleSneak],
            Self::PlayerToggleSprint => &[Self::PlayerToggleSprint],
            Self::PlayerUnleashEntity => &[Self::EntityUnleash, Self::PlayerUnleashEntity],
            Self::PlayerVelocity => &[Self::PlayerVelocity],
            Self::PluginDisable => &[Self::PluginDisable],
            Self::PluginEnable => &[Self::PluginEnable],
            Self::PortalCreate => &[Self::PortalCreate],
            Self::PotionSplash => &[Self::PotionSplash],
            Self::PrepareAnvil => &[Self::PrepareAnvil],
            Self::PrepareGrindstone => &[Self::PrepareGrindstone],
            Self::PrepareItemCraft => &[Self::PrepareItemCraft],
            Self::PrepareItemEnchant => &[Self::PrepareItemEnchant],
            Self::PrepareSmithing => &[Self::PrepareSmithing],
            Self::ProjectileHit => &[Self::ProjectileHit],
            Self::ProjectileLaunch => &[Self::EntitySpawn, Self::ProjectileLaunch],
            Self::RaidFinish => &[Self::RaidFinish],
            Self::RaidSpawnWave => &[Self::RaidSpawnWave],
            Self::RaidStop => &[Self::RaidStop],
            Self::RaidTrigger => &[Self::RaidTrigger],
            Self::RemoteServerCommand => &[Self::RemoteServerCommand],
            Self::SculkBloom => &[Self::SculkBloom],
            Self::ServerCommand => &[Self::ServerCommand],
            Self::ServerListPing => &[Self::ServerListPing],
            Self::ServerLoad => &[Self::ServerLoad],
            Self::SheepDyeWool => &[Self::SheepDyeWool],
            Self::SheepRegrowWool => &[Self::SheepRegrowWool],
            Self::SignChange => &[Self::SignChange],
            Self::SmithItem => &[Self::InventoryClick, Self::SmithItem],
            Self::SpawnChange => &[Self::SpawnChange],
            Self::SpawnerSpawn => &[Self::EntitySpawn, Self::SpawnerSpawn],
            Self::SpongeAbsorb => &[Self::SpongeAbsorb],
            Self::StriderTemperatureChange => &[Self::StriderTemperatureChange],
            Self::StructureGrow => &[Self::StructureGrow],
            Self::TNTPrime => &[Self::TNTPrime],
            Self::TabComplete => &[Self::TabComplete],
            Self::ThunderChange => &[Self::ThunderChange],
            Self::TimeSkip => &[Self::TimeSkip],
            Self::TradeSelect => &[Self::TradeSelect],
            Self::TrialSpawnerSpawn => &[Self::EntitySpawn, Self::TrialSpawnerSpawn],
            Self::VaultDisplayItem => &[Self::VaultDisplayItem],
            Self::VehicleBlockCollision => &[Self::VehicleBlockCollision],
            Self::VehicleCreate => &[Self::VehicleCreate],
            Self::VehicleDamage => &[Self::VehicleDamage],
            Self::VehicleDestroy => &[Self::VehicleDestroy],
            Self::VehicleEnter => &[Self::VehicleEnter],
            Self::VehicleEntityCollision => &[Self::VehicleEntityCollision],
            Self::VehicleExit => &[Self::VehicleExit],
            Self::VehicleMove => &[Self::VehicleMove],
            Self::VehicleUpdate => &[Self::VehicleUpdate],
            Self::VillagerAcquireTrade => &[Self::VillagerAcquireTrade],
            Self::VillagerCareerChange => &[Self::VillagerCareerChange],
            Self::VillagerReplenishTrade => &[Self::VillagerReplenishTrade],
            Self::VillagerReputationChange => &[Self::VillagerReputationChange],
            Self::WeatherChange => &[Self::WeatherChange],
            Self::WorldInit => &[Self::WorldInit],
            Self::WorldLoad => &[Self::WorldLoad],
            Self::WorldSave => &[Self::WorldSave],
            Self::WorldUnload => &[Self::WorldUnload],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AreaEffectCloudApply {
    pub affected_entities: Vec<EntityView>,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrowBodyCountChange {
    pub entity: EntityView,
    pub entity_type: String,
    pub new_amount: i32,
    pub old_amount: i32,
    pub reset: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncPlayerChat {
    pub format: String,
    pub message: String,
    pub player: PlayerSnapshot,
    pub recipients: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncPlayerPreLogin {
    pub address: IpAddr,
    pub kick_message: String,
    pub login_result: AsyncPlayerPreLoginResult,
    pub name: String,
    pub transferred: bool,
    pub unique_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncStructureGenerate {
    pub bounding_box: BoundingBox,
    pub cause: AsyncStructureGenerateCause,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub structure: String,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AsyncStructureSpawn {
    pub bounding_box: BoundingBox,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub structure: String,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatToggleSleep {
    pub awake: bool,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BellResonate {
    pub block: BlockPos,
    pub resonated_entities: Vec<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BellRing {
    pub block: BlockPos,
    pub direction: Face,
    pub entity: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockBreak {
    pub block: BlockPos,
    pub drop_items: bool,
    pub exp_to_drop: i32,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockBrush {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockBurn {
    pub block: BlockPos,
    pub igniting_block: Option<BlockPos>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockCanBuild {
    pub block: BlockPos,
    pub block_data: BlockState,
    pub buildable: bool,
    pub material: String,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockCook {
    pub block: BlockPos,
    pub result: ItemSpec,
    pub source: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDamage {
    pub block: BlockPos,
    pub insta_break: bool,
    pub item_in_hand: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDamageAbort {
    pub block: BlockPos,
    pub item_in_hand: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDispense {
    pub block: BlockPos,
    pub item: ItemSpec,
    pub velocity: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDispenseArmor {
    pub block: BlockPos,
    pub item: ItemSpec,
    pub target_entity: EntityView,
    pub velocity: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDispenseLoot {
    pub block: BlockPos,
    pub dispensed_loot: Vec<ItemSpec>,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDropItem {
    pub block: BlockPos,
    pub block_state: BlockSnapshot,
    pub items: Vec<DroppedItem>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockExp {
    pub block: BlockPos,
    pub exp_to_drop: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockExplode {
    pub block: BlockPos,
    pub block_list: Vec<BlockPos>,
    pub exploded_block_state: BlockSnapshot,
    pub explosion_result: ExplosionResult,
    pub explosion_yield: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockFade {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockFertilize {
    pub block: BlockPos,
    pub blocks: Vec<BlockSnapshot>,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockForm {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockFromTo {
    pub block: BlockPos,
    pub face: Face,
    pub to_block: BlockPos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockGrow {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockIgnite {
    pub block: BlockPos,
    pub cause: BlockIgniteCause,
    pub igniting_block: Option<BlockPos>,
    pub igniting_entity: Option<EntityView>,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockMultiPlace {
    pub block: BlockPos,
    pub block_against: BlockPos,
    pub block_placed: BlockPos,
    pub block_replaced_state: BlockSnapshot,
    pub build: bool,
    pub hand: EquipmentSlot,
    pub item_in_hand: ItemSpec,
    pub player: PlayerSnapshot,
    pub replaced_block_states: Vec<BlockSnapshot>,
    pub placed_state: BlockSnapshot,
    pub placed_states: Vec<BlockSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockPhysics {
    pub block: BlockPos,
    pub changed_type: String,
    pub source_block: BlockPos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockPistonExtend {
    pub block: BlockPos,
    pub blocks: Vec<BlockPos>,
    pub direction: Face,
    pub sticky: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockPistonRetract {
    pub block: BlockPos,
    pub blocks: Vec<BlockPos>,
    pub direction: Face,
    pub sticky: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockPlace {
    pub block: BlockPos,
    pub block_against: BlockPos,
    pub block_placed: BlockPos,
    pub block_replaced_state: BlockSnapshot,
    pub build: bool,
    pub hand: EquipmentSlot,
    pub item_in_hand: ItemSpec,
    pub player: PlayerSnapshot,
    pub placed_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockReceiveGame {
    pub block: BlockPos,
    pub entity: Option<EntityView>,
    pub event: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockRedstone {
    pub block: BlockPos,
    pub new_current: i32,
    pub old_current: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockShearEntity {
    pub block: BlockPos,
    pub entity: EntityView,
    pub tool: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockSpread {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
    pub source: BlockPos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Brew {
    pub block: BlockPos,
    pub contents: InventoryRef,
    pub fuel_level: i32,
    pub results: Vec<ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrewingStandFuel {
    pub block: BlockPos,
    pub consuming: bool,
    pub fuel: ItemSpec,
    pub fuel_power: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrewingStart {
    pub block: BlockPos,
    pub source: ItemSpec,
    pub total_brew_time: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BroadcastMessage {
    pub message: String,
    pub recipients: Vec<SenderView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CampfireStart {
    pub block: BlockPos,
    pub recipe: String,
    pub source: ItemSpec,
    pub total_cook_time: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CauldronLevelChange {
    pub block: BlockPos,
    pub entity: Option<EntityView>,
    pub new_state: BlockSnapshot,
    pub reason: CauldronLevelChangeReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkLoad {
    pub chunk: ChunkRef,
    pub new_chunk: bool,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkPopulate {
    pub chunk: ChunkRef,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkUnload {
    pub chunk: ChunkRef,
    pub save_chunk: bool,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CraftItem {
    pub action: InventoryAction,
    pub click: ClickKind,
    pub clicked_inventory: Option<InventoryRef>,
    pub current_item: Option<ItemSpec>,
    pub cursor: Option<ItemSpec>,
    pub hotbar_button: i32,
    pub inventory: InventoryRef,
    pub left_click: bool,
    pub raw_slot: i32,
    pub recipe: String,
    pub result: EventResult,
    pub right_click: bool,
    pub shift_click: bool,
    pub slot: i32,
    pub slot_type: InventoryTypeSlotType,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrafterCraft {
    pub block: BlockPos,
    pub recipe: String,
    pub remaining_items: Vec<ItemSpec>,
    pub result: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreatureSpawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
    pub spawn_reason: CreatureSpawnReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreeperPower {
    pub cause: CreeperPowerCause,
    pub entity: EntityView,
    pub entity_type: String,
    pub lightning: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CubeMobSplit {
    pub count: i32,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnchantItem {
    pub enchant_block: BlockPos,
    pub enchanter: PlayerSnapshot,
    pub enchantment_hint: String,
    pub enchants_to_add: Vec<(String, i32)>,
    pub exp_level_cost: i32,
    pub inventory: InventoryRef,
    pub item: ItemSpec,
    pub level_hint: i32,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub which_button: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnderDragonChangePhase {
    pub current_phase: Option<EnderDragonPhase>,
    pub entity: EntityView,
    pub entity_type: String,
    pub new_phase: EnderDragonPhase,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntitiesLoad {
    pub chunk: ChunkRef,
    pub entities: Vec<EntityView>,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntitiesUnload {
    pub chunk: ChunkRef,
    pub entities: Vec<EntityView>,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityAirChange {
    pub amount: i32,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityBlockForm {
    pub block: BlockPos,
    pub entity: EntityView,
    pub new_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityBreakDoor {
    pub block: BlockPos,
    pub block_data: BlockState,
    pub entity: EntityView,
    pub entity_type: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityBreed {
    pub bred_with: Option<ItemSpec>,
    pub breeder: Option<EntityView>,
    pub entity: EntityView,
    pub entity_type: String,
    pub experience: i32,
    pub father: EntityView,
    pub mother: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityChangeBlock {
    pub block: BlockPos,
    pub block_data: BlockState,
    pub entity: EntityView,
    pub entity_type: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityCombust {
    pub duration: f32,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityCombustByBlock {
    pub combuster: Option<BlockPos>,
    pub duration: f32,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityCombustByEntity {
    pub combuster: EntityView,
    pub duration: f32,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDamage {
    pub cause: EntityDamageCause,
    pub damage: f64,
    pub damage_source: DamageSourceView,
    pub entity: EntityView,
    pub entity_type: String,
    pub final_damage: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDamageByBlock {
    pub cause: EntityDamageCause,
    pub damage: f64,
    pub damage_source: DamageSourceView,
    pub damager: Option<BlockPos>,
    pub damager_block_state: Option<BlockSnapshot>,
    pub entity: EntityView,
    pub entity_type: String,
    pub final_damage: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDamageByEntity {
    pub cause: EntityDamageCause,
    pub damage: f64,
    pub damage_source: DamageSourceView,
    pub damager: EntityView,
    pub entity: EntityView,
    pub entity_type: String,
    pub final_damage: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDeath {
    pub damage_source: DamageSourceView,
    pub dropped_exp: i32,
    pub drops: Vec<ItemSpec>,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDismount {
    pub dismounted: EntityView,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityDropItem {
    pub entity: EntityView,
    pub entity_type: String,
    pub item_drop: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityEnterBlock {
    pub block: BlockPos,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityEnterLoveMode {
    pub entity: EntityView,
    pub entity_type: String,
    pub human_entity: Option<PlayerSnapshot>,
    pub ticks_in_love: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityExhaustion {
    pub entity: PlayerSnapshot,
    pub entity_type: String,
    pub exhaustion: f32,
    pub exhaustion_reason: EntityExhaustionReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityExplode {
    pub block_list: Vec<BlockPos>,
    pub entity: EntityView,
    pub entity_type: String,
    pub explosion_result: ExplosionResult,
    pub location: Location,
    pub explosion_yield: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityInteract {
    pub block: BlockPos,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityKnockback {
    pub cause: EntityKnockbackCause,
    pub entity: EntityView,
    pub entity_type: String,
    pub final_knockback: Vec3,
    pub force: f64,
    pub knockback: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityKnockbackByEntity {
    pub cause: EntityKnockbackCause,
    pub entity: EntityView,
    pub entity_type: String,
    pub final_knockback: Vec3,
    pub force: f64,
    pub knockback: Vec3,
    pub source_entity: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityMount {
    pub entity: EntityView,
    pub entity_type: String,
    pub mount: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPickupItem {
    pub entity: EntityView,
    pub entity_type: String,
    pub item: EntityView,
    pub remaining: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPlace {
    pub block: BlockPos,
    pub block_face: Face,
    pub entity: EntityView,
    pub entity_type: String,
    pub hand: EquipmentSlot,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPortal {
    pub can_create_portal: bool,
    pub creation_radius: i32,
    pub entity: EntityView,
    pub entity_type: String,
    pub from: Location,
    pub search_radius: i32,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPortalEnter {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPortalExit {
    pub after: Vec3,
    pub before: Vec3,
    pub entity: EntityView,
    pub entity_type: String,
    pub from: Location,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPoseChange {
    pub entity: EntityView,
    pub entity_type: String,
    pub pose: Pose,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityPotionEffect {
    pub action: EntityPotionEffectAction,
    pub cause: EntityPotionEffectCause,
    pub entity: EntityView,
    pub entity_type: String,
    pub modified_type: String,
    pub new_effect: Option<EffectSpec>,
    pub old_effect: Option<EffectSpec>,
    pub overrides: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityRegainHealth {
    pub amount: f64,
    pub entity: EntityView,
    pub entity_type: String,
    pub regain_reason: EntityRegainHealthRegainReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityRemove {
    pub cause: EntityRemoveCause,
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityResurrect {
    pub entity: EntityView,
    pub entity_type: String,
    pub hand: Option<EquipmentSlot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityShootBow {
    pub bow: Option<ItemSpec>,
    pub consumable: Option<ItemSpec>,
    pub consume_item: bool,
    pub entity: EntityView,
    pub entity_type: String,
    pub force: f32,
    pub hand: EquipmentSlot,
    pub projectile: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntitySpawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntitySpellCast {
    pub entity: EntityView,
    pub entity_type: String,
    pub spell: SpellcasterSpell,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTame {
    pub entity: EntityView,
    pub entity_type: String,
    pub owner: OwnerRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTarget {
    pub entity: EntityView,
    pub entity_type: String,
    pub reason: EntityTargetReason,
    pub target: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTargetBlock {
    pub entity: EntityView,
    pub entity_type: String,
    pub target: Option<BlockPos>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTargetLivingEntity {
    pub entity: EntityView,
    pub entity_type: String,
    pub reason: EntityTargetReason,
    pub target: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTeleport {
    pub entity: EntityView,
    pub entity_type: String,
    pub from: Location,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityToggleGlide {
    pub entity: EntityView,
    pub entity_type: String,
    pub gliding: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityToggleSwim {
    pub entity: EntityView,
    pub entity_type: String,
    pub swimming: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTransform {
    pub entity: EntityView,
    pub entity_type: String,
    pub transform_reason: EntityTransformReason,
    pub transformed_entities: Vec<EntityView>,
    pub transformed_entity: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityUnleash {
    pub entity: EntityView,
    pub entity_type: String,
    pub reason: EntityUnleashReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpBottle {
    pub entity: EntityView,
    pub entity_type: String,
    pub experience: i32,
    pub hit_block: Option<BlockPos>,
    pub hit_block_face: Option<Face>,
    pub hit_entity: Option<EntityView>,
    pub show_effect: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplosionPrime {
    pub entity: EntityView,
    pub entity_type: String,
    pub fire: bool,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FireworkExplode {
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FluidLevelChange {
    pub block: BlockPos,
    pub new_data: BlockState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoodLevelChange {
    pub entity: PlayerSnapshot,
    pub entity_type: String,
    pub food_level: i32,
    pub item: Option<ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FurnaceBurn {
    pub block: BlockPos,
    pub burn_time: i32,
    pub burning: bool,
    pub fuel: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FurnaceExtract {
    pub block: BlockPos,
    pub exp_to_drop: i32,
    pub item_amount: i32,
    pub item_type: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FurnaceSmelt {
    pub block: BlockPos,
    pub result: ItemSpec,
    pub source: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FurnaceStartSmelt {
    pub block: BlockPos,
    pub recipe: String,
    pub source: ItemSpec,
    pub total_cook_time: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericGame {
    pub entity: Option<EntityView>,
    pub event: String,
    pub location: Location,
    pub radius: i32,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HangingBreak {
    pub cause: HangingBreakRemoveCause,
    pub entity: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HangingBreakByEntity {
    pub cause: HangingBreakRemoveCause,
    pub entity: EntityView,
    pub remover: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HangingPlace {
    pub block: BlockPos,
    pub block_face: Face,
    pub entity: EntityView,
    pub hand: Option<EquipmentSlot>,
    pub item_stack: Option<ItemSpec>,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HopperInventorySearch {
    pub block: BlockPos,
    pub container_type: HopperInventorySearchContainerType,
    pub inventory: Option<InventoryRef>,
    pub search_block: BlockPos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HorseJump {
    pub entity: EntityView,
    pub entity_type: String,
    pub power: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryClick {
    pub action: InventoryAction,
    pub click: ClickKind,
    pub clicked_inventory: Option<InventoryRef>,
    pub current_item: Option<ItemSpec>,
    pub cursor: Option<ItemSpec>,
    pub hotbar_button: i32,
    pub inventory: InventoryRef,
    pub left_click: bool,
    pub raw_slot: i32,
    pub result: EventResult,
    pub right_click: bool,
    pub shift_click: bool,
    pub slot: i32,
    pub slot_type: InventoryTypeSlotType,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryClose {
    pub inventory: InventoryRef,
    pub player: PlayerSnapshot,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub by_player: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryCreative {
    pub action: InventoryAction,
    pub click: ClickKind,
    pub clicked_inventory: Option<InventoryRef>,
    pub current_item: Option<ItemSpec>,
    pub cursor: Option<ItemSpec>,
    pub hotbar_button: i32,
    pub inventory: InventoryRef,
    pub left_click: bool,
    pub raw_slot: i32,
    pub result: EventResult,
    pub right_click: bool,
    pub shift_click: bool,
    pub slot: i32,
    pub slot_type: InventoryTypeSlotType,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryDrag {
    pub cursor: Option<ItemSpec>,
    pub inventory: InventoryRef,
    pub inventory_slots: Vec<i32>,
    pub new_items: Vec<(i32, ItemSpec)>,
    pub old_cursor: ItemSpec,
    pub raw_slots: Vec<i32>,
    pub result: EventResult,
    pub drag_type: DragType,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryMoveItem {
    pub destination: InventoryRef,
    pub initiator: InventoryRef,
    pub item: ItemSpec,
    pub source: InventoryRef,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryOpen {
    pub inventory: InventoryRef,
    pub player: PlayerSnapshot,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryPickupItem {
    pub inventory: InventoryRef,
    pub item: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemDespawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemMerge {
    pub entity: EntityView,
    pub entity_type: String,
    pub target: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemSpawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeavesDecay {
    pub block: BlockPos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LightningStrike {
    pub cause: LightningStrikeCause,
    pub lightning: EntityView,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LingeringPotionSplash {
    pub area_effect_cloud: EntityView,
    pub entity: EntityView,
    pub entity_type: String,
    pub hit_block: Option<BlockPos>,
    pub hit_block_face: Option<Face>,
    pub hit_entity: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LootGenerate {
    pub entity: Option<EntityView>,
    pub inventory_holder: Option<HolderRef>,
    pub loot: Vec<ItemSpec>,
    pub loot_context: LootContext,
    pub loot_table: String,
    pub plugin: bool,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapInitialize;

#[derive(Debug, Clone, PartialEq)]
pub struct MoistureChange {
    pub block: BlockPos,
    pub new_state: BlockSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotePlay {
    pub block: BlockPos,
    pub instrument: Instrument,
    pub note: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PigZap {
    pub entity: EntityView,
    pub entity_type: String,
    pub lightning: EntityView,
    pub transform_reason: EntityTransformReason,
    pub transformed_entities: Vec<EntityView>,
    pub transformed_entity: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PigZombieAnger {
    pub entity: EntityView,
    pub entity_type: String,
    pub new_anger: i32,
    pub target: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PiglinBarter {
    pub entity: EntityView,
    pub entity_type: String,
    pub input: ItemSpec,
    pub outcome: Vec<ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerAdvancementCriterionGrant {
    pub advancement: String,
    pub criterion: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerAdvancementDone {
    pub advancement: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerAnimation {
    pub animation_type: PlayerAnimationType,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerArmorStandManipulate {
    pub armor_stand_item: ItemSpec,
    pub hand: EquipmentSlot,
    pub player: PlayerSnapshot,
    pub player_item: ItemSpec,
    pub right_clicked: EntityView,
    pub slot: EquipmentSlot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBedEnter {
    pub bed: BlockPos,
    pub bed_enter_result: PlayerBedEnterResult,
    pub player: PlayerSnapshot,
    pub use_bed: EventResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBedLeave {
    pub bed: BlockPos,
    pub player: PlayerSnapshot,
    pub set_spawn_location: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBucketEmpty {
    pub block: BlockPos,
    pub block_clicked: BlockPos,
    pub block_face: Face,
    pub bucket: String,
    pub hand: EquipmentSlot,
    pub item_stack: Option<ItemSpec>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBucketEntity {
    pub entity: EntityView,
    pub entity_bucket: ItemSpec,
    pub hand: EquipmentSlot,
    pub original_bucket: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerBucketFill {
    pub block: BlockPos,
    pub block_clicked: BlockPos,
    pub block_face: Face,
    pub bucket: String,
    pub hand: EquipmentSlot,
    pub item_stack: Option<ItemSpec>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerChangedMainHand {
    pub main_hand: MainHand,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerChangedWorld {
    pub from: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerCommandPreprocess {
    pub message: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerCommandSend {
    pub commands: Vec<String>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerCustomClick {
    pub data: Option<String>,
    pub click_id: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDeath {
    pub damage_source: DamageSourceView,
    pub death_message: Option<String>,
    pub dropped_exp: i32,
    pub drops: Vec<ItemSpec>,
    pub entity: PlayerSnapshot,
    pub entity_type: String,
    pub keep_inventory: bool,
    pub keep_level: bool,
    pub new_exp: i32,
    pub new_level: i32,
    pub new_total_exp: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDropItem {
    pub player: PlayerSnapshot,
    pub item: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerEditBook {
    pub new_book_meta: BookMeta,
    pub player: PlayerSnapshot,
    pub previous_book_meta: BookMeta,
    pub signing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerEggThrow {
    pub egg: EntityView,
    pub hatching: bool,
    pub hatching_type: String,
    pub num_hatches: i8,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerExpChange {
    pub amount: i32,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerExpCooldownChange {
    pub new_cooldown: i32,
    pub player: PlayerSnapshot,
    pub reason: PlayerExpCooldownChangeReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerFish {
    pub caught: Option<EntityView>,
    pub exp_to_drop: i32,
    pub hand: Option<EquipmentSlot>,
    pub hook: EntityView,
    pub player: PlayerSnapshot,
    pub state: PlayerFishState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerGameModeChange {
    pub new_game_mode: GameMode,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerHarvestBlock {
    pub hand: EquipmentSlot,
    pub harvested_block: BlockPos,
    pub items_harvested: Vec<ItemSpec>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerHideEntity {
    pub entity: EntityView,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInput {
    pub input: Input,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInteract {
    pub action: Action,
    pub block: bool,
    pub block_face: Face,
    pub block_in_hand: bool,
    pub clicked_block: Option<BlockPos>,
    pub clicked_position: Option<Vec3>,
    pub hand: Option<EquipmentSlot>,
    pub has_item: bool,
    pub item: Option<ItemSpec>,
    pub material: String,
    pub player: PlayerSnapshot,
    pub use_interacted_block: EventResult,
    pub use_item_in_hand: EventResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInteractAtEntity {
    pub clicked_position: Vec3,
    pub hand: EquipmentSlot,
    pub player: PlayerSnapshot,
    pub right_clicked: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInteractEntity {
    pub hand: EquipmentSlot,
    pub player: PlayerSnapshot,
    pub right_clicked: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerItemBreak {
    pub broken_item: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerItemConsume {
    pub hand: EquipmentSlot,
    pub item: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerItemDamage {
    pub damage: i32,
    pub item: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerItemHeld {
    pub new_slot: i32,
    pub player: PlayerSnapshot,
    pub previous_slot: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerItemMend {
    pub experience_orb: EntityView,
    pub item: ItemSpec,
    pub player: PlayerSnapshot,
    pub repair_amount: i32,
    pub slot: EquipmentSlot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerJoin {
    pub join_message: Option<String>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerKick {
    pub leave_message: String,
    pub player: PlayerSnapshot,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLeashEntity {
    pub entity: EntityView,
    pub hand: EquipmentSlot,
    pub leash_holder: EntityView,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLeave {
    pub player: PlayerSnapshot,
    pub quit_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLevelChange {
    pub new_level: i32,
    pub old_level: i32,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLinksSend {
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLocaleChange {
    pub locale: String,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerLogin {
    pub address: IpAddr,
    pub hostname: String,
    pub kick_message: String,
    pub player: PlayerSnapshot,
    pub real_address: IpAddr,
    pub result: PlayerLoginResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerMove {
    pub from: Location,
    pub player: PlayerSnapshot,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPickupArrow {
    pub arrow: EntityView,
    pub player: PlayerSnapshot,
    pub remaining: i32,
    pub item: ItemSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPortal {
    pub can_create_portal: bool,
    pub cause: PlayerTeleportCause,
    pub creation_radius: i32,
    pub from: Location,
    pub player: PlayerSnapshot,
    pub search_radius: i32,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRecipeBookClick {
    pub original_recipe: String,
    pub player: PlayerSnapshot,
    pub recipe: String,
    pub shift_click: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRecipeBookSettingsChange {
    pub filtering: bool,
    pub open: bool,
    pub player: PlayerSnapshot,
    pub recipe_book_type: PlayerRecipeBookSettingsChangeRecipeBookType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRecipeDiscover {
    pub player: PlayerSnapshot,
    pub recipe: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerResourcePackStatus {
    pub pack_id: Uuid,
    pub player: PlayerSnapshot,
    pub status: ResourcePackStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRespawn {
    pub anchor_spawn: bool,
    pub bed_spawn: bool,
    pub player: PlayerSnapshot,
    pub respawn_location: Location,
    pub respawn_reason: PlayerRespawnReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRiptide {
    pub item: ItemSpec,
    pub player: PlayerSnapshot,
    pub velocity: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerShearEntity {
    pub entity: EntityView,
    pub hand: EquipmentSlot,
    pub item: ItemSpec,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerShowEntity {
    pub entity: EntityView,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSignOpen {
    pub cause: PlayerSignOpenCause,
    pub player: PlayerSnapshot,
    pub side: Side,
    pub sign: SignData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSpawnChange {
    pub cause: PlayerSpawnChangeCause,
    pub forced: bool,
    pub new_spawn: Option<Location>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStatisticIncrement {
    pub entity_type: Option<String>,
    pub material: Option<String>,
    pub new_value: i32,
    pub player: PlayerSnapshot,
    pub previous_value: i32,
    pub statistic: Statistic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSwapHandItems {
    pub main_hand_item: Option<ItemSpec>,
    pub off_hand_item: Option<ItemSpec>,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerTakeLecternBook {
    pub book: Option<ItemSpec>,
    pub lectern: LecternData,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerTeleport {
    pub cause: PlayerTeleportCause,
    pub from: Location,
    pub player: PlayerSnapshot,
    pub to: Option<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerToggleFlight {
    pub flying: bool,
    pub player: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerToggleSneak {
    pub player: PlayerSnapshot,
    pub sneaking: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerToggleSprint {
    pub player: PlayerSnapshot,
    pub sprinting: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerUnleashEntity {
    pub entity: EntityView,
    pub entity_type: String,
    pub hand: EquipmentSlot,
    pub player: PlayerSnapshot,
    pub reason: EntityUnleashReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerVelocity {
    pub player: PlayerSnapshot,
    pub velocity: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginDisable {
    pub plugin: PluginInfo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginEnable {
    pub plugin: PluginInfo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortalCreate {
    pub blocks: Vec<BlockSnapshot>,
    pub entity: Option<EntityView>,
    pub reason: PortalCreateReason,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PotionSplash {
    pub affected_entities: Vec<EntityView>,
    pub entity: EntityView,
    pub entity_type: String,
    pub intensities: Vec<(EntityView, f64)>,
    pub hit_block: Option<BlockPos>,
    pub hit_block_face: Option<Face>,
    pub hit_entity: Option<EntityView>,
    pub potion: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareAnvil {
    pub inventory: InventoryRef,
    pub result: Option<ItemSpec>,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareGrindstone {
    pub inventory: InventoryRef,
    pub result: Option<ItemSpec>,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareItemCraft {
    pub inventory: InventoryRef,
    pub recipe: Option<String>,
    pub repair: bool,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub result: Option<ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareItemEnchant {
    pub enchant_block: BlockPos,
    pub enchanter: PlayerSnapshot,
    pub enchantment_bonus: i32,
    pub inventory: InventoryRef,
    pub item: ItemSpec,
    pub offers: Vec<EnchantOffer>,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSmithing {
    pub inventory: InventoryRef,
    pub result: Option<ItemSpec>,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileHit {
    pub entity: EntityView,
    pub entity_type: String,
    pub hit_block: Option<BlockPos>,
    pub hit_block_face: Option<Face>,
    pub hit_entity: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileLaunch {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidFinish {
    pub winners: Vec<PlayerSnapshot>,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidSpawnWave {
    pub patrol_leader: Option<EntityView>,
    pub raiders: Vec<EntityView>,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidStop {
    pub reason: RaidStopReason,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidTrigger {
    pub player: PlayerSnapshot,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteServerCommand {
    pub command: String,
    pub sender: SenderView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SculkBloom {
    pub block: BlockPos,
    pub charge: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerCommand {
    pub command: String,
    pub sender: SenderView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerListPing {
    pub address: IpAddr,
    pub hostname: String,
    pub players: Vec<PlayerSnapshot>,
    pub max_players: i32,
    pub motd: String,
    pub num_players: i32,
    pub hidden_players: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerLoad {
    pub load_type: ServerLoadType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheepDyeWool {
    pub color: DyeColor,
    pub entity: EntityView,
    pub entity_type: String,
    pub player: Option<PlayerSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SheepRegrowWool {
    pub entity: EntityView,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignChange {
    pub block: BlockPos,
    pub lines: Vec<String>,
    pub player: PlayerSnapshot,
    pub side: Side,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmithItem {
    pub action: InventoryAction,
    pub click: ClickKind,
    pub clicked_inventory: Option<InventoryRef>,
    pub current_item: Option<ItemSpec>,
    pub cursor: Option<ItemSpec>,
    pub hotbar_button: i32,
    pub inventory: InventoryRef,
    pub left_click: bool,
    pub raw_slot: i32,
    pub result: EventResult,
    pub right_click: bool,
    pub shift_click: bool,
    pub slot: i32,
    pub slot_type: InventoryTypeSlotType,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnChange {
    pub previous_location: Location,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnerSpawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
    pub spawner: SpawnerData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpongeAbsorb {
    pub block: BlockPos,
    pub blocks: Vec<BlockSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StriderTemperatureChange {
    pub entity: EntityView,
    pub entity_type: String,
    pub shivering: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureGrow {
    pub blocks: Vec<BlockSnapshot>,
    pub from_bonemeal: bool,
    pub location: Location,
    pub player: Option<PlayerSnapshot>,
    pub species: TreeType,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TNTPrime {
    pub block: BlockPos,
    pub cause: TNTPrimeCause,
    pub priming_block: Option<BlockPos>,
    pub priming_entity: Option<EntityView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabComplete {
    pub buffer: String,
    pub completions: Vec<String>,
    pub sender: SenderView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThunderChange {
    pub to_thunder_state: bool,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeSkip {
    pub skip_amount: i64,
    pub skip_reason: TimeSkipReason,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeSelect {
    pub index: i32,
    pub inventory: InventoryRef,
    pub merchant: MerchantRef,
    pub result: EventResult,
    pub view: InventoryView,
    pub viewers: Vec<PlayerSnapshot>,
    pub who_clicked: PlayerSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrialSpawnerSpawn {
    pub entity: EntityView,
    pub entity_type: String,
    pub location: Location,
    pub trial_spawner: TrialSpawnerData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultDisplayItem {
    pub block: BlockPos,
    pub display_item: Option<ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleBlockCollision {
    pub block: BlockPos,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleCreate {
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleDamage {
    pub attacker: Option<EntityView>,
    pub damage: f64,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleDestroy {
    pub attacker: Option<EntityView>,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleEnter {
    pub entered: EntityView,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleEntityCollision {
    pub collision_cancelled: bool,
    pub entity: EntityView,
    pub pickup_cancelled: bool,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleExit {
    pub exited: EntityView,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleMove {
    pub from: Location,
    pub to: Location,
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleUpdate {
    pub vehicle: EntityView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillagerAcquireTrade {
    pub entity: EntityView,
    pub entity_type: String,
    pub recipe: Trade,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillagerCareerChange {
    pub entity: EntityView,
    pub entity_type: String,
    pub profession: String,
    pub reason: VillagerCareerChangeReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillagerReplenishTrade {
    pub entity: EntityView,
    pub entity_type: String,
    pub recipe: Trade,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillagerReputationChange {
    pub entity: EntityView,
    pub entity_type: String,
    pub max_value: i32,
    pub new_value: i32,
    pub old_value: i32,
    pub reason: String,
    pub reputation_type: String,
    pub target: Option<EntityView>,
    pub target_uuid: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherChange {
    pub to_weather_state: bool,
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldInit {
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldLoad {
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSave {
    pub world: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldUnload {
    pub world: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ArrowBodyCountChangePatch {
    pub new_amount: Option<i32>,
}

impl ArrowBodyCountChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_amount: later.new_amount.or(self.new_amount),
        }
    }

    fn settle(self, event: &mut ArrowBodyCountChange) {
        if let Some(value) = self.new_amount {
            event.new_amount = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AsyncPlayerChatPatch {
    pub format: Option<String>,
    pub message: Option<String>,
    pub recipients: SetDiff<PlayerSnapshot>,
}

impl AsyncPlayerChatPatch {
    fn then(self, later: Self) -> Self {
        Self {
            format: later.format.or(self.format),
            message: later.message.or(self.message),
            recipients: self.recipients.then(later.recipients),
        }
    }

    fn settle(self, event: &mut AsyncPlayerChat) {
        if let Some(value) = self.format {
            event.format = value;
        }
        if let Some(value) = self.message {
            event.message = value;
        }
        event.recipients = self.recipients.apply(std::mem::take(&mut event.recipients));
    }

    fn refusal(&self, event: &AsyncPlayerChat) -> Option<String> {
        self.recipients
            .refusal("recipients", event.recipients.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AsyncPlayerPreLoginPatch {
    pub kick_message: Option<String>,
    pub login_result: Option<AsyncPlayerPreLoginResult>,
}

impl AsyncPlayerPreLoginPatch {
    fn then(self, later: Self) -> Self {
        Self {
            kick_message: later.kick_message.or(self.kick_message),
            login_result: later.login_result.or(self.login_result),
        }
    }

    fn settle(self, event: &mut AsyncPlayerPreLogin) {
        if let Some(value) = self.kick_message {
            event.kick_message = value;
        }
        if let Some(value) = self.login_result {
            event.login_result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockBreakPatch {
    pub drop_items: Option<bool>,
    pub exp_to_drop: Option<i32>,
}

impl BlockBreakPatch {
    fn then(self, later: Self) -> Self {
        Self {
            drop_items: later.drop_items.or(self.drop_items),
            exp_to_drop: later.exp_to_drop.or(self.exp_to_drop),
        }
    }

    fn settle(self, event: &mut BlockBreak) {
        if let Some(value) = self.drop_items {
            event.drop_items = value;
        }
        if let Some(value) = self.exp_to_drop {
            event.exp_to_drop = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockCanBuildPatch {
    pub buildable: Option<bool>,
}

impl BlockCanBuildPatch {
    fn then(self, later: Self) -> Self {
        Self {
            buildable: later.buildable.or(self.buildable),
        }
    }

    fn settle(self, event: &mut BlockCanBuild) {
        if let Some(value) = self.buildable {
            event.buildable = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockCookPatch {
    pub result: Option<ItemSpec>,
}

impl BlockCookPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut BlockCook) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDamagePatch {
    pub insta_break: Option<bool>,
}

impl BlockDamagePatch {
    fn then(self, later: Self) -> Self {
        Self {
            insta_break: later.insta_break.or(self.insta_break),
        }
    }

    fn settle(self, event: &mut BlockDamage) {
        if let Some(value) = self.insta_break {
            event.insta_break = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDispensePatch {
    pub item: Option<ItemSpec>,
    pub velocity: Option<Vec3>,
}

impl BlockDispensePatch {
    fn then(self, later: Self) -> Self {
        Self {
            item: later.item.or(self.item),
            velocity: later.velocity.or(self.velocity),
        }
    }

    fn settle(self, event: &mut BlockDispense) {
        if let Some(value) = self.item {
            event.item = value;
        }
        if let Some(value) = self.velocity {
            event.velocity = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.velocity
            .and_then(|value| vector_refusal("velocity", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDispenseArmorPatch {
    pub item: Option<ItemSpec>,
    pub velocity: Option<Vec3>,
}

impl BlockDispenseArmorPatch {
    fn then(self, later: Self) -> Self {
        Self {
            item: later.item.or(self.item),
            velocity: later.velocity.or(self.velocity),
        }
    }

    fn settle(self, event: &mut BlockDispenseArmor) {
        if let Some(value) = self.item {
            event.item = value;
        }
        if let Some(value) = self.velocity {
            event.velocity = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.velocity
            .and_then(|value| vector_refusal("velocity", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDispenseLootPatch {
    pub dispensed_loot: ListDiff<ItemSpec>,
}

impl BlockDispenseLootPatch {
    fn then(self, later: Self) -> Self {
        Self {
            dispensed_loot: self.dispensed_loot.then(later.dispensed_loot),
        }
    }

    fn settle(self, event: &mut BlockDispenseLoot) {
        event.dispensed_loot = self
            .dispensed_loot
            .apply(std::mem::take(&mut event.dispensed_loot));
    }

    fn refusal(&self, event: &BlockDispenseLoot) -> Option<String> {
        self.dispensed_loot
            .refusal("dispensed_loot", event.dispensed_loot.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDropItemPatch {
    pub items: ListDiff<DroppedItem>,
}

impl BlockDropItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            items: self.items.then(later.items),
        }
    }

    fn settle(self, event: &mut BlockDropItem) {
        event.items = self.items.apply(std::mem::take(&mut event.items));
    }

    fn refusal(&self, event: &BlockDropItem) -> Option<String> {
        self.items.refusal("items", event.items.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockExpPatch {
    pub exp_to_drop: Option<i32>,
}

impl BlockExpPatch {
    fn then(self, later: Self) -> Self {
        Self {
            exp_to_drop: later.exp_to_drop.or(self.exp_to_drop),
        }
    }

    fn settle(self, event: &mut BlockExp) {
        if let Some(value) = self.exp_to_drop {
            event.exp_to_drop = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockExplodePatch {
    pub block_list: SetDiff<BlockPos>,
    pub explosion_yield: Option<f32>,
}

impl BlockExplodePatch {
    fn then(self, later: Self) -> Self {
        Self {
            block_list: self.block_list.then(later.block_list),
            explosion_yield: later.explosion_yield.or(self.explosion_yield),
        }
    }

    fn settle(self, event: &mut BlockExplode) {
        event.block_list = self.block_list.apply(std::mem::take(&mut event.block_list));
        if let Some(value) = self.explosion_yield {
            event.explosion_yield = value;
        }
    }

    fn refusal(&self, event: &BlockExplode) -> Option<String> {
        self.block_list
            .refusal("block_list", event.block_list.len())
            .or_else(|| {
                self.explosion_yield
                    .and_then(|value| number_refusal("explosion_yield", f64::from(value)))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockMultiPlacePatch {
    pub build: Option<bool>,
}

impl BlockMultiPlacePatch {
    fn then(self, later: Self) -> Self {
        Self {
            build: later.build.or(self.build),
        }
    }

    fn settle(self, event: &mut BlockMultiPlace) {
        if let Some(value) = self.build {
            event.build = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockPlacePatch {
    pub build: Option<bool>,
}

impl BlockPlacePatch {
    fn then(self, later: Self) -> Self {
        Self {
            build: later.build.or(self.build),
        }
    }

    fn settle(self, event: &mut BlockPlace) {
        if let Some(value) = self.build {
            event.build = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockRedstonePatch {
    pub new_current: Option<i32>,
}

impl BlockRedstonePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_current: later.new_current.or(self.new_current),
        }
    }

    fn settle(self, event: &mut BlockRedstone) {
        if let Some(value) = self.new_current {
            event.new_current = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrewingStandFuelPatch {
    pub consuming: Option<bool>,
    pub fuel_power: Option<i32>,
}

impl BrewingStandFuelPatch {
    fn then(self, later: Self) -> Self {
        Self {
            consuming: later.consuming.or(self.consuming),
            fuel_power: later.fuel_power.or(self.fuel_power),
        }
    }

    fn settle(self, event: &mut BrewingStandFuel) {
        if let Some(value) = self.consuming {
            event.consuming = value;
        }
        if let Some(value) = self.fuel_power {
            event.fuel_power = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BrewingStartPatch {
    pub total_brew_time: Option<i32>,
}

impl BrewingStartPatch {
    fn then(self, later: Self) -> Self {
        Self {
            total_brew_time: later.total_brew_time.or(self.total_brew_time),
        }
    }

    fn settle(self, event: &mut BrewingStart) {
        if let Some(value) = self.total_brew_time {
            event.total_brew_time = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BroadcastMessagePatch {
    pub message: Option<String>,
    pub recipients: SetDiff<SenderView>,
}

impl BroadcastMessagePatch {
    fn then(self, later: Self) -> Self {
        Self {
            message: later.message.or(self.message),
            recipients: self.recipients.then(later.recipients),
        }
    }

    fn settle(self, event: &mut BroadcastMessage) {
        if let Some(value) = self.message {
            event.message = value;
        }
        event.recipients = self.recipients.apply(std::mem::take(&mut event.recipients));
    }

    fn refusal(&self, event: &BroadcastMessage) -> Option<String> {
        self.recipients
            .refusal("recipients", event.recipients.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CampfireStartPatch {
    pub total_cook_time: Option<i32>,
}

impl CampfireStartPatch {
    fn then(self, later: Self) -> Self {
        Self {
            total_cook_time: later.total_cook_time.or(self.total_cook_time),
        }
    }

    fn settle(self, event: &mut CampfireStart) {
        if let Some(value) = self.total_cook_time {
            event.total_cook_time = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChunkUnloadPatch {
    pub save_chunk: Option<bool>,
}

impl ChunkUnloadPatch {
    fn then(self, later: Self) -> Self {
        Self {
            save_chunk: later.save_chunk.or(self.save_chunk),
        }
    }

    fn settle(self, event: &mut ChunkUnload) {
        if let Some(value) = self.save_chunk {
            event.save_chunk = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CraftItemPatch {
    pub current_item: Option<Option<ItemSpec>>,
    pub cursor: Option<Option<ItemSpec>>,
    pub result: Option<EventResult>,
}

impl CraftItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            current_item: later.current_item.or(self.current_item),
            cursor: later.cursor.or(self.cursor),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut CraftItem) {
        if let Some(value) = self.current_item {
            event.current_item = value;
        }
        if let Some(value) = self.cursor {
            event.cursor = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            current_item: self.current_item,
            cursor: self.cursor,
            result: self.result,
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrafterCraftPatch {
    pub result: Option<ItemSpec>,
}

impl CrafterCraftPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut CrafterCraft) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CubeMobSplitPatch {
    pub count: Option<i32>,
}

impl CubeMobSplitPatch {
    fn then(self, later: Self) -> Self {
        Self {
            count: later.count.or(self.count),
        }
    }

    fn settle(self, event: &mut CubeMobSplit) {
        if let Some(value) = self.count {
            event.count = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnchantItemPatch {
    pub exp_level_cost: Option<i32>,
}

impl EnchantItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            exp_level_cost: later.exp_level_cost.or(self.exp_level_cost),
        }
    }

    fn settle(self, event: &mut EnchantItem) {
        if let Some(value) = self.exp_level_cost {
            event.exp_level_cost = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnderDragonChangePhasePatch {
    pub new_phase: Option<EnderDragonPhase>,
}

impl EnderDragonChangePhasePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_phase: later.new_phase.or(self.new_phase),
        }
    }

    fn settle(self, event: &mut EnderDragonChangePhase) {
        if let Some(value) = self.new_phase {
            event.new_phase = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityAirChangePatch {
    pub amount: Option<i32>,
}

impl EntityAirChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            amount: later.amount.or(self.amount),
        }
    }

    fn settle(self, event: &mut EntityAirChange) {
        if let Some(value) = self.amount {
            event.amount = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityBreedPatch {
    pub experience: Option<i32>,
}

impl EntityBreedPatch {
    fn then(self, later: Self) -> Self {
        Self {
            experience: later.experience.or(self.experience),
        }
    }

    fn settle(self, event: &mut EntityBreed) {
        if let Some(value) = self.experience {
            event.experience = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityCombustPatch {
    pub duration: Option<f32>,
}

impl EntityCombustPatch {
    fn then(self, later: Self) -> Self {
        Self {
            duration: later.duration.or(self.duration),
        }
    }

    fn settle(self, event: &mut EntityCombust) {
        if let Some(value) = self.duration {
            event.duration = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.duration
            .and_then(|value| number_refusal("duration", f64::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityCombustByBlockPatch {
    pub duration: Option<f32>,
}

impl EntityCombustByBlockPatch {
    fn then(self, later: Self) -> Self {
        Self {
            duration: later.duration.or(self.duration),
        }
    }

    fn settle(self, event: &mut EntityCombustByBlock) {
        if let Some(value) = self.duration {
            event.duration = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.duration
            .and_then(|value| number_refusal("duration", f64::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityCombustByEntityPatch {
    pub duration: Option<f32>,
}

impl EntityCombustByEntityPatch {
    fn then(self, later: Self) -> Self {
        Self {
            duration: later.duration.or(self.duration),
        }
    }

    fn settle(self, event: &mut EntityCombustByEntity) {
        if let Some(value) = self.duration {
            event.duration = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.duration
            .and_then(|value| number_refusal("duration", f64::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDamagePatch {
    pub damage: Option<f64>,
}

impl EntityDamagePatch {
    fn then(self, later: Self) -> Self {
        Self {
            damage: later.damage.or(self.damage),
        }
    }

    fn settle(self, event: &mut EntityDamage) {
        if let Some(value) = self.damage {
            event.damage = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.damage
            .and_then(|value| number_refusal("damage", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDamageByBlockPatch {
    pub damage: Option<f64>,
}

impl EntityDamageByBlockPatch {
    fn then(self, later: Self) -> Self {
        Self {
            damage: later.damage.or(self.damage),
        }
    }

    fn settle(self, event: &mut EntityDamageByBlock) {
        if let Some(value) = self.damage {
            event.damage = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.damage
            .and_then(|value| number_refusal("damage", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDamageByEntityPatch {
    pub damage: Option<f64>,
}

impl EntityDamageByEntityPatch {
    fn then(self, later: Self) -> Self {
        Self {
            damage: later.damage.or(self.damage),
        }
    }

    fn settle(self, event: &mut EntityDamageByEntity) {
        if let Some(value) = self.damage {
            event.damage = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.damage
            .and_then(|value| number_refusal("damage", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDeathPatch {
    pub dropped_exp: Option<i32>,
    pub drops: ListDiff<ItemSpec>,
}

impl EntityDeathPatch {
    fn then(self, later: Self) -> Self {
        Self {
            dropped_exp: later.dropped_exp.or(self.dropped_exp),
            drops: self.drops.then(later.drops),
        }
    }

    fn settle(self, event: &mut EntityDeath) {
        if let Some(value) = self.dropped_exp {
            event.dropped_exp = value;
        }
        event.drops = self.drops.apply(std::mem::take(&mut event.drops));
    }

    fn refusal(&self, event: &EntityDeath) -> Option<String> {
        self.drops.refusal("drops", event.drops.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityEnterLoveModePatch {
    pub ticks_in_love: Option<i32>,
}

impl EntityEnterLoveModePatch {
    fn then(self, later: Self) -> Self {
        Self {
            ticks_in_love: later.ticks_in_love.or(self.ticks_in_love),
        }
    }

    fn settle(self, event: &mut EntityEnterLoveMode) {
        if let Some(value) = self.ticks_in_love {
            event.ticks_in_love = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityExhaustionPatch {
    pub exhaustion: Option<f32>,
}

impl EntityExhaustionPatch {
    fn then(self, later: Self) -> Self {
        Self {
            exhaustion: later.exhaustion.or(self.exhaustion),
        }
    }

    fn settle(self, event: &mut EntityExhaustion) {
        if let Some(value) = self.exhaustion {
            event.exhaustion = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.exhaustion
            .and_then(|value| number_refusal("exhaustion", f64::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityExplodePatch {
    pub block_list: SetDiff<BlockPos>,
    pub explosion_yield: Option<f32>,
}

impl EntityExplodePatch {
    fn then(self, later: Self) -> Self {
        Self {
            block_list: self.block_list.then(later.block_list),
            explosion_yield: later.explosion_yield.or(self.explosion_yield),
        }
    }

    fn settle(self, event: &mut EntityExplode) {
        event.block_list = self.block_list.apply(std::mem::take(&mut event.block_list));
        if let Some(value) = self.explosion_yield {
            event.explosion_yield = value;
        }
    }

    fn refusal(&self, event: &EntityExplode) -> Option<String> {
        self.block_list
            .refusal("block_list", event.block_list.len())
            .or_else(|| {
                self.explosion_yield
                    .and_then(|value| number_refusal("explosion_yield", f64::from(value)))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityKnockbackPatch {
    pub final_knockback: Option<Vec3>,
}

impl EntityKnockbackPatch {
    fn then(self, later: Self) -> Self {
        Self {
            final_knockback: later.final_knockback.or(self.final_knockback),
        }
    }

    fn settle(self, event: &mut EntityKnockback) {
        if let Some(value) = self.final_knockback {
            event.final_knockback = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.final_knockback
            .and_then(|value| vector_refusal("final_knockback", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityKnockbackByEntityPatch {
    pub final_knockback: Option<Vec3>,
}

impl EntityKnockbackByEntityPatch {
    fn then(self, later: Self) -> Self {
        Self {
            final_knockback: later.final_knockback.or(self.final_knockback),
        }
    }

    fn settle(self, event: &mut EntityKnockbackByEntity) {
        if let Some(value) = self.final_knockback {
            event.final_knockback = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.final_knockback
            .and_then(|value| vector_refusal("final_knockback", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityPortalPatch {
    pub can_create_portal: Option<bool>,
    pub creation_radius: Option<i32>,
    pub search_radius: Option<i32>,
    pub to: Option<Option<Location>>,
}

impl EntityPortalPatch {
    fn then(self, later: Self) -> Self {
        Self {
            can_create_portal: later.can_create_portal.or(self.can_create_portal),
            creation_radius: later.creation_radius.or(self.creation_radius),
            search_radius: later.search_radius.or(self.search_radius),
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut EntityPortal) {
        if let Some(value) = self.can_create_portal {
            event.can_create_portal = value;
        }
        if let Some(value) = self.creation_radius {
            event.creation_radius = value;
        }
        if let Some(value) = self.search_radius {
            event.search_radius = value;
        }
        if let Some(value) = self.to {
            event.to = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.to
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityPortalExitPatch {
    pub after: Option<Vec3>,
    pub to: Option<Option<Location>>,
}

impl EntityPortalExitPatch {
    fn then(self, later: Self) -> Self {
        Self {
            after: later.after.or(self.after),
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut EntityPortalExit) {
        if let Some(value) = self.after {
            event.after = value;
        }
        if let Some(value) = self.to {
            event.to = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.after
            .and_then(|value| vector_refusal("after", value))
            .or_else(|| {
                self.to
                    .as_ref()
                    .and_then(Option::as_ref)
                    .and_then(location_refusal)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityPotionEffectPatch {
    pub overrides: Option<bool>,
}

impl EntityPotionEffectPatch {
    fn then(self, later: Self) -> Self {
        Self {
            overrides: later.overrides.or(self.overrides),
        }
    }

    fn settle(self, event: &mut EntityPotionEffect) {
        if let Some(value) = self.overrides {
            event.overrides = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityRegainHealthPatch {
    pub amount: Option<f64>,
}

impl EntityRegainHealthPatch {
    fn then(self, later: Self) -> Self {
        Self {
            amount: later.amount.or(self.amount),
        }
    }

    fn settle(self, event: &mut EntityRegainHealth) {
        if let Some(value) = self.amount {
            event.amount = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.amount
            .and_then(|value| number_refusal("amount", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityShootBowPatch {
    pub projectile: Option<EntityView>,
}

impl EntityShootBowPatch {
    fn then(self, later: Self) -> Self {
        Self {
            projectile: later.projectile.or(self.projectile),
        }
    }

    fn settle(self, event: &mut EntityShootBow) {
        if let Some(value) = self.projectile {
            event.projectile = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityTargetPatch {
    pub target: Option<Option<EntityView>>,
}

impl EntityTargetPatch {
    fn then(self, later: Self) -> Self {
        Self {
            target: later.target.or(self.target),
        }
    }

    fn settle(self, event: &mut EntityTarget) {
        if let Some(value) = self.target {
            event.target = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityTargetBlockPatch {
    pub target: Option<Option<BlockPos>>,
}

impl EntityTargetBlockPatch {
    fn then(self, later: Self) -> Self {
        Self {
            target: later.target.or(self.target),
        }
    }

    fn settle(self, event: &mut EntityTargetBlock) {
        if let Some(value) = self.target {
            event.target = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityTargetLivingEntityPatch {
    pub target: Option<Option<EntityView>>,
}

impl EntityTargetLivingEntityPatch {
    fn then(self, later: Self) -> Self {
        Self {
            target: later.target.or(self.target),
        }
    }

    fn settle(self, event: &mut EntityTargetLivingEntity) {
        if let Some(value) = self.target {
            event.target = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityTeleportPatch {
    pub to: Option<Option<Location>>,
}

impl EntityTeleportPatch {
    fn then(self, later: Self) -> Self {
        Self {
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut EntityTeleport) {
        if let Some(value) = self.to {
            event.to = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.to
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExpBottlePatch {
    pub experience: Option<i32>,
    pub show_effect: Option<bool>,
}

impl ExpBottlePatch {
    fn then(self, later: Self) -> Self {
        Self {
            experience: later.experience.or(self.experience),
            show_effect: later.show_effect.or(self.show_effect),
        }
    }

    fn settle(self, event: &mut ExpBottle) {
        if let Some(value) = self.experience {
            event.experience = value;
        }
        if let Some(value) = self.show_effect {
            event.show_effect = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExplosionPrimePatch {
    pub fire: Option<bool>,
    pub radius: Option<f32>,
}

impl ExplosionPrimePatch {
    fn then(self, later: Self) -> Self {
        Self {
            fire: later.fire.or(self.fire),
            radius: later.radius.or(self.radius),
        }
    }

    fn settle(self, event: &mut ExplosionPrime) {
        if let Some(value) = self.fire {
            event.fire = value;
        }
        if let Some(value) = self.radius {
            event.radius = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.radius
            .and_then(|value| number_refusal("radius", f64::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FluidLevelChangePatch {
    pub new_data: Option<BlockState>,
}

impl FluidLevelChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_data: later.new_data.or(self.new_data),
        }
    }

    fn settle(self, event: &mut FluidLevelChange) {
        if let Some(value) = self.new_data {
            event.new_data = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FoodLevelChangePatch {
    pub food_level: Option<i32>,
}

impl FoodLevelChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            food_level: later.food_level.or(self.food_level),
        }
    }

    fn settle(self, event: &mut FoodLevelChange) {
        if let Some(value) = self.food_level {
            event.food_level = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FurnaceBurnPatch {
    pub burn_time: Option<i32>,
    pub burning: Option<bool>,
}

impl FurnaceBurnPatch {
    fn then(self, later: Self) -> Self {
        Self {
            burn_time: later.burn_time.or(self.burn_time),
            burning: later.burning.or(self.burning),
        }
    }

    fn settle(self, event: &mut FurnaceBurn) {
        if let Some(value) = self.burn_time {
            event.burn_time = value;
        }
        if let Some(value) = self.burning {
            event.burning = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FurnaceExtractPatch {
    pub exp_to_drop: Option<i32>,
}

impl FurnaceExtractPatch {
    fn then(self, later: Self) -> Self {
        Self {
            exp_to_drop: later.exp_to_drop.or(self.exp_to_drop),
        }
    }

    fn settle(self, event: &mut FurnaceExtract) {
        if let Some(value) = self.exp_to_drop {
            event.exp_to_drop = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FurnaceSmeltPatch {
    pub result: Option<ItemSpec>,
}

impl FurnaceSmeltPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut FurnaceSmelt) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FurnaceStartSmeltPatch {
    pub total_cook_time: Option<i32>,
}

impl FurnaceStartSmeltPatch {
    fn then(self, later: Self) -> Self {
        Self {
            total_cook_time: later.total_cook_time.or(self.total_cook_time),
        }
    }

    fn settle(self, event: &mut FurnaceStartSmelt) {
        if let Some(value) = self.total_cook_time {
            event.total_cook_time = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenericGamePatch {
    pub radius: Option<i32>,
}

impl GenericGamePatch {
    fn then(self, later: Self) -> Self {
        Self {
            radius: later.radius.or(self.radius),
        }
    }

    fn settle(self, event: &mut GenericGame) {
        if let Some(value) = self.radius {
            event.radius = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HopperInventorySearchPatch {
    pub inventory: Option<Option<InventoryRef>>,
}

impl HopperInventorySearchPatch {
    fn then(self, later: Self) -> Self {
        Self {
            inventory: later.inventory.or(self.inventory),
        }
    }

    fn settle(self, event: &mut HopperInventorySearch) {
        if let Some(value) = self.inventory {
            event.inventory = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryClickPatch {
    pub current_item: Option<Option<ItemSpec>>,
    pub cursor: Option<Option<ItemSpec>>,
    pub result: Option<EventResult>,
}

impl InventoryClickPatch {
    fn then(self, later: Self) -> Self {
        Self {
            current_item: later.current_item.or(self.current_item),
            cursor: later.cursor.or(self.cursor),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut InventoryClick) {
        if let Some(value) = self.current_item {
            event.current_item = value;
        }
        if let Some(value) = self.cursor {
            event.cursor = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            current_item: self.current_item,
            cursor: self.cursor,
            result: self.result,
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryCreativePatch {
    pub current_item: Option<Option<ItemSpec>>,
    pub cursor: Option<Option<ItemSpec>>,
    pub result: Option<EventResult>,
}

impl InventoryCreativePatch {
    fn then(self, later: Self) -> Self {
        Self {
            current_item: later.current_item.or(self.current_item),
            cursor: later.cursor.or(self.cursor),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut InventoryCreative) {
        if let Some(value) = self.current_item {
            event.current_item = value;
        }
        if let Some(value) = self.cursor {
            event.cursor = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            current_item: self.current_item,
            result: self.result,
            ..Self::default()
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryDragPatch {
    pub cursor: Option<Option<ItemSpec>>,
    pub result: Option<EventResult>,
}

impl InventoryDragPatch {
    fn then(self, later: Self) -> Self {
        Self {
            cursor: later.cursor.or(self.cursor),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut InventoryDrag) {
        if let Some(value) = self.cursor {
            event.cursor = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            result: self.result,
            ..Self::default()
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryMoveItemPatch {
    pub item: Option<ItemSpec>,
}

impl InventoryMoveItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            item: later.item.or(self.item),
        }
    }

    fn settle(self, event: &mut InventoryMoveItem) {
        if let Some(value) = self.item {
            event.item = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LootGeneratePatch {
    pub loot: ListDiff<ItemSpec>,
}

impl LootGeneratePatch {
    fn then(self, later: Self) -> Self {
        Self {
            loot: self.loot.then(later.loot),
        }
    }

    fn settle(self, event: &mut LootGenerate) {
        event.loot = self.loot.apply(std::mem::take(&mut event.loot));
    }

    fn refusal(&self, event: &LootGenerate) -> Option<String> {
        self.loot.refusal("loot", event.loot.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PigZombieAngerPatch {
    pub new_anger: Option<i32>,
}

impl PigZombieAngerPatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_anger: later.new_anger.or(self.new_anger),
        }
    }

    fn settle(self, event: &mut PigZombieAnger) {
        if let Some(value) = self.new_anger {
            event.new_anger = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerBedEnterPatch {
    pub use_bed: Option<EventResult>,
}

impl PlayerBedEnterPatch {
    fn then(self, later: Self) -> Self {
        Self {
            use_bed: later.use_bed.or(self.use_bed),
        }
    }

    fn settle(self, event: &mut PlayerBedEnter) {
        if let Some(value) = self.use_bed {
            event.use_bed = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerBedLeavePatch {
    pub set_spawn_location: Option<bool>,
}

impl PlayerBedLeavePatch {
    fn then(self, later: Self) -> Self {
        Self {
            set_spawn_location: later.set_spawn_location.or(self.set_spawn_location),
        }
    }

    fn settle(self, event: &mut PlayerBedLeave) {
        if let Some(value) = self.set_spawn_location {
            event.set_spawn_location = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerBucketEmptyPatch {
    pub item_stack: Option<Option<ItemSpec>>,
}

impl PlayerBucketEmptyPatch {
    fn then(self, later: Self) -> Self {
        Self {
            item_stack: later.item_stack.or(self.item_stack),
        }
    }

    fn settle(self, event: &mut PlayerBucketEmpty) {
        if let Some(value) = self.item_stack {
            event.item_stack = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerBucketFillPatch {
    pub item_stack: Option<Option<ItemSpec>>,
}

impl PlayerBucketFillPatch {
    fn then(self, later: Self) -> Self {
        Self {
            item_stack: later.item_stack.or(self.item_stack),
        }
    }

    fn settle(self, event: &mut PlayerBucketFill) {
        if let Some(value) = self.item_stack {
            event.item_stack = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerCommandPreprocessPatch {
    pub message: Option<String>,
}

impl PlayerCommandPreprocessPatch {
    fn then(self, later: Self) -> Self {
        Self {
            message: later.message.or(self.message),
        }
    }

    fn settle(self, event: &mut PlayerCommandPreprocess) {
        if let Some(value) = self.message {
            event.message = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerCommandSendPatch {
    pub commands: SetDiff<String>,
}

impl PlayerCommandSendPatch {
    fn then(self, later: Self) -> Self {
        Self {
            commands: self.commands.then(later.commands),
        }
    }

    fn settle(self, event: &mut PlayerCommandSend) {
        event.commands = self.commands.apply(std::mem::take(&mut event.commands));
    }

    fn refusal(&self, event: &PlayerCommandSend) -> Option<String> {
        self.commands.refusal("commands", event.commands.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerDeathPatch {
    pub death_message: Option<Option<String>>,
    pub dropped_exp: Option<i32>,
    pub drops: ListDiff<ItemSpec>,
    pub keep_inventory: Option<bool>,
    pub keep_level: Option<bool>,
    pub new_exp: Option<i32>,
    pub new_level: Option<i32>,
    pub new_total_exp: Option<i32>,
}

impl PlayerDeathPatch {
    fn then(self, later: Self) -> Self {
        Self {
            death_message: later.death_message.or(self.death_message),
            dropped_exp: later.dropped_exp.or(self.dropped_exp),
            drops: self.drops.then(later.drops),
            keep_inventory: later.keep_inventory.or(self.keep_inventory),
            keep_level: later.keep_level.or(self.keep_level),
            new_exp: later.new_exp.or(self.new_exp),
            new_level: later.new_level.or(self.new_level),
            new_total_exp: later.new_total_exp.or(self.new_total_exp),
        }
    }

    fn settle(self, event: &mut PlayerDeath) {
        if let Some(value) = self.death_message {
            event.death_message = value;
        }
        if let Some(value) = self.dropped_exp {
            event.dropped_exp = value;
        }
        event.drops = self.drops.apply(std::mem::take(&mut event.drops));
        if let Some(value) = self.keep_inventory {
            event.keep_inventory = value;
        }
        if let Some(value) = self.keep_level {
            event.keep_level = value;
        }
        if let Some(value) = self.new_exp {
            event.new_exp = value;
        }
        if let Some(value) = self.new_level {
            event.new_level = value;
        }
        if let Some(value) = self.new_total_exp {
            event.new_total_exp = value;
        }
    }

    fn refusal(&self, event: &PlayerDeath) -> Option<String> {
        self.drops.refusal("drops", event.drops.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerDropItemPatch {
    pub item: Option<ItemSpec>,
}

impl PlayerDropItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            item: later.item.or(self.item),
        }
    }

    fn settle(self, event: &mut PlayerDropItem) {
        if let Some(value) = self.item {
            event.item = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerEditBookPatch {
    pub new_book_meta: Option<BookMeta>,
    pub signing: Option<bool>,
}

impl PlayerEditBookPatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_book_meta: later.new_book_meta.or(self.new_book_meta),
            signing: later.signing.or(self.signing),
        }
    }

    fn settle(self, event: &mut PlayerEditBook) {
        if let Some(value) = self.new_book_meta {
            event.new_book_meta = value;
        }
        if let Some(value) = self.signing {
            event.signing = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerEggThrowPatch {
    pub hatching: Option<bool>,
    pub hatching_type: Option<String>,
    pub num_hatches: Option<i8>,
}

impl PlayerEggThrowPatch {
    fn then(self, later: Self) -> Self {
        Self {
            hatching: later.hatching.or(self.hatching),
            hatching_type: later.hatching_type.or(self.hatching_type),
            num_hatches: later.num_hatches.or(self.num_hatches),
        }
    }

    fn settle(self, event: &mut PlayerEggThrow) {
        if let Some(value) = self.hatching {
            event.hatching = value;
        }
        if let Some(value) = self.hatching_type {
            event.hatching_type = value;
        }
        if let Some(value) = self.num_hatches {
            event.num_hatches = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerExpChangePatch {
    pub amount: Option<i32>,
}

impl PlayerExpChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            amount: later.amount.or(self.amount),
        }
    }

    fn settle(self, event: &mut PlayerExpChange) {
        if let Some(value) = self.amount {
            event.amount = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerExpCooldownChangePatch {
    pub new_cooldown: Option<i32>,
}

impl PlayerExpCooldownChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_cooldown: later.new_cooldown.or(self.new_cooldown),
        }
    }

    fn settle(self, event: &mut PlayerExpCooldownChange) {
        if let Some(value) = self.new_cooldown {
            event.new_cooldown = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerFishPatch {
    pub exp_to_drop: Option<i32>,
}

impl PlayerFishPatch {
    fn then(self, later: Self) -> Self {
        Self {
            exp_to_drop: later.exp_to_drop.or(self.exp_to_drop),
        }
    }

    fn settle(self, event: &mut PlayerFish) {
        if let Some(value) = self.exp_to_drop {
            event.exp_to_drop = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerHarvestBlockPatch {
    pub items_harvested: ListDiff<ItemSpec>,
}

impl PlayerHarvestBlockPatch {
    fn then(self, later: Self) -> Self {
        Self {
            items_harvested: self.items_harvested.then(later.items_harvested),
        }
    }

    fn settle(self, event: &mut PlayerHarvestBlock) {
        event.items_harvested = self
            .items_harvested
            .apply(std::mem::take(&mut event.items_harvested));
    }

    fn refusal(&self, event: &PlayerHarvestBlock) -> Option<String> {
        self.items_harvested
            .refusal("items_harvested", event.items_harvested.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerInteractPatch {
    pub use_interacted_block: Option<EventResult>,
    pub use_item_in_hand: Option<EventResult>,
}

impl PlayerInteractPatch {
    fn then(self, later: Self) -> Self {
        Self {
            use_interacted_block: later.use_interacted_block.or(self.use_interacted_block),
            use_item_in_hand: later.use_item_in_hand.or(self.use_item_in_hand),
        }
    }

    fn settle(self, event: &mut PlayerInteract) {
        if let Some(value) = self.use_interacted_block {
            event.use_interacted_block = value;
        }
        if let Some(value) = self.use_item_in_hand {
            event.use_item_in_hand = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            use_interacted_block: self.use_interacted_block,
            use_item_in_hand: self.use_item_in_hand,
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerItemConsumePatch {
    pub item: Option<ItemSpec>,
}

impl PlayerItemConsumePatch {
    fn then(self, later: Self) -> Self {
        Self {
            item: later.item.or(self.item),
        }
    }

    fn settle(self, event: &mut PlayerItemConsume) {
        if let Some(value) = self.item {
            event.item = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerItemDamagePatch {
    pub damage: Option<i32>,
}

impl PlayerItemDamagePatch {
    fn then(self, later: Self) -> Self {
        Self {
            damage: later.damage.or(self.damage),
        }
    }

    fn settle(self, event: &mut PlayerItemDamage) {
        if let Some(value) = self.damage {
            event.damage = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerItemMendPatch {
    pub repair_amount: Option<i32>,
}

impl PlayerItemMendPatch {
    fn then(self, later: Self) -> Self {
        Self {
            repair_amount: later.repair_amount.or(self.repair_amount),
        }
    }

    fn settle(self, event: &mut PlayerItemMend) {
        if let Some(value) = self.repair_amount {
            event.repair_amount = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerJoinPatch {
    pub join_message: Option<Option<String>>,
}

impl PlayerJoinPatch {
    fn then(self, later: Self) -> Self {
        Self {
            join_message: later.join_message.or(self.join_message),
        }
    }

    fn settle(self, event: &mut PlayerJoin) {
        if let Some(value) = self.join_message {
            event.join_message = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerKickPatch {
    pub leave_message: Option<String>,
    pub reason: Option<String>,
}

impl PlayerKickPatch {
    fn then(self, later: Self) -> Self {
        Self {
            leave_message: later.leave_message.or(self.leave_message),
            reason: later.reason.or(self.reason),
        }
    }

    fn settle(self, event: &mut PlayerKick) {
        if let Some(value) = self.leave_message {
            event.leave_message = value;
        }
        if let Some(value) = self.reason {
            event.reason = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerLeavePatch {
    pub quit_message: Option<Option<String>>,
}

impl PlayerLeavePatch {
    fn then(self, later: Self) -> Self {
        Self {
            quit_message: later.quit_message.or(self.quit_message),
        }
    }

    fn settle(self, event: &mut PlayerLeave) {
        if let Some(value) = self.quit_message {
            event.quit_message = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerLoginPatch {
    pub kick_message: Option<String>,
    pub result: Option<PlayerLoginResult>,
}

impl PlayerLoginPatch {
    fn then(self, later: Self) -> Self {
        Self {
            kick_message: later.kick_message.or(self.kick_message),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut PlayerLogin) {
        if let Some(value) = self.kick_message {
            event.kick_message = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerMovePatch {
    pub to: Option<Location>,
}

impl PlayerMovePatch {
    fn then(self, later: Self) -> Self {
        Self {
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut PlayerMove) {
        if let Some(value) = self.to {
            event.to = Some(value);
        }
    }

    fn refusal(&self) -> Option<String> {
        self.to.as_ref().and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerPortalPatch {
    pub can_create_portal: Option<bool>,
    pub creation_radius: Option<i32>,
    pub search_radius: Option<i32>,
    pub to: Option<Location>,
}

impl PlayerPortalPatch {
    fn then(self, later: Self) -> Self {
        Self {
            can_create_portal: later.can_create_portal.or(self.can_create_portal),
            creation_radius: later.creation_radius.or(self.creation_radius),
            search_radius: later.search_radius.or(self.search_radius),
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut PlayerPortal) {
        if let Some(value) = self.can_create_portal {
            event.can_create_portal = value;
        }
        if let Some(value) = self.creation_radius {
            event.creation_radius = value;
        }
        if let Some(value) = self.search_radius {
            event.search_radius = value;
        }
        if let Some(value) = self.to {
            event.to = Some(value);
        }
    }

    fn refusal(&self) -> Option<String> {
        self.to.as_ref().and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerRecipeBookClickPatch {
    pub recipe: Option<String>,
    pub shift_click: Option<bool>,
}

impl PlayerRecipeBookClickPatch {
    fn then(self, later: Self) -> Self {
        Self {
            recipe: later.recipe.or(self.recipe),
            shift_click: later.shift_click.or(self.shift_click),
        }
    }

    fn settle(self, event: &mut PlayerRecipeBookClick) {
        if let Some(value) = self.recipe {
            event.recipe = value;
        }
        if let Some(value) = self.shift_click {
            event.shift_click = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerRespawnPatch {
    pub respawn_location: Option<Location>,
}

impl PlayerRespawnPatch {
    fn then(self, later: Self) -> Self {
        Self {
            respawn_location: later.respawn_location.or(self.respawn_location),
        }
    }

    fn settle(self, event: &mut PlayerRespawn) {
        if let Some(value) = self.respawn_location {
            event.respawn_location = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.respawn_location.as_ref().and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerSpawnChangePatch {
    pub forced: Option<bool>,
    pub new_spawn: Option<Option<Location>>,
}

impl PlayerSpawnChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            forced: later.forced.or(self.forced),
            new_spawn: later.new_spawn.or(self.new_spawn),
        }
    }

    fn settle(self, event: &mut PlayerSpawnChange) {
        if let Some(value) = self.forced {
            event.forced = value;
        }
        if let Some(value) = self.new_spawn {
            event.new_spawn = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.new_spawn
            .as_ref()
            .and_then(Option::as_ref)
            .and_then(location_refusal)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerSwapHandItemsPatch {
    pub main_hand_item: Option<Option<ItemSpec>>,
    pub off_hand_item: Option<Option<ItemSpec>>,
}

impl PlayerSwapHandItemsPatch {
    fn then(self, later: Self) -> Self {
        Self {
            main_hand_item: later.main_hand_item.or(self.main_hand_item),
            off_hand_item: later.off_hand_item.or(self.off_hand_item),
        }
    }

    fn settle(self, event: &mut PlayerSwapHandItems) {
        if let Some(value) = self.main_hand_item {
            event.main_hand_item = value;
        }
        if let Some(value) = self.off_hand_item {
            event.off_hand_item = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerTeleportPatch {
    pub from: Option<Location>,
    pub to: Option<Location>,
}

impl PlayerTeleportPatch {
    fn then(self, later: Self) -> Self {
        Self {
            from: later.from.or(self.from),
            to: later.to.or(self.to),
        }
    }

    fn settle(self, event: &mut PlayerTeleport) {
        if let Some(value) = self.from {
            event.from = value;
        }
        if let Some(value) = self.to {
            event.to = Some(value);
        }
    }

    fn refusal(&self) -> Option<String> {
        self.from
            .as_ref()
            .and_then(location_refusal)
            .or_else(|| self.to.as_ref().and_then(location_refusal))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerVelocityPatch {
    pub velocity: Option<Vec3>,
}

impl PlayerVelocityPatch {
    fn then(self, later: Self) -> Self {
        Self {
            velocity: later.velocity.or(self.velocity),
        }
    }

    fn settle(self, event: &mut PlayerVelocity) {
        if let Some(value) = self.velocity {
            event.velocity = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.velocity
            .and_then(|value| vector_refusal("velocity", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PotionSplashPatch {
    pub intensities: Vec<(EntityView, f64)>,
}

impl PotionSplashPatch {
    fn then(self, later: Self) -> Self {
        Self {
            intensities: merge_indexed(self.intensities, later.intensities),
        }
    }

    fn settle(self, event: &mut PotionSplash) {
        event.intensities = merge_indexed(std::mem::take(&mut event.intensities), self.intensities);
    }

    fn refusal(&self) -> Option<String> {
        self.intensities
            .iter()
            .find_map(|(_, value)| number_refusal("intensities", *value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrepareAnvilPatch {
    pub result: Option<Option<ItemSpec>>,
}

impl PrepareAnvilPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut PrepareAnvil) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrepareGrindstonePatch {
    pub result: Option<Option<ItemSpec>>,
}

impl PrepareGrindstonePatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut PrepareGrindstone) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrepareItemCraftPatch {
    pub result: Option<Option<ItemSpec>>,
}

impl PrepareItemCraftPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut PrepareItemCraft) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrepareSmithingPatch {
    pub result: Option<Option<ItemSpec>>,
}

impl PrepareSmithingPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut PrepareSmithing) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RemoteServerCommandPatch {
    pub command: Option<String>,
}

impl RemoteServerCommandPatch {
    fn then(self, later: Self) -> Self {
        Self {
            command: later.command.or(self.command),
        }
    }

    fn settle(self, event: &mut RemoteServerCommand) {
        if let Some(value) = self.command {
            event.command = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SculkBloomPatch {
    pub charge: Option<i32>,
}

impl SculkBloomPatch {
    fn then(self, later: Self) -> Self {
        Self {
            charge: later.charge.or(self.charge),
        }
    }

    fn settle(self, event: &mut SculkBloom) {
        if let Some(value) = self.charge {
            event.charge = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerCommandPatch {
    pub command: Option<String>,
}

impl ServerCommandPatch {
    fn then(self, later: Self) -> Self {
        Self {
            command: later.command.or(self.command),
        }
    }

    fn settle(self, event: &mut ServerCommand) {
        if let Some(value) = self.command {
            event.command = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerListPingPatch {
    pub max_players: Option<i32>,
    pub motd: Option<String>,
    pub server_icon: Option<Vec<u8>>,
    pub hidden_players: SetDiff<Uuid>,
}

impl ServerListPingPatch {
    fn then(self, later: Self) -> Self {
        Self {
            max_players: later.max_players.or(self.max_players),
            motd: later.motd.or(self.motd),
            server_icon: later.server_icon.or(self.server_icon),
            hidden_players: self.hidden_players.then(later.hidden_players),
        }
    }

    fn settle(self, event: &mut ServerListPing) {
        if let Some(value) = self.max_players {
            event.max_players = value;
        }
        if let Some(value) = self.motd {
            event.motd = value;
        }
        event.hidden_players = self
            .hidden_players
            .apply(std::mem::take(&mut event.hidden_players));
    }

    fn refusal(&self, event: &ServerListPing) -> Option<String> {
        self.motd
            .as_deref()
            .and_then(|value| text_refusal("motd", value, 32767))
            .or_else(|| {
                self.server_icon
                    .as_deref()
                    .and_then(|value| icon_refusal("server_icon", value))
            })
            .or_else(|| {
                self.hidden_players
                    .refusal("hidden_players", event.hidden_players.len())
            })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SheepDyeWoolPatch {
    pub color: Option<DyeColor>,
}

impl SheepDyeWoolPatch {
    fn then(self, later: Self) -> Self {
        Self {
            color: later.color.or(self.color),
        }
    }

    fn settle(self, event: &mut SheepDyeWool) {
        if let Some(value) = self.color {
            event.color = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignChangePatch {
    pub lines: ListDiff<String>,
}

impl SignChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            lines: self.lines.then(later.lines),
        }
    }

    fn settle(self, event: &mut SignChange) {
        event.lines = self.lines.apply(std::mem::take(&mut event.lines));
    }

    fn refusal(&self, event: &SignChange) -> Option<String> {
        self.lines.slot_refusal("lines", event.lines.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SmithItemPatch {
    pub current_item: Option<Option<ItemSpec>>,
    pub cursor: Option<Option<ItemSpec>>,
    pub result: Option<EventResult>,
}

impl SmithItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            current_item: later.current_item.or(self.current_item),
            cursor: later.cursor.or(self.cursor),
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut SmithItem) {
        if let Some(value) = self.current_item {
            event.current_item = value;
        }
        if let Some(value) = self.cursor {
            event.cursor = value;
        }
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            current_item: self.current_item,
            cursor: self.cursor,
            result: self.result,
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TabCompletePatch {
    pub completions: ListDiff<String>,
}

impl TabCompletePatch {
    fn then(self, later: Self) -> Self {
        Self {
            completions: self.completions.then(later.completions),
        }
    }

    fn settle(self, event: &mut TabComplete) {
        event.completions = self
            .completions
            .apply(std::mem::take(&mut event.completions));
    }

    fn refusal(&self, event: &TabComplete) -> Option<String> {
        self.completions
            .refusal("completions", event.completions.len())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimeSkipPatch {
    pub skip_amount: Option<i64>,
}

impl TimeSkipPatch {
    fn then(self, later: Self) -> Self {
        Self {
            skip_amount: later.skip_amount.or(self.skip_amount),
        }
    }

    fn settle(self, event: &mut TimeSkip) {
        if let Some(value) = self.skip_amount {
            event.skip_amount = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TradeSelectPatch {
    pub result: Option<EventResult>,
}

impl TradeSelectPatch {
    fn then(self, later: Self) -> Self {
        Self {
            result: later.result.or(self.result),
        }
    }

    fn settle(self, event: &mut TradeSelect) {
        if let Some(value) = self.result {
            event.result = value;
        }
    }

    fn when_cancelled(self) -> Option<Self> {
        let kept = Self {
            result: self.result,
        };
        (kept != Self::default()).then_some(kept)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VaultDisplayItemPatch {
    pub display_item: Option<Option<ItemSpec>>,
}

impl VaultDisplayItemPatch {
    fn then(self, later: Self) -> Self {
        Self {
            display_item: later.display_item.or(self.display_item),
        }
    }

    fn settle(self, event: &mut VaultDisplayItem) {
        if let Some(value) = self.display_item {
            event.display_item = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VehicleDamagePatch {
    pub damage: Option<f64>,
}

impl VehicleDamagePatch {
    fn then(self, later: Self) -> Self {
        Self {
            damage: later.damage.or(self.damage),
        }
    }

    fn settle(self, event: &mut VehicleDamage) {
        if let Some(value) = self.damage {
            event.damage = value;
        }
    }

    fn refusal(&self) -> Option<String> {
        self.damage
            .and_then(|value| number_refusal("damage", value))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VillagerAcquireTradePatch {
    pub recipe: Option<Trade>,
}

impl VillagerAcquireTradePatch {
    fn then(self, later: Self) -> Self {
        Self {
            recipe: later.recipe.or(self.recipe),
        }
    }

    fn settle(self, event: &mut VillagerAcquireTrade) {
        if let Some(value) = self.recipe {
            event.recipe = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VillagerCareerChangePatch {
    pub profession: Option<String>,
}

impl VillagerCareerChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            profession: later.profession.or(self.profession),
        }
    }

    fn settle(self, event: &mut VillagerCareerChange) {
        if let Some(value) = self.profession {
            event.profession = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VillagerReplenishTradePatch {
    pub recipe: Option<Trade>,
}

impl VillagerReplenishTradePatch {
    fn then(self, later: Self) -> Self {
        Self {
            recipe: later.recipe.or(self.recipe),
        }
    }

    fn settle(self, event: &mut VillagerReplenishTrade) {
        if let Some(value) = self.recipe {
            event.recipe = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VillagerReputationChangePatch {
    pub new_value: Option<i32>,
}

impl VillagerReputationChangePatch {
    fn then(self, later: Self) -> Self {
        Self {
            new_value: later.new_value.or(self.new_value),
        }
    }

    fn settle(self, event: &mut VillagerReputationChange) {
        if let Some(value) = self.new_value {
            event.new_value = value;
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    AreaEffectCloudApply(AreaEffectCloudApply),
    ArrowBodyCountChange(ArrowBodyCountChange),
    AsyncPlayerChat(AsyncPlayerChat),
    AsyncPlayerPreLogin(AsyncPlayerPreLogin),
    AsyncStructureGenerate(AsyncStructureGenerate),
    AsyncStructureSpawn(AsyncStructureSpawn),
    BatToggleSleep(BatToggleSleep),
    BellResonate(BellResonate),
    BellRing(BellRing),
    BlockBreak(BlockBreak),
    BlockBrush(BlockBrush),
    BlockBurn(BlockBurn),
    BlockCanBuild(BlockCanBuild),
    BlockCook(BlockCook),
    BlockDamage(BlockDamage),
    BlockDamageAbort(BlockDamageAbort),
    BlockDispense(BlockDispense),
    BlockDispenseArmor(BlockDispenseArmor),
    BlockDispenseLoot(BlockDispenseLoot),
    BlockDropItem(BlockDropItem),
    BlockExp(BlockExp),
    BlockExplode(BlockExplode),
    BlockFade(BlockFade),
    BlockFertilize(BlockFertilize),
    BlockForm(BlockForm),
    BlockFromTo(BlockFromTo),
    BlockGrow(BlockGrow),
    BlockIgnite(BlockIgnite),
    BlockMultiPlace(BlockMultiPlace),
    BlockPhysics(BlockPhysics),
    BlockPistonExtend(BlockPistonExtend),
    BlockPistonRetract(BlockPistonRetract),
    BlockPlace(BlockPlace),
    BlockReceiveGame(BlockReceiveGame),
    BlockRedstone(BlockRedstone),
    BlockShearEntity(BlockShearEntity),
    BlockSpread(BlockSpread),
    Brew(Brew),
    BrewingStandFuel(BrewingStandFuel),
    BrewingStart(BrewingStart),
    BroadcastMessage(BroadcastMessage),
    CampfireStart(CampfireStart),
    CauldronLevelChange(CauldronLevelChange),
    ChunkLoad(ChunkLoad),
    ChunkPopulate(ChunkPopulate),
    ChunkUnload(ChunkUnload),
    CraftItem(CraftItem),
    CrafterCraft(CrafterCraft),
    CreatureSpawn(CreatureSpawn),
    CreeperPower(CreeperPower),
    CubeMobSplit(CubeMobSplit),
    EnchantItem(EnchantItem),
    EnderDragonChangePhase(EnderDragonChangePhase),
    EntitiesLoad(EntitiesLoad),
    EntitiesUnload(EntitiesUnload),
    EntityAirChange(EntityAirChange),
    EntityBlockForm(EntityBlockForm),
    EntityBreakDoor(EntityBreakDoor),
    EntityBreed(EntityBreed),
    EntityChangeBlock(EntityChangeBlock),
    EntityCombust(EntityCombust),
    EntityCombustByBlock(EntityCombustByBlock),
    EntityCombustByEntity(EntityCombustByEntity),
    EntityDamage(EntityDamage),
    EntityDamageByBlock(EntityDamageByBlock),
    EntityDamageByEntity(EntityDamageByEntity),
    EntityDeath(EntityDeath),
    EntityDismount(EntityDismount),
    EntityDropItem(EntityDropItem),
    EntityEnterBlock(EntityEnterBlock),
    EntityEnterLoveMode(EntityEnterLoveMode),
    EntityExhaustion(EntityExhaustion),
    EntityExplode(EntityExplode),
    EntityInteract(EntityInteract),
    EntityKnockback(EntityKnockback),
    EntityKnockbackByEntity(EntityKnockbackByEntity),
    EntityMount(EntityMount),
    EntityPickupItem(EntityPickupItem),
    EntityPlace(EntityPlace),
    EntityPortal(EntityPortal),
    EntityPortalEnter(EntityPortalEnter),
    EntityPortalExit(EntityPortalExit),
    EntityPoseChange(EntityPoseChange),
    EntityPotionEffect(EntityPotionEffect),
    EntityRegainHealth(EntityRegainHealth),
    EntityRemove(EntityRemove),
    EntityResurrect(EntityResurrect),
    EntityShootBow(EntityShootBow),
    EntitySpawn(EntitySpawn),
    EntitySpellCast(EntitySpellCast),
    EntityTame(EntityTame),
    EntityTarget(EntityTarget),
    EntityTargetBlock(EntityTargetBlock),
    EntityTargetLivingEntity(EntityTargetLivingEntity),
    EntityTeleport(EntityTeleport),
    EntityToggleGlide(EntityToggleGlide),
    EntityToggleSwim(EntityToggleSwim),
    EntityTransform(EntityTransform),
    EntityUnleash(EntityUnleash),
    ExpBottle(ExpBottle),
    ExplosionPrime(ExplosionPrime),
    FireworkExplode(FireworkExplode),
    FluidLevelChange(FluidLevelChange),
    FoodLevelChange(FoodLevelChange),
    FurnaceBurn(FurnaceBurn),
    FurnaceExtract(FurnaceExtract),
    FurnaceSmelt(FurnaceSmelt),
    FurnaceStartSmelt(FurnaceStartSmelt),
    GenericGame(GenericGame),
    HangingBreak(HangingBreak),
    HangingBreakByEntity(HangingBreakByEntity),
    HangingPlace(HangingPlace),
    HopperInventorySearch(HopperInventorySearch),
    HorseJump(HorseJump),
    InventoryClick(InventoryClick),
    InventoryClose(InventoryClose),
    InventoryCreative(InventoryCreative),
    InventoryDrag(InventoryDrag),
    InventoryMoveItem(InventoryMoveItem),
    InventoryOpen(InventoryOpen),
    InventoryPickupItem(InventoryPickupItem),
    ItemDespawn(ItemDespawn),
    ItemMerge(ItemMerge),
    ItemSpawn(ItemSpawn),
    LeavesDecay(LeavesDecay),
    LightningStrike(LightningStrike),
    LingeringPotionSplash(LingeringPotionSplash),
    LootGenerate(LootGenerate),
    MapInitialize(MapInitialize),
    MoistureChange(MoistureChange),
    NotePlay(NotePlay),
    PigZap(PigZap),
    PigZombieAnger(PigZombieAnger),
    PiglinBarter(PiglinBarter),
    PlayerAdvancementCriterionGrant(PlayerAdvancementCriterionGrant),
    PlayerAdvancementDone(PlayerAdvancementDone),
    PlayerAnimation(PlayerAnimation),
    PlayerArmorStandManipulate(PlayerArmorStandManipulate),
    PlayerBedEnter(PlayerBedEnter),
    PlayerBedLeave(PlayerBedLeave),
    PlayerBucketEmpty(PlayerBucketEmpty),
    PlayerBucketEntity(PlayerBucketEntity),
    PlayerBucketFill(PlayerBucketFill),
    PlayerChangedMainHand(PlayerChangedMainHand),
    PlayerChangedWorld(PlayerChangedWorld),
    PlayerCommandPreprocess(PlayerCommandPreprocess),
    PlayerCommandSend(PlayerCommandSend),
    PlayerCustomClick(PlayerCustomClick),
    PlayerDeath(PlayerDeath),
    PlayerDropItem(PlayerDropItem),
    PlayerEditBook(PlayerEditBook),
    PlayerEggThrow(PlayerEggThrow),
    PlayerExpChange(PlayerExpChange),
    PlayerExpCooldownChange(PlayerExpCooldownChange),
    PlayerFish(PlayerFish),
    PlayerGameModeChange(PlayerGameModeChange),
    PlayerHarvestBlock(PlayerHarvestBlock),
    PlayerHideEntity(PlayerHideEntity),
    PlayerInput(PlayerInput),
    PlayerInteract(PlayerInteract),
    PlayerInteractAtEntity(PlayerInteractAtEntity),
    PlayerInteractEntity(PlayerInteractEntity),
    PlayerItemBreak(PlayerItemBreak),
    PlayerItemConsume(PlayerItemConsume),
    PlayerItemDamage(PlayerItemDamage),
    PlayerItemHeld(PlayerItemHeld),
    PlayerItemMend(PlayerItemMend),
    PlayerJoin(PlayerJoin),
    PlayerKick(PlayerKick),
    PlayerLeashEntity(PlayerLeashEntity),
    PlayerLeave(PlayerLeave),
    PlayerLevelChange(PlayerLevelChange),
    PlayerLinksSend(PlayerLinksSend),
    PlayerLocaleChange(PlayerLocaleChange),
    PlayerLogin(PlayerLogin),
    PlayerMove(PlayerMove),
    PlayerPickupArrow(PlayerPickupArrow),
    PlayerPortal(PlayerPortal),
    PlayerRecipeBookClick(PlayerRecipeBookClick),
    PlayerRecipeBookSettingsChange(PlayerRecipeBookSettingsChange),
    PlayerRecipeDiscover(PlayerRecipeDiscover),
    PlayerResourcePackStatus(PlayerResourcePackStatus),
    PlayerRespawn(PlayerRespawn),
    PlayerRiptide(PlayerRiptide),
    PlayerShearEntity(PlayerShearEntity),
    PlayerShowEntity(PlayerShowEntity),
    PlayerSignOpen(PlayerSignOpen),
    PlayerSpawnChange(PlayerSpawnChange),
    PlayerStatisticIncrement(PlayerStatisticIncrement),
    PlayerSwapHandItems(PlayerSwapHandItems),
    PlayerTakeLecternBook(PlayerTakeLecternBook),
    PlayerTeleport(PlayerTeleport),
    PlayerToggleFlight(PlayerToggleFlight),
    PlayerToggleSneak(PlayerToggleSneak),
    PlayerToggleSprint(PlayerToggleSprint),
    PlayerUnleashEntity(PlayerUnleashEntity),
    PlayerVelocity(PlayerVelocity),
    PluginDisable(PluginDisable),
    PluginEnable(PluginEnable),
    PortalCreate(PortalCreate),
    PotionSplash(PotionSplash),
    PrepareAnvil(PrepareAnvil),
    PrepareGrindstone(PrepareGrindstone),
    PrepareItemCraft(PrepareItemCraft),
    PrepareItemEnchant(PrepareItemEnchant),
    PrepareSmithing(PrepareSmithing),
    ProjectileHit(ProjectileHit),
    ProjectileLaunch(ProjectileLaunch),
    RaidFinish(RaidFinish),
    RaidSpawnWave(RaidSpawnWave),
    RaidStop(RaidStop),
    RaidTrigger(RaidTrigger),
    RemoteServerCommand(RemoteServerCommand),
    SculkBloom(SculkBloom),
    ServerCommand(ServerCommand),
    ServerListPing(ServerListPing),
    ServerLoad(ServerLoad),
    SheepDyeWool(SheepDyeWool),
    SheepRegrowWool(SheepRegrowWool),
    SignChange(SignChange),
    SmithItem(SmithItem),
    SpawnChange(SpawnChange),
    SpawnerSpawn(SpawnerSpawn),
    SpongeAbsorb(SpongeAbsorb),
    StriderTemperatureChange(StriderTemperatureChange),
    StructureGrow(StructureGrow),
    TNTPrime(TNTPrime),
    TabComplete(TabComplete),
    ThunderChange(ThunderChange),
    TimeSkip(TimeSkip),
    TradeSelect(TradeSelect),
    TrialSpawnerSpawn(TrialSpawnerSpawn),
    VaultDisplayItem(VaultDisplayItem),
    VehicleBlockCollision(VehicleBlockCollision),
    VehicleCreate(VehicleCreate),
    VehicleDamage(VehicleDamage),
    VehicleDestroy(VehicleDestroy),
    VehicleEnter(VehicleEnter),
    VehicleEntityCollision(VehicleEntityCollision),
    VehicleExit(VehicleExit),
    VehicleMove(VehicleMove),
    VehicleUpdate(VehicleUpdate),
    VillagerAcquireTrade(VillagerAcquireTrade),
    VillagerCareerChange(VillagerCareerChange),
    VillagerReplenishTrade(VillagerReplenishTrade),
    VillagerReputationChange(VillagerReputationChange),
    WeatherChange(WeatherChange),
    WorldInit(WorldInit),
    WorldLoad(WorldLoad),
    WorldSave(WorldSave),
    WorldUnload(WorldUnload),
}

impl EventPayload {
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::AreaEffectCloudApply(_) => EventKind::AreaEffectCloudApply,
            Self::ArrowBodyCountChange(_) => EventKind::ArrowBodyCountChange,
            Self::AsyncPlayerChat(_) => EventKind::AsyncPlayerChat,
            Self::AsyncPlayerPreLogin(_) => EventKind::AsyncPlayerPreLogin,
            Self::AsyncStructureGenerate(_) => EventKind::AsyncStructureGenerate,
            Self::AsyncStructureSpawn(_) => EventKind::AsyncStructureSpawn,
            Self::BatToggleSleep(_) => EventKind::BatToggleSleep,
            Self::BellResonate(_) => EventKind::BellResonate,
            Self::BellRing(_) => EventKind::BellRing,
            Self::BlockBreak(_) => EventKind::BlockBreak,
            Self::BlockBrush(_) => EventKind::BlockBrush,
            Self::BlockBurn(_) => EventKind::BlockBurn,
            Self::BlockCanBuild(_) => EventKind::BlockCanBuild,
            Self::BlockCook(_) => EventKind::BlockCook,
            Self::BlockDamage(_) => EventKind::BlockDamage,
            Self::BlockDamageAbort(_) => EventKind::BlockDamageAbort,
            Self::BlockDispense(_) => EventKind::BlockDispense,
            Self::BlockDispenseArmor(_) => EventKind::BlockDispenseArmor,
            Self::BlockDispenseLoot(_) => EventKind::BlockDispenseLoot,
            Self::BlockDropItem(_) => EventKind::BlockDropItem,
            Self::BlockExp(_) => EventKind::BlockExp,
            Self::BlockExplode(_) => EventKind::BlockExplode,
            Self::BlockFade(_) => EventKind::BlockFade,
            Self::BlockFertilize(_) => EventKind::BlockFertilize,
            Self::BlockForm(_) => EventKind::BlockForm,
            Self::BlockFromTo(_) => EventKind::BlockFromTo,
            Self::BlockGrow(_) => EventKind::BlockGrow,
            Self::BlockIgnite(_) => EventKind::BlockIgnite,
            Self::BlockMultiPlace(_) => EventKind::BlockMultiPlace,
            Self::BlockPhysics(_) => EventKind::BlockPhysics,
            Self::BlockPistonExtend(_) => EventKind::BlockPistonExtend,
            Self::BlockPistonRetract(_) => EventKind::BlockPistonRetract,
            Self::BlockPlace(_) => EventKind::BlockPlace,
            Self::BlockReceiveGame(_) => EventKind::BlockReceiveGame,
            Self::BlockRedstone(_) => EventKind::BlockRedstone,
            Self::BlockShearEntity(_) => EventKind::BlockShearEntity,
            Self::BlockSpread(_) => EventKind::BlockSpread,
            Self::Brew(_) => EventKind::Brew,
            Self::BrewingStandFuel(_) => EventKind::BrewingStandFuel,
            Self::BrewingStart(_) => EventKind::BrewingStart,
            Self::BroadcastMessage(_) => EventKind::BroadcastMessage,
            Self::CampfireStart(_) => EventKind::CampfireStart,
            Self::CauldronLevelChange(_) => EventKind::CauldronLevelChange,
            Self::ChunkLoad(_) => EventKind::ChunkLoad,
            Self::ChunkPopulate(_) => EventKind::ChunkPopulate,
            Self::ChunkUnload(_) => EventKind::ChunkUnload,
            Self::CraftItem(_) => EventKind::CraftItem,
            Self::CrafterCraft(_) => EventKind::CrafterCraft,
            Self::CreatureSpawn(_) => EventKind::CreatureSpawn,
            Self::CreeperPower(_) => EventKind::CreeperPower,
            Self::CubeMobSplit(_) => EventKind::CubeMobSplit,
            Self::EnchantItem(_) => EventKind::EnchantItem,
            Self::EnderDragonChangePhase(_) => EventKind::EnderDragonChangePhase,
            Self::EntitiesLoad(_) => EventKind::EntitiesLoad,
            Self::EntitiesUnload(_) => EventKind::EntitiesUnload,
            Self::EntityAirChange(_) => EventKind::EntityAirChange,
            Self::EntityBlockForm(_) => EventKind::EntityBlockForm,
            Self::EntityBreakDoor(_) => EventKind::EntityBreakDoor,
            Self::EntityBreed(_) => EventKind::EntityBreed,
            Self::EntityChangeBlock(_) => EventKind::EntityChangeBlock,
            Self::EntityCombust(_) => EventKind::EntityCombust,
            Self::EntityCombustByBlock(_) => EventKind::EntityCombustByBlock,
            Self::EntityCombustByEntity(_) => EventKind::EntityCombustByEntity,
            Self::EntityDamage(_) => EventKind::EntityDamage,
            Self::EntityDamageByBlock(_) => EventKind::EntityDamageByBlock,
            Self::EntityDamageByEntity(_) => EventKind::EntityDamageByEntity,
            Self::EntityDeath(_) => EventKind::EntityDeath,
            Self::EntityDismount(_) => EventKind::EntityDismount,
            Self::EntityDropItem(_) => EventKind::EntityDropItem,
            Self::EntityEnterBlock(_) => EventKind::EntityEnterBlock,
            Self::EntityEnterLoveMode(_) => EventKind::EntityEnterLoveMode,
            Self::EntityExhaustion(_) => EventKind::EntityExhaustion,
            Self::EntityExplode(_) => EventKind::EntityExplode,
            Self::EntityInteract(_) => EventKind::EntityInteract,
            Self::EntityKnockback(_) => EventKind::EntityKnockback,
            Self::EntityKnockbackByEntity(_) => EventKind::EntityKnockbackByEntity,
            Self::EntityMount(_) => EventKind::EntityMount,
            Self::EntityPickupItem(_) => EventKind::EntityPickupItem,
            Self::EntityPlace(_) => EventKind::EntityPlace,
            Self::EntityPortal(_) => EventKind::EntityPortal,
            Self::EntityPortalEnter(_) => EventKind::EntityPortalEnter,
            Self::EntityPortalExit(_) => EventKind::EntityPortalExit,
            Self::EntityPoseChange(_) => EventKind::EntityPoseChange,
            Self::EntityPotionEffect(_) => EventKind::EntityPotionEffect,
            Self::EntityRegainHealth(_) => EventKind::EntityRegainHealth,
            Self::EntityRemove(_) => EventKind::EntityRemove,
            Self::EntityResurrect(_) => EventKind::EntityResurrect,
            Self::EntityShootBow(_) => EventKind::EntityShootBow,
            Self::EntitySpawn(_) => EventKind::EntitySpawn,
            Self::EntitySpellCast(_) => EventKind::EntitySpellCast,
            Self::EntityTame(_) => EventKind::EntityTame,
            Self::EntityTarget(_) => EventKind::EntityTarget,
            Self::EntityTargetBlock(_) => EventKind::EntityTargetBlock,
            Self::EntityTargetLivingEntity(_) => EventKind::EntityTargetLivingEntity,
            Self::EntityTeleport(_) => EventKind::EntityTeleport,
            Self::EntityToggleGlide(_) => EventKind::EntityToggleGlide,
            Self::EntityToggleSwim(_) => EventKind::EntityToggleSwim,
            Self::EntityTransform(_) => EventKind::EntityTransform,
            Self::EntityUnleash(_) => EventKind::EntityUnleash,
            Self::ExpBottle(_) => EventKind::ExpBottle,
            Self::ExplosionPrime(_) => EventKind::ExplosionPrime,
            Self::FireworkExplode(_) => EventKind::FireworkExplode,
            Self::FluidLevelChange(_) => EventKind::FluidLevelChange,
            Self::FoodLevelChange(_) => EventKind::FoodLevelChange,
            Self::FurnaceBurn(_) => EventKind::FurnaceBurn,
            Self::FurnaceExtract(_) => EventKind::FurnaceExtract,
            Self::FurnaceSmelt(_) => EventKind::FurnaceSmelt,
            Self::FurnaceStartSmelt(_) => EventKind::FurnaceStartSmelt,
            Self::GenericGame(_) => EventKind::GenericGame,
            Self::HangingBreak(_) => EventKind::HangingBreak,
            Self::HangingBreakByEntity(_) => EventKind::HangingBreakByEntity,
            Self::HangingPlace(_) => EventKind::HangingPlace,
            Self::HopperInventorySearch(_) => EventKind::HopperInventorySearch,
            Self::HorseJump(_) => EventKind::HorseJump,
            Self::InventoryClick(_) => EventKind::InventoryClick,
            Self::InventoryClose(_) => EventKind::InventoryClose,
            Self::InventoryCreative(_) => EventKind::InventoryCreative,
            Self::InventoryDrag(_) => EventKind::InventoryDrag,
            Self::InventoryMoveItem(_) => EventKind::InventoryMoveItem,
            Self::InventoryOpen(_) => EventKind::InventoryOpen,
            Self::InventoryPickupItem(_) => EventKind::InventoryPickupItem,
            Self::ItemDespawn(_) => EventKind::ItemDespawn,
            Self::ItemMerge(_) => EventKind::ItemMerge,
            Self::ItemSpawn(_) => EventKind::ItemSpawn,
            Self::LeavesDecay(_) => EventKind::LeavesDecay,
            Self::LightningStrike(_) => EventKind::LightningStrike,
            Self::LingeringPotionSplash(_) => EventKind::LingeringPotionSplash,
            Self::LootGenerate(_) => EventKind::LootGenerate,
            Self::MapInitialize(_) => EventKind::MapInitialize,
            Self::MoistureChange(_) => EventKind::MoistureChange,
            Self::NotePlay(_) => EventKind::NotePlay,
            Self::PigZap(_) => EventKind::PigZap,
            Self::PigZombieAnger(_) => EventKind::PigZombieAnger,
            Self::PiglinBarter(_) => EventKind::PiglinBarter,
            Self::PlayerAdvancementCriterionGrant(_) => EventKind::PlayerAdvancementCriterionGrant,
            Self::PlayerAdvancementDone(_) => EventKind::PlayerAdvancementDone,
            Self::PlayerAnimation(_) => EventKind::PlayerAnimation,
            Self::PlayerArmorStandManipulate(_) => EventKind::PlayerArmorStandManipulate,
            Self::PlayerBedEnter(_) => EventKind::PlayerBedEnter,
            Self::PlayerBedLeave(_) => EventKind::PlayerBedLeave,
            Self::PlayerBucketEmpty(_) => EventKind::PlayerBucketEmpty,
            Self::PlayerBucketEntity(_) => EventKind::PlayerBucketEntity,
            Self::PlayerBucketFill(_) => EventKind::PlayerBucketFill,
            Self::PlayerChangedMainHand(_) => EventKind::PlayerChangedMainHand,
            Self::PlayerChangedWorld(_) => EventKind::PlayerChangedWorld,
            Self::PlayerCommandPreprocess(_) => EventKind::PlayerCommandPreprocess,
            Self::PlayerCommandSend(_) => EventKind::PlayerCommandSend,
            Self::PlayerCustomClick(_) => EventKind::PlayerCustomClick,
            Self::PlayerDeath(_) => EventKind::PlayerDeath,
            Self::PlayerDropItem(_) => EventKind::PlayerDropItem,
            Self::PlayerEditBook(_) => EventKind::PlayerEditBook,
            Self::PlayerEggThrow(_) => EventKind::PlayerEggThrow,
            Self::PlayerExpChange(_) => EventKind::PlayerExpChange,
            Self::PlayerExpCooldownChange(_) => EventKind::PlayerExpCooldownChange,
            Self::PlayerFish(_) => EventKind::PlayerFish,
            Self::PlayerGameModeChange(_) => EventKind::PlayerGameModeChange,
            Self::PlayerHarvestBlock(_) => EventKind::PlayerHarvestBlock,
            Self::PlayerHideEntity(_) => EventKind::PlayerHideEntity,
            Self::PlayerInput(_) => EventKind::PlayerInput,
            Self::PlayerInteract(_) => EventKind::PlayerInteract,
            Self::PlayerInteractAtEntity(_) => EventKind::PlayerInteractAtEntity,
            Self::PlayerInteractEntity(_) => EventKind::PlayerInteractEntity,
            Self::PlayerItemBreak(_) => EventKind::PlayerItemBreak,
            Self::PlayerItemConsume(_) => EventKind::PlayerItemConsume,
            Self::PlayerItemDamage(_) => EventKind::PlayerItemDamage,
            Self::PlayerItemHeld(_) => EventKind::PlayerItemHeld,
            Self::PlayerItemMend(_) => EventKind::PlayerItemMend,
            Self::PlayerJoin(_) => EventKind::PlayerJoin,
            Self::PlayerKick(_) => EventKind::PlayerKick,
            Self::PlayerLeashEntity(_) => EventKind::PlayerLeashEntity,
            Self::PlayerLeave(_) => EventKind::PlayerLeave,
            Self::PlayerLevelChange(_) => EventKind::PlayerLevelChange,
            Self::PlayerLinksSend(_) => EventKind::PlayerLinksSend,
            Self::PlayerLocaleChange(_) => EventKind::PlayerLocaleChange,
            Self::PlayerLogin(_) => EventKind::PlayerLogin,
            Self::PlayerMove(_) => EventKind::PlayerMove,
            Self::PlayerPickupArrow(_) => EventKind::PlayerPickupArrow,
            Self::PlayerPortal(_) => EventKind::PlayerPortal,
            Self::PlayerRecipeBookClick(_) => EventKind::PlayerRecipeBookClick,
            Self::PlayerRecipeBookSettingsChange(_) => EventKind::PlayerRecipeBookSettingsChange,
            Self::PlayerRecipeDiscover(_) => EventKind::PlayerRecipeDiscover,
            Self::PlayerResourcePackStatus(_) => EventKind::PlayerResourcePackStatus,
            Self::PlayerRespawn(_) => EventKind::PlayerRespawn,
            Self::PlayerRiptide(_) => EventKind::PlayerRiptide,
            Self::PlayerShearEntity(_) => EventKind::PlayerShearEntity,
            Self::PlayerShowEntity(_) => EventKind::PlayerShowEntity,
            Self::PlayerSignOpen(_) => EventKind::PlayerSignOpen,
            Self::PlayerSpawnChange(_) => EventKind::PlayerSpawnChange,
            Self::PlayerStatisticIncrement(_) => EventKind::PlayerStatisticIncrement,
            Self::PlayerSwapHandItems(_) => EventKind::PlayerSwapHandItems,
            Self::PlayerTakeLecternBook(_) => EventKind::PlayerTakeLecternBook,
            Self::PlayerTeleport(_) => EventKind::PlayerTeleport,
            Self::PlayerToggleFlight(_) => EventKind::PlayerToggleFlight,
            Self::PlayerToggleSneak(_) => EventKind::PlayerToggleSneak,
            Self::PlayerToggleSprint(_) => EventKind::PlayerToggleSprint,
            Self::PlayerUnleashEntity(_) => EventKind::PlayerUnleashEntity,
            Self::PlayerVelocity(_) => EventKind::PlayerVelocity,
            Self::PluginDisable(_) => EventKind::PluginDisable,
            Self::PluginEnable(_) => EventKind::PluginEnable,
            Self::PortalCreate(_) => EventKind::PortalCreate,
            Self::PotionSplash(_) => EventKind::PotionSplash,
            Self::PrepareAnvil(_) => EventKind::PrepareAnvil,
            Self::PrepareGrindstone(_) => EventKind::PrepareGrindstone,
            Self::PrepareItemCraft(_) => EventKind::PrepareItemCraft,
            Self::PrepareItemEnchant(_) => EventKind::PrepareItemEnchant,
            Self::PrepareSmithing(_) => EventKind::PrepareSmithing,
            Self::ProjectileHit(_) => EventKind::ProjectileHit,
            Self::ProjectileLaunch(_) => EventKind::ProjectileLaunch,
            Self::RaidFinish(_) => EventKind::RaidFinish,
            Self::RaidSpawnWave(_) => EventKind::RaidSpawnWave,
            Self::RaidStop(_) => EventKind::RaidStop,
            Self::RaidTrigger(_) => EventKind::RaidTrigger,
            Self::RemoteServerCommand(_) => EventKind::RemoteServerCommand,
            Self::SculkBloom(_) => EventKind::SculkBloom,
            Self::ServerCommand(_) => EventKind::ServerCommand,
            Self::ServerListPing(_) => EventKind::ServerListPing,
            Self::ServerLoad(_) => EventKind::ServerLoad,
            Self::SheepDyeWool(_) => EventKind::SheepDyeWool,
            Self::SheepRegrowWool(_) => EventKind::SheepRegrowWool,
            Self::SignChange(_) => EventKind::SignChange,
            Self::SmithItem(_) => EventKind::SmithItem,
            Self::SpawnChange(_) => EventKind::SpawnChange,
            Self::SpawnerSpawn(_) => EventKind::SpawnerSpawn,
            Self::SpongeAbsorb(_) => EventKind::SpongeAbsorb,
            Self::StriderTemperatureChange(_) => EventKind::StriderTemperatureChange,
            Self::StructureGrow(_) => EventKind::StructureGrow,
            Self::TNTPrime(_) => EventKind::TNTPrime,
            Self::TabComplete(_) => EventKind::TabComplete,
            Self::ThunderChange(_) => EventKind::ThunderChange,
            Self::TimeSkip(_) => EventKind::TimeSkip,
            Self::TradeSelect(_) => EventKind::TradeSelect,
            Self::TrialSpawnerSpawn(_) => EventKind::TrialSpawnerSpawn,
            Self::VaultDisplayItem(_) => EventKind::VaultDisplayItem,
            Self::VehicleBlockCollision(_) => EventKind::VehicleBlockCollision,
            Self::VehicleCreate(_) => EventKind::VehicleCreate,
            Self::VehicleDamage(_) => EventKind::VehicleDamage,
            Self::VehicleDestroy(_) => EventKind::VehicleDestroy,
            Self::VehicleEnter(_) => EventKind::VehicleEnter,
            Self::VehicleEntityCollision(_) => EventKind::VehicleEntityCollision,
            Self::VehicleExit(_) => EventKind::VehicleExit,
            Self::VehicleMove(_) => EventKind::VehicleMove,
            Self::VehicleUpdate(_) => EventKind::VehicleUpdate,
            Self::VillagerAcquireTrade(_) => EventKind::VillagerAcquireTrade,
            Self::VillagerCareerChange(_) => EventKind::VillagerCareerChange,
            Self::VillagerReplenishTrade(_) => EventKind::VillagerReplenishTrade,
            Self::VillagerReputationChange(_) => EventKind::VillagerReputationChange,
            Self::WeatherChange(_) => EventKind::WeatherChange,
            Self::WorldInit(_) => EventKind::WorldInit,
            Self::WorldLoad(_) => EventKind::WorldLoad,
            Self::WorldSave(_) => EventKind::WorldSave,
            Self::WorldUnload(_) => EventKind::WorldUnload,
        }
    }

    pub const fn cancelled(&self) -> bool {
        match self {
            Self::CraftItem(event) => matches!(event.result, EventResult::Deny),
            Self::InventoryClick(event) => matches!(event.result, EventResult::Deny),
            Self::InventoryCreative(event) => matches!(event.result, EventResult::Deny),
            Self::InventoryDrag(event) => matches!(event.result, EventResult::Deny),
            Self::PlayerInteract(event) => matches!(event.use_interacted_block, EventResult::Deny),
            Self::SmithItem(event) => matches!(event.result, EventResult::Deny),
            Self::TradeSelect(event) => matches!(event.result, EventResult::Deny),
            _ => false,
        }
    }

    pub fn block_positions(&self, out: &mut Vec<BlockPos>) {
        match self {
            Self::BellResonate(event) => {
                out.push(event.block);
            }
            Self::BellRing(event) => {
                out.push(event.block);
            }
            Self::BlockBreak(event) => {
                out.push(event.block);
            }
            Self::BlockBrush(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::BlockBurn(event) => {
                out.push(event.block);
                if let Some(item) = &event.igniting_block {
                    out.push(*item);
                }
            }
            Self::BlockCanBuild(event) => {
                out.push(event.block);
            }
            Self::BlockCook(event) => {
                out.push(event.block);
            }
            Self::BlockDamage(event) => {
                out.push(event.block);
            }
            Self::BlockDamageAbort(event) => {
                out.push(event.block);
            }
            Self::BlockDispense(event) => {
                out.push(event.block);
            }
            Self::BlockDispenseArmor(event) => {
                out.push(event.block);
            }
            Self::BlockDispenseLoot(event) => {
                out.push(event.block);
            }
            Self::BlockDropItem(event) => {
                out.push(event.block);
                out.push(event.block_state.pos);
            }
            Self::BlockExp(event) => {
                out.push(event.block);
            }
            Self::BlockExplode(event) => {
                out.push(event.block);
                out.extend(event.block_list.iter().copied());
                out.push(event.exploded_block_state.pos);
            }
            Self::BlockFade(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::BlockFertilize(event) => {
                out.push(event.block);
                out.extend(event.blocks.iter().map(|item| item.pos));
            }
            Self::BlockForm(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::BlockFromTo(event) => {
                out.push(event.block);
                out.push(event.to_block);
            }
            Self::BlockGrow(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::BlockIgnite(event) => {
                out.push(event.block);
                if let Some(item) = &event.igniting_block {
                    out.push(*item);
                }
            }
            Self::BlockMultiPlace(event) => {
                out.push(event.block);
                out.push(event.block_against);
                out.push(event.block_placed);
                out.push(event.block_replaced_state.pos);
                out.extend(event.replaced_block_states.iter().map(|item| item.pos));
                out.push(event.placed_state.pos);
                out.extend(event.placed_states.iter().map(|item| item.pos));
            }
            Self::BlockPhysics(event) => {
                out.push(event.block);
                out.push(event.source_block);
            }
            Self::BlockPistonExtend(event) => {
                out.push(event.block);
                out.extend(event.blocks.iter().copied());
            }
            Self::BlockPistonRetract(event) => {
                out.push(event.block);
                out.extend(event.blocks.iter().copied());
            }
            Self::BlockPlace(event) => {
                out.push(event.block);
                out.push(event.block_against);
                out.push(event.block_placed);
                out.push(event.block_replaced_state.pos);
                out.push(event.placed_state.pos);
            }
            Self::BlockReceiveGame(event) => {
                out.push(event.block);
            }
            Self::BlockRedstone(event) => {
                out.push(event.block);
            }
            Self::BlockShearEntity(event) => {
                out.push(event.block);
            }
            Self::BlockSpread(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
                out.push(event.source);
            }
            Self::Brew(event) => {
                out.push(event.block);
            }
            Self::BrewingStandFuel(event) => {
                out.push(event.block);
            }
            Self::BrewingStart(event) => {
                out.push(event.block);
            }
            Self::CampfireStart(event) => {
                out.push(event.block);
            }
            Self::CauldronLevelChange(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::CrafterCraft(event) => {
                out.push(event.block);
            }
            Self::EnchantItem(event) => {
                out.push(event.enchant_block);
            }
            Self::EntityBlockForm(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::EntityBreakDoor(event) => {
                out.push(event.block);
            }
            Self::EntityChangeBlock(event) => {
                out.push(event.block);
            }
            Self::EntityCombustByBlock(event) => {
                if let Some(item) = &event.combuster {
                    out.push(*item);
                }
            }
            Self::EntityDamageByBlock(event) => {
                if let Some(item) = &event.damager {
                    out.push(*item);
                }
                if let Some(item) = &event.damager_block_state {
                    out.push(item.pos);
                }
            }
            Self::EntityEnterBlock(event) => {
                out.push(event.block);
            }
            Self::EntityExplode(event) => {
                out.extend(event.block_list.iter().copied());
            }
            Self::EntityInteract(event) => {
                out.push(event.block);
            }
            Self::EntityPlace(event) => {
                out.push(event.block);
            }
            Self::EntityTargetBlock(event) => {
                if let Some(item) = &event.target {
                    out.push(*item);
                }
            }
            Self::ExpBottle(event) => {
                if let Some(item) = &event.hit_block {
                    out.push(*item);
                }
            }
            Self::FluidLevelChange(event) => {
                out.push(event.block);
            }
            Self::FurnaceBurn(event) => {
                out.push(event.block);
            }
            Self::FurnaceExtract(event) => {
                out.push(event.block);
            }
            Self::FurnaceSmelt(event) => {
                out.push(event.block);
            }
            Self::FurnaceStartSmelt(event) => {
                out.push(event.block);
            }
            Self::HangingPlace(event) => {
                out.push(event.block);
            }
            Self::HopperInventorySearch(event) => {
                out.push(event.block);
                out.push(event.search_block);
            }
            Self::LeavesDecay(event) => {
                out.push(event.block);
            }
            Self::LingeringPotionSplash(event) => {
                if let Some(item) = &event.hit_block {
                    out.push(*item);
                }
            }
            Self::MoistureChange(event) => {
                out.push(event.block);
                out.push(event.new_state.pos);
            }
            Self::NotePlay(event) => {
                out.push(event.block);
            }
            Self::PlayerBedEnter(event) => {
                out.push(event.bed);
            }
            Self::PlayerBedLeave(event) => {
                out.push(event.bed);
            }
            Self::PlayerBucketEmpty(event) => {
                out.push(event.block);
                out.push(event.block_clicked);
            }
            Self::PlayerBucketFill(event) => {
                out.push(event.block);
                out.push(event.block_clicked);
            }
            Self::PlayerHarvestBlock(event) => {
                out.push(event.harvested_block);
            }
            Self::PlayerInteract(event) => {
                if let Some(item) = &event.clicked_block {
                    out.push(*item);
                }
            }
            Self::PortalCreate(event) => {
                out.extend(event.blocks.iter().map(|item| item.pos));
            }
            Self::PotionSplash(event) => {
                if let Some(item) = &event.hit_block {
                    out.push(*item);
                }
            }
            Self::PrepareItemEnchant(event) => {
                out.push(event.enchant_block);
            }
            Self::ProjectileHit(event) => {
                if let Some(item) = &event.hit_block {
                    out.push(*item);
                }
            }
            Self::SculkBloom(event) => {
                out.push(event.block);
            }
            Self::SignChange(event) => {
                out.push(event.block);
            }
            Self::SpongeAbsorb(event) => {
                out.push(event.block);
                out.extend(event.blocks.iter().map(|item| item.pos));
            }
            Self::StructureGrow(event) => {
                out.extend(event.blocks.iter().map(|item| item.pos));
            }
            Self::TNTPrime(event) => {
                out.push(event.block);
                if let Some(item) = &event.priming_block {
                    out.push(*item);
                }
            }
            Self::VaultDisplayItem(event) => {
                out.push(event.block);
            }
            Self::VehicleBlockCollision(event) => {
                out.push(event.block);
            }
            _ => {}
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum EventPatch {
    ArrowBodyCountChange(ArrowBodyCountChangePatch),
    AsyncPlayerChat(AsyncPlayerChatPatch),
    AsyncPlayerPreLogin(AsyncPlayerPreLoginPatch),
    BlockBreak(BlockBreakPatch),
    BlockCanBuild(BlockCanBuildPatch),
    BlockCook(BlockCookPatch),
    BlockDamage(BlockDamagePatch),
    BlockDispense(BlockDispensePatch),
    BlockDispenseArmor(BlockDispenseArmorPatch),
    BlockDispenseLoot(BlockDispenseLootPatch),
    BlockDropItem(BlockDropItemPatch),
    BlockExp(BlockExpPatch),
    BlockExplode(BlockExplodePatch),
    BlockMultiPlace(BlockMultiPlacePatch),
    BlockPlace(BlockPlacePatch),
    BlockRedstone(BlockRedstonePatch),
    BrewingStandFuel(BrewingStandFuelPatch),
    BrewingStart(BrewingStartPatch),
    BroadcastMessage(BroadcastMessagePatch),
    CampfireStart(CampfireStartPatch),
    ChunkUnload(ChunkUnloadPatch),
    CraftItem(CraftItemPatch),
    CrafterCraft(CrafterCraftPatch),
    CubeMobSplit(CubeMobSplitPatch),
    EnchantItem(EnchantItemPatch),
    EnderDragonChangePhase(EnderDragonChangePhasePatch),
    EntityAirChange(EntityAirChangePatch),
    EntityBreed(EntityBreedPatch),
    EntityCombust(EntityCombustPatch),
    EntityCombustByBlock(EntityCombustByBlockPatch),
    EntityCombustByEntity(EntityCombustByEntityPatch),
    EntityDamage(EntityDamagePatch),
    EntityDamageByBlock(EntityDamageByBlockPatch),
    EntityDamageByEntity(EntityDamageByEntityPatch),
    EntityDeath(EntityDeathPatch),
    EntityEnterLoveMode(EntityEnterLoveModePatch),
    EntityExhaustion(EntityExhaustionPatch),
    EntityExplode(EntityExplodePatch),
    EntityKnockback(EntityKnockbackPatch),
    EntityKnockbackByEntity(EntityKnockbackByEntityPatch),
    EntityPortal(EntityPortalPatch),
    EntityPortalExit(EntityPortalExitPatch),
    EntityPotionEffect(EntityPotionEffectPatch),
    EntityRegainHealth(EntityRegainHealthPatch),
    EntityShootBow(EntityShootBowPatch),
    EntityTarget(EntityTargetPatch),
    EntityTargetBlock(EntityTargetBlockPatch),
    EntityTargetLivingEntity(EntityTargetLivingEntityPatch),
    EntityTeleport(EntityTeleportPatch),
    ExpBottle(ExpBottlePatch),
    ExplosionPrime(ExplosionPrimePatch),
    FluidLevelChange(FluidLevelChangePatch),
    FoodLevelChange(FoodLevelChangePatch),
    FurnaceBurn(FurnaceBurnPatch),
    FurnaceExtract(FurnaceExtractPatch),
    FurnaceSmelt(FurnaceSmeltPatch),
    FurnaceStartSmelt(FurnaceStartSmeltPatch),
    GenericGame(GenericGamePatch),
    HopperInventorySearch(HopperInventorySearchPatch),
    InventoryClick(InventoryClickPatch),
    InventoryCreative(InventoryCreativePatch),
    InventoryDrag(InventoryDragPatch),
    InventoryMoveItem(InventoryMoveItemPatch),
    LootGenerate(LootGeneratePatch),
    PigZombieAnger(PigZombieAngerPatch),
    PlayerBedEnter(PlayerBedEnterPatch),
    PlayerBedLeave(PlayerBedLeavePatch),
    PlayerBucketEmpty(PlayerBucketEmptyPatch),
    PlayerBucketFill(PlayerBucketFillPatch),
    PlayerCommandPreprocess(PlayerCommandPreprocessPatch),
    PlayerCommandSend(PlayerCommandSendPatch),
    PlayerDeath(PlayerDeathPatch),
    PlayerDropItem(PlayerDropItemPatch),
    PlayerEditBook(PlayerEditBookPatch),
    PlayerEggThrow(PlayerEggThrowPatch),
    PlayerExpChange(PlayerExpChangePatch),
    PlayerExpCooldownChange(PlayerExpCooldownChangePatch),
    PlayerFish(PlayerFishPatch),
    PlayerHarvestBlock(PlayerHarvestBlockPatch),
    PlayerInteract(PlayerInteractPatch),
    PlayerItemConsume(PlayerItemConsumePatch),
    PlayerItemDamage(PlayerItemDamagePatch),
    PlayerItemMend(PlayerItemMendPatch),
    PlayerJoin(PlayerJoinPatch),
    PlayerKick(PlayerKickPatch),
    PlayerLeave(PlayerLeavePatch),
    PlayerLogin(PlayerLoginPatch),
    PlayerMove(PlayerMovePatch),
    PlayerPortal(PlayerPortalPatch),
    PlayerRecipeBookClick(PlayerRecipeBookClickPatch),
    PlayerRespawn(PlayerRespawnPatch),
    PlayerSpawnChange(PlayerSpawnChangePatch),
    PlayerSwapHandItems(PlayerSwapHandItemsPatch),
    PlayerTeleport(PlayerTeleportPatch),
    PlayerVelocity(PlayerVelocityPatch),
    PotionSplash(PotionSplashPatch),
    PrepareAnvil(PrepareAnvilPatch),
    PrepareGrindstone(PrepareGrindstonePatch),
    PrepareItemCraft(PrepareItemCraftPatch),
    PrepareSmithing(PrepareSmithingPatch),
    RemoteServerCommand(RemoteServerCommandPatch),
    SculkBloom(SculkBloomPatch),
    ServerCommand(ServerCommandPatch),
    ServerListPing(ServerListPingPatch),
    SheepDyeWool(SheepDyeWoolPatch),
    SignChange(SignChangePatch),
    SmithItem(SmithItemPatch),
    TabComplete(TabCompletePatch),
    TimeSkip(TimeSkipPatch),
    TradeSelect(TradeSelectPatch),
    VaultDisplayItem(VaultDisplayItemPatch),
    VehicleDamage(VehicleDamagePatch),
    VillagerAcquireTrade(VillagerAcquireTradePatch),
    VillagerCareerChange(VillagerCareerChangePatch),
    VillagerReplenishTrade(VillagerReplenishTradePatch),
    VillagerReputationChange(VillagerReputationChangePatch),
}

impl EventPatch {
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::ArrowBodyCountChange(_) => EventKind::ArrowBodyCountChange,
            Self::AsyncPlayerChat(_) => EventKind::AsyncPlayerChat,
            Self::AsyncPlayerPreLogin(_) => EventKind::AsyncPlayerPreLogin,
            Self::BlockBreak(_) => EventKind::BlockBreak,
            Self::BlockCanBuild(_) => EventKind::BlockCanBuild,
            Self::BlockCook(_) => EventKind::BlockCook,
            Self::BlockDamage(_) => EventKind::BlockDamage,
            Self::BlockDispense(_) => EventKind::BlockDispense,
            Self::BlockDispenseArmor(_) => EventKind::BlockDispenseArmor,
            Self::BlockDispenseLoot(_) => EventKind::BlockDispenseLoot,
            Self::BlockDropItem(_) => EventKind::BlockDropItem,
            Self::BlockExp(_) => EventKind::BlockExp,
            Self::BlockExplode(_) => EventKind::BlockExplode,
            Self::BlockMultiPlace(_) => EventKind::BlockMultiPlace,
            Self::BlockPlace(_) => EventKind::BlockPlace,
            Self::BlockRedstone(_) => EventKind::BlockRedstone,
            Self::BrewingStandFuel(_) => EventKind::BrewingStandFuel,
            Self::BrewingStart(_) => EventKind::BrewingStart,
            Self::BroadcastMessage(_) => EventKind::BroadcastMessage,
            Self::CampfireStart(_) => EventKind::CampfireStart,
            Self::ChunkUnload(_) => EventKind::ChunkUnload,
            Self::CraftItem(_) => EventKind::CraftItem,
            Self::CrafterCraft(_) => EventKind::CrafterCraft,
            Self::CubeMobSplit(_) => EventKind::CubeMobSplit,
            Self::EnchantItem(_) => EventKind::EnchantItem,
            Self::EnderDragonChangePhase(_) => EventKind::EnderDragonChangePhase,
            Self::EntityAirChange(_) => EventKind::EntityAirChange,
            Self::EntityBreed(_) => EventKind::EntityBreed,
            Self::EntityCombust(_) => EventKind::EntityCombust,
            Self::EntityCombustByBlock(_) => EventKind::EntityCombustByBlock,
            Self::EntityCombustByEntity(_) => EventKind::EntityCombustByEntity,
            Self::EntityDamage(_) => EventKind::EntityDamage,
            Self::EntityDamageByBlock(_) => EventKind::EntityDamageByBlock,
            Self::EntityDamageByEntity(_) => EventKind::EntityDamageByEntity,
            Self::EntityDeath(_) => EventKind::EntityDeath,
            Self::EntityEnterLoveMode(_) => EventKind::EntityEnterLoveMode,
            Self::EntityExhaustion(_) => EventKind::EntityExhaustion,
            Self::EntityExplode(_) => EventKind::EntityExplode,
            Self::EntityKnockback(_) => EventKind::EntityKnockback,
            Self::EntityKnockbackByEntity(_) => EventKind::EntityKnockbackByEntity,
            Self::EntityPortal(_) => EventKind::EntityPortal,
            Self::EntityPortalExit(_) => EventKind::EntityPortalExit,
            Self::EntityPotionEffect(_) => EventKind::EntityPotionEffect,
            Self::EntityRegainHealth(_) => EventKind::EntityRegainHealth,
            Self::EntityShootBow(_) => EventKind::EntityShootBow,
            Self::EntityTarget(_) => EventKind::EntityTarget,
            Self::EntityTargetBlock(_) => EventKind::EntityTargetBlock,
            Self::EntityTargetLivingEntity(_) => EventKind::EntityTargetLivingEntity,
            Self::EntityTeleport(_) => EventKind::EntityTeleport,
            Self::ExpBottle(_) => EventKind::ExpBottle,
            Self::ExplosionPrime(_) => EventKind::ExplosionPrime,
            Self::FluidLevelChange(_) => EventKind::FluidLevelChange,
            Self::FoodLevelChange(_) => EventKind::FoodLevelChange,
            Self::FurnaceBurn(_) => EventKind::FurnaceBurn,
            Self::FurnaceExtract(_) => EventKind::FurnaceExtract,
            Self::FurnaceSmelt(_) => EventKind::FurnaceSmelt,
            Self::FurnaceStartSmelt(_) => EventKind::FurnaceStartSmelt,
            Self::GenericGame(_) => EventKind::GenericGame,
            Self::HopperInventorySearch(_) => EventKind::HopperInventorySearch,
            Self::InventoryClick(_) => EventKind::InventoryClick,
            Self::InventoryCreative(_) => EventKind::InventoryCreative,
            Self::InventoryDrag(_) => EventKind::InventoryDrag,
            Self::InventoryMoveItem(_) => EventKind::InventoryMoveItem,
            Self::LootGenerate(_) => EventKind::LootGenerate,
            Self::PigZombieAnger(_) => EventKind::PigZombieAnger,
            Self::PlayerBedEnter(_) => EventKind::PlayerBedEnter,
            Self::PlayerBedLeave(_) => EventKind::PlayerBedLeave,
            Self::PlayerBucketEmpty(_) => EventKind::PlayerBucketEmpty,
            Self::PlayerBucketFill(_) => EventKind::PlayerBucketFill,
            Self::PlayerCommandPreprocess(_) => EventKind::PlayerCommandPreprocess,
            Self::PlayerCommandSend(_) => EventKind::PlayerCommandSend,
            Self::PlayerDeath(_) => EventKind::PlayerDeath,
            Self::PlayerDropItem(_) => EventKind::PlayerDropItem,
            Self::PlayerEditBook(_) => EventKind::PlayerEditBook,
            Self::PlayerEggThrow(_) => EventKind::PlayerEggThrow,
            Self::PlayerExpChange(_) => EventKind::PlayerExpChange,
            Self::PlayerExpCooldownChange(_) => EventKind::PlayerExpCooldownChange,
            Self::PlayerFish(_) => EventKind::PlayerFish,
            Self::PlayerHarvestBlock(_) => EventKind::PlayerHarvestBlock,
            Self::PlayerInteract(_) => EventKind::PlayerInteract,
            Self::PlayerItemConsume(_) => EventKind::PlayerItemConsume,
            Self::PlayerItemDamage(_) => EventKind::PlayerItemDamage,
            Self::PlayerItemMend(_) => EventKind::PlayerItemMend,
            Self::PlayerJoin(_) => EventKind::PlayerJoin,
            Self::PlayerKick(_) => EventKind::PlayerKick,
            Self::PlayerLeave(_) => EventKind::PlayerLeave,
            Self::PlayerLogin(_) => EventKind::PlayerLogin,
            Self::PlayerMove(_) => EventKind::PlayerMove,
            Self::PlayerPortal(_) => EventKind::PlayerPortal,
            Self::PlayerRecipeBookClick(_) => EventKind::PlayerRecipeBookClick,
            Self::PlayerRespawn(_) => EventKind::PlayerRespawn,
            Self::PlayerSpawnChange(_) => EventKind::PlayerSpawnChange,
            Self::PlayerSwapHandItems(_) => EventKind::PlayerSwapHandItems,
            Self::PlayerTeleport(_) => EventKind::PlayerTeleport,
            Self::PlayerVelocity(_) => EventKind::PlayerVelocity,
            Self::PotionSplash(_) => EventKind::PotionSplash,
            Self::PrepareAnvil(_) => EventKind::PrepareAnvil,
            Self::PrepareGrindstone(_) => EventKind::PrepareGrindstone,
            Self::PrepareItemCraft(_) => EventKind::PrepareItemCraft,
            Self::PrepareSmithing(_) => EventKind::PrepareSmithing,
            Self::RemoteServerCommand(_) => EventKind::RemoteServerCommand,
            Self::SculkBloom(_) => EventKind::SculkBloom,
            Self::ServerCommand(_) => EventKind::ServerCommand,
            Self::ServerListPing(_) => EventKind::ServerListPing,
            Self::SheepDyeWool(_) => EventKind::SheepDyeWool,
            Self::SignChange(_) => EventKind::SignChange,
            Self::SmithItem(_) => EventKind::SmithItem,
            Self::TabComplete(_) => EventKind::TabComplete,
            Self::TimeSkip(_) => EventKind::TimeSkip,
            Self::TradeSelect(_) => EventKind::TradeSelect,
            Self::VaultDisplayItem(_) => EventKind::VaultDisplayItem,
            Self::VehicleDamage(_) => EventKind::VehicleDamage,
            Self::VillagerAcquireTrade(_) => EventKind::VillagerAcquireTrade,
            Self::VillagerCareerChange(_) => EventKind::VillagerCareerChange,
            Self::VillagerReplenishTrade(_) => EventKind::VillagerReplenishTrade,
            Self::VillagerReputationChange(_) => EventKind::VillagerReputationChange,
        }
    }

    pub(crate) fn then(self, later: Self) -> Self {
        match (self, later) {
            (Self::ArrowBodyCountChange(earlier), Self::ArrowBodyCountChange(later)) => {
                Self::ArrowBodyCountChange(earlier.then(later))
            }
            (Self::AsyncPlayerChat(earlier), Self::AsyncPlayerChat(later)) => {
                Self::AsyncPlayerChat(earlier.then(later))
            }
            (Self::AsyncPlayerPreLogin(earlier), Self::AsyncPlayerPreLogin(later)) => {
                Self::AsyncPlayerPreLogin(earlier.then(later))
            }
            (Self::BlockBreak(earlier), Self::BlockBreak(later)) => {
                Self::BlockBreak(earlier.then(later))
            }
            (Self::BlockCanBuild(earlier), Self::BlockCanBuild(later)) => {
                Self::BlockCanBuild(earlier.then(later))
            }
            (Self::BlockCook(earlier), Self::BlockCook(later)) => {
                Self::BlockCook(earlier.then(later))
            }
            (Self::BlockDamage(earlier), Self::BlockDamage(later)) => {
                Self::BlockDamage(earlier.then(later))
            }
            (Self::BlockDispense(earlier), Self::BlockDispense(later)) => {
                Self::BlockDispense(earlier.then(later))
            }
            (Self::BlockDispenseArmor(earlier), Self::BlockDispenseArmor(later)) => {
                Self::BlockDispenseArmor(earlier.then(later))
            }
            (Self::BlockDispenseLoot(earlier), Self::BlockDispenseLoot(later)) => {
                Self::BlockDispenseLoot(earlier.then(later))
            }
            (Self::BlockDropItem(earlier), Self::BlockDropItem(later)) => {
                Self::BlockDropItem(earlier.then(later))
            }
            (Self::BlockExp(earlier), Self::BlockExp(later)) => Self::BlockExp(earlier.then(later)),
            (Self::BlockExplode(earlier), Self::BlockExplode(later)) => {
                Self::BlockExplode(earlier.then(later))
            }
            (Self::BlockMultiPlace(earlier), Self::BlockMultiPlace(later)) => {
                Self::BlockMultiPlace(earlier.then(later))
            }
            (Self::BlockPlace(earlier), Self::BlockPlace(later)) => {
                Self::BlockPlace(earlier.then(later))
            }
            (Self::BlockRedstone(earlier), Self::BlockRedstone(later)) => {
                Self::BlockRedstone(earlier.then(later))
            }
            (Self::BrewingStandFuel(earlier), Self::BrewingStandFuel(later)) => {
                Self::BrewingStandFuel(earlier.then(later))
            }
            (Self::BrewingStart(earlier), Self::BrewingStart(later)) => {
                Self::BrewingStart(earlier.then(later))
            }
            (Self::BroadcastMessage(earlier), Self::BroadcastMessage(later)) => {
                Self::BroadcastMessage(earlier.then(later))
            }
            (Self::CampfireStart(earlier), Self::CampfireStart(later)) => {
                Self::CampfireStart(earlier.then(later))
            }
            (Self::ChunkUnload(earlier), Self::ChunkUnload(later)) => {
                Self::ChunkUnload(earlier.then(later))
            }
            (Self::CraftItem(earlier), Self::CraftItem(later)) => {
                Self::CraftItem(earlier.then(later))
            }
            (Self::CrafterCraft(earlier), Self::CrafterCraft(later)) => {
                Self::CrafterCraft(earlier.then(later))
            }
            (Self::CubeMobSplit(earlier), Self::CubeMobSplit(later)) => {
                Self::CubeMobSplit(earlier.then(later))
            }
            (Self::EnchantItem(earlier), Self::EnchantItem(later)) => {
                Self::EnchantItem(earlier.then(later))
            }
            (Self::EnderDragonChangePhase(earlier), Self::EnderDragonChangePhase(later)) => {
                Self::EnderDragonChangePhase(earlier.then(later))
            }
            (Self::EntityAirChange(earlier), Self::EntityAirChange(later)) => {
                Self::EntityAirChange(earlier.then(later))
            }
            (Self::EntityBreed(earlier), Self::EntityBreed(later)) => {
                Self::EntityBreed(earlier.then(later))
            }
            (Self::EntityCombust(earlier), Self::EntityCombust(later)) => {
                Self::EntityCombust(earlier.then(later))
            }
            (Self::EntityCombustByBlock(earlier), Self::EntityCombustByBlock(later)) => {
                Self::EntityCombustByBlock(earlier.then(later))
            }
            (Self::EntityCombustByEntity(earlier), Self::EntityCombustByEntity(later)) => {
                Self::EntityCombustByEntity(earlier.then(later))
            }
            (Self::EntityDamage(earlier), Self::EntityDamage(later)) => {
                Self::EntityDamage(earlier.then(later))
            }
            (Self::EntityDamageByBlock(earlier), Self::EntityDamageByBlock(later)) => {
                Self::EntityDamageByBlock(earlier.then(later))
            }
            (Self::EntityDamageByEntity(earlier), Self::EntityDamageByEntity(later)) => {
                Self::EntityDamageByEntity(earlier.then(later))
            }
            (Self::EntityDeath(earlier), Self::EntityDeath(later)) => {
                Self::EntityDeath(earlier.then(later))
            }
            (Self::EntityEnterLoveMode(earlier), Self::EntityEnterLoveMode(later)) => {
                Self::EntityEnterLoveMode(earlier.then(later))
            }
            (Self::EntityExhaustion(earlier), Self::EntityExhaustion(later)) => {
                Self::EntityExhaustion(earlier.then(later))
            }
            (Self::EntityExplode(earlier), Self::EntityExplode(later)) => {
                Self::EntityExplode(earlier.then(later))
            }
            (Self::EntityKnockback(earlier), Self::EntityKnockback(later)) => {
                Self::EntityKnockback(earlier.then(later))
            }
            (Self::EntityKnockbackByEntity(earlier), Self::EntityKnockbackByEntity(later)) => {
                Self::EntityKnockbackByEntity(earlier.then(later))
            }
            (Self::EntityPortal(earlier), Self::EntityPortal(later)) => {
                Self::EntityPortal(earlier.then(later))
            }
            (Self::EntityPortalExit(earlier), Self::EntityPortalExit(later)) => {
                Self::EntityPortalExit(earlier.then(later))
            }
            (Self::EntityPotionEffect(earlier), Self::EntityPotionEffect(later)) => {
                Self::EntityPotionEffect(earlier.then(later))
            }
            (Self::EntityRegainHealth(earlier), Self::EntityRegainHealth(later)) => {
                Self::EntityRegainHealth(earlier.then(later))
            }
            (Self::EntityShootBow(earlier), Self::EntityShootBow(later)) => {
                Self::EntityShootBow(earlier.then(later))
            }
            (Self::EntityTarget(earlier), Self::EntityTarget(later)) => {
                Self::EntityTarget(earlier.then(later))
            }
            (Self::EntityTargetBlock(earlier), Self::EntityTargetBlock(later)) => {
                Self::EntityTargetBlock(earlier.then(later))
            }
            (Self::EntityTargetLivingEntity(earlier), Self::EntityTargetLivingEntity(later)) => {
                Self::EntityTargetLivingEntity(earlier.then(later))
            }
            (Self::EntityTeleport(earlier), Self::EntityTeleport(later)) => {
                Self::EntityTeleport(earlier.then(later))
            }
            (Self::ExpBottle(earlier), Self::ExpBottle(later)) => {
                Self::ExpBottle(earlier.then(later))
            }
            (Self::ExplosionPrime(earlier), Self::ExplosionPrime(later)) => {
                Self::ExplosionPrime(earlier.then(later))
            }
            (Self::FluidLevelChange(earlier), Self::FluidLevelChange(later)) => {
                Self::FluidLevelChange(earlier.then(later))
            }
            (Self::FoodLevelChange(earlier), Self::FoodLevelChange(later)) => {
                Self::FoodLevelChange(earlier.then(later))
            }
            (Self::FurnaceBurn(earlier), Self::FurnaceBurn(later)) => {
                Self::FurnaceBurn(earlier.then(later))
            }
            (Self::FurnaceExtract(earlier), Self::FurnaceExtract(later)) => {
                Self::FurnaceExtract(earlier.then(later))
            }
            (Self::FurnaceSmelt(earlier), Self::FurnaceSmelt(later)) => {
                Self::FurnaceSmelt(earlier.then(later))
            }
            (Self::FurnaceStartSmelt(earlier), Self::FurnaceStartSmelt(later)) => {
                Self::FurnaceStartSmelt(earlier.then(later))
            }
            (Self::GenericGame(earlier), Self::GenericGame(later)) => {
                Self::GenericGame(earlier.then(later))
            }
            (Self::HopperInventorySearch(earlier), Self::HopperInventorySearch(later)) => {
                Self::HopperInventorySearch(earlier.then(later))
            }
            (Self::InventoryClick(earlier), Self::InventoryClick(later)) => {
                Self::InventoryClick(earlier.then(later))
            }
            (Self::InventoryCreative(earlier), Self::InventoryCreative(later)) => {
                Self::InventoryCreative(earlier.then(later))
            }
            (Self::InventoryDrag(earlier), Self::InventoryDrag(later)) => {
                Self::InventoryDrag(earlier.then(later))
            }
            (Self::InventoryMoveItem(earlier), Self::InventoryMoveItem(later)) => {
                Self::InventoryMoveItem(earlier.then(later))
            }
            (Self::LootGenerate(earlier), Self::LootGenerate(later)) => {
                Self::LootGenerate(earlier.then(later))
            }
            (Self::PigZombieAnger(earlier), Self::PigZombieAnger(later)) => {
                Self::PigZombieAnger(earlier.then(later))
            }
            (Self::PlayerBedEnter(earlier), Self::PlayerBedEnter(later)) => {
                Self::PlayerBedEnter(earlier.then(later))
            }
            (Self::PlayerBedLeave(earlier), Self::PlayerBedLeave(later)) => {
                Self::PlayerBedLeave(earlier.then(later))
            }
            (Self::PlayerBucketEmpty(earlier), Self::PlayerBucketEmpty(later)) => {
                Self::PlayerBucketEmpty(earlier.then(later))
            }
            (Self::PlayerBucketFill(earlier), Self::PlayerBucketFill(later)) => {
                Self::PlayerBucketFill(earlier.then(later))
            }
            (Self::PlayerCommandPreprocess(earlier), Self::PlayerCommandPreprocess(later)) => {
                Self::PlayerCommandPreprocess(earlier.then(later))
            }
            (Self::PlayerCommandSend(earlier), Self::PlayerCommandSend(later)) => {
                Self::PlayerCommandSend(earlier.then(later))
            }
            (Self::PlayerDeath(earlier), Self::PlayerDeath(later)) => {
                Self::PlayerDeath(earlier.then(later))
            }
            (Self::PlayerDropItem(earlier), Self::PlayerDropItem(later)) => {
                Self::PlayerDropItem(earlier.then(later))
            }
            (Self::PlayerEditBook(earlier), Self::PlayerEditBook(later)) => {
                Self::PlayerEditBook(earlier.then(later))
            }
            (Self::PlayerEggThrow(earlier), Self::PlayerEggThrow(later)) => {
                Self::PlayerEggThrow(earlier.then(later))
            }
            (Self::PlayerExpChange(earlier), Self::PlayerExpChange(later)) => {
                Self::PlayerExpChange(earlier.then(later))
            }
            (Self::PlayerExpCooldownChange(earlier), Self::PlayerExpCooldownChange(later)) => {
                Self::PlayerExpCooldownChange(earlier.then(later))
            }
            (Self::PlayerFish(earlier), Self::PlayerFish(later)) => {
                Self::PlayerFish(earlier.then(later))
            }
            (Self::PlayerHarvestBlock(earlier), Self::PlayerHarvestBlock(later)) => {
                Self::PlayerHarvestBlock(earlier.then(later))
            }
            (Self::PlayerInteract(earlier), Self::PlayerInteract(later)) => {
                Self::PlayerInteract(earlier.then(later))
            }
            (Self::PlayerItemConsume(earlier), Self::PlayerItemConsume(later)) => {
                Self::PlayerItemConsume(earlier.then(later))
            }
            (Self::PlayerItemDamage(earlier), Self::PlayerItemDamage(later)) => {
                Self::PlayerItemDamage(earlier.then(later))
            }
            (Self::PlayerItemMend(earlier), Self::PlayerItemMend(later)) => {
                Self::PlayerItemMend(earlier.then(later))
            }
            (Self::PlayerJoin(earlier), Self::PlayerJoin(later)) => {
                Self::PlayerJoin(earlier.then(later))
            }
            (Self::PlayerKick(earlier), Self::PlayerKick(later)) => {
                Self::PlayerKick(earlier.then(later))
            }
            (Self::PlayerLeave(earlier), Self::PlayerLeave(later)) => {
                Self::PlayerLeave(earlier.then(later))
            }
            (Self::PlayerLogin(earlier), Self::PlayerLogin(later)) => {
                Self::PlayerLogin(earlier.then(later))
            }
            (Self::PlayerMove(earlier), Self::PlayerMove(later)) => {
                Self::PlayerMove(earlier.then(later))
            }
            (Self::PlayerPortal(earlier), Self::PlayerPortal(later)) => {
                Self::PlayerPortal(earlier.then(later))
            }
            (Self::PlayerRecipeBookClick(earlier), Self::PlayerRecipeBookClick(later)) => {
                Self::PlayerRecipeBookClick(earlier.then(later))
            }
            (Self::PlayerRespawn(earlier), Self::PlayerRespawn(later)) => {
                Self::PlayerRespawn(earlier.then(later))
            }
            (Self::PlayerSpawnChange(earlier), Self::PlayerSpawnChange(later)) => {
                Self::PlayerSpawnChange(earlier.then(later))
            }
            (Self::PlayerSwapHandItems(earlier), Self::PlayerSwapHandItems(later)) => {
                Self::PlayerSwapHandItems(earlier.then(later))
            }
            (Self::PlayerTeleport(earlier), Self::PlayerTeleport(later)) => {
                Self::PlayerTeleport(earlier.then(later))
            }
            (Self::PlayerVelocity(earlier), Self::PlayerVelocity(later)) => {
                Self::PlayerVelocity(earlier.then(later))
            }
            (Self::PotionSplash(earlier), Self::PotionSplash(later)) => {
                Self::PotionSplash(earlier.then(later))
            }
            (Self::PrepareAnvil(earlier), Self::PrepareAnvil(later)) => {
                Self::PrepareAnvil(earlier.then(later))
            }
            (Self::PrepareGrindstone(earlier), Self::PrepareGrindstone(later)) => {
                Self::PrepareGrindstone(earlier.then(later))
            }
            (Self::PrepareItemCraft(earlier), Self::PrepareItemCraft(later)) => {
                Self::PrepareItemCraft(earlier.then(later))
            }
            (Self::PrepareSmithing(earlier), Self::PrepareSmithing(later)) => {
                Self::PrepareSmithing(earlier.then(later))
            }
            (Self::RemoteServerCommand(earlier), Self::RemoteServerCommand(later)) => {
                Self::RemoteServerCommand(earlier.then(later))
            }
            (Self::SculkBloom(earlier), Self::SculkBloom(later)) => {
                Self::SculkBloom(earlier.then(later))
            }
            (Self::ServerCommand(earlier), Self::ServerCommand(later)) => {
                Self::ServerCommand(earlier.then(later))
            }
            (Self::ServerListPing(earlier), Self::ServerListPing(later)) => {
                Self::ServerListPing(earlier.then(later))
            }
            (Self::SheepDyeWool(earlier), Self::SheepDyeWool(later)) => {
                Self::SheepDyeWool(earlier.then(later))
            }
            (Self::SignChange(earlier), Self::SignChange(later)) => {
                Self::SignChange(earlier.then(later))
            }
            (Self::SmithItem(earlier), Self::SmithItem(later)) => {
                Self::SmithItem(earlier.then(later))
            }
            (Self::TabComplete(earlier), Self::TabComplete(later)) => {
                Self::TabComplete(earlier.then(later))
            }
            (Self::TimeSkip(earlier), Self::TimeSkip(later)) => Self::TimeSkip(earlier.then(later)),
            (Self::TradeSelect(earlier), Self::TradeSelect(later)) => {
                Self::TradeSelect(earlier.then(later))
            }
            (Self::VaultDisplayItem(earlier), Self::VaultDisplayItem(later)) => {
                Self::VaultDisplayItem(earlier.then(later))
            }
            (Self::VehicleDamage(earlier), Self::VehicleDamage(later)) => {
                Self::VehicleDamage(earlier.then(later))
            }
            (Self::VillagerAcquireTrade(earlier), Self::VillagerAcquireTrade(later)) => {
                Self::VillagerAcquireTrade(earlier.then(later))
            }
            (Self::VillagerCareerChange(earlier), Self::VillagerCareerChange(later)) => {
                Self::VillagerCareerChange(earlier.then(later))
            }
            (Self::VillagerReplenishTrade(earlier), Self::VillagerReplenishTrade(later)) => {
                Self::VillagerReplenishTrade(earlier.then(later))
            }
            (Self::VillagerReputationChange(earlier), Self::VillagerReputationChange(later)) => {
                Self::VillagerReputationChange(earlier.then(later))
            }
            (_, later) => later,
        }
    }

    pub(crate) fn settle(self, payload: &mut EventPayload) {
        match (self, payload) {
            (Self::ArrowBodyCountChange(patch), EventPayload::ArrowBodyCountChange(event)) => {
                patch.settle(event)
            }
            (Self::AsyncPlayerChat(patch), EventPayload::AsyncPlayerChat(event)) => {
                patch.settle(event)
            }
            (Self::AsyncPlayerPreLogin(patch), EventPayload::AsyncPlayerPreLogin(event)) => {
                patch.settle(event)
            }
            (Self::BlockBreak(patch), EventPayload::BlockBreak(event)) => patch.settle(event),
            (Self::BlockCanBuild(patch), EventPayload::BlockCanBuild(event)) => patch.settle(event),
            (Self::BlockCook(patch), EventPayload::BlockCook(event)) => patch.settle(event),
            (Self::BlockDamage(patch), EventPayload::BlockDamage(event)) => patch.settle(event),
            (Self::BlockDispense(patch), EventPayload::BlockDispense(event)) => patch.settle(event),
            (Self::BlockDispenseArmor(patch), EventPayload::BlockDispenseArmor(event)) => {
                patch.settle(event)
            }
            (Self::BlockDispenseLoot(patch), EventPayload::BlockDispenseLoot(event)) => {
                patch.settle(event)
            }
            (Self::BlockDropItem(patch), EventPayload::BlockDropItem(event)) => patch.settle(event),
            (Self::BlockExp(patch), EventPayload::BlockExp(event)) => patch.settle(event),
            (Self::BlockExplode(patch), EventPayload::BlockExplode(event)) => patch.settle(event),
            (Self::BlockMultiPlace(patch), EventPayload::BlockMultiPlace(event)) => {
                patch.settle(event)
            }
            (Self::BlockPlace(patch), EventPayload::BlockPlace(event)) => patch.settle(event),
            (Self::BlockRedstone(patch), EventPayload::BlockRedstone(event)) => patch.settle(event),
            (Self::BrewingStandFuel(patch), EventPayload::BrewingStandFuel(event)) => {
                patch.settle(event)
            }
            (Self::BrewingStart(patch), EventPayload::BrewingStart(event)) => patch.settle(event),
            (Self::BroadcastMessage(patch), EventPayload::BroadcastMessage(event)) => {
                patch.settle(event)
            }
            (Self::CampfireStart(patch), EventPayload::CampfireStart(event)) => patch.settle(event),
            (Self::ChunkUnload(patch), EventPayload::ChunkUnload(event)) => patch.settle(event),
            (Self::CraftItem(patch), EventPayload::CraftItem(event)) => patch.settle(event),
            (Self::CrafterCraft(patch), EventPayload::CrafterCraft(event)) => patch.settle(event),
            (Self::CubeMobSplit(patch), EventPayload::CubeMobSplit(event)) => patch.settle(event),
            (Self::EnchantItem(patch), EventPayload::EnchantItem(event)) => patch.settle(event),
            (Self::EnderDragonChangePhase(patch), EventPayload::EnderDragonChangePhase(event)) => {
                patch.settle(event)
            }
            (Self::EntityAirChange(patch), EventPayload::EntityAirChange(event)) => {
                patch.settle(event)
            }
            (Self::EntityBreed(patch), EventPayload::EntityBreed(event)) => patch.settle(event),
            (Self::EntityCombust(patch), EventPayload::EntityCombust(event)) => patch.settle(event),
            (Self::EntityCombustByBlock(patch), EventPayload::EntityCombustByBlock(event)) => {
                patch.settle(event)
            }
            (Self::EntityCombustByEntity(patch), EventPayload::EntityCombustByEntity(event)) => {
                patch.settle(event)
            }
            (Self::EntityDamage(patch), EventPayload::EntityDamage(event)) => patch.settle(event),
            (Self::EntityDamageByBlock(patch), EventPayload::EntityDamageByBlock(event)) => {
                patch.settle(event)
            }
            (Self::EntityDamageByEntity(patch), EventPayload::EntityDamageByEntity(event)) => {
                patch.settle(event)
            }
            (Self::EntityDeath(patch), EventPayload::EntityDeath(event)) => patch.settle(event),
            (Self::EntityEnterLoveMode(patch), EventPayload::EntityEnterLoveMode(event)) => {
                patch.settle(event)
            }
            (Self::EntityExhaustion(patch), EventPayload::EntityExhaustion(event)) => {
                patch.settle(event)
            }
            (Self::EntityExplode(patch), EventPayload::EntityExplode(event)) => patch.settle(event),
            (Self::EntityKnockback(patch), EventPayload::EntityKnockback(event)) => {
                patch.settle(event)
            }
            (
                Self::EntityKnockbackByEntity(patch),
                EventPayload::EntityKnockbackByEntity(event),
            ) => patch.settle(event),
            (Self::EntityPortal(patch), EventPayload::EntityPortal(event)) => patch.settle(event),
            (Self::EntityPortalExit(patch), EventPayload::EntityPortalExit(event)) => {
                patch.settle(event)
            }
            (Self::EntityPotionEffect(patch), EventPayload::EntityPotionEffect(event)) => {
                patch.settle(event)
            }
            (Self::EntityRegainHealth(patch), EventPayload::EntityRegainHealth(event)) => {
                patch.settle(event)
            }
            (Self::EntityShootBow(patch), EventPayload::EntityShootBow(event)) => {
                patch.settle(event)
            }
            (Self::EntityTarget(patch), EventPayload::EntityTarget(event)) => patch.settle(event),
            (Self::EntityTargetBlock(patch), EventPayload::EntityTargetBlock(event)) => {
                patch.settle(event)
            }
            (
                Self::EntityTargetLivingEntity(patch),
                EventPayload::EntityTargetLivingEntity(event),
            ) => patch.settle(event),
            (Self::EntityTeleport(patch), EventPayload::EntityTeleport(event)) => {
                patch.settle(event)
            }
            (Self::ExpBottle(patch), EventPayload::ExpBottle(event)) => patch.settle(event),
            (Self::ExplosionPrime(patch), EventPayload::ExplosionPrime(event)) => {
                patch.settle(event)
            }
            (Self::FluidLevelChange(patch), EventPayload::FluidLevelChange(event)) => {
                patch.settle(event)
            }
            (Self::FoodLevelChange(patch), EventPayload::FoodLevelChange(event)) => {
                patch.settle(event)
            }
            (Self::FurnaceBurn(patch), EventPayload::FurnaceBurn(event)) => patch.settle(event),
            (Self::FurnaceExtract(patch), EventPayload::FurnaceExtract(event)) => {
                patch.settle(event)
            }
            (Self::FurnaceSmelt(patch), EventPayload::FurnaceSmelt(event)) => patch.settle(event),
            (Self::FurnaceStartSmelt(patch), EventPayload::FurnaceStartSmelt(event)) => {
                patch.settle(event)
            }
            (Self::GenericGame(patch), EventPayload::GenericGame(event)) => patch.settle(event),
            (Self::HopperInventorySearch(patch), EventPayload::HopperInventorySearch(event)) => {
                patch.settle(event)
            }
            (Self::InventoryClick(patch), EventPayload::InventoryClick(event)) => {
                patch.settle(event)
            }
            (Self::InventoryCreative(patch), EventPayload::InventoryCreative(event)) => {
                patch.settle(event)
            }
            (Self::InventoryDrag(patch), EventPayload::InventoryDrag(event)) => patch.settle(event),
            (Self::InventoryMoveItem(patch), EventPayload::InventoryMoveItem(event)) => {
                patch.settle(event)
            }
            (Self::LootGenerate(patch), EventPayload::LootGenerate(event)) => patch.settle(event),
            (Self::PigZombieAnger(patch), EventPayload::PigZombieAnger(event)) => {
                patch.settle(event)
            }
            (Self::PlayerBedEnter(patch), EventPayload::PlayerBedEnter(event)) => {
                patch.settle(event)
            }
            (Self::PlayerBedLeave(patch), EventPayload::PlayerBedLeave(event)) => {
                patch.settle(event)
            }
            (Self::PlayerBucketEmpty(patch), EventPayload::PlayerBucketEmpty(event)) => {
                patch.settle(event)
            }
            (Self::PlayerBucketFill(patch), EventPayload::PlayerBucketFill(event)) => {
                patch.settle(event)
            }
            (
                Self::PlayerCommandPreprocess(patch),
                EventPayload::PlayerCommandPreprocess(event),
            ) => patch.settle(event),
            (Self::PlayerCommandSend(patch), EventPayload::PlayerCommandSend(event)) => {
                patch.settle(event)
            }
            (Self::PlayerDeath(patch), EventPayload::PlayerDeath(event)) => patch.settle(event),
            (Self::PlayerDropItem(patch), EventPayload::PlayerDropItem(event)) => {
                patch.settle(event)
            }
            (Self::PlayerEditBook(patch), EventPayload::PlayerEditBook(event)) => {
                patch.settle(event)
            }
            (Self::PlayerEggThrow(patch), EventPayload::PlayerEggThrow(event)) => {
                patch.settle(event)
            }
            (Self::PlayerExpChange(patch), EventPayload::PlayerExpChange(event)) => {
                patch.settle(event)
            }
            (
                Self::PlayerExpCooldownChange(patch),
                EventPayload::PlayerExpCooldownChange(event),
            ) => patch.settle(event),
            (Self::PlayerFish(patch), EventPayload::PlayerFish(event)) => patch.settle(event),
            (Self::PlayerHarvestBlock(patch), EventPayload::PlayerHarvestBlock(event)) => {
                patch.settle(event)
            }
            (Self::PlayerInteract(patch), EventPayload::PlayerInteract(event)) => {
                patch.settle(event)
            }
            (Self::PlayerItemConsume(patch), EventPayload::PlayerItemConsume(event)) => {
                patch.settle(event)
            }
            (Self::PlayerItemDamage(patch), EventPayload::PlayerItemDamage(event)) => {
                patch.settle(event)
            }
            (Self::PlayerItemMend(patch), EventPayload::PlayerItemMend(event)) => {
                patch.settle(event)
            }
            (Self::PlayerJoin(patch), EventPayload::PlayerJoin(event)) => patch.settle(event),
            (Self::PlayerKick(patch), EventPayload::PlayerKick(event)) => patch.settle(event),
            (Self::PlayerLeave(patch), EventPayload::PlayerLeave(event)) => patch.settle(event),
            (Self::PlayerLogin(patch), EventPayload::PlayerLogin(event)) => patch.settle(event),
            (Self::PlayerMove(patch), EventPayload::PlayerMove(event)) => patch.settle(event),
            (Self::PlayerPortal(patch), EventPayload::PlayerPortal(event)) => patch.settle(event),
            (Self::PlayerRecipeBookClick(patch), EventPayload::PlayerRecipeBookClick(event)) => {
                patch.settle(event)
            }
            (Self::PlayerRespawn(patch), EventPayload::PlayerRespawn(event)) => patch.settle(event),
            (Self::PlayerSpawnChange(patch), EventPayload::PlayerSpawnChange(event)) => {
                patch.settle(event)
            }
            (Self::PlayerSwapHandItems(patch), EventPayload::PlayerSwapHandItems(event)) => {
                patch.settle(event)
            }
            (Self::PlayerTeleport(patch), EventPayload::PlayerTeleport(event)) => {
                patch.settle(event)
            }
            (Self::PlayerVelocity(patch), EventPayload::PlayerVelocity(event)) => {
                patch.settle(event)
            }
            (Self::PotionSplash(patch), EventPayload::PotionSplash(event)) => patch.settle(event),
            (Self::PrepareAnvil(patch), EventPayload::PrepareAnvil(event)) => patch.settle(event),
            (Self::PrepareGrindstone(patch), EventPayload::PrepareGrindstone(event)) => {
                patch.settle(event)
            }
            (Self::PrepareItemCraft(patch), EventPayload::PrepareItemCraft(event)) => {
                patch.settle(event)
            }
            (Self::PrepareSmithing(patch), EventPayload::PrepareSmithing(event)) => {
                patch.settle(event)
            }
            (Self::RemoteServerCommand(patch), EventPayload::RemoteServerCommand(event)) => {
                patch.settle(event)
            }
            (Self::SculkBloom(patch), EventPayload::SculkBloom(event)) => patch.settle(event),
            (Self::ServerCommand(patch), EventPayload::ServerCommand(event)) => patch.settle(event),
            (Self::ServerListPing(patch), EventPayload::ServerListPing(event)) => {
                patch.settle(event)
            }
            (Self::SheepDyeWool(patch), EventPayload::SheepDyeWool(event)) => patch.settle(event),
            (Self::SignChange(patch), EventPayload::SignChange(event)) => patch.settle(event),
            (Self::SmithItem(patch), EventPayload::SmithItem(event)) => patch.settle(event),
            (Self::TabComplete(patch), EventPayload::TabComplete(event)) => patch.settle(event),
            (Self::TimeSkip(patch), EventPayload::TimeSkip(event)) => patch.settle(event),
            (Self::TradeSelect(patch), EventPayload::TradeSelect(event)) => patch.settle(event),
            (Self::VaultDisplayItem(patch), EventPayload::VaultDisplayItem(event)) => {
                patch.settle(event)
            }
            (Self::VehicleDamage(patch), EventPayload::VehicleDamage(event)) => patch.settle(event),
            (Self::VillagerAcquireTrade(patch), EventPayload::VillagerAcquireTrade(event)) => {
                patch.settle(event)
            }
            (Self::VillagerCareerChange(patch), EventPayload::VillagerCareerChange(event)) => {
                patch.settle(event)
            }
            (Self::VillagerReplenishTrade(patch), EventPayload::VillagerReplenishTrade(event)) => {
                patch.settle(event)
            }
            (
                Self::VillagerReputationChange(patch),
                EventPayload::VillagerReputationChange(event),
            ) => patch.settle(event),
            _ => {}
        }
    }

    pub(crate) fn denial(kind: EventKind) -> Option<Self> {
        match kind {
            EventKind::CraftItem => Some(Self::CraftItem(CraftItemPatch {
                result: Some(EventResult::Deny),
                ..Default::default()
            })),
            EventKind::InventoryClick => Some(Self::InventoryClick(InventoryClickPatch {
                result: Some(EventResult::Deny),
                ..Default::default()
            })),
            EventKind::InventoryCreative => Some(Self::InventoryCreative(InventoryCreativePatch {
                result: Some(EventResult::Deny),
                ..Default::default()
            })),
            EventKind::InventoryDrag => Some(Self::InventoryDrag(InventoryDragPatch {
                result: Some(EventResult::Deny),
                ..Default::default()
            })),
            EventKind::PlayerInteract => Some(Self::PlayerInteract(PlayerInteractPatch {
                use_interacted_block: Some(EventResult::Deny),
                use_item_in_hand: Some(EventResult::Deny),
            })),
            EventKind::SmithItem => Some(Self::SmithItem(SmithItemPatch {
                result: Some(EventResult::Deny),
                ..Default::default()
            })),
            EventKind::TradeSelect => Some(Self::TradeSelect(TradeSelectPatch {
                result: Some(EventResult::Deny),
            })),
            _ => None,
        }
    }

    pub(crate) fn when_cancelled(self) -> Option<Self> {
        match self {
            Self::CraftItem(patch) => patch.when_cancelled().map(Self::CraftItem),
            Self::InventoryClick(patch) => patch.when_cancelled().map(Self::InventoryClick),
            Self::InventoryCreative(patch) => patch.when_cancelled().map(Self::InventoryCreative),
            Self::InventoryDrag(patch) => patch.when_cancelled().map(Self::InventoryDrag),
            Self::PlayerInteract(patch) => patch.when_cancelled().map(Self::PlayerInteract),
            Self::SmithItem(patch) => patch.when_cancelled().map(Self::SmithItem),
            Self::TradeSelect(patch) => patch.when_cancelled().map(Self::TradeSelect),
            _ => None,
        }
    }

    pub(crate) fn refusal(&self, payload: &EventPayload) -> Option<String> {
        match (self, payload) {
            (Self::AsyncPlayerChat(patch), EventPayload::AsyncPlayerChat(event)) => {
                patch.refusal(event)
            }
            (Self::BlockDispense(patch), _) => patch.refusal(),
            (Self::BlockDispenseArmor(patch), _) => patch.refusal(),
            (Self::BlockDispenseLoot(patch), EventPayload::BlockDispenseLoot(event)) => {
                patch.refusal(event)
            }
            (Self::BlockDropItem(patch), EventPayload::BlockDropItem(event)) => {
                patch.refusal(event)
            }
            (Self::BlockExplode(patch), EventPayload::BlockExplode(event)) => patch.refusal(event),
            (Self::BroadcastMessage(patch), EventPayload::BroadcastMessage(event)) => {
                patch.refusal(event)
            }
            (Self::EntityCombust(patch), _) => patch.refusal(),
            (Self::EntityCombustByBlock(patch), _) => patch.refusal(),
            (Self::EntityCombustByEntity(patch), _) => patch.refusal(),
            (Self::EntityDamage(patch), _) => patch.refusal(),
            (Self::EntityDamageByBlock(patch), _) => patch.refusal(),
            (Self::EntityDamageByEntity(patch), _) => patch.refusal(),
            (Self::EntityDeath(patch), EventPayload::EntityDeath(event)) => patch.refusal(event),
            (Self::EntityExhaustion(patch), _) => patch.refusal(),
            (Self::EntityExplode(patch), EventPayload::EntityExplode(event)) => {
                patch.refusal(event)
            }
            (Self::EntityKnockback(patch), _) => patch.refusal(),
            (Self::EntityKnockbackByEntity(patch), _) => patch.refusal(),
            (Self::EntityPortal(patch), _) => patch.refusal(),
            (Self::EntityPortalExit(patch), _) => patch.refusal(),
            (Self::EntityRegainHealth(patch), _) => patch.refusal(),
            (Self::EntityTeleport(patch), _) => patch.refusal(),
            (Self::ExplosionPrime(patch), _) => patch.refusal(),
            (Self::LootGenerate(patch), EventPayload::LootGenerate(event)) => patch.refusal(event),
            (Self::PlayerCommandSend(patch), EventPayload::PlayerCommandSend(event)) => {
                patch.refusal(event)
            }
            (Self::PlayerDeath(patch), EventPayload::PlayerDeath(event)) => patch.refusal(event),
            (Self::PlayerHarvestBlock(patch), EventPayload::PlayerHarvestBlock(event)) => {
                patch.refusal(event)
            }
            (Self::PlayerMove(patch), _) => patch.refusal(),
            (Self::PlayerPortal(patch), _) => patch.refusal(),
            (Self::PlayerRespawn(patch), _) => patch.refusal(),
            (Self::PlayerSpawnChange(patch), _) => patch.refusal(),
            (Self::PlayerTeleport(patch), _) => patch.refusal(),
            (Self::PlayerVelocity(patch), _) => patch.refusal(),
            (Self::PotionSplash(patch), _) => patch.refusal(),
            (Self::ServerListPing(patch), EventPayload::ServerListPing(event)) => {
                patch.refusal(event)
            }
            (Self::SignChange(patch), EventPayload::SignChange(event)) => patch.refusal(event),
            (Self::TabComplete(patch), EventPayload::TabComplete(event)) => patch.refusal(event),
            (Self::VehicleDamage(patch), _) => patch.refusal(),
            _ => None,
        }
    }

    pub(crate) fn lift(self, to: EventKind) -> Option<Self> {
        match (self, to) {
            (patch @ Self::ArrowBodyCountChange(_), EventKind::ArrowBodyCountChange) => Some(patch),
            (patch @ Self::AsyncPlayerChat(_), EventKind::AsyncPlayerChat) => Some(patch),
            (patch @ Self::AsyncPlayerPreLogin(_), EventKind::AsyncPlayerPreLogin) => Some(patch),
            (patch @ Self::BlockBreak(_), EventKind::BlockBreak) => Some(patch),
            (patch @ Self::BlockCanBuild(_), EventKind::BlockCanBuild) => Some(patch),
            (patch @ Self::BlockCook(_), EventKind::BlockCook) => Some(patch),
            (patch @ Self::BlockDamage(_), EventKind::BlockDamage) => Some(patch),
            (patch @ Self::BlockDispense(_), EventKind::BlockDispense) => Some(patch),
            (patch @ Self::BlockDispenseArmor(_), EventKind::BlockDispenseArmor) => Some(patch),
            (patch @ Self::BlockDispenseLoot(_), EventKind::BlockDispenseLoot) => Some(patch),
            (patch @ Self::BlockDropItem(_), EventKind::BlockDropItem) => Some(patch),
            (patch @ Self::BlockExp(_), EventKind::BlockExp) => Some(patch),
            (patch @ Self::BlockExplode(_), EventKind::BlockExplode) => Some(patch),
            (patch @ Self::BlockMultiPlace(_), EventKind::BlockMultiPlace) => Some(patch),
            (patch @ Self::BlockPlace(_), EventKind::BlockPlace) => Some(patch),
            (patch @ Self::BlockRedstone(_), EventKind::BlockRedstone) => Some(patch),
            (patch @ Self::BrewingStandFuel(_), EventKind::BrewingStandFuel) => Some(patch),
            (patch @ Self::BrewingStart(_), EventKind::BrewingStart) => Some(patch),
            (patch @ Self::BroadcastMessage(_), EventKind::BroadcastMessage) => Some(patch),
            (patch @ Self::CampfireStart(_), EventKind::CampfireStart) => Some(patch),
            (patch @ Self::ChunkUnload(_), EventKind::ChunkUnload) => Some(patch),
            (patch @ Self::CraftItem(_), EventKind::CraftItem) => Some(patch),
            (patch @ Self::CrafterCraft(_), EventKind::CrafterCraft) => Some(patch),
            (patch @ Self::CubeMobSplit(_), EventKind::CubeMobSplit) => Some(patch),
            (patch @ Self::EnchantItem(_), EventKind::EnchantItem) => Some(patch),
            (patch @ Self::EnderDragonChangePhase(_), EventKind::EnderDragonChangePhase) => {
                Some(patch)
            }
            (patch @ Self::EntityAirChange(_), EventKind::EntityAirChange) => Some(patch),
            (patch @ Self::EntityBreed(_), EventKind::EntityBreed) => Some(patch),
            (patch @ Self::EntityCombust(_), EventKind::EntityCombust) => Some(patch),
            (patch @ Self::EntityCombustByBlock(_), EventKind::EntityCombustByBlock) => Some(patch),
            (patch @ Self::EntityCombustByEntity(_), EventKind::EntityCombustByEntity) => {
                Some(patch)
            }
            (patch @ Self::EntityDamage(_), EventKind::EntityDamage) => Some(patch),
            (patch @ Self::EntityDamageByBlock(_), EventKind::EntityDamageByBlock) => Some(patch),
            (patch @ Self::EntityDamageByEntity(_), EventKind::EntityDamageByEntity) => Some(patch),
            (patch @ Self::EntityDeath(_), EventKind::EntityDeath) => Some(patch),
            (patch @ Self::EntityEnterLoveMode(_), EventKind::EntityEnterLoveMode) => Some(patch),
            (patch @ Self::EntityExhaustion(_), EventKind::EntityExhaustion) => Some(patch),
            (patch @ Self::EntityExplode(_), EventKind::EntityExplode) => Some(patch),
            (patch @ Self::EntityKnockback(_), EventKind::EntityKnockback) => Some(patch),
            (patch @ Self::EntityKnockbackByEntity(_), EventKind::EntityKnockbackByEntity) => {
                Some(patch)
            }
            (patch @ Self::EntityPortal(_), EventKind::EntityPortal) => Some(patch),
            (patch @ Self::EntityPortalExit(_), EventKind::EntityPortalExit) => Some(patch),
            (patch @ Self::EntityPotionEffect(_), EventKind::EntityPotionEffect) => Some(patch),
            (patch @ Self::EntityRegainHealth(_), EventKind::EntityRegainHealth) => Some(patch),
            (patch @ Self::EntityShootBow(_), EventKind::EntityShootBow) => Some(patch),
            (patch @ Self::EntityTarget(_), EventKind::EntityTarget) => Some(patch),
            (patch @ Self::EntityTargetBlock(_), EventKind::EntityTargetBlock) => Some(patch),
            (patch @ Self::EntityTargetLivingEntity(_), EventKind::EntityTargetLivingEntity) => {
                Some(patch)
            }
            (patch @ Self::EntityTeleport(_), EventKind::EntityTeleport) => Some(patch),
            (patch @ Self::ExpBottle(_), EventKind::ExpBottle) => Some(patch),
            (patch @ Self::ExplosionPrime(_), EventKind::ExplosionPrime) => Some(patch),
            (patch @ Self::FluidLevelChange(_), EventKind::FluidLevelChange) => Some(patch),
            (patch @ Self::FoodLevelChange(_), EventKind::FoodLevelChange) => Some(patch),
            (patch @ Self::FurnaceBurn(_), EventKind::FurnaceBurn) => Some(patch),
            (patch @ Self::FurnaceExtract(_), EventKind::FurnaceExtract) => Some(patch),
            (patch @ Self::FurnaceSmelt(_), EventKind::FurnaceSmelt) => Some(patch),
            (patch @ Self::FurnaceStartSmelt(_), EventKind::FurnaceStartSmelt) => Some(patch),
            (patch @ Self::GenericGame(_), EventKind::GenericGame) => Some(patch),
            (patch @ Self::HopperInventorySearch(_), EventKind::HopperInventorySearch) => {
                Some(patch)
            }
            (patch @ Self::InventoryClick(_), EventKind::InventoryClick) => Some(patch),
            (patch @ Self::InventoryCreative(_), EventKind::InventoryCreative) => Some(patch),
            (patch @ Self::InventoryDrag(_), EventKind::InventoryDrag) => Some(patch),
            (patch @ Self::InventoryMoveItem(_), EventKind::InventoryMoveItem) => Some(patch),
            (patch @ Self::LootGenerate(_), EventKind::LootGenerate) => Some(patch),
            (patch @ Self::PigZombieAnger(_), EventKind::PigZombieAnger) => Some(patch),
            (patch @ Self::PlayerBedEnter(_), EventKind::PlayerBedEnter) => Some(patch),
            (patch @ Self::PlayerBedLeave(_), EventKind::PlayerBedLeave) => Some(patch),
            (patch @ Self::PlayerBucketEmpty(_), EventKind::PlayerBucketEmpty) => Some(patch),
            (patch @ Self::PlayerBucketFill(_), EventKind::PlayerBucketFill) => Some(patch),
            (patch @ Self::PlayerCommandPreprocess(_), EventKind::PlayerCommandPreprocess) => {
                Some(patch)
            }
            (patch @ Self::PlayerCommandSend(_), EventKind::PlayerCommandSend) => Some(patch),
            (patch @ Self::PlayerDeath(_), EventKind::PlayerDeath) => Some(patch),
            (patch @ Self::PlayerDropItem(_), EventKind::PlayerDropItem) => Some(patch),
            (patch @ Self::PlayerEditBook(_), EventKind::PlayerEditBook) => Some(patch),
            (patch @ Self::PlayerEggThrow(_), EventKind::PlayerEggThrow) => Some(patch),
            (patch @ Self::PlayerExpChange(_), EventKind::PlayerExpChange) => Some(patch),
            (patch @ Self::PlayerExpCooldownChange(_), EventKind::PlayerExpCooldownChange) => {
                Some(patch)
            }
            (patch @ Self::PlayerFish(_), EventKind::PlayerFish) => Some(patch),
            (patch @ Self::PlayerHarvestBlock(_), EventKind::PlayerHarvestBlock) => Some(patch),
            (patch @ Self::PlayerInteract(_), EventKind::PlayerInteract) => Some(patch),
            (patch @ Self::PlayerItemConsume(_), EventKind::PlayerItemConsume) => Some(patch),
            (patch @ Self::PlayerItemDamage(_), EventKind::PlayerItemDamage) => Some(patch),
            (patch @ Self::PlayerItemMend(_), EventKind::PlayerItemMend) => Some(patch),
            (patch @ Self::PlayerJoin(_), EventKind::PlayerJoin) => Some(patch),
            (patch @ Self::PlayerKick(_), EventKind::PlayerKick) => Some(patch),
            (patch @ Self::PlayerLeave(_), EventKind::PlayerLeave) => Some(patch),
            (patch @ Self::PlayerLogin(_), EventKind::PlayerLogin) => Some(patch),
            (patch @ Self::PlayerMove(_), EventKind::PlayerMove) => Some(patch),
            (patch @ Self::PlayerPortal(_), EventKind::PlayerPortal) => Some(patch),
            (patch @ Self::PlayerRecipeBookClick(_), EventKind::PlayerRecipeBookClick) => {
                Some(patch)
            }
            (patch @ Self::PlayerRespawn(_), EventKind::PlayerRespawn) => Some(patch),
            (patch @ Self::PlayerSpawnChange(_), EventKind::PlayerSpawnChange) => Some(patch),
            (patch @ Self::PlayerSwapHandItems(_), EventKind::PlayerSwapHandItems) => Some(patch),
            (patch @ Self::PlayerTeleport(_), EventKind::PlayerTeleport) => Some(patch),
            (patch @ Self::PlayerVelocity(_), EventKind::PlayerVelocity) => Some(patch),
            (patch @ Self::PotionSplash(_), EventKind::PotionSplash) => Some(patch),
            (patch @ Self::PrepareAnvil(_), EventKind::PrepareAnvil) => Some(patch),
            (patch @ Self::PrepareGrindstone(_), EventKind::PrepareGrindstone) => Some(patch),
            (patch @ Self::PrepareItemCraft(_), EventKind::PrepareItemCraft) => Some(patch),
            (patch @ Self::PrepareSmithing(_), EventKind::PrepareSmithing) => Some(patch),
            (patch @ Self::RemoteServerCommand(_), EventKind::RemoteServerCommand) => Some(patch),
            (patch @ Self::SculkBloom(_), EventKind::SculkBloom) => Some(patch),
            (patch @ Self::ServerCommand(_), EventKind::ServerCommand) => Some(patch),
            (patch @ Self::ServerListPing(_), EventKind::ServerListPing) => Some(patch),
            (patch @ Self::SheepDyeWool(_), EventKind::SheepDyeWool) => Some(patch),
            (patch @ Self::SignChange(_), EventKind::SignChange) => Some(patch),
            (patch @ Self::SmithItem(_), EventKind::SmithItem) => Some(patch),
            (patch @ Self::TabComplete(_), EventKind::TabComplete) => Some(patch),
            (patch @ Self::TimeSkip(_), EventKind::TimeSkip) => Some(patch),
            (patch @ Self::TradeSelect(_), EventKind::TradeSelect) => Some(patch),
            (patch @ Self::VaultDisplayItem(_), EventKind::VaultDisplayItem) => Some(patch),
            (patch @ Self::VehicleDamage(_), EventKind::VehicleDamage) => Some(patch),
            (patch @ Self::VillagerAcquireTrade(_), EventKind::VillagerAcquireTrade) => Some(patch),
            (patch @ Self::VillagerCareerChange(_), EventKind::VillagerCareerChange) => Some(patch),
            (patch @ Self::VillagerReplenishTrade(_), EventKind::VillagerReplenishTrade) => {
                Some(patch)
            }
            (patch @ Self::VillagerReputationChange(_), EventKind::VillagerReputationChange) => {
                Some(patch)
            }
            (Self::BlockExp(patch), EventKind::BlockBreak) => {
                Some(Self::BlockBreak(BlockBreakPatch {
                    exp_to_drop: patch.exp_to_drop,
                    ..Default::default()
                }))
            }
            (Self::BlockDispense(patch), EventKind::BlockDispenseArmor) => {
                Some(Self::BlockDispenseArmor(BlockDispenseArmorPatch {
                    item: patch.item,
                    velocity: patch.velocity,
                }))
            }
            (Self::BlockPlace(patch), EventKind::BlockMultiPlace) => {
                Some(Self::BlockMultiPlace(BlockMultiPlacePatch {
                    build: patch.build,
                }))
            }
            (Self::InventoryClick(patch), EventKind::CraftItem) => {
                Some(Self::CraftItem(CraftItemPatch {
                    current_item: patch.current_item,
                    cursor: patch.cursor,
                    result: patch.result,
                }))
            }
            (Self::EntityCombust(patch), EventKind::EntityCombustByBlock) => {
                Some(Self::EntityCombustByBlock(EntityCombustByBlockPatch {
                    duration: patch.duration,
                }))
            }
            (Self::EntityCombust(patch), EventKind::EntityCombustByEntity) => {
                Some(Self::EntityCombustByEntity(EntityCombustByEntityPatch {
                    duration: patch.duration,
                }))
            }
            (Self::EntityDamage(patch), EventKind::EntityDamageByBlock) => {
                Some(Self::EntityDamageByBlock(EntityDamageByBlockPatch {
                    damage: patch.damage,
                }))
            }
            (Self::EntityDamage(patch), EventKind::EntityDamageByEntity) => {
                Some(Self::EntityDamageByEntity(EntityDamageByEntityPatch {
                    damage: patch.damage,
                }))
            }
            (Self::EntityKnockback(patch), EventKind::EntityKnockbackByEntity) => Some(
                Self::EntityKnockbackByEntity(EntityKnockbackByEntityPatch {
                    final_knockback: patch.final_knockback,
                }),
            ),
            (Self::EntityTarget(patch), EventKind::EntityTargetLivingEntity) => Some(
                Self::EntityTargetLivingEntity(EntityTargetLivingEntityPatch {
                    target: patch.target,
                }),
            ),
            (Self::BlockExp(patch), EventKind::FurnaceExtract) => {
                Some(Self::FurnaceExtract(FurnaceExtractPatch {
                    exp_to_drop: patch.exp_to_drop,
                }))
            }
            (Self::BlockCook(patch), EventKind::FurnaceSmelt) => {
                Some(Self::FurnaceSmelt(FurnaceSmeltPatch {
                    result: patch.result,
                }))
            }
            (Self::InventoryClick(patch), EventKind::InventoryCreative) => {
                Some(Self::InventoryCreative(InventoryCreativePatch {
                    current_item: patch.current_item,
                    cursor: patch.cursor,
                    result: patch.result,
                }))
            }
            (Self::EntityDeath(patch), EventKind::PlayerDeath) => {
                Some(Self::PlayerDeath(PlayerDeathPatch {
                    dropped_exp: patch.dropped_exp,
                    drops: patch.drops,
                    ..Default::default()
                }))
            }
            (Self::InventoryClick(patch), EventKind::SmithItem) => {
                Some(Self::SmithItem(SmithItemPatch {
                    current_item: patch.current_item,
                    cursor: patch.cursor,
                    result: patch.result,
                }))
            }
            _ => None,
        }
    }
}
