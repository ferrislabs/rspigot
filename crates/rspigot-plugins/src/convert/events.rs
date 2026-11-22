use rspigot_core::plugins::ListDiff;
use rspigot_core::plugins::events::{
    Action, ArrowBodyCountChangePatch, AsyncPlayerChatPatch, AsyncPlayerPreLoginPatch,
    AsyncPlayerPreLoginResult, AsyncStructureGenerateCause, BlockBreakPatch, BlockCanBuildPatch,
    BlockCookPatch, BlockDamagePatch, BlockDispenseArmorPatch, BlockDispenseLootPatch,
    BlockDispensePatch, BlockDropItemPatch, BlockExpPatch, BlockExplodePatch, BlockIgniteCause,
    BlockMultiPlacePatch, BlockPlacePatch, BlockRedstonePatch, BrewingStandFuelPatch,
    BrewingStartPatch, BroadcastMessagePatch, CampfireStartPatch, CauldronLevelChangeReason,
    ChunkUnloadPatch, ClickKind, CraftItemPatch, CrafterCraftPatch, CreatureSpawnReason,
    CreeperPowerCause, CubeMobSplitPatch, DragType, DyeColor, EnchantItemPatch,
    EnderDragonChangePhasePatch, EnderDragonPhase, EntityAirChangePatch, EntityBreedPatch,
    EntityCombustByBlockPatch, EntityCombustByEntityPatch, EntityCombustPatch,
    EntityDamageByBlockPatch, EntityDamageByEntityPatch, EntityDamageCause, EntityDamagePatch,
    EntityDeathPatch, EntityEnterLoveModePatch, EntityExhaustionPatch, EntityExhaustionReason,
    EntityExplodePatch, EntityKnockbackByEntityPatch, EntityKnockbackCause, EntityKnockbackPatch,
    EntityPortalExitPatch, EntityPortalPatch, EntityPotionEffectAction, EntityPotionEffectCause,
    EntityPotionEffectPatch, EntityRegainHealthPatch, EntityRegainHealthRegainReason,
    EntityRemoveCause, EntityShootBowPatch, EntityTargetBlockPatch, EntityTargetLivingEntityPatch,
    EntityTargetPatch, EntityTargetReason, EntityTeleportPatch, EntityTransformReason,
    EntityUnleashReason, EquipmentSlot, EventKind, EventPatch, EventPayload, EventResult,
    ExpBottlePatch, ExplosionPrimePatch, ExplosionResult, Face, FluidLevelChangePatch,
    FoodLevelChangePatch, FurnaceBurnPatch, FurnaceExtractPatch, FurnaceSmeltPatch,
    FurnaceStartSmeltPatch, GenericGamePatch, HangingBreakRemoveCause,
    HopperInventorySearchContainerType, HopperInventorySearchPatch, Instrument, InventoryAction,
    InventoryClickPatch, InventoryCreativePatch, InventoryDragPatch, InventoryMoveItemPatch,
    InventoryTypeSlotType, LightningStrikeCause, LootGeneratePatch, MainHand, PigZombieAngerPatch,
    PlayerAnimationType, PlayerBedEnterPatch, PlayerBedEnterResult, PlayerBedLeavePatch,
    PlayerBucketEmptyPatch, PlayerBucketFillPatch, PlayerCommandPreprocessPatch,
    PlayerCommandSendPatch, PlayerDeathPatch, PlayerDropItemPatch, PlayerEditBookPatch,
    PlayerEggThrowPatch, PlayerExpChangePatch, PlayerExpCooldownChangePatch,
    PlayerExpCooldownChangeReason, PlayerFishPatch, PlayerFishState, PlayerHarvestBlockPatch,
    PlayerInteractPatch, PlayerItemConsumePatch, PlayerItemDamagePatch, PlayerItemMendPatch,
    PlayerJoinPatch, PlayerKickPatch, PlayerLeavePatch, PlayerLoginPatch, PlayerLoginResult,
    PlayerMovePatch, PlayerPortalPatch, PlayerRecipeBookClickPatch,
    PlayerRecipeBookSettingsChangeRecipeBookType, PlayerRespawnPatch, PlayerRespawnReason,
    PlayerSignOpenCause, PlayerSpawnChangeCause, PlayerSpawnChangePatch, PlayerSwapHandItemsPatch,
    PlayerTeleportCause, PlayerTeleportPatch, PlayerVelocityPatch, PortalCreateReason, Pose,
    PotionSplashPatch, PrepareAnvilPatch, PrepareGrindstonePatch, PrepareItemCraftPatch,
    PrepareSmithingPatch, RaidStopReason, RemoteServerCommandPatch, ResourcePackStatus,
    SculkBloomPatch, ServerCommandPatch, ServerListPingPatch, ServerLoadType, SheepDyeWoolPatch,
    Side, SignChangePatch, SmithItemPatch, SpellcasterSpell, Statistic, TNTPrimeCause,
    TabCompletePatch, TimeSkipPatch, TimeSkipReason, TradeSelectPatch, TreeType,
    VaultDisplayItemPatch, VehicleDamagePatch, VillagerAcquireTradePatch,
    VillagerCareerChangePatch, VillagerCareerChangeReason, VillagerReplenishTradePatch,
    VillagerReputationChangePatch,
};
use rspigot_plugin_protocol::v1::{self, event, patch};

use super::Malformed;
use super::{block, contract, entity, player, shared};

pub(super) fn event_kind(value: i32) -> Option<EventKind> {
    Some(match v1::EventKind::try_from(value).ok()? {
        v1::EventKind::AreaEffectCloudApply => EventKind::AreaEffectCloudApply,
        v1::EventKind::ArrowBodyCountChange => EventKind::ArrowBodyCountChange,
        v1::EventKind::AsyncPlayerChat => EventKind::AsyncPlayerChat,
        v1::EventKind::AsyncPlayerPreLogin => EventKind::AsyncPlayerPreLogin,
        v1::EventKind::AsyncStructureGenerate => EventKind::AsyncStructureGenerate,
        v1::EventKind::AsyncStructureSpawn => EventKind::AsyncStructureSpawn,
        v1::EventKind::BatToggleSleep => EventKind::BatToggleSleep,
        v1::EventKind::BellResonate => EventKind::BellResonate,
        v1::EventKind::BellRing => EventKind::BellRing,
        v1::EventKind::BlockBreak => EventKind::BlockBreak,
        v1::EventKind::BlockBrush => EventKind::BlockBrush,
        v1::EventKind::BlockBurn => EventKind::BlockBurn,
        v1::EventKind::BlockCanBuild => EventKind::BlockCanBuild,
        v1::EventKind::BlockCook => EventKind::BlockCook,
        v1::EventKind::BlockDamage => EventKind::BlockDamage,
        v1::EventKind::BlockDamageAbort => EventKind::BlockDamageAbort,
        v1::EventKind::BlockDispense => EventKind::BlockDispense,
        v1::EventKind::BlockDispenseArmor => EventKind::BlockDispenseArmor,
        v1::EventKind::BlockDispenseLoot => EventKind::BlockDispenseLoot,
        v1::EventKind::BlockDropItem => EventKind::BlockDropItem,
        v1::EventKind::BlockExp => EventKind::BlockExp,
        v1::EventKind::BlockExplode => EventKind::BlockExplode,
        v1::EventKind::BlockFade => EventKind::BlockFade,
        v1::EventKind::BlockFertilize => EventKind::BlockFertilize,
        v1::EventKind::BlockForm => EventKind::BlockForm,
        v1::EventKind::BlockFromTo => EventKind::BlockFromTo,
        v1::EventKind::BlockGrow => EventKind::BlockGrow,
        v1::EventKind::BlockIgnite => EventKind::BlockIgnite,
        v1::EventKind::BlockMultiPlace => EventKind::BlockMultiPlace,
        v1::EventKind::BlockPhysics => EventKind::BlockPhysics,
        v1::EventKind::BlockPistonExtend => EventKind::BlockPistonExtend,
        v1::EventKind::BlockPistonRetract => EventKind::BlockPistonRetract,
        v1::EventKind::BlockPlace => EventKind::BlockPlace,
        v1::EventKind::BlockReceiveGame => EventKind::BlockReceiveGame,
        v1::EventKind::BlockRedstone => EventKind::BlockRedstone,
        v1::EventKind::BlockShearEntity => EventKind::BlockShearEntity,
        v1::EventKind::BlockSpread => EventKind::BlockSpread,
        v1::EventKind::Brew => EventKind::Brew,
        v1::EventKind::BrewingStandFuel => EventKind::BrewingStandFuel,
        v1::EventKind::BrewingStart => EventKind::BrewingStart,
        v1::EventKind::BroadcastMessage => EventKind::BroadcastMessage,
        v1::EventKind::CampfireStart => EventKind::CampfireStart,
        v1::EventKind::CauldronLevelChange => EventKind::CauldronLevelChange,
        v1::EventKind::ChunkLoad => EventKind::ChunkLoad,
        v1::EventKind::ChunkPopulate => EventKind::ChunkPopulate,
        v1::EventKind::ChunkUnload => EventKind::ChunkUnload,
        v1::EventKind::CraftItem => EventKind::CraftItem,
        v1::EventKind::CrafterCraft => EventKind::CrafterCraft,
        v1::EventKind::CreatureSpawn => EventKind::CreatureSpawn,
        v1::EventKind::CreeperPower => EventKind::CreeperPower,
        v1::EventKind::CubeMobSplit => EventKind::CubeMobSplit,
        v1::EventKind::EnchantItem => EventKind::EnchantItem,
        v1::EventKind::EnderDragonChangePhase => EventKind::EnderDragonChangePhase,
        v1::EventKind::EntitiesLoad => EventKind::EntitiesLoad,
        v1::EventKind::EntitiesUnload => EventKind::EntitiesUnload,
        v1::EventKind::EntityAirChange => EventKind::EntityAirChange,
        v1::EventKind::EntityBlockForm => EventKind::EntityBlockForm,
        v1::EventKind::EntityBreakDoor => EventKind::EntityBreakDoor,
        v1::EventKind::EntityBreed => EventKind::EntityBreed,
        v1::EventKind::EntityChangeBlock => EventKind::EntityChangeBlock,
        v1::EventKind::EntityCombust => EventKind::EntityCombust,
        v1::EventKind::EntityCombustByBlock => EventKind::EntityCombustByBlock,
        v1::EventKind::EntityCombustByEntity => EventKind::EntityCombustByEntity,
        v1::EventKind::EntityDamage => EventKind::EntityDamage,
        v1::EventKind::EntityDamageByBlock => EventKind::EntityDamageByBlock,
        v1::EventKind::EntityDamageByEntity => EventKind::EntityDamageByEntity,
        v1::EventKind::EntityDeath => EventKind::EntityDeath,
        v1::EventKind::EntityDismount => EventKind::EntityDismount,
        v1::EventKind::EntityDropItem => EventKind::EntityDropItem,
        v1::EventKind::EntityEnterBlock => EventKind::EntityEnterBlock,
        v1::EventKind::EntityEnterLoveMode => EventKind::EntityEnterLoveMode,
        v1::EventKind::EntityExhaustion => EventKind::EntityExhaustion,
        v1::EventKind::EntityExplode => EventKind::EntityExplode,
        v1::EventKind::EntityInteract => EventKind::EntityInteract,
        v1::EventKind::EntityKnockback => EventKind::EntityKnockback,
        v1::EventKind::EntityKnockbackByEntity => EventKind::EntityKnockbackByEntity,
        v1::EventKind::EntityMount => EventKind::EntityMount,
        v1::EventKind::EntityPickupItem => EventKind::EntityPickupItem,
        v1::EventKind::EntityPlace => EventKind::EntityPlace,
        v1::EventKind::EntityPortal => EventKind::EntityPortal,
        v1::EventKind::EntityPortalEnter => EventKind::EntityPortalEnter,
        v1::EventKind::EntityPortalExit => EventKind::EntityPortalExit,
        v1::EventKind::EntityPoseChange => EventKind::EntityPoseChange,
        v1::EventKind::EntityPotionEffect => EventKind::EntityPotionEffect,
        v1::EventKind::EntityRegainHealth => EventKind::EntityRegainHealth,
        v1::EventKind::EntityRemove => EventKind::EntityRemove,
        v1::EventKind::EntityResurrect => EventKind::EntityResurrect,
        v1::EventKind::EntityShootBow => EventKind::EntityShootBow,
        v1::EventKind::EntitySpawn => EventKind::EntitySpawn,
        v1::EventKind::EntitySpellCast => EventKind::EntitySpellCast,
        v1::EventKind::EntityTame => EventKind::EntityTame,
        v1::EventKind::EntityTarget => EventKind::EntityTarget,
        v1::EventKind::EntityTargetBlock => EventKind::EntityTargetBlock,
        v1::EventKind::EntityTargetLivingEntity => EventKind::EntityTargetLivingEntity,
        v1::EventKind::EntityTeleport => EventKind::EntityTeleport,
        v1::EventKind::EntityToggleGlide => EventKind::EntityToggleGlide,
        v1::EventKind::EntityToggleSwim => EventKind::EntityToggleSwim,
        v1::EventKind::EntityTransform => EventKind::EntityTransform,
        v1::EventKind::EntityUnleash => EventKind::EntityUnleash,
        v1::EventKind::ExpBottle => EventKind::ExpBottle,
        v1::EventKind::ExplosionPrime => EventKind::ExplosionPrime,
        v1::EventKind::FireworkExplode => EventKind::FireworkExplode,
        v1::EventKind::FluidLevelChange => EventKind::FluidLevelChange,
        v1::EventKind::FoodLevelChange => EventKind::FoodLevelChange,
        v1::EventKind::FurnaceBurn => EventKind::FurnaceBurn,
        v1::EventKind::FurnaceExtract => EventKind::FurnaceExtract,
        v1::EventKind::FurnaceSmelt => EventKind::FurnaceSmelt,
        v1::EventKind::FurnaceStartSmelt => EventKind::FurnaceStartSmelt,
        v1::EventKind::GenericGame => EventKind::GenericGame,
        v1::EventKind::HangingBreak => EventKind::HangingBreak,
        v1::EventKind::HangingBreakByEntity => EventKind::HangingBreakByEntity,
        v1::EventKind::HangingPlace => EventKind::HangingPlace,
        v1::EventKind::HopperInventorySearch => EventKind::HopperInventorySearch,
        v1::EventKind::HorseJump => EventKind::HorseJump,
        v1::EventKind::InventoryClick => EventKind::InventoryClick,
        v1::EventKind::InventoryClose => EventKind::InventoryClose,
        v1::EventKind::InventoryCreative => EventKind::InventoryCreative,
        v1::EventKind::InventoryDrag => EventKind::InventoryDrag,
        v1::EventKind::InventoryMoveItem => EventKind::InventoryMoveItem,
        v1::EventKind::InventoryOpen => EventKind::InventoryOpen,
        v1::EventKind::InventoryPickupItem => EventKind::InventoryPickupItem,
        v1::EventKind::ItemDespawn => EventKind::ItemDespawn,
        v1::EventKind::ItemMerge => EventKind::ItemMerge,
        v1::EventKind::ItemSpawn => EventKind::ItemSpawn,
        v1::EventKind::LeavesDecay => EventKind::LeavesDecay,
        v1::EventKind::LightningStrike => EventKind::LightningStrike,
        v1::EventKind::LingeringPotionSplash => EventKind::LingeringPotionSplash,
        v1::EventKind::LootGenerate => EventKind::LootGenerate,
        v1::EventKind::MapInitialize => EventKind::MapInitialize,
        v1::EventKind::MoistureChange => EventKind::MoistureChange,
        v1::EventKind::NotePlay => EventKind::NotePlay,
        v1::EventKind::PigZap => EventKind::PigZap,
        v1::EventKind::PigZombieAnger => EventKind::PigZombieAnger,
        v1::EventKind::PiglinBarter => EventKind::PiglinBarter,
        v1::EventKind::PlayerAdvancementCriterionGrant => {
            EventKind::PlayerAdvancementCriterionGrant
        }
        v1::EventKind::PlayerAdvancementDone => EventKind::PlayerAdvancementDone,
        v1::EventKind::PlayerAnimation => EventKind::PlayerAnimation,
        v1::EventKind::PlayerArmorStandManipulate => EventKind::PlayerArmorStandManipulate,
        v1::EventKind::PlayerBedEnter => EventKind::PlayerBedEnter,
        v1::EventKind::PlayerBedLeave => EventKind::PlayerBedLeave,
        v1::EventKind::PlayerBucketEmpty => EventKind::PlayerBucketEmpty,
        v1::EventKind::PlayerBucketEntity => EventKind::PlayerBucketEntity,
        v1::EventKind::PlayerBucketFill => EventKind::PlayerBucketFill,
        v1::EventKind::PlayerChangedMainHand => EventKind::PlayerChangedMainHand,
        v1::EventKind::PlayerChangedWorld => EventKind::PlayerChangedWorld,
        v1::EventKind::PlayerCommandPreprocess => EventKind::PlayerCommandPreprocess,
        v1::EventKind::PlayerCommandSend => EventKind::PlayerCommandSend,
        v1::EventKind::PlayerCustomClick => EventKind::PlayerCustomClick,
        v1::EventKind::PlayerDeath => EventKind::PlayerDeath,
        v1::EventKind::PlayerDropItem => EventKind::PlayerDropItem,
        v1::EventKind::PlayerEditBook => EventKind::PlayerEditBook,
        v1::EventKind::PlayerEggThrow => EventKind::PlayerEggThrow,
        v1::EventKind::PlayerExpChange => EventKind::PlayerExpChange,
        v1::EventKind::PlayerExpCooldownChange => EventKind::PlayerExpCooldownChange,
        v1::EventKind::PlayerFish => EventKind::PlayerFish,
        v1::EventKind::PlayerGameModeChange => EventKind::PlayerGameModeChange,
        v1::EventKind::PlayerHarvestBlock => EventKind::PlayerHarvestBlock,
        v1::EventKind::PlayerHideEntity => EventKind::PlayerHideEntity,
        v1::EventKind::PlayerInput => EventKind::PlayerInput,
        v1::EventKind::PlayerInteract => EventKind::PlayerInteract,
        v1::EventKind::PlayerInteractAtEntity => EventKind::PlayerInteractAtEntity,
        v1::EventKind::PlayerInteractEntity => EventKind::PlayerInteractEntity,
        v1::EventKind::PlayerItemBreak => EventKind::PlayerItemBreak,
        v1::EventKind::PlayerItemConsume => EventKind::PlayerItemConsume,
        v1::EventKind::PlayerItemDamage => EventKind::PlayerItemDamage,
        v1::EventKind::PlayerItemHeld => EventKind::PlayerItemHeld,
        v1::EventKind::PlayerItemMend => EventKind::PlayerItemMend,
        v1::EventKind::PlayerJoin => EventKind::PlayerJoin,
        v1::EventKind::PlayerKick => EventKind::PlayerKick,
        v1::EventKind::PlayerLeashEntity => EventKind::PlayerLeashEntity,
        v1::EventKind::PlayerLeave => EventKind::PlayerLeave,
        v1::EventKind::PlayerLevelChange => EventKind::PlayerLevelChange,
        v1::EventKind::PlayerLinksSend => EventKind::PlayerLinksSend,
        v1::EventKind::PlayerLocaleChange => EventKind::PlayerLocaleChange,
        v1::EventKind::PlayerLogin => EventKind::PlayerLogin,
        v1::EventKind::PlayerMove => EventKind::PlayerMove,
        v1::EventKind::PlayerPickupArrow => EventKind::PlayerPickupArrow,
        v1::EventKind::PlayerPortal => EventKind::PlayerPortal,
        v1::EventKind::PlayerRecipeBookClick => EventKind::PlayerRecipeBookClick,
        v1::EventKind::PlayerRecipeBookSettingsChange => EventKind::PlayerRecipeBookSettingsChange,
        v1::EventKind::PlayerRecipeDiscover => EventKind::PlayerRecipeDiscover,
        v1::EventKind::PlayerResourcePackStatus => EventKind::PlayerResourcePackStatus,
        v1::EventKind::PlayerRespawn => EventKind::PlayerRespawn,
        v1::EventKind::PlayerRiptide => EventKind::PlayerRiptide,
        v1::EventKind::PlayerShearEntity => EventKind::PlayerShearEntity,
        v1::EventKind::PlayerShowEntity => EventKind::PlayerShowEntity,
        v1::EventKind::PlayerSignOpen => EventKind::PlayerSignOpen,
        v1::EventKind::PlayerSpawnChange => EventKind::PlayerSpawnChange,
        v1::EventKind::PlayerStatisticIncrement => EventKind::PlayerStatisticIncrement,
        v1::EventKind::PlayerSwapHandItems => EventKind::PlayerSwapHandItems,
        v1::EventKind::PlayerTakeLecternBook => EventKind::PlayerTakeLecternBook,
        v1::EventKind::PlayerTeleport => EventKind::PlayerTeleport,
        v1::EventKind::PlayerToggleFlight => EventKind::PlayerToggleFlight,
        v1::EventKind::PlayerToggleSneak => EventKind::PlayerToggleSneak,
        v1::EventKind::PlayerToggleSprint => EventKind::PlayerToggleSprint,
        v1::EventKind::PlayerUnleashEntity => EventKind::PlayerUnleashEntity,
        v1::EventKind::PlayerVelocity => EventKind::PlayerVelocity,
        v1::EventKind::PluginDisable => EventKind::PluginDisable,
        v1::EventKind::PluginEnable => EventKind::PluginEnable,
        v1::EventKind::PortalCreate => EventKind::PortalCreate,
        v1::EventKind::PotionSplash => EventKind::PotionSplash,
        v1::EventKind::PrepareAnvil => EventKind::PrepareAnvil,
        v1::EventKind::PrepareGrindstone => EventKind::PrepareGrindstone,
        v1::EventKind::PrepareItemCraft => EventKind::PrepareItemCraft,
        v1::EventKind::PrepareItemEnchant => EventKind::PrepareItemEnchant,
        v1::EventKind::PrepareSmithing => EventKind::PrepareSmithing,
        v1::EventKind::ProjectileHit => EventKind::ProjectileHit,
        v1::EventKind::ProjectileLaunch => EventKind::ProjectileLaunch,
        v1::EventKind::RaidFinish => EventKind::RaidFinish,
        v1::EventKind::RaidSpawnWave => EventKind::RaidSpawnWave,
        v1::EventKind::RaidStop => EventKind::RaidStop,
        v1::EventKind::RaidTrigger => EventKind::RaidTrigger,
        v1::EventKind::RemoteServerCommand => EventKind::RemoteServerCommand,
        v1::EventKind::SculkBloom => EventKind::SculkBloom,
        v1::EventKind::ServerCommand => EventKind::ServerCommand,
        v1::EventKind::ServerListPing => EventKind::ServerListPing,
        v1::EventKind::ServerLoad => EventKind::ServerLoad,
        v1::EventKind::SheepDyeWool => EventKind::SheepDyeWool,
        v1::EventKind::SheepRegrowWool => EventKind::SheepRegrowWool,
        v1::EventKind::SignChange => EventKind::SignChange,
        v1::EventKind::SmithItem => EventKind::SmithItem,
        v1::EventKind::SpawnChange => EventKind::SpawnChange,
        v1::EventKind::SpawnerSpawn => EventKind::SpawnerSpawn,
        v1::EventKind::SpongeAbsorb => EventKind::SpongeAbsorb,
        v1::EventKind::StriderTemperatureChange => EventKind::StriderTemperatureChange,
        v1::EventKind::StructureGrow => EventKind::StructureGrow,
        v1::EventKind::TntPrime => EventKind::TNTPrime,
        v1::EventKind::TabComplete => EventKind::TabComplete,
        v1::EventKind::ThunderChange => EventKind::ThunderChange,
        v1::EventKind::TimeSkip => EventKind::TimeSkip,
        v1::EventKind::TradeSelect => EventKind::TradeSelect,
        v1::EventKind::TrialSpawnerSpawn => EventKind::TrialSpawnerSpawn,
        v1::EventKind::VaultDisplayItem => EventKind::VaultDisplayItem,
        v1::EventKind::VehicleBlockCollision => EventKind::VehicleBlockCollision,
        v1::EventKind::VehicleCreate => EventKind::VehicleCreate,
        v1::EventKind::VehicleDamage => EventKind::VehicleDamage,
        v1::EventKind::VehicleDestroy => EventKind::VehicleDestroy,
        v1::EventKind::VehicleEnter => EventKind::VehicleEnter,
        v1::EventKind::VehicleEntityCollision => EventKind::VehicleEntityCollision,
        v1::EventKind::VehicleExit => EventKind::VehicleExit,
        v1::EventKind::VehicleMove => EventKind::VehicleMove,
        v1::EventKind::VehicleUpdate => EventKind::VehicleUpdate,
        v1::EventKind::VillagerAcquireTrade => EventKind::VillagerAcquireTrade,
        v1::EventKind::VillagerCareerChange => EventKind::VillagerCareerChange,
        v1::EventKind::VillagerReplenishTrade => EventKind::VillagerReplenishTrade,
        v1::EventKind::VillagerReputationChange => EventKind::VillagerReputationChange,
        v1::EventKind::WeatherChange => EventKind::WeatherChange,
        v1::EventKind::WorldInit => EventKind::WorldInit,
        v1::EventKind::WorldLoad => EventKind::WorldLoad,
        v1::EventKind::WorldSave => EventKind::WorldSave,
        v1::EventKind::WorldUnload => EventKind::WorldUnload,
        v1::EventKind::Unspecified => return None,
    })
}

pub(super) fn wire_payload(payload: EventPayload) -> event::Payload {
    match payload {
        EventPayload::AreaEffectCloudApply(event) => {
            event::Payload::AreaEffectCloudApply(v1::AreaEffectCloudApply {
                affected_entities: event
                    .affected_entities
                    .into_iter()
                    .map(entity::wire_view)
                    .collect(),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::ArrowBodyCountChange(event) => {
            event::Payload::ArrowBodyCountChange(v1::ArrowBodyCountChange {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                new_amount: event.new_amount,
                old_amount: event.old_amount,
                reset: event.reset,
            })
        }
        EventPayload::AsyncPlayerChat(event) => {
            event::Payload::AsyncPlayerChat(v1::AsyncPlayerChat {
                format: event.format,
                message: event.message,
                player: Some(player::wire_player(event.player)),
                recipients: event
                    .recipients
                    .into_iter()
                    .map(player::wire_player)
                    .collect(),
            })
        }
        EventPayload::AsyncPlayerPreLogin(event) => {
            event::Payload::AsyncPlayerPreLogin(v1::AsyncPlayerPreLogin {
                address: event.address.to_string(),
                kick_message: event.kick_message,
                login_result: async_player_pre_login_result_to_wire(event.login_result) as i32,
                name: event.name,
                transferred: event.transferred,
                unique_id: event.unique_id.hyphenated().to_string(),
            })
        }
        EventPayload::AsyncStructureGenerate(event) => {
            event::Payload::AsyncStructureGenerate(v1::AsyncStructureGenerate {
                bounding_box: Some(contract::wire_bounding_box(event.bounding_box)),
                cause: async_structure_generate_cause_to_wire(event.cause) as i32,
                chunk_x: event.chunk_x,
                chunk_z: event.chunk_z,
                structure: event.structure,
                world: event.world,
            })
        }
        EventPayload::AsyncStructureSpawn(event) => {
            event::Payload::AsyncStructureSpawn(v1::AsyncStructureSpawn {
                bounding_box: Some(contract::wire_bounding_box(event.bounding_box)),
                chunk_x: event.chunk_x,
                chunk_z: event.chunk_z,
                structure: event.structure,
                world: event.world,
            })
        }
        EventPayload::BatToggleSleep(event) => event::Payload::BatToggleSleep(v1::BatToggleSleep {
            awake: event.awake,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::BellResonate(event) => event::Payload::BellResonate(v1::BellResonate {
            block: Some(shared::wire_block_pos(event.block)),
            resonated_entities: event
                .resonated_entities
                .into_iter()
                .map(entity::wire_view)
                .collect(),
        }),
        EventPayload::BellRing(event) => event::Payload::BellRing(v1::BellRing {
            block: Some(shared::wire_block_pos(event.block)),
            direction: face_to_wire(event.direction) as i32,
            entity: event.entity.map(entity::wire_view),
        }),
        EventPayload::BlockBreak(event) => event::Payload::BlockBreak(v1::BlockBreak {
            block: Some(shared::wire_block_pos(event.block)),
            drop_items: event.drop_items,
            exp_to_drop: event.exp_to_drop,
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::BlockBrush(event) => event::Payload::BlockBrush(v1::BlockBrush {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::BlockBurn(event) => event::Payload::BlockBurn(v1::BlockBurn {
            block: Some(shared::wire_block_pos(event.block)),
            igniting_block: event.igniting_block.map(shared::wire_block_pos),
        }),
        EventPayload::BlockCanBuild(event) => event::Payload::BlockCanBuild(v1::BlockCanBuild {
            block: Some(shared::wire_block_pos(event.block)),
            block_data: Some(block::wire_state(event.block_data)),
            buildable: event.buildable,
            material: event.material,
            player: event.player.map(player::wire_player),
        }),
        EventPayload::BlockCook(event) => event::Payload::BlockCook(v1::BlockCook {
            block: Some(shared::wire_block_pos(event.block)),
            result: Some(shared::wire_item(event.result)),
            source: Some(shared::wire_item(event.source)),
        }),
        EventPayload::BlockDamage(event) => event::Payload::BlockDamage(v1::BlockDamage {
            block: Some(shared::wire_block_pos(event.block)),
            insta_break: event.insta_break,
            item_in_hand: Some(shared::wire_item(event.item_in_hand)),
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::BlockDamageAbort(event) => {
            event::Payload::BlockDamageAbort(v1::BlockDamageAbort {
                block: Some(shared::wire_block_pos(event.block)),
                item_in_hand: Some(shared::wire_item(event.item_in_hand)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::BlockDispense(event) => event::Payload::BlockDispense(v1::BlockDispense {
            block: Some(shared::wire_block_pos(event.block)),
            item: Some(shared::wire_item(event.item)),
            velocity: Some(shared::wire_vec(event.velocity)),
        }),
        EventPayload::BlockDispenseArmor(event) => {
            event::Payload::BlockDispenseArmor(v1::BlockDispenseArmor {
                block: Some(shared::wire_block_pos(event.block)),
                item: Some(shared::wire_item(event.item)),
                target_entity: Some(entity::wire_view(event.target_entity)),
                velocity: Some(shared::wire_vec(event.velocity)),
            })
        }
        EventPayload::BlockDispenseLoot(event) => {
            event::Payload::BlockDispenseLoot(v1::BlockDispenseLoot {
                block: Some(shared::wire_block_pos(event.block)),
                dispensed_loot: event
                    .dispensed_loot
                    .into_iter()
                    .map(shared::wire_item)
                    .collect(),
                player: event.player.map(player::wire_player),
            })
        }
        EventPayload::BlockDropItem(event) => event::Payload::BlockDropItem(v1::BlockDropItem {
            block: Some(shared::wire_block_pos(event.block)),
            block_state: Some(contract::wire_block_snapshot(event.block_state)),
            items: event
                .items
                .into_iter()
                .map(contract::wire_dropped_item)
                .collect(),
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::BlockExp(event) => event::Payload::BlockExp(v1::BlockExp {
            block: Some(shared::wire_block_pos(event.block)),
            exp_to_drop: event.exp_to_drop,
        }),
        EventPayload::BlockExplode(event) => event::Payload::BlockExplode(v1::BlockExplode {
            block: Some(shared::wire_block_pos(event.block)),
            block_list: event
                .block_list
                .into_iter()
                .map(shared::wire_block_pos)
                .collect(),
            exploded_block_state: Some(contract::wire_block_snapshot(event.exploded_block_state)),
            explosion_result: explosion_result_to_wire(event.explosion_result) as i32,
            explosion_yield: event.explosion_yield,
        }),
        EventPayload::BlockFade(event) => event::Payload::BlockFade(v1::BlockFade {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
        }),
        EventPayload::BlockFertilize(event) => event::Payload::BlockFertilize(v1::BlockFertilize {
            block: Some(shared::wire_block_pos(event.block)),
            blocks: event
                .blocks
                .into_iter()
                .map(contract::wire_block_snapshot)
                .collect(),
            player: event.player.map(player::wire_player),
        }),
        EventPayload::BlockForm(event) => event::Payload::BlockForm(v1::BlockForm {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
        }),
        EventPayload::BlockFromTo(event) => event::Payload::BlockFromTo(v1::BlockFromTo {
            block: Some(shared::wire_block_pos(event.block)),
            face: face_to_wire(event.face) as i32,
            to_block: Some(shared::wire_block_pos(event.to_block)),
        }),
        EventPayload::BlockGrow(event) => event::Payload::BlockGrow(v1::BlockGrow {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
        }),
        EventPayload::BlockIgnite(event) => event::Payload::BlockIgnite(v1::BlockIgnite {
            block: Some(shared::wire_block_pos(event.block)),
            cause: block_ignite_cause_to_wire(event.cause) as i32,
            igniting_block: event.igniting_block.map(shared::wire_block_pos),
            igniting_entity: event.igniting_entity.map(entity::wire_view),
            player: event.player.map(player::wire_player),
        }),
        EventPayload::BlockMultiPlace(event) => {
            event::Payload::BlockMultiPlace(v1::BlockMultiPlace {
                block: Some(shared::wire_block_pos(event.block)),
                block_against: Some(shared::wire_block_pos(event.block_against)),
                block_placed: Some(shared::wire_block_pos(event.block_placed)),
                block_replaced_state: Some(contract::wire_block_snapshot(
                    event.block_replaced_state,
                )),
                build: event.build,
                hand: equipment_slot_to_wire(event.hand) as i32,
                item_in_hand: Some(shared::wire_item(event.item_in_hand)),
                player: Some(player::wire_player(event.player)),
                replaced_block_states: event
                    .replaced_block_states
                    .into_iter()
                    .map(contract::wire_block_snapshot)
                    .collect(),
                placed_state: Some(contract::wire_block_snapshot(event.placed_state)),
                placed_states: event
                    .placed_states
                    .into_iter()
                    .map(contract::wire_block_snapshot)
                    .collect(),
            })
        }
        EventPayload::BlockPhysics(event) => event::Payload::BlockPhysics(v1::BlockPhysics {
            block: Some(shared::wire_block_pos(event.block)),
            changed_type: event.changed_type,
            source_block: Some(shared::wire_block_pos(event.source_block)),
        }),
        EventPayload::BlockPistonExtend(event) => {
            event::Payload::BlockPistonExtend(v1::BlockPistonExtend {
                block: Some(shared::wire_block_pos(event.block)),
                blocks: event
                    .blocks
                    .into_iter()
                    .map(shared::wire_block_pos)
                    .collect(),
                direction: face_to_wire(event.direction) as i32,
                sticky: event.sticky,
            })
        }
        EventPayload::BlockPistonRetract(event) => {
            event::Payload::BlockPistonRetract(v1::BlockPistonRetract {
                block: Some(shared::wire_block_pos(event.block)),
                blocks: event
                    .blocks
                    .into_iter()
                    .map(shared::wire_block_pos)
                    .collect(),
                direction: face_to_wire(event.direction) as i32,
                sticky: event.sticky,
            })
        }
        EventPayload::BlockPlace(event) => event::Payload::BlockPlace(v1::BlockPlace {
            block: Some(shared::wire_block_pos(event.block)),
            block_against: Some(shared::wire_block_pos(event.block_against)),
            block_placed: Some(shared::wire_block_pos(event.block_placed)),
            block_replaced_state: Some(contract::wire_block_snapshot(event.block_replaced_state)),
            build: event.build,
            hand: equipment_slot_to_wire(event.hand) as i32,
            item_in_hand: Some(shared::wire_item(event.item_in_hand)),
            player: Some(player::wire_player(event.player)),
            placed_state: Some(contract::wire_block_snapshot(event.placed_state)),
        }),
        EventPayload::BlockReceiveGame(event) => {
            event::Payload::BlockReceiveGame(v1::BlockReceiveGame {
                block: Some(shared::wire_block_pos(event.block)),
                entity: event.entity.map(entity::wire_view),
                event: event.event,
            })
        }
        EventPayload::BlockRedstone(event) => event::Payload::BlockRedstone(v1::BlockRedstone {
            block: Some(shared::wire_block_pos(event.block)),
            new_current: event.new_current,
            old_current: event.old_current,
        }),
        EventPayload::BlockShearEntity(event) => {
            event::Payload::BlockShearEntity(v1::BlockShearEntity {
                block: Some(shared::wire_block_pos(event.block)),
                entity: Some(entity::wire_view(event.entity)),
                tool: Some(shared::wire_item(event.tool)),
            })
        }
        EventPayload::BlockSpread(event) => event::Payload::BlockSpread(v1::BlockSpread {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
            source: Some(shared::wire_block_pos(event.source)),
        }),
        EventPayload::Brew(event) => event::Payload::Brew(v1::Brew {
            block: Some(shared::wire_block_pos(event.block)),
            contents: Some(contract::wire_inventory_ref(event.contents)),
            fuel_level: event.fuel_level,
            results: event.results.into_iter().map(shared::wire_item).collect(),
        }),
        EventPayload::BrewingStandFuel(event) => {
            event::Payload::BrewingStandFuel(v1::BrewingStandFuel {
                block: Some(shared::wire_block_pos(event.block)),
                consuming: event.consuming,
                fuel: Some(shared::wire_item(event.fuel)),
                fuel_power: event.fuel_power,
            })
        }
        EventPayload::BrewingStart(event) => event::Payload::BrewingStart(v1::BrewingStart {
            block: Some(shared::wire_block_pos(event.block)),
            source: Some(shared::wire_item(event.source)),
            total_brew_time: event.total_brew_time,
        }),
        EventPayload::BroadcastMessage(event) => {
            event::Payload::BroadcastMessage(v1::BroadcastMessage {
                message: event.message,
                recipients: event
                    .recipients
                    .into_iter()
                    .map(contract::wire_command_sender)
                    .collect(),
            })
        }
        EventPayload::CampfireStart(event) => event::Payload::CampfireStart(v1::CampfireStart {
            block: Some(shared::wire_block_pos(event.block)),
            recipe: event.recipe,
            source: Some(shared::wire_item(event.source)),
            total_cook_time: event.total_cook_time,
        }),
        EventPayload::CauldronLevelChange(event) => {
            event::Payload::CauldronLevelChange(v1::CauldronLevelChange {
                block: Some(shared::wire_block_pos(event.block)),
                entity: event.entity.map(entity::wire_view),
                new_state: Some(contract::wire_block_snapshot(event.new_state)),
                reason: cauldron_level_change_reason_to_wire(event.reason) as i32,
            })
        }
        EventPayload::ChunkLoad(event) => event::Payload::ChunkLoad(v1::ChunkLoad {
            chunk: Some(contract::wire_chunk_ref(event.chunk)),
            new_chunk: event.new_chunk,
            world: event.world,
        }),
        EventPayload::ChunkPopulate(event) => event::Payload::ChunkPopulate(v1::ChunkPopulate {
            chunk: Some(contract::wire_chunk_ref(event.chunk)),
            world: event.world,
        }),
        EventPayload::ChunkUnload(event) => event::Payload::ChunkUnload(v1::ChunkUnload {
            chunk: Some(contract::wire_chunk_ref(event.chunk)),
            save_chunk: event.save_chunk,
            world: event.world,
        }),
        EventPayload::CraftItem(event) => event::Payload::CraftItem(v1::CraftItem {
            action: inventory_action_to_wire(event.action) as i32,
            click: click_kind_to_wire(event.click) as i32,
            clicked_inventory: event.clicked_inventory.map(contract::wire_inventory_ref),
            current_item: event.current_item.map(shared::wire_item),
            cursor: event.cursor.map(shared::wire_item),
            hotbar_button: event.hotbar_button,
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            left_click: event.left_click,
            raw_slot: event.raw_slot,
            recipe: event.recipe,
            result: event_result_to_wire(event.result) as i32,
            right_click: event.right_click,
            shift_click: event.shift_click,
            slot: event.slot,
            slot_type: inventory_type_slot_type_to_wire(event.slot_type) as i32,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            who_clicked: Some(player::wire_player(event.who_clicked)),
        }),
        EventPayload::CrafterCraft(event) => event::Payload::CrafterCraft(v1::CrafterCraft {
            block: Some(shared::wire_block_pos(event.block)),
            recipe: event.recipe,
            remaining_items: event
                .remaining_items
                .into_iter()
                .map(shared::wire_item)
                .collect(),
            result: Some(shared::wire_item(event.result)),
        }),
        EventPayload::CreatureSpawn(event) => event::Payload::CreatureSpawn(v1::CreatureSpawn {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            location: Some(contract::wire_location(event.location)),
            spawn_reason: creature_spawn_reason_to_wire(event.spawn_reason) as i32,
        }),
        EventPayload::CreeperPower(event) => event::Payload::CreeperPower(v1::CreeperPower {
            cause: creeper_power_cause_to_wire(event.cause) as i32,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            lightning: event.lightning.map(entity::wire_view),
        }),
        EventPayload::CubeMobSplit(event) => event::Payload::CubeMobSplit(v1::CubeMobSplit {
            count: event.count,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EnchantItem(event) => event::Payload::EnchantItem(v1::EnchantItem {
            enchant_block: Some(shared::wire_block_pos(event.enchant_block)),
            enchanter: Some(player::wire_player(event.enchanter)),
            enchantment_hint: event.enchantment_hint,
            enchants_to_add: event
                .enchants_to_add
                .into_iter()
                .map(shared::wire_enchant)
                .collect(),
            exp_level_cost: event.exp_level_cost,
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            item: Some(shared::wire_item(event.item)),
            level_hint: event.level_hint,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            which_button: event.which_button,
        }),
        EventPayload::EnderDragonChangePhase(event) => {
            event::Payload::EnderDragonChangePhase(v1::EnderDragonChangePhase {
                current_phase: event
                    .current_phase
                    .map(|value| ender_dragon_phase_to_wire(value) as i32),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                new_phase: ender_dragon_phase_to_wire(event.new_phase) as i32,
            })
        }
        EventPayload::EntitiesLoad(event) => event::Payload::EntitiesLoad(v1::EntitiesLoad {
            chunk: Some(contract::wire_chunk_ref(event.chunk)),
            entities: event.entities.into_iter().map(entity::wire_view).collect(),
            world: event.world,
        }),
        EventPayload::EntitiesUnload(event) => event::Payload::EntitiesUnload(v1::EntitiesUnload {
            chunk: Some(contract::wire_chunk_ref(event.chunk)),
            entities: event.entities.into_iter().map(entity::wire_view).collect(),
            world: event.world,
        }),
        EventPayload::EntityAirChange(event) => {
            event::Payload::EntityAirChange(v1::EntityAirChange {
                amount: event.amount,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::EntityBlockForm(event) => {
            event::Payload::EntityBlockForm(v1::EntityBlockForm {
                block: Some(shared::wire_block_pos(event.block)),
                entity: Some(entity::wire_view(event.entity)),
                new_state: Some(contract::wire_block_snapshot(event.new_state)),
            })
        }
        EventPayload::EntityBreakDoor(event) => {
            event::Payload::EntityBreakDoor(v1::EntityBreakDoor {
                block: Some(shared::wire_block_pos(event.block)),
                block_data: Some(block::wire_state(event.block_data)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                to: event.to,
            })
        }
        EventPayload::EntityBreed(event) => event::Payload::EntityBreed(v1::EntityBreed {
            bred_with: event.bred_with.map(shared::wire_item),
            breeder: event.breeder.map(entity::wire_view),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            experience: event.experience,
            father: Some(entity::wire_view(event.father)),
            mother: Some(entity::wire_view(event.mother)),
        }),
        EventPayload::EntityChangeBlock(event) => {
            event::Payload::EntityChangeBlock(v1::EntityChangeBlock {
                block: Some(shared::wire_block_pos(event.block)),
                block_data: Some(block::wire_state(event.block_data)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                to: event.to,
            })
        }
        EventPayload::EntityCombust(event) => event::Payload::EntityCombust(v1::EntityCombust {
            duration: event.duration,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EntityCombustByBlock(event) => {
            event::Payload::EntityCombustByBlock(v1::EntityCombustByBlock {
                combuster: event.combuster.map(shared::wire_block_pos),
                duration: event.duration,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::EntityCombustByEntity(event) => {
            event::Payload::EntityCombustByEntity(v1::EntityCombustByEntity {
                combuster: Some(entity::wire_view(event.combuster)),
                duration: event.duration,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::EntityDamage(event) => event::Payload::EntityDamage(v1::EntityDamage {
            cause: entity_damage_cause_to_wire(event.cause) as i32,
            damage: event.damage,
            damage_source: Some(contract::wire_damage_source(event.damage_source)),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            final_damage: event.final_damage,
        }),
        EventPayload::EntityDamageByBlock(event) => {
            event::Payload::EntityDamageByBlock(v1::EntityDamageByBlock {
                cause: entity_damage_cause_to_wire(event.cause) as i32,
                damage: event.damage,
                damage_source: Some(contract::wire_damage_source(event.damage_source)),
                damager: event.damager.map(shared::wire_block_pos),
                damager_block_state: event.damager_block_state.map(contract::wire_block_snapshot),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                final_damage: event.final_damage,
            })
        }
        EventPayload::EntityDamageByEntity(event) => {
            event::Payload::EntityDamageByEntity(v1::EntityDamageByEntity {
                cause: entity_damage_cause_to_wire(event.cause) as i32,
                damage: event.damage,
                damage_source: Some(contract::wire_damage_source(event.damage_source)),
                damager: Some(entity::wire_view(event.damager)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                final_damage: event.final_damage,
            })
        }
        EventPayload::EntityDeath(event) => event::Payload::EntityDeath(v1::EntityDeath {
            damage_source: Some(contract::wire_damage_source(event.damage_source)),
            dropped_exp: event.dropped_exp,
            drops: event.drops.into_iter().map(shared::wire_item).collect(),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EntityDismount(event) => event::Payload::EntityDismount(v1::EntityDismount {
            dismounted: Some(entity::wire_view(event.dismounted)),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EntityDropItem(event) => event::Payload::EntityDropItem(v1::EntityDropItem {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            item_drop: Some(entity::wire_view(event.item_drop)),
        }),
        EventPayload::EntityEnterBlock(event) => {
            event::Payload::EntityEnterBlock(v1::EntityEnterBlock {
                block: Some(shared::wire_block_pos(event.block)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::EntityEnterLoveMode(event) => {
            event::Payload::EntityEnterLoveMode(v1::EntityEnterLoveMode {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                human_entity: event.human_entity.map(player::wire_player),
                ticks_in_love: event.ticks_in_love,
            })
        }
        EventPayload::EntityExhaustion(event) => {
            event::Payload::EntityExhaustion(v1::EntityExhaustion {
                entity: Some(player::wire_player(event.entity)),
                entity_type: event.entity_type,
                exhaustion: event.exhaustion,
                exhaustion_reason: entity_exhaustion_reason_to_wire(event.exhaustion_reason) as i32,
            })
        }
        EventPayload::EntityExplode(event) => event::Payload::EntityExplode(v1::EntityExplode {
            block_list: event
                .block_list
                .into_iter()
                .map(shared::wire_block_pos)
                .collect(),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            explosion_result: explosion_result_to_wire(event.explosion_result) as i32,
            location: Some(contract::wire_location(event.location)),
            explosion_yield: event.explosion_yield,
        }),
        EventPayload::EntityInteract(event) => event::Payload::EntityInteract(v1::EntityInteract {
            block: Some(shared::wire_block_pos(event.block)),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EntityKnockback(event) => {
            event::Payload::EntityKnockback(v1::EntityKnockback {
                cause: entity_knockback_cause_to_wire(event.cause) as i32,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                final_knockback: Some(shared::wire_vec(event.final_knockback)),
                force: event.force,
                knockback: Some(shared::wire_vec(event.knockback)),
            })
        }
        EventPayload::EntityKnockbackByEntity(event) => {
            event::Payload::EntityKnockbackByEntity(v1::EntityKnockbackByEntity {
                cause: entity_knockback_cause_to_wire(event.cause) as i32,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                final_knockback: Some(shared::wire_vec(event.final_knockback)),
                force: event.force,
                knockback: Some(shared::wire_vec(event.knockback)),
                source_entity: Some(entity::wire_view(event.source_entity)),
            })
        }
        EventPayload::EntityMount(event) => event::Payload::EntityMount(v1::EntityMount {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            mount: Some(entity::wire_view(event.mount)),
        }),
        EventPayload::EntityPickupItem(event) => {
            event::Payload::EntityPickupItem(v1::EntityPickupItem {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                item: Some(entity::wire_view(event.item)),
                remaining: event.remaining,
            })
        }
        EventPayload::EntityPlace(event) => event::Payload::EntityPlace(v1::EntityPlace {
            block: Some(shared::wire_block_pos(event.block)),
            block_face: face_to_wire(event.block_face) as i32,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            hand: equipment_slot_to_wire(event.hand) as i32,
            player: event.player.map(player::wire_player),
        }),
        EventPayload::EntityPortal(event) => event::Payload::EntityPortal(v1::EntityPortal {
            can_create_portal: event.can_create_portal,
            creation_radius: event.creation_radius,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            from: Some(contract::wire_location(event.from)),
            search_radius: event.search_radius,
            to: event.to.map(contract::wire_location),
        }),
        EventPayload::EntityPortalEnter(event) => {
            event::Payload::EntityPortalEnter(v1::EntityPortalEnter {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                location: Some(contract::wire_location(event.location)),
            })
        }
        EventPayload::EntityPortalExit(event) => {
            event::Payload::EntityPortalExit(v1::EntityPortalExit {
                after: Some(shared::wire_vec(event.after)),
                before: Some(shared::wire_vec(event.before)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                from: Some(contract::wire_location(event.from)),
                to: event.to.map(contract::wire_location),
            })
        }
        EventPayload::EntityPoseChange(event) => {
            event::Payload::EntityPoseChange(v1::EntityPoseChange {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                pose: pose_to_wire(event.pose) as i32,
            })
        }
        EventPayload::EntityPotionEffect(event) => {
            event::Payload::EntityPotionEffect(v1::EntityPotionEffect {
                action: entity_potion_effect_action_to_wire(event.action) as i32,
                cause: entity_potion_effect_cause_to_wire(event.cause) as i32,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                modified_type: event.modified_type,
                new_effect: event.new_effect.map(shared::wire_effect),
                old_effect: event.old_effect.map(shared::wire_effect),
                overrides: event.overrides,
            })
        }
        EventPayload::EntityRegainHealth(event) => {
            event::Payload::EntityRegainHealth(v1::EntityRegainHealth {
                amount: event.amount,
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                regain_reason: entity_regain_health_regain_reason_to_wire(event.regain_reason)
                    as i32,
            })
        }
        EventPayload::EntityRemove(event) => event::Payload::EntityRemove(v1::EntityRemove {
            cause: entity_remove_cause_to_wire(event.cause) as i32,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
        }),
        EventPayload::EntityResurrect(event) => {
            event::Payload::EntityResurrect(v1::EntityResurrect {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                hand: event.hand.map(|value| equipment_slot_to_wire(value) as i32),
            })
        }
        EventPayload::EntityShootBow(event) => event::Payload::EntityShootBow(v1::EntityShootBow {
            bow: event.bow.map(shared::wire_item),
            consumable: event.consumable.map(shared::wire_item),
            consume_item: event.consume_item,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            force: event.force,
            hand: equipment_slot_to_wire(event.hand) as i32,
            projectile: Some(entity::wire_view(event.projectile)),
        }),
        EventPayload::EntitySpawn(event) => event::Payload::EntitySpawn(v1::EntitySpawn {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            location: Some(contract::wire_location(event.location)),
        }),
        EventPayload::EntitySpellCast(event) => {
            event::Payload::EntitySpellCast(v1::EntitySpellCast {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                spell: spellcaster_spell_to_wire(event.spell) as i32,
            })
        }
        EventPayload::EntityTame(event) => event::Payload::EntityTame(v1::EntityTame {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            owner: Some(contract::wire_owner_ref(event.owner)),
        }),
        EventPayload::EntityTarget(event) => event::Payload::EntityTarget(v1::EntityTarget {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            reason: entity_target_reason_to_wire(event.reason) as i32,
            target: event.target.map(entity::wire_view),
        }),
        EventPayload::EntityTargetBlock(event) => {
            event::Payload::EntityTargetBlock(v1::EntityTargetBlock {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                target: event.target.map(shared::wire_block_pos),
            })
        }
        EventPayload::EntityTargetLivingEntity(event) => {
            event::Payload::EntityTargetLivingEntity(v1::EntityTargetLivingEntity {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                reason: entity_target_reason_to_wire(event.reason) as i32,
                target: event.target.map(entity::wire_view),
            })
        }
        EventPayload::EntityTeleport(event) => event::Payload::EntityTeleport(v1::EntityTeleport {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            from: Some(contract::wire_location(event.from)),
            to: event.to.map(contract::wire_location),
        }),
        EventPayload::EntityToggleGlide(event) => {
            event::Payload::EntityToggleGlide(v1::EntityToggleGlide {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                gliding: event.gliding,
            })
        }
        EventPayload::EntityToggleSwim(event) => {
            event::Payload::EntityToggleSwim(v1::EntityToggleSwim {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                swimming: event.swimming,
            })
        }
        EventPayload::EntityTransform(event) => {
            event::Payload::EntityTransform(v1::EntityTransform {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                transform_reason: entity_transform_reason_to_wire(event.transform_reason) as i32,
                transformed_entities: event
                    .transformed_entities
                    .into_iter()
                    .map(entity::wire_view)
                    .collect(),
                transformed_entity: Some(entity::wire_view(event.transformed_entity)),
            })
        }
        EventPayload::EntityUnleash(event) => event::Payload::EntityUnleash(v1::EntityUnleash {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            reason: entity_unleash_reason_to_wire(event.reason) as i32,
        }),
        EventPayload::ExpBottle(event) => event::Payload::ExpBottle(v1::ExpBottle {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            experience: event.experience,
            hit_block: event.hit_block.map(shared::wire_block_pos),
            hit_block_face: event.hit_block_face.map(|value| face_to_wire(value) as i32),
            hit_entity: event.hit_entity.map(entity::wire_view),
            show_effect: event.show_effect,
        }),
        EventPayload::ExplosionPrime(event) => event::Payload::ExplosionPrime(v1::ExplosionPrime {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            fire: event.fire,
            radius: event.radius,
        }),
        EventPayload::FireworkExplode(event) => {
            event::Payload::FireworkExplode(v1::FireworkExplode {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::FluidLevelChange(event) => {
            event::Payload::FluidLevelChange(v1::FluidLevelChange {
                block: Some(shared::wire_block_pos(event.block)),
                new_data: Some(block::wire_state(event.new_data)),
            })
        }
        EventPayload::FoodLevelChange(event) => {
            event::Payload::FoodLevelChange(v1::FoodLevelChange {
                entity: Some(player::wire_player(event.entity)),
                entity_type: event.entity_type,
                food_level: event.food_level,
                item: event.item.map(shared::wire_item),
            })
        }
        EventPayload::FurnaceBurn(event) => event::Payload::FurnaceBurn(v1::FurnaceBurn {
            block: Some(shared::wire_block_pos(event.block)),
            burn_time: event.burn_time,
            burning: event.burning,
            fuel: Some(shared::wire_item(event.fuel)),
        }),
        EventPayload::FurnaceExtract(event) => event::Payload::FurnaceExtract(v1::FurnaceExtract {
            block: Some(shared::wire_block_pos(event.block)),
            exp_to_drop: event.exp_to_drop,
            item_amount: event.item_amount,
            item_type: event.item_type,
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::FurnaceSmelt(event) => event::Payload::FurnaceSmelt(v1::FurnaceSmelt {
            block: Some(shared::wire_block_pos(event.block)),
            result: Some(shared::wire_item(event.result)),
            source: Some(shared::wire_item(event.source)),
        }),
        EventPayload::FurnaceStartSmelt(event) => {
            event::Payload::FurnaceStartSmelt(v1::FurnaceStartSmelt {
                block: Some(shared::wire_block_pos(event.block)),
                recipe: event.recipe,
                source: Some(shared::wire_item(event.source)),
                total_cook_time: event.total_cook_time,
            })
        }
        EventPayload::GenericGame(event) => event::Payload::GenericGame(v1::GenericGame {
            entity: event.entity.map(entity::wire_view),
            event: event.event,
            location: Some(contract::wire_location(event.location)),
            radius: event.radius,
            world: event.world,
        }),
        EventPayload::HangingBreak(event) => event::Payload::HangingBreak(v1::HangingBreak {
            cause: hanging_break_remove_cause_to_wire(event.cause) as i32,
            entity: Some(entity::wire_view(event.entity)),
        }),
        EventPayload::HangingBreakByEntity(event) => {
            event::Payload::HangingBreakByEntity(v1::HangingBreakByEntity {
                cause: hanging_break_remove_cause_to_wire(event.cause) as i32,
                entity: Some(entity::wire_view(event.entity)),
                remover: event.remover.map(entity::wire_view),
            })
        }
        EventPayload::HangingPlace(event) => event::Payload::HangingPlace(v1::HangingPlace {
            block: Some(shared::wire_block_pos(event.block)),
            block_face: face_to_wire(event.block_face) as i32,
            entity: Some(entity::wire_view(event.entity)),
            hand: event.hand.map(|value| equipment_slot_to_wire(value) as i32),
            item_stack: event.item_stack.map(shared::wire_item),
            player: event.player.map(player::wire_player),
        }),
        EventPayload::HopperInventorySearch(event) => {
            event::Payload::HopperInventorySearch(v1::HopperInventorySearch {
                block: Some(shared::wire_block_pos(event.block)),
                container_type: hopper_inventory_search_container_type_to_wire(event.container_type)
                    as i32,
                inventory: event.inventory.map(contract::wire_inventory_ref),
                search_block: Some(shared::wire_block_pos(event.search_block)),
            })
        }
        EventPayload::HorseJump(event) => event::Payload::HorseJump(v1::HorseJump {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            power: event.power,
        }),
        EventPayload::InventoryClick(event) => event::Payload::InventoryClick(v1::InventoryClick {
            action: inventory_action_to_wire(event.action) as i32,
            click: click_kind_to_wire(event.click) as i32,
            clicked_inventory: event.clicked_inventory.map(contract::wire_inventory_ref),
            current_item: event.current_item.map(shared::wire_item),
            cursor: event.cursor.map(shared::wire_item),
            hotbar_button: event.hotbar_button,
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            left_click: event.left_click,
            raw_slot: event.raw_slot,
            result: event_result_to_wire(event.result) as i32,
            right_click: event.right_click,
            shift_click: event.shift_click,
            slot: event.slot,
            slot_type: inventory_type_slot_type_to_wire(event.slot_type) as i32,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            who_clicked: Some(player::wire_player(event.who_clicked)),
        }),
        EventPayload::InventoryClose(event) => event::Payload::InventoryClose(v1::InventoryClose {
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            player: Some(player::wire_player(event.player)),
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            by_player: event.by_player,
        }),
        EventPayload::InventoryCreative(event) => {
            event::Payload::InventoryCreative(v1::InventoryCreative {
                action: inventory_action_to_wire(event.action) as i32,
                click: click_kind_to_wire(event.click) as i32,
                clicked_inventory: event.clicked_inventory.map(contract::wire_inventory_ref),
                current_item: event.current_item.map(shared::wire_item),
                cursor: event.cursor.map(shared::wire_item),
                hotbar_button: event.hotbar_button,
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                left_click: event.left_click,
                raw_slot: event.raw_slot,
                result: event_result_to_wire(event.result) as i32,
                right_click: event.right_click,
                shift_click: event.shift_click,
                slot: event.slot,
                slot_type: inventory_type_slot_type_to_wire(event.slot_type) as i32,
                view: Some(contract::wire_inventory_view(event.view)),
                viewers: event.viewers.into_iter().map(player::wire_player).collect(),
                who_clicked: Some(player::wire_player(event.who_clicked)),
            })
        }
        EventPayload::InventoryDrag(event) => event::Payload::InventoryDrag(v1::InventoryDrag {
            cursor: event.cursor.map(shared::wire_item),
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            inventory_slots: event.inventory_slots,
            new_items: event.new_items.into_iter().map(shared::wire_slot).collect(),
            old_cursor: Some(shared::wire_item(event.old_cursor)),
            raw_slots: event.raw_slots,
            result: event_result_to_wire(event.result) as i32,
            drag_type: drag_type_to_wire(event.drag_type) as i32,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            who_clicked: Some(player::wire_player(event.who_clicked)),
        }),
        EventPayload::InventoryMoveItem(event) => {
            event::Payload::InventoryMoveItem(v1::InventoryMoveItem {
                destination: Some(contract::wire_inventory_ref(event.destination)),
                initiator: Some(contract::wire_inventory_ref(event.initiator)),
                item: Some(shared::wire_item(event.item)),
                source: Some(contract::wire_inventory_ref(event.source)),
            })
        }
        EventPayload::InventoryOpen(event) => event::Payload::InventoryOpen(v1::InventoryOpen {
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            player: Some(player::wire_player(event.player)),
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
        }),
        EventPayload::InventoryPickupItem(event) => {
            event::Payload::InventoryPickupItem(v1::InventoryPickupItem {
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                item: Some(entity::wire_view(event.item)),
            })
        }
        EventPayload::ItemDespawn(event) => event::Payload::ItemDespawn(v1::ItemDespawn {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            location: Some(contract::wire_location(event.location)),
        }),
        EventPayload::ItemMerge(event) => event::Payload::ItemMerge(v1::ItemMerge {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            target: Some(entity::wire_view(event.target)),
        }),
        EventPayload::ItemSpawn(event) => event::Payload::ItemSpawn(v1::ItemSpawn {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            location: Some(contract::wire_location(event.location)),
        }),
        EventPayload::LeavesDecay(event) => event::Payload::LeavesDecay(v1::LeavesDecay {
            block: Some(shared::wire_block_pos(event.block)),
        }),
        EventPayload::LightningStrike(event) => {
            event::Payload::LightningStrike(v1::LightningStrike {
                cause: lightning_strike_cause_to_wire(event.cause) as i32,
                lightning: Some(entity::wire_view(event.lightning)),
                world: event.world,
            })
        }
        EventPayload::LingeringPotionSplash(event) => {
            event::Payload::LingeringPotionSplash(v1::LingeringPotionSplash {
                area_effect_cloud: Some(entity::wire_view(event.area_effect_cloud)),
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                hit_block: event.hit_block.map(shared::wire_block_pos),
                hit_block_face: event.hit_block_face.map(|value| face_to_wire(value) as i32),
                hit_entity: event.hit_entity.map(entity::wire_view),
            })
        }
        EventPayload::LootGenerate(event) => event::Payload::LootGenerate(v1::LootGenerate {
            entity: event.entity.map(entity::wire_view),
            inventory_holder: event.inventory_holder.map(contract::wire_holder_ref),
            loot: event.loot.into_iter().map(shared::wire_item).collect(),
            loot_context: Some(contract::wire_loot_context(event.loot_context)),
            loot_table: event.loot_table,
            plugin: event.plugin,
            world: event.world,
        }),
        EventPayload::MapInitialize(event) => event::Payload::MapInitialize(v1::MapInitialize {
            map_id: event.map_id,
        }),
        EventPayload::MoistureChange(event) => event::Payload::MoistureChange(v1::MoistureChange {
            block: Some(shared::wire_block_pos(event.block)),
            new_state: Some(contract::wire_block_snapshot(event.new_state)),
        }),
        EventPayload::NotePlay(event) => event::Payload::NotePlay(v1::NotePlay {
            block: Some(shared::wire_block_pos(event.block)),
            instrument: instrument_to_wire(event.instrument) as i32,
            note: i32::from(event.note),
        }),
        EventPayload::PigZap(event) => event::Payload::PigZap(v1::PigZap {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            lightning: Some(entity::wire_view(event.lightning)),
            transform_reason: entity_transform_reason_to_wire(event.transform_reason) as i32,
            transformed_entities: event
                .transformed_entities
                .into_iter()
                .map(entity::wire_view)
                .collect(),
            transformed_entity: Some(entity::wire_view(event.transformed_entity)),
        }),
        EventPayload::PigZombieAnger(event) => event::Payload::PigZombieAnger(v1::PigZombieAnger {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            new_anger: event.new_anger,
            target: event.target.map(entity::wire_view),
        }),
        EventPayload::PiglinBarter(event) => event::Payload::PiglinBarter(v1::PiglinBarter {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            input: Some(shared::wire_item(event.input)),
            outcome: event.outcome.into_iter().map(shared::wire_item).collect(),
        }),
        EventPayload::PlayerAdvancementCriterionGrant(event) => {
            event::Payload::PlayerAdvancementCriterionGrant(v1::PlayerAdvancementCriterionGrant {
                advancement: event.advancement,
                criterion: event.criterion,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerAdvancementDone(event) => {
            event::Payload::PlayerAdvancementDone(v1::PlayerAdvancementDone {
                advancement: event.advancement,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerAnimation(event) => {
            event::Payload::PlayerAnimation(v1::PlayerAnimation {
                animation_type: player_animation_type_to_wire(event.animation_type) as i32,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerArmorStandManipulate(event) => {
            event::Payload::PlayerArmorStandManipulate(v1::PlayerArmorStandManipulate {
                armor_stand_item: Some(shared::wire_item(event.armor_stand_item)),
                hand: equipment_slot_to_wire(event.hand) as i32,
                player: Some(player::wire_player(event.player)),
                player_item: Some(shared::wire_item(event.player_item)),
                right_clicked: Some(entity::wire_view(event.right_clicked)),
                slot: equipment_slot_to_wire(event.slot) as i32,
            })
        }
        EventPayload::PlayerBedEnter(event) => event::Payload::PlayerBedEnter(v1::PlayerBedEnter {
            bed: Some(shared::wire_block_pos(event.bed)),
            bed_enter_result: player_bed_enter_result_to_wire(event.bed_enter_result) as i32,
            player: Some(player::wire_player(event.player)),
            use_bed: event_result_to_wire(event.use_bed) as i32,
        }),
        EventPayload::PlayerBedLeave(event) => event::Payload::PlayerBedLeave(v1::PlayerBedLeave {
            bed: Some(shared::wire_block_pos(event.bed)),
            player: Some(player::wire_player(event.player)),
            set_spawn_location: event.set_spawn_location,
        }),
        EventPayload::PlayerBucketEmpty(event) => {
            event::Payload::PlayerBucketEmpty(v1::PlayerBucketEmpty {
                block: Some(shared::wire_block_pos(event.block)),
                block_clicked: Some(shared::wire_block_pos(event.block_clicked)),
                block_face: face_to_wire(event.block_face) as i32,
                bucket: event.bucket,
                hand: equipment_slot_to_wire(event.hand) as i32,
                item_stack: event.item_stack.map(shared::wire_item),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerBucketEntity(event) => {
            event::Payload::PlayerBucketEntity(v1::PlayerBucketEntity {
                entity: Some(entity::wire_view(event.entity)),
                entity_bucket: Some(shared::wire_item(event.entity_bucket)),
                hand: equipment_slot_to_wire(event.hand) as i32,
                original_bucket: Some(shared::wire_item(event.original_bucket)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerBucketFill(event) => {
            event::Payload::PlayerBucketFill(v1::PlayerBucketFill {
                block: Some(shared::wire_block_pos(event.block)),
                block_clicked: Some(shared::wire_block_pos(event.block_clicked)),
                block_face: face_to_wire(event.block_face) as i32,
                bucket: event.bucket,
                hand: equipment_slot_to_wire(event.hand) as i32,
                item_stack: event.item_stack.map(shared::wire_item),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerChangedMainHand(event) => {
            event::Payload::PlayerChangedMainHand(v1::PlayerChangedMainHand {
                main_hand: main_hand_to_wire(event.main_hand) as i32,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerChangedWorld(event) => {
            event::Payload::PlayerChangedWorld(v1::PlayerChangedWorld {
                from: event.from,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerCommandPreprocess(event) => {
            event::Payload::PlayerCommandPreprocess(v1::PlayerCommandPreprocess {
                message: event.message,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerCommandSend(event) => {
            event::Payload::PlayerCommandSend(v1::PlayerCommandSend {
                commands: event.commands,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerCustomClick(event) => {
            event::Payload::PlayerCustomClick(v1::PlayerCustomClick {
                data: event.data,
                click_id: event.click_id,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerDeath(event) => event::Payload::PlayerDeath(v1::PlayerDeath {
            damage_source: Some(contract::wire_damage_source(event.damage_source)),
            death_message: event.death_message,
            dropped_exp: event.dropped_exp,
            drops: event.drops.into_iter().map(shared::wire_item).collect(),
            entity: Some(player::wire_player(event.entity)),
            entity_type: event.entity_type,
            keep_inventory: event.keep_inventory,
            keep_level: event.keep_level,
            new_exp: event.new_exp,
            new_level: event.new_level,
            new_total_exp: event.new_total_exp,
        }),
        EventPayload::PlayerDropItem(event) => event::Payload::PlayerDropItem(v1::PlayerDropItem {
            item_drop: Some(entity::wire_view(event.item_drop)),
            player: Some(player::wire_player(event.player)),
            item: Some(shared::wire_item(event.item)),
        }),
        EventPayload::PlayerEditBook(event) => event::Payload::PlayerEditBook(v1::PlayerEditBook {
            new_book_meta: Some(contract::wire_book_meta(event.new_book_meta)),
            player: Some(player::wire_player(event.player)),
            previous_book_meta: Some(contract::wire_book_meta(event.previous_book_meta)),
            signing: event.signing,
        }),
        EventPayload::PlayerEggThrow(event) => event::Payload::PlayerEggThrow(v1::PlayerEggThrow {
            egg: Some(entity::wire_view(event.egg)),
            hatching: event.hatching,
            hatching_type: event.hatching_type,
            num_hatches: i32::from(event.num_hatches),
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::PlayerExpChange(event) => {
            event::Payload::PlayerExpChange(v1::PlayerExpChange {
                amount: event.amount,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerExpCooldownChange(event) => {
            event::Payload::PlayerExpCooldownChange(v1::PlayerExpCooldownChange {
                new_cooldown: event.new_cooldown,
                player: Some(player::wire_player(event.player)),
                reason: player_exp_cooldown_change_reason_to_wire(event.reason) as i32,
            })
        }
        EventPayload::PlayerFish(event) => event::Payload::PlayerFish(v1::PlayerFish {
            caught: event.caught.map(entity::wire_view),
            exp_to_drop: event.exp_to_drop,
            hand: event.hand.map(|value| equipment_slot_to_wire(value) as i32),
            hook: Some(entity::wire_view(event.hook)),
            player: Some(player::wire_player(event.player)),
            state: player_fish_state_to_wire(event.state) as i32,
        }),
        EventPayload::PlayerGameModeChange(event) => {
            event::Payload::PlayerGameModeChange(v1::PlayerGameModeChange {
                new_game_mode: player::wire_game_mode(event.new_game_mode) as i32,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerHarvestBlock(event) => {
            event::Payload::PlayerHarvestBlock(v1::PlayerHarvestBlock {
                hand: equipment_slot_to_wire(event.hand) as i32,
                harvested_block: Some(shared::wire_block_pos(event.harvested_block)),
                items_harvested: event
                    .items_harvested
                    .into_iter()
                    .map(shared::wire_item)
                    .collect(),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerHideEntity(event) => {
            event::Payload::PlayerHideEntity(v1::PlayerHideEntity {
                entity: Some(entity::wire_view(event.entity)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerInput(event) => event::Payload::PlayerInput(v1::PlayerInput {
            input: Some(contract::wire_input(event.input)),
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::PlayerInteract(event) => event::Payload::PlayerInteract(v1::PlayerInteract {
            action: action_to_wire(event.action) as i32,
            block: event.block,
            block_face: face_to_wire(event.block_face) as i32,
            block_in_hand: event.block_in_hand,
            clicked_block: event.clicked_block.map(shared::wire_block_pos),
            clicked_position: event.clicked_position.map(shared::wire_vec),
            hand: event.hand.map(|value| equipment_slot_to_wire(value) as i32),
            has_item: event.has_item,
            item: event.item.map(shared::wire_item),
            material: event.material,
            player: Some(player::wire_player(event.player)),
            use_interacted_block: event_result_to_wire(event.use_interacted_block) as i32,
            use_item_in_hand: event_result_to_wire(event.use_item_in_hand) as i32,
        }),
        EventPayload::PlayerInteractAtEntity(event) => {
            event::Payload::PlayerInteractAtEntity(v1::PlayerInteractAtEntity {
                clicked_position: Some(shared::wire_vec(event.clicked_position)),
                hand: equipment_slot_to_wire(event.hand) as i32,
                player: Some(player::wire_player(event.player)),
                right_clicked: Some(entity::wire_view(event.right_clicked)),
            })
        }
        EventPayload::PlayerInteractEntity(event) => {
            event::Payload::PlayerInteractEntity(v1::PlayerInteractEntity {
                hand: equipment_slot_to_wire(event.hand) as i32,
                player: Some(player::wire_player(event.player)),
                right_clicked: Some(entity::wire_view(event.right_clicked)),
            })
        }
        EventPayload::PlayerItemBreak(event) => {
            event::Payload::PlayerItemBreak(v1::PlayerItemBreak {
                broken_item: Some(shared::wire_item(event.broken_item)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerItemConsume(event) => {
            event::Payload::PlayerItemConsume(v1::PlayerItemConsume {
                hand: equipment_slot_to_wire(event.hand) as i32,
                item: Some(shared::wire_item(event.item)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerItemDamage(event) => {
            event::Payload::PlayerItemDamage(v1::PlayerItemDamage {
                damage: event.damage,
                item: Some(shared::wire_item(event.item)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerItemHeld(event) => event::Payload::PlayerItemHeld(v1::PlayerItemHeld {
            new_slot: event.new_slot,
            player: Some(player::wire_player(event.player)),
            previous_slot: event.previous_slot,
        }),
        EventPayload::PlayerItemMend(event) => event::Payload::PlayerItemMend(v1::PlayerItemMend {
            experience_orb: Some(entity::wire_view(event.experience_orb)),
            item: Some(shared::wire_item(event.item)),
            player: Some(player::wire_player(event.player)),
            repair_amount: event.repair_amount,
            slot: equipment_slot_to_wire(event.slot) as i32,
        }),
        EventPayload::PlayerJoin(event) => event::Payload::PlayerJoin(v1::PlayerJoin {
            join_message: event.join_message,
            player: Some(player::wire_player(event.player)),
        }),
        EventPayload::PlayerKick(event) => event::Payload::PlayerKick(v1::PlayerKick {
            leave_message: event.leave_message,
            player: Some(player::wire_player(event.player)),
            reason: event.reason,
        }),
        EventPayload::PlayerLeashEntity(event) => {
            event::Payload::PlayerLeashEntity(v1::PlayerLeashEntity {
                entity: Some(entity::wire_view(event.entity)),
                hand: equipment_slot_to_wire(event.hand) as i32,
                leash_holder: Some(entity::wire_view(event.leash_holder)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerLeave(event) => event::Payload::PlayerLeave(v1::PlayerLeave {
            player: Some(player::wire_player(event.player)),
            quit_message: event.quit_message,
        }),
        EventPayload::PlayerLevelChange(event) => {
            event::Payload::PlayerLevelChange(v1::PlayerLevelChange {
                new_level: event.new_level,
                old_level: event.old_level,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerLinksSend(event) => {
            event::Payload::PlayerLinksSend(v1::PlayerLinksSend {
                links: Some(contract::wire_server_links(event.links)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerLocaleChange(event) => {
            event::Payload::PlayerLocaleChange(v1::PlayerLocaleChange {
                locale: event.locale,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerLogin(event) => event::Payload::PlayerLogin(v1::PlayerLogin {
            address: event.address.to_string(),
            hostname: event.hostname,
            kick_message: event.kick_message,
            player: Some(player::wire_player(event.player)),
            real_address: event.real_address.to_string(),
            result: player_login_result_to_wire(event.result) as i32,
        }),
        EventPayload::PlayerMove(event) => event::Payload::PlayerMove(v1::PlayerMove {
            from: Some(contract::wire_location(event.from)),
            player: Some(player::wire_player(event.player)),
            to: event.to.map(contract::wire_location),
        }),
        EventPayload::PlayerPickupArrow(event) => {
            event::Payload::PlayerPickupArrow(v1::PlayerPickupArrow {
                arrow: Some(entity::wire_view(event.arrow)),
                player: Some(player::wire_player(event.player)),
                remaining: event.remaining,
                item: Some(shared::wire_item(event.item)),
            })
        }
        EventPayload::PlayerPortal(event) => event::Payload::PlayerPortal(v1::PlayerPortal {
            can_create_portal: event.can_create_portal,
            cause: player_teleport_cause_to_wire(event.cause) as i32,
            creation_radius: event.creation_radius,
            from: Some(contract::wire_location(event.from)),
            player: Some(player::wire_player(event.player)),
            search_radius: event.search_radius,
            to: event.to.map(contract::wire_location),
        }),
        EventPayload::PlayerRecipeBookClick(event) => {
            event::Payload::PlayerRecipeBookClick(v1::PlayerRecipeBookClick {
                original_recipe: event.original_recipe,
                player: Some(player::wire_player(event.player)),
                recipe: event.recipe,
                shift_click: event.shift_click,
            })
        }
        EventPayload::PlayerRecipeBookSettingsChange(event) => {
            event::Payload::PlayerRecipeBookSettingsChange(v1::PlayerRecipeBookSettingsChange {
                filtering: event.filtering,
                open: event.open,
                player: Some(player::wire_player(event.player)),
                recipe_book_type: player_recipe_book_settings_change_recipe_book_type_to_wire(
                    event.recipe_book_type,
                ) as i32,
            })
        }
        EventPayload::PlayerRecipeDiscover(event) => {
            event::Payload::PlayerRecipeDiscover(v1::PlayerRecipeDiscover {
                player: Some(player::wire_player(event.player)),
                recipe: event.recipe,
            })
        }
        EventPayload::PlayerResourcePackStatus(event) => {
            event::Payload::PlayerResourcePackStatus(v1::PlayerResourcePackStatus {
                pack_id: event.pack_id.hyphenated().to_string(),
                player: Some(player::wire_player(event.player)),
                status: resource_pack_status_to_wire(event.status) as i32,
            })
        }
        EventPayload::PlayerRespawn(event) => event::Payload::PlayerRespawn(v1::PlayerRespawn {
            anchor_spawn: event.anchor_spawn,
            bed_spawn: event.bed_spawn,
            player: Some(player::wire_player(event.player)),
            respawn_location: Some(contract::wire_location(event.respawn_location)),
            respawn_reason: player_respawn_reason_to_wire(event.respawn_reason) as i32,
        }),
        EventPayload::PlayerRiptide(event) => event::Payload::PlayerRiptide(v1::PlayerRiptide {
            item: Some(shared::wire_item(event.item)),
            player: Some(player::wire_player(event.player)),
            velocity: Some(shared::wire_vec(event.velocity)),
        }),
        EventPayload::PlayerShearEntity(event) => {
            event::Payload::PlayerShearEntity(v1::PlayerShearEntity {
                entity: Some(entity::wire_view(event.entity)),
                hand: equipment_slot_to_wire(event.hand) as i32,
                item: Some(shared::wire_item(event.item)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerShowEntity(event) => {
            event::Payload::PlayerShowEntity(v1::PlayerShowEntity {
                entity: Some(entity::wire_view(event.entity)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerSignOpen(event) => event::Payload::PlayerSignOpen(v1::PlayerSignOpen {
            cause: player_sign_open_cause_to_wire(event.cause) as i32,
            player: Some(player::wire_player(event.player)),
            side: side_to_wire(event.side) as i32,
            sign: Some(contract::wire_sign_data(event.sign)),
        }),
        EventPayload::PlayerSpawnChange(event) => {
            event::Payload::PlayerSpawnChange(v1::PlayerSpawnChange {
                cause: player_spawn_change_cause_to_wire(event.cause) as i32,
                forced: event.forced,
                new_spawn: event.new_spawn.map(contract::wire_location),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerStatisticIncrement(event) => {
            event::Payload::PlayerStatisticIncrement(v1::PlayerStatisticIncrement {
                entity_type: event.entity_type,
                material: event.material,
                new_value: event.new_value,
                player: Some(player::wire_player(event.player)),
                previous_value: event.previous_value,
                statistic: statistic_to_wire(event.statistic) as i32,
            })
        }
        EventPayload::PlayerSwapHandItems(event) => {
            event::Payload::PlayerSwapHandItems(v1::PlayerSwapHandItems {
                main_hand_item: event.main_hand_item.map(shared::wire_item),
                off_hand_item: event.off_hand_item.map(shared::wire_item),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerTakeLecternBook(event) => {
            event::Payload::PlayerTakeLecternBook(v1::PlayerTakeLecternBook {
                book: event.book.map(shared::wire_item),
                lectern: Some(contract::wire_lectern_data(event.lectern)),
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerTeleport(event) => event::Payload::PlayerTeleport(v1::PlayerTeleport {
            cause: player_teleport_cause_to_wire(event.cause) as i32,
            from: Some(contract::wire_location(event.from)),
            player: Some(player::wire_player(event.player)),
            to: event.to.map(contract::wire_location),
        }),
        EventPayload::PlayerToggleFlight(event) => {
            event::Payload::PlayerToggleFlight(v1::PlayerToggleFlight {
                flying: event.flying,
                player: Some(player::wire_player(event.player)),
            })
        }
        EventPayload::PlayerToggleSneak(event) => {
            event::Payload::PlayerToggleSneak(v1::PlayerToggleSneak {
                player: Some(player::wire_player(event.player)),
                sneaking: event.sneaking,
            })
        }
        EventPayload::PlayerToggleSprint(event) => {
            event::Payload::PlayerToggleSprint(v1::PlayerToggleSprint {
                player: Some(player::wire_player(event.player)),
                sprinting: event.sprinting,
            })
        }
        EventPayload::PlayerUnleashEntity(event) => {
            event::Payload::PlayerUnleashEntity(v1::PlayerUnleashEntity {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                hand: equipment_slot_to_wire(event.hand) as i32,
                player: Some(player::wire_player(event.player)),
                reason: entity_unleash_reason_to_wire(event.reason) as i32,
            })
        }
        EventPayload::PlayerVelocity(event) => event::Payload::PlayerVelocity(v1::PlayerVelocity {
            player: Some(player::wire_player(event.player)),
            velocity: Some(shared::wire_vec(event.velocity)),
        }),
        EventPayload::PluginDisable(event) => event::Payload::PluginDisable(v1::PluginDisable {
            plugin: Some(contract::wire_plugin_info(event.plugin)),
        }),
        EventPayload::PluginEnable(event) => event::Payload::PluginEnable(v1::PluginEnable {
            plugin: Some(contract::wire_plugin_info(event.plugin)),
        }),
        EventPayload::PortalCreate(event) => event::Payload::PortalCreate(v1::PortalCreate {
            blocks: event
                .blocks
                .into_iter()
                .map(contract::wire_block_snapshot)
                .collect(),
            entity: event.entity.map(entity::wire_view),
            reason: portal_create_reason_to_wire(event.reason) as i32,
            world: event.world,
        }),
        EventPayload::PotionSplash(event) => event::Payload::PotionSplash(v1::PotionSplash {
            affected_entities: event
                .affected_entities
                .into_iter()
                .map(entity::wire_view)
                .collect(),
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            intensities: event
                .intensities
                .into_iter()
                .map(|(key, value)| v1::EntitySnapshotDoubleEntry {
                    key: Some(entity::wire_view(key)),
                    value,
                })
                .collect(),
            hit_block: event.hit_block.map(shared::wire_block_pos),
            hit_block_face: event.hit_block_face.map(|value| face_to_wire(value) as i32),
            hit_entity: event.hit_entity.map(entity::wire_view),
            potion: Some(entity::wire_view(event.potion)),
        }),
        EventPayload::PrepareAnvil(event) => event::Payload::PrepareAnvil(v1::PrepareAnvil {
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            result: event.result.map(shared::wire_item),
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
        }),
        EventPayload::PrepareGrindstone(event) => {
            event::Payload::PrepareGrindstone(v1::PrepareGrindstone {
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                result: event.result.map(shared::wire_item),
                view: Some(contract::wire_inventory_view(event.view)),
                viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            })
        }
        EventPayload::PrepareItemCraft(event) => {
            event::Payload::PrepareItemCraft(v1::PrepareItemCraft {
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                recipe: event.recipe,
                repair: event.repair,
                view: Some(contract::wire_inventory_view(event.view)),
                viewers: event.viewers.into_iter().map(player::wire_player).collect(),
                result: event.result.map(shared::wire_item),
            })
        }
        EventPayload::PrepareItemEnchant(event) => {
            event::Payload::PrepareItemEnchant(v1::PrepareItemEnchant {
                enchant_block: Some(shared::wire_block_pos(event.enchant_block)),
                enchanter: Some(player::wire_player(event.enchanter)),
                enchantment_bonus: event.enchantment_bonus,
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                item: Some(shared::wire_item(event.item)),
                offers: event
                    .offers
                    .into_iter()
                    .map(contract::wire_enchant_offer)
                    .collect(),
                view: Some(contract::wire_inventory_view(event.view)),
                viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            })
        }
        EventPayload::PrepareSmithing(event) => {
            event::Payload::PrepareSmithing(v1::PrepareSmithing {
                inventory: Some(contract::wire_inventory_ref(event.inventory)),
                result: event.result.map(shared::wire_item),
                view: Some(contract::wire_inventory_view(event.view)),
                viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            })
        }
        EventPayload::ProjectileHit(event) => event::Payload::ProjectileHit(v1::ProjectileHit {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            hit_block: event.hit_block.map(shared::wire_block_pos),
            hit_block_face: event.hit_block_face.map(|value| face_to_wire(value) as i32),
            hit_entity: event.hit_entity.map(entity::wire_view),
        }),
        EventPayload::ProjectileLaunch(event) => {
            event::Payload::ProjectileLaunch(v1::ProjectileLaunch {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                location: Some(contract::wire_location(event.location)),
            })
        }
        EventPayload::RaidFinish(event) => event::Payload::RaidFinish(v1::RaidFinish {
            raid: Some(contract::wire_raid_ref(event.raid)),
            winners: event.winners.into_iter().map(player::wire_player).collect(),
            world: event.world,
        }),
        EventPayload::RaidSpawnWave(event) => event::Payload::RaidSpawnWave(v1::RaidSpawnWave {
            patrol_leader: event.patrol_leader.map(entity::wire_view),
            raid: Some(contract::wire_raid_ref(event.raid)),
            raiders: event.raiders.into_iter().map(entity::wire_view).collect(),
            world: event.world,
        }),
        EventPayload::RaidStop(event) => event::Payload::RaidStop(v1::RaidStop {
            raid: Some(contract::wire_raid_ref(event.raid)),
            reason: raid_stop_reason_to_wire(event.reason) as i32,
            world: event.world,
        }),
        EventPayload::RaidTrigger(event) => event::Payload::RaidTrigger(v1::RaidTrigger {
            player: Some(player::wire_player(event.player)),
            raid: Some(contract::wire_raid_ref(event.raid)),
            world: event.world,
        }),
        EventPayload::RemoteServerCommand(event) => {
            event::Payload::RemoteServerCommand(v1::RemoteServerCommand {
                command: event.command,
                sender: Some(contract::wire_command_sender(event.sender)),
            })
        }
        EventPayload::SculkBloom(event) => event::Payload::SculkBloom(v1::SculkBloom {
            block: Some(shared::wire_block_pos(event.block)),
            charge: event.charge,
        }),
        EventPayload::ServerCommand(event) => event::Payload::ServerCommand(v1::ServerCommand {
            command: event.command,
            sender: Some(contract::wire_command_sender(event.sender)),
        }),
        EventPayload::ServerListPing(event) => event::Payload::ServerListPing(v1::ServerListPing {
            address: event.address.to_string(),
            hostname: event.hostname,
            players: event.players.into_iter().map(player::wire_player).collect(),
            max_players: event.max_players,
            motd: event.motd,
            num_players: event.num_players,
            hidden_players: event
                .hidden_players
                .into_iter()
                .map(|value| value.hyphenated().to_string())
                .collect(),
        }),
        EventPayload::ServerLoad(event) => event::Payload::ServerLoad(v1::ServerLoad {
            load_type: server_load_type_to_wire(event.load_type) as i32,
        }),
        EventPayload::SheepDyeWool(event) => event::Payload::SheepDyeWool(v1::SheepDyeWool {
            color: dye_color_to_wire(event.color) as i32,
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            player: event.player.map(player::wire_player),
        }),
        EventPayload::SheepRegrowWool(event) => {
            event::Payload::SheepRegrowWool(v1::SheepRegrowWool {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
            })
        }
        EventPayload::SignChange(event) => event::Payload::SignChange(v1::SignChange {
            block: Some(shared::wire_block_pos(event.block)),
            lines: event.lines,
            player: Some(player::wire_player(event.player)),
            side: side_to_wire(event.side) as i32,
        }),
        EventPayload::SmithItem(event) => event::Payload::SmithItem(v1::SmithItem {
            action: inventory_action_to_wire(event.action) as i32,
            click: click_kind_to_wire(event.click) as i32,
            clicked_inventory: event.clicked_inventory.map(contract::wire_inventory_ref),
            current_item: event.current_item.map(shared::wire_item),
            cursor: event.cursor.map(shared::wire_item),
            hotbar_button: event.hotbar_button,
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            left_click: event.left_click,
            raw_slot: event.raw_slot,
            result: event_result_to_wire(event.result) as i32,
            right_click: event.right_click,
            shift_click: event.shift_click,
            slot: event.slot,
            slot_type: inventory_type_slot_type_to_wire(event.slot_type) as i32,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            who_clicked: Some(player::wire_player(event.who_clicked)),
        }),
        EventPayload::SpawnChange(event) => event::Payload::SpawnChange(v1::SpawnChange {
            previous_location: Some(contract::wire_location(event.previous_location)),
            world: event.world,
        }),
        EventPayload::SpawnerSpawn(event) => event::Payload::SpawnerSpawn(v1::SpawnerSpawn {
            entity: Some(entity::wire_view(event.entity)),
            entity_type: event.entity_type,
            location: Some(contract::wire_location(event.location)),
            spawner: Some(contract::wire_spawner_data(event.spawner)),
        }),
        EventPayload::SpongeAbsorb(event) => event::Payload::SpongeAbsorb(v1::SpongeAbsorb {
            block: Some(shared::wire_block_pos(event.block)),
            blocks: event
                .blocks
                .into_iter()
                .map(contract::wire_block_snapshot)
                .collect(),
        }),
        EventPayload::StriderTemperatureChange(event) => {
            event::Payload::StriderTemperatureChange(v1::StriderTemperatureChange {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                shivering: event.shivering,
            })
        }
        EventPayload::StructureGrow(event) => event::Payload::StructureGrow(v1::StructureGrow {
            blocks: event
                .blocks
                .into_iter()
                .map(contract::wire_block_snapshot)
                .collect(),
            from_bonemeal: event.from_bonemeal,
            location: Some(contract::wire_location(event.location)),
            player: event.player.map(player::wire_player),
            species: tree_type_to_wire(event.species) as i32,
            world: event.world,
        }),
        EventPayload::TNTPrime(event) => event::Payload::TntPrime(v1::TntPrime {
            block: Some(shared::wire_block_pos(event.block)),
            cause: tnt_prime_cause_to_wire(event.cause) as i32,
            priming_block: event.priming_block.map(shared::wire_block_pos),
            priming_entity: event.priming_entity.map(entity::wire_view),
        }),
        EventPayload::TabComplete(event) => event::Payload::TabComplete(v1::TabComplete {
            buffer: event.buffer,
            completions: event.completions,
            sender: Some(contract::wire_command_sender(event.sender)),
        }),
        EventPayload::ThunderChange(event) => event::Payload::ThunderChange(v1::ThunderChange {
            to_thunder_state: event.to_thunder_state,
            world: event.world,
        }),
        EventPayload::TimeSkip(event) => event::Payload::TimeSkip(v1::TimeSkip {
            skip_amount: event.skip_amount,
            skip_reason: time_skip_reason_to_wire(event.skip_reason) as i32,
            world: event.world,
        }),
        EventPayload::TradeSelect(event) => event::Payload::TradeSelect(v1::TradeSelect {
            index: event.index,
            inventory: Some(contract::wire_inventory_ref(event.inventory)),
            merchant: Some(contract::wire_merchant_ref(event.merchant)),
            result: event_result_to_wire(event.result) as i32,
            view: Some(contract::wire_inventory_view(event.view)),
            viewers: event.viewers.into_iter().map(player::wire_player).collect(),
            who_clicked: Some(player::wire_player(event.who_clicked)),
        }),
        EventPayload::TrialSpawnerSpawn(event) => {
            event::Payload::TrialSpawnerSpawn(v1::TrialSpawnerSpawn {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                location: Some(contract::wire_location(event.location)),
                trial_spawner: Some(contract::wire_trial_spawner_data(event.trial_spawner)),
            })
        }
        EventPayload::VaultDisplayItem(event) => {
            event::Payload::VaultDisplayItem(v1::VaultDisplayItem {
                block: Some(shared::wire_block_pos(event.block)),
                display_item: event.display_item.map(shared::wire_item),
            })
        }
        EventPayload::VehicleBlockCollision(event) => {
            event::Payload::VehicleBlockCollision(v1::VehicleBlockCollision {
                block: Some(shared::wire_block_pos(event.block)),
                vehicle: Some(entity::wire_view(event.vehicle)),
            })
        }
        EventPayload::VehicleCreate(event) => event::Payload::VehicleCreate(v1::VehicleCreate {
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleDamage(event) => event::Payload::VehicleDamage(v1::VehicleDamage {
            attacker: event.attacker.map(entity::wire_view),
            damage: event.damage,
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleDestroy(event) => event::Payload::VehicleDestroy(v1::VehicleDestroy {
            attacker: event.attacker.map(entity::wire_view),
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleEnter(event) => event::Payload::VehicleEnter(v1::VehicleEnter {
            entered: Some(entity::wire_view(event.entered)),
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleEntityCollision(event) => {
            event::Payload::VehicleEntityCollision(v1::VehicleEntityCollision {
                collision_cancelled: event.collision_cancelled,
                entity: Some(entity::wire_view(event.entity)),
                pickup_cancelled: event.pickup_cancelled,
                vehicle: Some(entity::wire_view(event.vehicle)),
            })
        }
        EventPayload::VehicleExit(event) => event::Payload::VehicleExit(v1::VehicleExit {
            exited: Some(entity::wire_view(event.exited)),
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleMove(event) => event::Payload::VehicleMove(v1::VehicleMove {
            from: Some(contract::wire_location(event.from)),
            to: Some(contract::wire_location(event.to)),
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VehicleUpdate(event) => event::Payload::VehicleUpdate(v1::VehicleUpdate {
            vehicle: Some(entity::wire_view(event.vehicle)),
        }),
        EventPayload::VillagerAcquireTrade(event) => {
            event::Payload::VillagerAcquireTrade(v1::VillagerAcquireTrade {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                recipe: Some(contract::wire_trade(event.recipe)),
            })
        }
        EventPayload::VillagerCareerChange(event) => {
            event::Payload::VillagerCareerChange(v1::VillagerCareerChange {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                profession: event.profession,
                reason: villager_career_change_reason_to_wire(event.reason) as i32,
            })
        }
        EventPayload::VillagerReplenishTrade(event) => {
            event::Payload::VillagerReplenishTrade(v1::VillagerReplenishTrade {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                recipe: Some(contract::wire_trade(event.recipe)),
            })
        }
        EventPayload::VillagerReputationChange(event) => {
            event::Payload::VillagerReputationChange(v1::VillagerReputationChange {
                entity: Some(entity::wire_view(event.entity)),
                entity_type: event.entity_type,
                max_value: event.max_value,
                new_value: event.new_value,
                old_value: event.old_value,
                reason: event.reason,
                reputation_type: event.reputation_type,
                target: event.target.map(entity::wire_view),
                target_uuid: event.target_uuid.hyphenated().to_string(),
            })
        }
        EventPayload::WeatherChange(event) => event::Payload::WeatherChange(v1::WeatherChange {
            to_weather_state: event.to_weather_state,
            world: event.world,
        }),
        EventPayload::WorldInit(event) => {
            event::Payload::WorldInit(v1::WorldInit { world: event.world })
        }
        EventPayload::WorldLoad(event) => {
            event::Payload::WorldLoad(v1::WorldLoad { world: event.world })
        }
        EventPayload::WorldSave(event) => {
            event::Payload::WorldSave(v1::WorldSave { world: event.world })
        }
        EventPayload::WorldUnload(event) => {
            event::Payload::WorldUnload(v1::WorldUnload { world: event.world })
        }
    }
}

pub(super) fn change(change: patch::Change) -> Result<EventPatch, Malformed> {
    Ok(match change {
        patch::Change::ArrowBodyCountChange(patch) => {
            EventPatch::ArrowBodyCountChange(ArrowBodyCountChangePatch {
                new_amount: patch.new_amount,
            })
        }
        patch::Change::AsyncPlayerChat(patch) => {
            EventPatch::AsyncPlayerChat(AsyncPlayerChatPatch {
                format: patch.format,
                message: patch.message,
                recipients: match patch.recipients {
                    Some(diff) => ListDiff {
                        removed: diff.removed,
                        replaced: diff
                            .replaced
                            .into_iter()
                            .map(|entry| {
                                Ok((
                                    entry.index,
                                    player::snapshot(shared::required(entry.value)?)?,
                                ))
                            })
                            .collect::<Result<Vec<_>, Malformed>>()?,
                        added: diff
                            .added
                            .into_iter()
                            .map(player::snapshot)
                            .collect::<Result<Vec<_>, _>>()?,
                    },
                    None => ListDiff::default(),
                },
            })
        }
        patch::Change::AsyncPlayerPreLogin(patch) => {
            EventPatch::AsyncPlayerPreLogin(AsyncPlayerPreLoginPatch {
                kick_message: patch.kick_message,
                login_result: patch
                    .login_result
                    .map(async_player_pre_login_result_from_wire)
                    .transpose()?,
            })
        }
        patch::Change::BlockBreak(patch) => EventPatch::BlockBreak(BlockBreakPatch {
            drop_items: patch.drop_items,
            exp_to_drop: patch.exp_to_drop,
        }),
        patch::Change::BlockCanBuild(patch) => EventPatch::BlockCanBuild(BlockCanBuildPatch {
            buildable: patch.buildable,
        }),
        patch::Change::BlockCook(patch) => EventPatch::BlockCook(BlockCookPatch {
            result: patch.result.map(shared::item),
        }),
        patch::Change::BlockDamage(patch) => EventPatch::BlockDamage(BlockDamagePatch {
            insta_break: patch.insta_break,
        }),
        patch::Change::BlockDispense(patch) => EventPatch::BlockDispense(BlockDispensePatch {
            item: patch.item.map(shared::item),
            velocity: patch.velocity.map(shared::vec3),
        }),
        patch::Change::BlockDispenseArmor(patch) => {
            EventPatch::BlockDispenseArmor(BlockDispenseArmorPatch {
                item: patch.item.map(shared::item),
                velocity: patch.velocity.map(shared::vec3),
            })
        }
        patch::Change::BlockDispenseLoot(patch) => {
            EventPatch::BlockDispenseLoot(BlockDispenseLootPatch {
                dispensed_loot: match patch.dispensed_loot {
                    Some(diff) => ListDiff {
                        removed: diff.removed,
                        replaced: diff
                            .replaced
                            .into_iter()
                            .map(|entry| {
                                Ok((entry.index, shared::item(shared::required(entry.value)?)))
                            })
                            .collect::<Result<Vec<_>, Malformed>>()?,
                        added: diff.added.into_iter().map(shared::item).collect(),
                    },
                    None => ListDiff::default(),
                },
            })
        }
        patch::Change::BlockDropItem(patch) => EventPatch::BlockDropItem(BlockDropItemPatch {
            items: match patch.items {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((
                                entry.index,
                                contract::dropped_item(shared::required(entry.value)?)?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff
                        .added
                        .into_iter()
                        .map(contract::dropped_item)
                        .collect::<Result<Vec<_>, _>>()?,
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::BlockExp(patch) => EventPatch::BlockExp(BlockExpPatch {
            exp_to_drop: patch.exp_to_drop,
        }),
        patch::Change::BlockExplode(patch) => EventPatch::BlockExplode(BlockExplodePatch {
            block_list: match patch.block_list {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((
                                entry.index,
                                shared::block_pos(shared::required(entry.value)?),
                            ))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added.into_iter().map(shared::block_pos).collect(),
                },
                None => ListDiff::default(),
            },
            explosion_yield: patch.explosion_yield,
        }),
        patch::Change::BlockMultiPlace(patch) => {
            EventPatch::BlockMultiPlace(BlockMultiPlacePatch { build: patch.build })
        }
        patch::Change::BlockPlace(patch) => {
            EventPatch::BlockPlace(BlockPlacePatch { build: patch.build })
        }
        patch::Change::BlockRedstone(patch) => EventPatch::BlockRedstone(BlockRedstonePatch {
            new_current: patch.new_current,
        }),
        patch::Change::BrewingStandFuel(patch) => {
            EventPatch::BrewingStandFuel(BrewingStandFuelPatch {
                consuming: patch.consuming,
                fuel_power: patch.fuel_power,
            })
        }
        patch::Change::BrewingStart(patch) => EventPatch::BrewingStart(BrewingStartPatch {
            total_brew_time: patch.total_brew_time,
        }),
        patch::Change::BroadcastMessage(patch) => {
            EventPatch::BroadcastMessage(BroadcastMessagePatch {
                message: patch.message,
                recipients: match patch.recipients {
                    Some(diff) => ListDiff {
                        removed: diff.removed,
                        replaced: diff
                            .replaced
                            .into_iter()
                            .map(|entry| {
                                Ok((
                                    entry.index,
                                    contract::command_sender(shared::required(entry.value)?)?,
                                ))
                            })
                            .collect::<Result<Vec<_>, Malformed>>()?,
                        added: diff
                            .added
                            .into_iter()
                            .map(contract::command_sender)
                            .collect::<Result<Vec<_>, _>>()?,
                    },
                    None => ListDiff::default(),
                },
            })
        }
        patch::Change::CampfireStart(patch) => EventPatch::CampfireStart(CampfireStartPatch {
            total_cook_time: patch.total_cook_time,
        }),
        patch::Change::ChunkUnload(patch) => EventPatch::ChunkUnload(ChunkUnloadPatch {
            save_chunk: patch.save_chunk,
        }),
        patch::Change::CraftItem(patch) => EventPatch::CraftItem(CraftItemPatch {
            current_item: patch
                .current_item
                .map(|wrapped| wrapped.value.map(shared::item)),
            cursor: patch.cursor.map(|wrapped| wrapped.value.map(shared::item)),
            result: patch.result.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::CrafterCraft(patch) => EventPatch::CrafterCraft(CrafterCraftPatch {
            result: patch.result.map(shared::item),
        }),
        patch::Change::CubeMobSplit(patch) => {
            EventPatch::CubeMobSplit(CubeMobSplitPatch { count: patch.count })
        }
        patch::Change::EnchantItem(patch) => EventPatch::EnchantItem(EnchantItemPatch {
            exp_level_cost: patch.exp_level_cost,
        }),
        patch::Change::EnderDragonChangePhase(patch) => {
            EventPatch::EnderDragonChangePhase(EnderDragonChangePhasePatch {
                new_phase: patch
                    .new_phase
                    .map(ender_dragon_phase_from_wire)
                    .transpose()?,
            })
        }
        patch::Change::EntityAirChange(patch) => {
            EventPatch::EntityAirChange(EntityAirChangePatch {
                amount: patch.amount,
            })
        }
        patch::Change::EntityBreed(patch) => EventPatch::EntityBreed(EntityBreedPatch {
            experience: patch.experience,
        }),
        patch::Change::EntityCombust(patch) => EventPatch::EntityCombust(EntityCombustPatch {
            duration: patch.duration,
        }),
        patch::Change::EntityCombustByBlock(patch) => {
            EventPatch::EntityCombustByBlock(EntityCombustByBlockPatch {
                duration: patch.duration,
            })
        }
        patch::Change::EntityCombustByEntity(patch) => {
            EventPatch::EntityCombustByEntity(EntityCombustByEntityPatch {
                duration: patch.duration,
            })
        }
        patch::Change::EntityDamage(patch) => EventPatch::EntityDamage(EntityDamagePatch {
            damage: patch.damage,
        }),
        patch::Change::EntityDamageByBlock(patch) => {
            EventPatch::EntityDamageByBlock(EntityDamageByBlockPatch {
                damage: patch.damage,
            })
        }
        patch::Change::EntityDamageByEntity(patch) => {
            EventPatch::EntityDamageByEntity(EntityDamageByEntityPatch {
                damage: patch.damage,
            })
        }
        patch::Change::EntityDeath(patch) => EventPatch::EntityDeath(EntityDeathPatch {
            dropped_exp: patch.dropped_exp,
            drops: match patch.drops {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((entry.index, shared::item(shared::required(entry.value)?)))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added.into_iter().map(shared::item).collect(),
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::EntityEnterLoveMode(patch) => {
            EventPatch::EntityEnterLoveMode(EntityEnterLoveModePatch {
                ticks_in_love: patch.ticks_in_love,
            })
        }
        patch::Change::EntityExhaustion(patch) => {
            EventPatch::EntityExhaustion(EntityExhaustionPatch {
                exhaustion: patch.exhaustion,
            })
        }
        patch::Change::EntityExplode(patch) => EventPatch::EntityExplode(EntityExplodePatch {
            block_list: match patch.block_list {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((
                                entry.index,
                                shared::block_pos(shared::required(entry.value)?),
                            ))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added.into_iter().map(shared::block_pos).collect(),
                },
                None => ListDiff::default(),
            },
            explosion_yield: patch.explosion_yield,
        }),
        patch::Change::EntityKnockback(patch) => {
            EventPatch::EntityKnockback(EntityKnockbackPatch {
                final_knockback: patch.final_knockback.map(shared::vec3),
            })
        }
        patch::Change::EntityKnockbackByEntity(patch) => {
            EventPatch::EntityKnockbackByEntity(EntityKnockbackByEntityPatch {
                final_knockback: patch.final_knockback.map(shared::vec3),
            })
        }
        patch::Change::EntityPortal(patch) => EventPatch::EntityPortal(EntityPortalPatch {
            can_create_portal: patch.can_create_portal,
            creation_radius: patch.creation_radius,
            search_radius: patch.search_radius,
            to: patch
                .to
                .map(|wrapped| wrapped.value.map(contract::location).transpose())
                .transpose()?,
        }),
        patch::Change::EntityPortalExit(patch) => {
            EventPatch::EntityPortalExit(EntityPortalExitPatch {
                after: patch.after.map(shared::vec3),
                to: patch
                    .to
                    .map(|wrapped| wrapped.value.map(contract::location).transpose())
                    .transpose()?,
            })
        }
        patch::Change::EntityPotionEffect(patch) => {
            EventPatch::EntityPotionEffect(EntityPotionEffectPatch {
                overrides: patch.overrides,
            })
        }
        patch::Change::EntityRegainHealth(patch) => {
            EventPatch::EntityRegainHealth(EntityRegainHealthPatch {
                amount: patch.amount,
            })
        }
        patch::Change::EntityShootBow(patch) => EventPatch::EntityShootBow(EntityShootBowPatch {
            projectile: patch.projectile.map(entity::view).transpose()?,
        }),
        patch::Change::EntityTarget(patch) => EventPatch::EntityTarget(EntityTargetPatch {
            target: patch
                .target
                .map(|wrapped| wrapped.value.map(entity::view).transpose())
                .transpose()?,
        }),
        patch::Change::EntityTargetBlock(patch) => {
            EventPatch::EntityTargetBlock(EntityTargetBlockPatch {
                target: patch
                    .target
                    .map(|wrapped| wrapped.value.map(shared::block_pos)),
            })
        }
        patch::Change::EntityTargetLivingEntity(patch) => {
            EventPatch::EntityTargetLivingEntity(EntityTargetLivingEntityPatch {
                target: patch
                    .target
                    .map(|wrapped| wrapped.value.map(entity::view).transpose())
                    .transpose()?,
            })
        }
        patch::Change::EntityTeleport(patch) => EventPatch::EntityTeleport(EntityTeleportPatch {
            to: patch
                .to
                .map(|wrapped| wrapped.value.map(contract::location).transpose())
                .transpose()?,
        }),
        patch::Change::ExpBottle(patch) => EventPatch::ExpBottle(ExpBottlePatch {
            experience: patch.experience,
            show_effect: patch.show_effect,
        }),
        patch::Change::ExplosionPrime(patch) => EventPatch::ExplosionPrime(ExplosionPrimePatch {
            fire: patch.fire,
            radius: patch.radius,
        }),
        patch::Change::FluidLevelChange(patch) => {
            EventPatch::FluidLevelChange(FluidLevelChangePatch {
                new_data: patch.new_data.map(block::state),
            })
        }
        patch::Change::FoodLevelChange(patch) => {
            EventPatch::FoodLevelChange(FoodLevelChangePatch {
                food_level: patch.food_level,
            })
        }
        patch::Change::FurnaceBurn(patch) => EventPatch::FurnaceBurn(FurnaceBurnPatch {
            burn_time: patch.burn_time,
            burning: patch.burning,
        }),
        patch::Change::FurnaceExtract(patch) => EventPatch::FurnaceExtract(FurnaceExtractPatch {
            exp_to_drop: patch.exp_to_drop,
        }),
        patch::Change::FurnaceSmelt(patch) => EventPatch::FurnaceSmelt(FurnaceSmeltPatch {
            result: patch.result.map(shared::item),
        }),
        patch::Change::FurnaceStartSmelt(patch) => {
            EventPatch::FurnaceStartSmelt(FurnaceStartSmeltPatch {
                total_cook_time: patch.total_cook_time,
            })
        }
        patch::Change::GenericGame(patch) => EventPatch::GenericGame(GenericGamePatch {
            radius: patch.radius,
        }),
        patch::Change::HopperInventorySearch(patch) => {
            EventPatch::HopperInventorySearch(HopperInventorySearchPatch {
                inventory: patch
                    .inventory
                    .map(|wrapped| wrapped.value.map(contract::inventory_ref).transpose())
                    .transpose()?,
            })
        }
        patch::Change::InventoryClick(patch) => EventPatch::InventoryClick(InventoryClickPatch {
            current_item: patch
                .current_item
                .map(|wrapped| wrapped.value.map(shared::item)),
            cursor: patch.cursor.map(|wrapped| wrapped.value.map(shared::item)),
            result: patch.result.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::InventoryCreative(patch) => {
            EventPatch::InventoryCreative(InventoryCreativePatch {
                current_item: patch
                    .current_item
                    .map(|wrapped| wrapped.value.map(shared::item)),
                cursor: patch.cursor.map(|wrapped| wrapped.value.map(shared::item)),
                result: patch.result.map(event_result_from_wire).transpose()?,
            })
        }
        patch::Change::InventoryDrag(patch) => EventPatch::InventoryDrag(InventoryDragPatch {
            cursor: patch.cursor.map(|wrapped| wrapped.value.map(shared::item)),
            result: patch.result.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::InventoryMoveItem(patch) => {
            EventPatch::InventoryMoveItem(InventoryMoveItemPatch {
                item: patch.item.map(shared::item),
            })
        }
        patch::Change::LootGenerate(patch) => EventPatch::LootGenerate(LootGeneratePatch {
            loot: match patch.loot {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((entry.index, shared::item(shared::required(entry.value)?)))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added.into_iter().map(shared::item).collect(),
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::PigZombieAnger(patch) => EventPatch::PigZombieAnger(PigZombieAngerPatch {
            new_anger: patch.new_anger,
        }),
        patch::Change::PlayerBedEnter(patch) => EventPatch::PlayerBedEnter(PlayerBedEnterPatch {
            use_bed: patch.use_bed.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::PlayerBedLeave(patch) => EventPatch::PlayerBedLeave(PlayerBedLeavePatch {
            set_spawn_location: patch.set_spawn_location,
        }),
        patch::Change::PlayerBucketEmpty(patch) => {
            EventPatch::PlayerBucketEmpty(PlayerBucketEmptyPatch {
                item_stack: patch
                    .item_stack
                    .map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::PlayerBucketFill(patch) => {
            EventPatch::PlayerBucketFill(PlayerBucketFillPatch {
                item_stack: patch
                    .item_stack
                    .map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::PlayerCommandPreprocess(patch) => {
            EventPatch::PlayerCommandPreprocess(PlayerCommandPreprocessPatch {
                message: patch.message,
            })
        }
        patch::Change::PlayerCommandSend(patch) => {
            EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
                commands: match patch.commands {
                    Some(diff) => ListDiff {
                        removed: diff.removed,
                        replaced: diff
                            .replaced
                            .into_iter()
                            .map(|entry| Ok((entry.index, entry.value)))
                            .collect::<Result<Vec<_>, Malformed>>()?,
                        added: diff.added,
                    },
                    None => ListDiff::default(),
                },
            })
        }
        patch::Change::PlayerDeath(patch) => EventPatch::PlayerDeath(PlayerDeathPatch {
            death_message: patch.death_message.map(|wrapped| wrapped.value),
            dropped_exp: patch.dropped_exp,
            drops: match patch.drops {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| {
                            Ok((entry.index, shared::item(shared::required(entry.value)?)))
                        })
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added.into_iter().map(shared::item).collect(),
                },
                None => ListDiff::default(),
            },
            keep_inventory: patch.keep_inventory,
            keep_level: patch.keep_level,
            new_exp: patch.new_exp,
            new_level: patch.new_level,
            new_total_exp: patch.new_total_exp,
        }),
        patch::Change::PlayerDropItem(patch) => EventPatch::PlayerDropItem(PlayerDropItemPatch {
            item: patch.item.map(shared::item),
        }),
        patch::Change::PlayerEditBook(patch) => EventPatch::PlayerEditBook(PlayerEditBookPatch {
            new_book_meta: patch.new_book_meta.map(contract::book_meta).transpose()?,
            signing: patch.signing,
        }),
        patch::Change::PlayerEggThrow(patch) => EventPatch::PlayerEggThrow(PlayerEggThrowPatch {
            hatching: patch.hatching,
            hatching_type: patch.hatching_type,
            num_hatches: patch.num_hatches.map(shared::byte).transpose()?,
        }),
        patch::Change::PlayerExpChange(patch) => {
            EventPatch::PlayerExpChange(PlayerExpChangePatch {
                amount: patch.amount,
            })
        }
        patch::Change::PlayerExpCooldownChange(patch) => {
            EventPatch::PlayerExpCooldownChange(PlayerExpCooldownChangePatch {
                new_cooldown: patch.new_cooldown,
            })
        }
        patch::Change::PlayerFish(patch) => EventPatch::PlayerFish(PlayerFishPatch {
            exp_to_drop: patch.exp_to_drop,
        }),
        patch::Change::PlayerHarvestBlock(patch) => {
            EventPatch::PlayerHarvestBlock(PlayerHarvestBlockPatch {
                items_harvested: match patch.items_harvested {
                    Some(diff) => ListDiff {
                        removed: diff.removed,
                        replaced: diff
                            .replaced
                            .into_iter()
                            .map(|entry| {
                                Ok((entry.index, shared::item(shared::required(entry.value)?)))
                            })
                            .collect::<Result<Vec<_>, Malformed>>()?,
                        added: diff.added.into_iter().map(shared::item).collect(),
                    },
                    None => ListDiff::default(),
                },
            })
        }
        patch::Change::PlayerInteract(patch) => EventPatch::PlayerInteract(PlayerInteractPatch {
            use_interacted_block: patch
                .use_interacted_block
                .map(event_result_from_wire)
                .transpose()?,
            use_item_in_hand: patch
                .use_item_in_hand
                .map(event_result_from_wire)
                .transpose()?,
        }),
        patch::Change::PlayerItemConsume(patch) => {
            EventPatch::PlayerItemConsume(PlayerItemConsumePatch {
                item: patch.item.map(shared::item),
            })
        }
        patch::Change::PlayerItemDamage(patch) => {
            EventPatch::PlayerItemDamage(PlayerItemDamagePatch {
                damage: patch.damage,
            })
        }
        patch::Change::PlayerItemMend(patch) => EventPatch::PlayerItemMend(PlayerItemMendPatch {
            repair_amount: patch.repair_amount,
        }),
        patch::Change::PlayerJoin(patch) => EventPatch::PlayerJoin(PlayerJoinPatch {
            join_message: patch.join_message.map(|wrapped| wrapped.value),
        }),
        patch::Change::PlayerKick(patch) => EventPatch::PlayerKick(PlayerKickPatch {
            leave_message: patch.leave_message,
            reason: patch.reason,
        }),
        patch::Change::PlayerLeave(patch) => EventPatch::PlayerLeave(PlayerLeavePatch {
            quit_message: patch.quit_message.map(|wrapped| wrapped.value),
        }),
        patch::Change::PlayerLogin(patch) => EventPatch::PlayerLogin(PlayerLoginPatch {
            kick_message: patch.kick_message,
            result: patch
                .result
                .map(player_login_result_from_wire)
                .transpose()?,
        }),
        patch::Change::PlayerMove(patch) => EventPatch::PlayerMove(PlayerMovePatch {
            to: patch.to.map(contract::location).transpose()?,
        }),
        patch::Change::PlayerPortal(patch) => EventPatch::PlayerPortal(PlayerPortalPatch {
            can_create_portal: patch.can_create_portal,
            creation_radius: patch.creation_radius,
            search_radius: patch.search_radius,
            to: patch.to.map(contract::location).transpose()?,
        }),
        patch::Change::PlayerRecipeBookClick(patch) => {
            EventPatch::PlayerRecipeBookClick(PlayerRecipeBookClickPatch {
                recipe: patch.recipe,
                shift_click: patch.shift_click,
            })
        }
        patch::Change::PlayerRespawn(patch) => EventPatch::PlayerRespawn(PlayerRespawnPatch {
            respawn_location: patch.respawn_location.map(contract::location).transpose()?,
        }),
        patch::Change::PlayerSpawnChange(patch) => {
            EventPatch::PlayerSpawnChange(PlayerSpawnChangePatch {
                forced: patch.forced,
                new_spawn: patch
                    .new_spawn
                    .map(|wrapped| wrapped.value.map(contract::location).transpose())
                    .transpose()?,
            })
        }
        patch::Change::PlayerSwapHandItems(patch) => {
            EventPatch::PlayerSwapHandItems(PlayerSwapHandItemsPatch {
                main_hand_item: patch
                    .main_hand_item
                    .map(|wrapped| wrapped.value.map(shared::item)),
                off_hand_item: patch
                    .off_hand_item
                    .map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::PlayerTeleport(patch) => EventPatch::PlayerTeleport(PlayerTeleportPatch {
            from: patch.from.map(contract::location).transpose()?,
            to: patch.to.map(contract::location).transpose()?,
        }),
        patch::Change::PlayerVelocity(patch) => EventPatch::PlayerVelocity(PlayerVelocityPatch {
            velocity: patch.velocity.map(shared::vec3),
        }),
        patch::Change::PotionSplash(patch) => EventPatch::PotionSplash(PotionSplashPatch {
            intensities: patch
                .intensities
                .into_iter()
                .map(|entry| Ok((entity::view(shared::required(entry.key)?)?, entry.value)))
                .collect::<Result<Vec<_>, Malformed>>()?,
        }),
        patch::Change::PrepareAnvil(patch) => EventPatch::PrepareAnvil(PrepareAnvilPatch {
            result: patch.result.map(|wrapped| wrapped.value.map(shared::item)),
        }),
        patch::Change::PrepareGrindstone(patch) => {
            EventPatch::PrepareGrindstone(PrepareGrindstonePatch {
                result: patch.result.map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::PrepareItemCraft(patch) => {
            EventPatch::PrepareItemCraft(PrepareItemCraftPatch {
                result: patch.result.map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::PrepareSmithing(patch) => {
            EventPatch::PrepareSmithing(PrepareSmithingPatch {
                result: patch.result.map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::RemoteServerCommand(patch) => {
            EventPatch::RemoteServerCommand(RemoteServerCommandPatch {
                command: patch.command,
            })
        }
        patch::Change::SculkBloom(patch) => EventPatch::SculkBloom(SculkBloomPatch {
            charge: patch.charge,
        }),
        patch::Change::ServerCommand(patch) => EventPatch::ServerCommand(ServerCommandPatch {
            command: patch.command,
        }),
        patch::Change::ServerListPing(patch) => EventPatch::ServerListPing(ServerListPingPatch {
            max_players: patch.max_players,
            motd: patch.motd,
            server_icon: patch.server_icon,
            hidden_players: match patch.hidden_players {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| Ok((entry.index, shared::uuid(entry.value)?)))
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff
                        .added
                        .into_iter()
                        .map(shared::uuid)
                        .collect::<Result<Vec<_>, _>>()?,
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::SheepDyeWool(patch) => EventPatch::SheepDyeWool(SheepDyeWoolPatch {
            color: patch.color.map(dye_color_from_wire).transpose()?,
        }),
        patch::Change::SignChange(patch) => EventPatch::SignChange(SignChangePatch {
            lines: match patch.lines {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| Ok((entry.index, entry.value)))
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added,
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::SmithItem(patch) => EventPatch::SmithItem(SmithItemPatch {
            current_item: patch
                .current_item
                .map(|wrapped| wrapped.value.map(shared::item)),
            cursor: patch.cursor.map(|wrapped| wrapped.value.map(shared::item)),
            result: patch.result.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::TabComplete(patch) => EventPatch::TabComplete(TabCompletePatch {
            completions: match patch.completions {
                Some(diff) => ListDiff {
                    removed: diff.removed,
                    replaced: diff
                        .replaced
                        .into_iter()
                        .map(|entry| Ok((entry.index, entry.value)))
                        .collect::<Result<Vec<_>, Malformed>>()?,
                    added: diff.added,
                },
                None => ListDiff::default(),
            },
        }),
        patch::Change::TimeSkip(patch) => EventPatch::TimeSkip(TimeSkipPatch {
            skip_amount: patch.skip_amount,
        }),
        patch::Change::TradeSelect(patch) => EventPatch::TradeSelect(TradeSelectPatch {
            result: patch.result.map(event_result_from_wire).transpose()?,
        }),
        patch::Change::VaultDisplayItem(patch) => {
            EventPatch::VaultDisplayItem(VaultDisplayItemPatch {
                display_item: patch
                    .display_item
                    .map(|wrapped| wrapped.value.map(shared::item)),
            })
        }
        patch::Change::VehicleDamage(patch) => EventPatch::VehicleDamage(VehicleDamagePatch {
            damage: patch.damage,
        }),
        patch::Change::VillagerAcquireTrade(patch) => {
            EventPatch::VillagerAcquireTrade(VillagerAcquireTradePatch {
                recipe: patch.recipe.map(contract::trade).transpose()?,
            })
        }
        patch::Change::VillagerCareerChange(patch) => {
            EventPatch::VillagerCareerChange(VillagerCareerChangePatch {
                profession: patch.profession,
            })
        }
        patch::Change::VillagerReplenishTrade(patch) => {
            EventPatch::VillagerReplenishTrade(VillagerReplenishTradePatch {
                recipe: patch.recipe.map(contract::trade).transpose()?,
            })
        }
        patch::Change::VillagerReputationChange(patch) => {
            EventPatch::VillagerReputationChange(VillagerReputationChangePatch {
                new_value: patch.new_value,
            })
        }
    })
}

const fn action_to_wire(value: Action) -> v1::Action {
    match value {
        Action::LeftClickBlock => v1::Action::LeftClickBlock,
        Action::RightClickBlock => v1::Action::RightClickBlock,
        Action::LeftClickAir => v1::Action::LeftClickAir,
        Action::RightClickAir => v1::Action::RightClickAir,
        Action::Physical => v1::Action::Physical,
    }
}

const fn async_player_pre_login_result_to_wire(
    value: AsyncPlayerPreLoginResult,
) -> v1::AsyncPlayerPreLoginResult {
    match value {
        AsyncPlayerPreLoginResult::Allowed => v1::AsyncPlayerPreLoginResult::Allowed,
        AsyncPlayerPreLoginResult::KickFull => v1::AsyncPlayerPreLoginResult::KickFull,
        AsyncPlayerPreLoginResult::KickBanned => v1::AsyncPlayerPreLoginResult::KickBanned,
        AsyncPlayerPreLoginResult::KickWhitelist => v1::AsyncPlayerPreLoginResult::KickWhitelist,
        AsyncPlayerPreLoginResult::KickOther => v1::AsyncPlayerPreLoginResult::KickOther,
    }
}

fn async_player_pre_login_result_from_wire(
    value: i32,
) -> Result<AsyncPlayerPreLoginResult, Malformed> {
    Ok(match v1::AsyncPlayerPreLoginResult::try_from(value) {
        Ok(v1::AsyncPlayerPreLoginResult::Allowed) => AsyncPlayerPreLoginResult::Allowed,
        Ok(v1::AsyncPlayerPreLoginResult::KickFull) => AsyncPlayerPreLoginResult::KickFull,
        Ok(v1::AsyncPlayerPreLoginResult::KickBanned) => AsyncPlayerPreLoginResult::KickBanned,
        Ok(v1::AsyncPlayerPreLoginResult::KickWhitelist) => {
            AsyncPlayerPreLoginResult::KickWhitelist
        }
        Ok(v1::AsyncPlayerPreLoginResult::KickOther) => AsyncPlayerPreLoginResult::KickOther,
        Ok(v1::AsyncPlayerPreLoginResult::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "async_player_pre_login_result",
                value,
            });
        }
    })
}

const fn async_structure_generate_cause_to_wire(
    value: AsyncStructureGenerateCause,
) -> v1::AsyncStructureGenerateCause {
    match value {
        AsyncStructureGenerateCause::Command => v1::AsyncStructureGenerateCause::Command,
        AsyncStructureGenerateCause::WorldGeneration => {
            v1::AsyncStructureGenerateCause::WorldGeneration
        }
        AsyncStructureGenerateCause::Custom => v1::AsyncStructureGenerateCause::Custom,
    }
}

const fn block_ignite_cause_to_wire(value: BlockIgniteCause) -> v1::BlockIgniteCause {
    match value {
        BlockIgniteCause::Lava => v1::BlockIgniteCause::Lava,
        BlockIgniteCause::FlintAndSteel => v1::BlockIgniteCause::FlintAndSteel,
        BlockIgniteCause::Spread => v1::BlockIgniteCause::Spread,
        BlockIgniteCause::Lightning => v1::BlockIgniteCause::Lightning,
        BlockIgniteCause::Fireball => v1::BlockIgniteCause::Fireball,
        BlockIgniteCause::EnderCrystal => v1::BlockIgniteCause::EnderCrystal,
        BlockIgniteCause::Explosion => v1::BlockIgniteCause::Explosion,
        BlockIgniteCause::Arrow => v1::BlockIgniteCause::Arrow,
    }
}

const fn cauldron_level_change_reason_to_wire(
    value: CauldronLevelChangeReason,
) -> v1::CauldronLevelChangeReason {
    match value {
        CauldronLevelChangeReason::BucketFill => v1::CauldronLevelChangeReason::BucketFill,
        CauldronLevelChangeReason::BucketEmpty => v1::CauldronLevelChangeReason::BucketEmpty,
        CauldronLevelChangeReason::BottleFill => v1::CauldronLevelChangeReason::BottleFill,
        CauldronLevelChangeReason::BottleEmpty => v1::CauldronLevelChangeReason::BottleEmpty,
        CauldronLevelChangeReason::BannerWash => v1::CauldronLevelChangeReason::BannerWash,
        CauldronLevelChangeReason::ArmorWash => v1::CauldronLevelChangeReason::ArmorWash,
        CauldronLevelChangeReason::ShulkerWash => v1::CauldronLevelChangeReason::ShulkerWash,
        CauldronLevelChangeReason::Extinguish => v1::CauldronLevelChangeReason::Extinguish,
        CauldronLevelChangeReason::Evaporate => v1::CauldronLevelChangeReason::Evaporate,
        CauldronLevelChangeReason::NaturalFill => v1::CauldronLevelChangeReason::NaturalFill,
        CauldronLevelChangeReason::Unknown => v1::CauldronLevelChangeReason::Unknown,
    }
}

const fn click_kind_to_wire(value: ClickKind) -> v1::ClickKind {
    match value {
        ClickKind::Left => v1::ClickKind::Left,
        ClickKind::Right => v1::ClickKind::Right,
        ClickKind::ShiftLeft => v1::ClickKind::ShiftLeft,
        ClickKind::ShiftRight => v1::ClickKind::ShiftRight,
        ClickKind::NumberKey => v1::ClickKind::NumberKey,
        ClickKind::SwapOffhand => v1::ClickKind::SwapOffhand,
        ClickKind::Drop => v1::ClickKind::Drop,
        ClickKind::ControlDrop => v1::ClickKind::ControlDrop,
        ClickKind::DoubleClick => v1::ClickKind::DoubleClick,
        ClickKind::Middle => v1::ClickKind::Middle,
        ClickKind::WindowBorderLeft => v1::ClickKind::WindowBorderLeft,
        ClickKind::WindowBorderRight => v1::ClickKind::WindowBorderRight,
        ClickKind::Creative => v1::ClickKind::Creative,
        ClickKind::Unknown => v1::ClickKind::Unknown,
    }
}

const fn creature_spawn_reason_to_wire(value: CreatureSpawnReason) -> v1::CreatureSpawnReason {
    match value {
        CreatureSpawnReason::Natural => v1::CreatureSpawnReason::Natural,
        CreatureSpawnReason::Jockey => v1::CreatureSpawnReason::Jockey,
        CreatureSpawnReason::ChunkGen => v1::CreatureSpawnReason::ChunkGen,
        CreatureSpawnReason::Spawner => v1::CreatureSpawnReason::Spawner,
        CreatureSpawnReason::TrialSpawner => v1::CreatureSpawnReason::TrialSpawner,
        CreatureSpawnReason::Egg => v1::CreatureSpawnReason::Egg,
        CreatureSpawnReason::SpawnerEgg => v1::CreatureSpawnReason::SpawnerEgg,
        CreatureSpawnReason::Bucket => v1::CreatureSpawnReason::Bucket,
        CreatureSpawnReason::Lightning => v1::CreatureSpawnReason::Lightning,
        CreatureSpawnReason::BuildSnowman => v1::CreatureSpawnReason::BuildSnowman,
        CreatureSpawnReason::BuildIrongolem => v1::CreatureSpawnReason::BuildIrongolem,
        CreatureSpawnReason::BuildCoppergolem => v1::CreatureSpawnReason::BuildCoppergolem,
        CreatureSpawnReason::BuildWither => v1::CreatureSpawnReason::BuildWither,
        CreatureSpawnReason::VillageDefense => v1::CreatureSpawnReason::VillageDefense,
        CreatureSpawnReason::VillageInvasion => v1::CreatureSpawnReason::VillageInvasion,
        CreatureSpawnReason::Breeding => v1::CreatureSpawnReason::Breeding,
        CreatureSpawnReason::SlimeSplit => v1::CreatureSpawnReason::SlimeSplit,
        CreatureSpawnReason::Reinforcements => v1::CreatureSpawnReason::Reinforcements,
        CreatureSpawnReason::NetherPortal => v1::CreatureSpawnReason::NetherPortal,
        CreatureSpawnReason::DispenseEgg => v1::CreatureSpawnReason::DispenseEgg,
        CreatureSpawnReason::Infection => v1::CreatureSpawnReason::Infection,
        CreatureSpawnReason::Cured => v1::CreatureSpawnReason::Cured,
        CreatureSpawnReason::OcelotBaby => v1::CreatureSpawnReason::OcelotBaby,
        CreatureSpawnReason::SilverfishBlock => v1::CreatureSpawnReason::SilverfishBlock,
        CreatureSpawnReason::Mount => v1::CreatureSpawnReason::Mount,
        CreatureSpawnReason::Trap => v1::CreatureSpawnReason::Trap,
        CreatureSpawnReason::EnderPearl => v1::CreatureSpawnReason::EnderPearl,
        CreatureSpawnReason::ShoulderEntity => v1::CreatureSpawnReason::ShoulderEntity,
        CreatureSpawnReason::Drowned => v1::CreatureSpawnReason::Drowned,
        CreatureSpawnReason::Sheared => v1::CreatureSpawnReason::Sheared,
        CreatureSpawnReason::Explosion => v1::CreatureSpawnReason::Explosion,
        CreatureSpawnReason::Raid => v1::CreatureSpawnReason::Raid,
        CreatureSpawnReason::Patrol => v1::CreatureSpawnReason::Patrol,
        CreatureSpawnReason::Beehive => v1::CreatureSpawnReason::Beehive,
        CreatureSpawnReason::PiglinZombified => v1::CreatureSpawnReason::PiglinZombified,
        CreatureSpawnReason::Spell => v1::CreatureSpawnReason::Spell,
        CreatureSpawnReason::Frozen => v1::CreatureSpawnReason::Frozen,
        CreatureSpawnReason::Metamorphosis => v1::CreatureSpawnReason::Metamorphosis,
        CreatureSpawnReason::Duplication => v1::CreatureSpawnReason::Duplication,
        CreatureSpawnReason::Command => v1::CreatureSpawnReason::Command,
        CreatureSpawnReason::Enchantment => v1::CreatureSpawnReason::Enchantment,
        CreatureSpawnReason::PotionEffect => v1::CreatureSpawnReason::PotionEffect,
        CreatureSpawnReason::Reanimate => v1::CreatureSpawnReason::Reanimate,
        CreatureSpawnReason::Custom => v1::CreatureSpawnReason::Custom,
        CreatureSpawnReason::Default => v1::CreatureSpawnReason::Default,
    }
}

const fn creeper_power_cause_to_wire(value: CreeperPowerCause) -> v1::CreeperPowerCause {
    match value {
        CreeperPowerCause::Lightning => v1::CreeperPowerCause::Lightning,
        CreeperPowerCause::SetOn => v1::CreeperPowerCause::SetOn,
        CreeperPowerCause::SetOff => v1::CreeperPowerCause::SetOff,
    }
}

const fn drag_type_to_wire(value: DragType) -> v1::DragType {
    match value {
        DragType::Single => v1::DragType::Single,
        DragType::Even => v1::DragType::Even,
    }
}

const fn dye_color_to_wire(value: DyeColor) -> v1::DyeColor {
    match value {
        DyeColor::White => v1::DyeColor::White,
        DyeColor::Orange => v1::DyeColor::Orange,
        DyeColor::Magenta => v1::DyeColor::Magenta,
        DyeColor::LightBlue => v1::DyeColor::LightBlue,
        DyeColor::Yellow => v1::DyeColor::Yellow,
        DyeColor::Lime => v1::DyeColor::Lime,
        DyeColor::Pink => v1::DyeColor::Pink,
        DyeColor::Gray => v1::DyeColor::Gray,
        DyeColor::LightGray => v1::DyeColor::LightGray,
        DyeColor::Cyan => v1::DyeColor::Cyan,
        DyeColor::Purple => v1::DyeColor::Purple,
        DyeColor::Blue => v1::DyeColor::Blue,
        DyeColor::Brown => v1::DyeColor::Brown,
        DyeColor::Green => v1::DyeColor::Green,
        DyeColor::Red => v1::DyeColor::Red,
        DyeColor::Black => v1::DyeColor::Black,
    }
}

fn dye_color_from_wire(value: i32) -> Result<DyeColor, Malformed> {
    Ok(match v1::DyeColor::try_from(value) {
        Ok(v1::DyeColor::White) => DyeColor::White,
        Ok(v1::DyeColor::Orange) => DyeColor::Orange,
        Ok(v1::DyeColor::Magenta) => DyeColor::Magenta,
        Ok(v1::DyeColor::LightBlue) => DyeColor::LightBlue,
        Ok(v1::DyeColor::Yellow) => DyeColor::Yellow,
        Ok(v1::DyeColor::Lime) => DyeColor::Lime,
        Ok(v1::DyeColor::Pink) => DyeColor::Pink,
        Ok(v1::DyeColor::Gray) => DyeColor::Gray,
        Ok(v1::DyeColor::LightGray) => DyeColor::LightGray,
        Ok(v1::DyeColor::Cyan) => DyeColor::Cyan,
        Ok(v1::DyeColor::Purple) => DyeColor::Purple,
        Ok(v1::DyeColor::Blue) => DyeColor::Blue,
        Ok(v1::DyeColor::Brown) => DyeColor::Brown,
        Ok(v1::DyeColor::Green) => DyeColor::Green,
        Ok(v1::DyeColor::Red) => DyeColor::Red,
        Ok(v1::DyeColor::Black) => DyeColor::Black,
        Ok(v1::DyeColor::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "dye_color",
                value,
            });
        }
    })
}

const fn ender_dragon_phase_to_wire(value: EnderDragonPhase) -> v1::EnderDragonPhase {
    match value {
        EnderDragonPhase::Circling => v1::EnderDragonPhase::Circling,
        EnderDragonPhase::Strafing => v1::EnderDragonPhase::Strafing,
        EnderDragonPhase::FlyToPortal => v1::EnderDragonPhase::FlyToPortal,
        EnderDragonPhase::LandOnPortal => v1::EnderDragonPhase::LandOnPortal,
        EnderDragonPhase::LeavePortal => v1::EnderDragonPhase::LeavePortal,
        EnderDragonPhase::BreathAttack => v1::EnderDragonPhase::BreathAttack,
        EnderDragonPhase::SearchForBreathAttackTarget => {
            v1::EnderDragonPhase::SearchForBreathAttackTarget
        }
        EnderDragonPhase::RoarBeforeAttack => v1::EnderDragonPhase::RoarBeforeAttack,
        EnderDragonPhase::ChargePlayer => v1::EnderDragonPhase::ChargePlayer,
        EnderDragonPhase::Dying => v1::EnderDragonPhase::Dying,
        EnderDragonPhase::Hover => v1::EnderDragonPhase::Hover,
    }
}

fn ender_dragon_phase_from_wire(value: i32) -> Result<EnderDragonPhase, Malformed> {
    Ok(match v1::EnderDragonPhase::try_from(value) {
        Ok(v1::EnderDragonPhase::Circling) => EnderDragonPhase::Circling,
        Ok(v1::EnderDragonPhase::Strafing) => EnderDragonPhase::Strafing,
        Ok(v1::EnderDragonPhase::FlyToPortal) => EnderDragonPhase::FlyToPortal,
        Ok(v1::EnderDragonPhase::LandOnPortal) => EnderDragonPhase::LandOnPortal,
        Ok(v1::EnderDragonPhase::LeavePortal) => EnderDragonPhase::LeavePortal,
        Ok(v1::EnderDragonPhase::BreathAttack) => EnderDragonPhase::BreathAttack,
        Ok(v1::EnderDragonPhase::SearchForBreathAttackTarget) => {
            EnderDragonPhase::SearchForBreathAttackTarget
        }
        Ok(v1::EnderDragonPhase::RoarBeforeAttack) => EnderDragonPhase::RoarBeforeAttack,
        Ok(v1::EnderDragonPhase::ChargePlayer) => EnderDragonPhase::ChargePlayer,
        Ok(v1::EnderDragonPhase::Dying) => EnderDragonPhase::Dying,
        Ok(v1::EnderDragonPhase::Hover) => EnderDragonPhase::Hover,
        Ok(v1::EnderDragonPhase::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "ender_dragon_phase",
                value,
            });
        }
    })
}

const fn entity_damage_cause_to_wire(value: EntityDamageCause) -> v1::EntityDamageCause {
    match value {
        EntityDamageCause::Kill => v1::EntityDamageCause::Kill,
        EntityDamageCause::WorldBorder => v1::EntityDamageCause::WorldBorder,
        EntityDamageCause::Contact => v1::EntityDamageCause::Contact,
        EntityDamageCause::EntityAttack => v1::EntityDamageCause::EntityAttack,
        EntityDamageCause::EntitySweepAttack => v1::EntityDamageCause::EntitySweepAttack,
        EntityDamageCause::Projectile => v1::EntityDamageCause::Projectile,
        EntityDamageCause::Suffocation => v1::EntityDamageCause::Suffocation,
        EntityDamageCause::Fall => v1::EntityDamageCause::Fall,
        EntityDamageCause::Fire => v1::EntityDamageCause::Fire,
        EntityDamageCause::FireTick => v1::EntityDamageCause::FireTick,
        EntityDamageCause::Melting => v1::EntityDamageCause::Melting,
        EntityDamageCause::Lava => v1::EntityDamageCause::Lava,
        EntityDamageCause::Drowning => v1::EntityDamageCause::Drowning,
        EntityDamageCause::BlockExplosion => v1::EntityDamageCause::BlockExplosion,
        EntityDamageCause::EntityExplosion => v1::EntityDamageCause::EntityExplosion,
        EntityDamageCause::Void => v1::EntityDamageCause::Void,
        EntityDamageCause::Lightning => v1::EntityDamageCause::Lightning,
        EntityDamageCause::Suicide => v1::EntityDamageCause::Suicide,
        EntityDamageCause::Starvation => v1::EntityDamageCause::Starvation,
        EntityDamageCause::Poison => v1::EntityDamageCause::Poison,
        EntityDamageCause::Magic => v1::EntityDamageCause::Magic,
        EntityDamageCause::Wither => v1::EntityDamageCause::Wither,
        EntityDamageCause::FallingBlock => v1::EntityDamageCause::FallingBlock,
        EntityDamageCause::Thorns => v1::EntityDamageCause::Thorns,
        EntityDamageCause::DragonBreath => v1::EntityDamageCause::DragonBreath,
        EntityDamageCause::Custom => v1::EntityDamageCause::Custom,
        EntityDamageCause::FlyIntoWall => v1::EntityDamageCause::FlyIntoWall,
        EntityDamageCause::HotFloor => v1::EntityDamageCause::HotFloor,
        EntityDamageCause::Campfire => v1::EntityDamageCause::Campfire,
        EntityDamageCause::Cramming => v1::EntityDamageCause::Cramming,
        EntityDamageCause::Dryout => v1::EntityDamageCause::Dryout,
        EntityDamageCause::Freeze => v1::EntityDamageCause::Freeze,
        EntityDamageCause::SonicBoom => v1::EntityDamageCause::SonicBoom,
    }
}

const fn entity_exhaustion_reason_to_wire(
    value: EntityExhaustionReason,
) -> v1::EntityExhaustionReason {
    match value {
        EntityExhaustionReason::BlockMined => v1::EntityExhaustionReason::BlockMined,
        EntityExhaustionReason::HungerEffect => v1::EntityExhaustionReason::HungerEffect,
        EntityExhaustionReason::Damaged => v1::EntityExhaustionReason::Damaged,
        EntityExhaustionReason::Attack => v1::EntityExhaustionReason::Attack,
        EntityExhaustionReason::JumpSprint => v1::EntityExhaustionReason::JumpSprint,
        EntityExhaustionReason::Jump => v1::EntityExhaustionReason::Jump,
        EntityExhaustionReason::Swim => v1::EntityExhaustionReason::Swim,
        EntityExhaustionReason::WalkUnderwater => v1::EntityExhaustionReason::WalkUnderwater,
        EntityExhaustionReason::WalkOnWater => v1::EntityExhaustionReason::WalkOnWater,
        EntityExhaustionReason::Sprint => v1::EntityExhaustionReason::Sprint,
        EntityExhaustionReason::Crouch => v1::EntityExhaustionReason::Crouch,
        EntityExhaustionReason::Walk => v1::EntityExhaustionReason::Walk,
        EntityExhaustionReason::Regen => v1::EntityExhaustionReason::Regen,
        EntityExhaustionReason::Unknown => v1::EntityExhaustionReason::Unknown,
    }
}

const fn entity_knockback_cause_to_wire(value: EntityKnockbackCause) -> v1::EntityKnockbackCause {
    match value {
        EntityKnockbackCause::Damage => v1::EntityKnockbackCause::Damage,
        EntityKnockbackCause::EntityAttack => v1::EntityKnockbackCause::EntityAttack,
        EntityKnockbackCause::Explosion => v1::EntityKnockbackCause::Explosion,
        EntityKnockbackCause::ShieldBlock => v1::EntityKnockbackCause::ShieldBlock,
        EntityKnockbackCause::SweepAttack => v1::EntityKnockbackCause::SweepAttack,
        EntityKnockbackCause::Unknown => v1::EntityKnockbackCause::Unknown,
    }
}

const fn entity_potion_effect_action_to_wire(
    value: EntityPotionEffectAction,
) -> v1::EntityPotionEffectAction {
    match value {
        EntityPotionEffectAction::Added => v1::EntityPotionEffectAction::Added,
        EntityPotionEffectAction::Changed => v1::EntityPotionEffectAction::Changed,
        EntityPotionEffectAction::Cleared => v1::EntityPotionEffectAction::Cleared,
        EntityPotionEffectAction::Removed => v1::EntityPotionEffectAction::Removed,
    }
}

const fn entity_potion_effect_cause_to_wire(
    value: EntityPotionEffectCause,
) -> v1::EntityPotionEffectCause {
    match value {
        EntityPotionEffectCause::AreaEffectCloud => v1::EntityPotionEffectCause::AreaEffectCloud,
        EntityPotionEffectCause::Arrow => v1::EntityPotionEffectCause::Arrow,
        EntityPotionEffectCause::Attack => v1::EntityPotionEffectCause::Attack,
        EntityPotionEffectCause::Axolotl => v1::EntityPotionEffectCause::Axolotl,
        EntityPotionEffectCause::Beacon => v1::EntityPotionEffectCause::Beacon,
        EntityPotionEffectCause::Command => v1::EntityPotionEffectCause::Command,
        EntityPotionEffectCause::Conduit => v1::EntityPotionEffectCause::Conduit,
        EntityPotionEffectCause::Conversion => v1::EntityPotionEffectCause::Conversion,
        EntityPotionEffectCause::Death => v1::EntityPotionEffectCause::Death,
        EntityPotionEffectCause::Dolphin => v1::EntityPotionEffectCause::Dolphin,
        EntityPotionEffectCause::Expiration => v1::EntityPotionEffectCause::Expiration,
        EntityPotionEffectCause::Food => v1::EntityPotionEffectCause::Food,
        EntityPotionEffectCause::Illusion => v1::EntityPotionEffectCause::Illusion,
        EntityPotionEffectCause::Milk => v1::EntityPotionEffectCause::Milk,
        EntityPotionEffectCause::PatrolCaptain => v1::EntityPotionEffectCause::PatrolCaptain,
        EntityPotionEffectCause::Plugin => v1::EntityPotionEffectCause::Plugin,
        EntityPotionEffectCause::PotionDrink => v1::EntityPotionEffectCause::PotionDrink,
        EntityPotionEffectCause::PotionSplash => v1::EntityPotionEffectCause::PotionSplash,
        EntityPotionEffectCause::SpiderSpawn => v1::EntityPotionEffectCause::SpiderSpawn,
        EntityPotionEffectCause::Totem => v1::EntityPotionEffectCause::Totem,
        EntityPotionEffectCause::TurtleHelmet => v1::EntityPotionEffectCause::TurtleHelmet,
        EntityPotionEffectCause::Unknown => v1::EntityPotionEffectCause::Unknown,
        EntityPotionEffectCause::VillagerTrade => v1::EntityPotionEffectCause::VillagerTrade,
        EntityPotionEffectCause::Warden => v1::EntityPotionEffectCause::Warden,
        EntityPotionEffectCause::WitherRose => v1::EntityPotionEffectCause::WitherRose,
    }
}

const fn entity_regain_health_regain_reason_to_wire(
    value: EntityRegainHealthRegainReason,
) -> v1::EntityRegainHealthRegainReason {
    match value {
        EntityRegainHealthRegainReason::Regen => v1::EntityRegainHealthRegainReason::Regen,
        EntityRegainHealthRegainReason::Satiated => v1::EntityRegainHealthRegainReason::Satiated,
        EntityRegainHealthRegainReason::Eating => v1::EntityRegainHealthRegainReason::Eating,
        EntityRegainHealthRegainReason::EnderCrystal => {
            v1::EntityRegainHealthRegainReason::EnderCrystal
        }
        EntityRegainHealthRegainReason::Magic => v1::EntityRegainHealthRegainReason::Magic,
        EntityRegainHealthRegainReason::MagicRegen => {
            v1::EntityRegainHealthRegainReason::MagicRegen
        }
        EntityRegainHealthRegainReason::WitherSpawn => {
            v1::EntityRegainHealthRegainReason::WitherSpawn
        }
        EntityRegainHealthRegainReason::Wither => v1::EntityRegainHealthRegainReason::Wither,
        EntityRegainHealthRegainReason::Custom => v1::EntityRegainHealthRegainReason::Custom,
    }
}

const fn entity_remove_cause_to_wire(value: EntityRemoveCause) -> v1::EntityRemoveCause {
    match value {
        EntityRemoveCause::Death => v1::EntityRemoveCause::Death,
        EntityRemoveCause::Despawn => v1::EntityRemoveCause::Despawn,
        EntityRemoveCause::Drop => v1::EntityRemoveCause::Drop,
        EntityRemoveCause::EnterBlock => v1::EntityRemoveCause::EnterBlock,
        EntityRemoveCause::Explode => v1::EntityRemoveCause::Explode,
        EntityRemoveCause::Hit => v1::EntityRemoveCause::Hit,
        EntityRemoveCause::Merge => v1::EntityRemoveCause::Merge,
        EntityRemoveCause::OutOfWorld => v1::EntityRemoveCause::OutOfWorld,
        EntityRemoveCause::Pickup => v1::EntityRemoveCause::Pickup,
        EntityRemoveCause::PlayerQuit => v1::EntityRemoveCause::PlayerQuit,
        EntityRemoveCause::Plugin => v1::EntityRemoveCause::Plugin,
        EntityRemoveCause::Transformation => v1::EntityRemoveCause::Transformation,
        EntityRemoveCause::Unload => v1::EntityRemoveCause::Unload,
    }
}

const fn entity_target_reason_to_wire(value: EntityTargetReason) -> v1::EntityTargetReason {
    match value {
        EntityTargetReason::TargetDied => v1::EntityTargetReason::TargetDied,
        EntityTargetReason::ClosestPlayer => v1::EntityTargetReason::ClosestPlayer,
        EntityTargetReason::TargetAttackedEntity => v1::EntityTargetReason::TargetAttackedEntity,
        EntityTargetReason::PigZombieTarget => v1::EntityTargetReason::PigZombieTarget,
        EntityTargetReason::ForgotTarget => v1::EntityTargetReason::ForgotTarget,
        EntityTargetReason::TargetAttackedOwner => v1::EntityTargetReason::TargetAttackedOwner,
        EntityTargetReason::OwnerAttackedTarget => v1::EntityTargetReason::OwnerAttackedTarget,
        EntityTargetReason::RandomTarget => v1::EntityTargetReason::RandomTarget,
        EntityTargetReason::DefendVillage => v1::EntityTargetReason::DefendVillage,
        EntityTargetReason::TargetAttackedNearbyEntity => {
            v1::EntityTargetReason::TargetAttackedNearbyEntity
        }
        EntityTargetReason::ReinforcementTarget => v1::EntityTargetReason::ReinforcementTarget,
        EntityTargetReason::Collision => v1::EntityTargetReason::Collision,
        EntityTargetReason::Custom => v1::EntityTargetReason::Custom,
        EntityTargetReason::ClosestEntity => v1::EntityTargetReason::ClosestEntity,
        EntityTargetReason::FollowLeader => v1::EntityTargetReason::FollowLeader,
        EntityTargetReason::Tempt => v1::EntityTargetReason::Tempt,
        EntityTargetReason::Unknown => v1::EntityTargetReason::Unknown,
    }
}

const fn entity_transform_reason_to_wire(
    value: EntityTransformReason,
) -> v1::EntityTransformReason {
    match value {
        EntityTransformReason::Cured => v1::EntityTransformReason::Cured,
        EntityTransformReason::Frozen => v1::EntityTransformReason::Frozen,
        EntityTransformReason::Infection => v1::EntityTransformReason::Infection,
        EntityTransformReason::Drowned => v1::EntityTransformReason::Drowned,
        EntityTransformReason::Sheared => v1::EntityTransformReason::Sheared,
        EntityTransformReason::Lightning => v1::EntityTransformReason::Lightning,
        EntityTransformReason::Split => v1::EntityTransformReason::Split,
        EntityTransformReason::PiglinZombified => v1::EntityTransformReason::PiglinZombified,
        EntityTransformReason::Metamorphosis => v1::EntityTransformReason::Metamorphosis,
        EntityTransformReason::Unknown => v1::EntityTransformReason::Unknown,
    }
}

const fn entity_unleash_reason_to_wire(value: EntityUnleashReason) -> v1::EntityUnleashReason {
    match value {
        EntityUnleashReason::HolderGone => v1::EntityUnleashReason::HolderGone,
        EntityUnleashReason::PlayerUnleash => v1::EntityUnleashReason::PlayerUnleash,
        EntityUnleashReason::Distance => v1::EntityUnleashReason::Distance,
        EntityUnleashReason::Shear => v1::EntityUnleashReason::Shear,
        EntityUnleashReason::Firework => v1::EntityUnleashReason::Firework,
        EntityUnleashReason::Unknown => v1::EntityUnleashReason::Unknown,
    }
}

const fn equipment_slot_to_wire(value: EquipmentSlot) -> v1::EquipmentSlot {
    match value {
        EquipmentSlot::Hand => v1::EquipmentSlot::Hand,
        EquipmentSlot::OffHand => v1::EquipmentSlot::OffHand,
        EquipmentSlot::Feet => v1::EquipmentSlot::Feet,
        EquipmentSlot::Legs => v1::EquipmentSlot::Legs,
        EquipmentSlot::Chest => v1::EquipmentSlot::Chest,
        EquipmentSlot::Head => v1::EquipmentSlot::Head,
        EquipmentSlot::Body => v1::EquipmentSlot::Body,
        EquipmentSlot::Saddle => v1::EquipmentSlot::Saddle,
    }
}

const fn event_result_to_wire(value: EventResult) -> v1::EventResult {
    match value {
        EventResult::Deny => v1::EventResult::Deny,
        EventResult::Default => v1::EventResult::Default,
        EventResult::Allow => v1::EventResult::Allow,
    }
}

fn event_result_from_wire(value: i32) -> Result<EventResult, Malformed> {
    Ok(match v1::EventResult::try_from(value) {
        Ok(v1::EventResult::Deny) => EventResult::Deny,
        Ok(v1::EventResult::Default) => EventResult::Default,
        Ok(v1::EventResult::Allow) => EventResult::Allow,
        Ok(v1::EventResult::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "event_result",
                value,
            });
        }
    })
}

const fn explosion_result_to_wire(value: ExplosionResult) -> v1::ExplosionResult {
    match value {
        ExplosionResult::Keep => v1::ExplosionResult::Keep,
        ExplosionResult::Destroy => v1::ExplosionResult::Destroy,
        ExplosionResult::DestroyWithDecay => v1::ExplosionResult::DestroyWithDecay,
        ExplosionResult::TriggerBlock => v1::ExplosionResult::TriggerBlock,
    }
}

const fn face_to_wire(value: Face) -> v1::Face {
    match value {
        Face::Down => v1::Face::Down,
        Face::Up => v1::Face::Up,
        Face::North => v1::Face::North,
        Face::South => v1::Face::South,
        Face::West => v1::Face::West,
        Face::East => v1::Face::East,
        Face::NorthEast => v1::Face::NorthEast,
        Face::NorthWest => v1::Face::NorthWest,
        Face::SouthEast => v1::Face::SouthEast,
        Face::SouthWest => v1::Face::SouthWest,
        Face::WestNorthWest => v1::Face::WestNorthWest,
        Face::NorthNorthWest => v1::Face::NorthNorthWest,
        Face::NorthNorthEast => v1::Face::NorthNorthEast,
        Face::EastNorthEast => v1::Face::EastNorthEast,
        Face::EastSouthEast => v1::Face::EastSouthEast,
        Face::SouthSouthEast => v1::Face::SouthSouthEast,
        Face::SouthSouthWest => v1::Face::SouthSouthWest,
        Face::WestSouthWest => v1::Face::WestSouthWest,
        Face::Itself => v1::Face::Self_,
    }
}

const fn hanging_break_remove_cause_to_wire(
    value: HangingBreakRemoveCause,
) -> v1::HangingBreakRemoveCause {
    match value {
        HangingBreakRemoveCause::Entity => v1::HangingBreakRemoveCause::Entity,
        HangingBreakRemoveCause::Explosion => v1::HangingBreakRemoveCause::Explosion,
        HangingBreakRemoveCause::Obstruction => v1::HangingBreakRemoveCause::Obstruction,
        HangingBreakRemoveCause::Physics => v1::HangingBreakRemoveCause::Physics,
        HangingBreakRemoveCause::Default => v1::HangingBreakRemoveCause::Default,
    }
}

const fn hopper_inventory_search_container_type_to_wire(
    value: HopperInventorySearchContainerType,
) -> v1::HopperInventorySearchContainerType {
    match value {
        HopperInventorySearchContainerType::Source => {
            v1::HopperInventorySearchContainerType::Source
        }
        HopperInventorySearchContainerType::Destination => {
            v1::HopperInventorySearchContainerType::Destination
        }
    }
}

const fn instrument_to_wire(value: Instrument) -> v1::Instrument {
    match value {
        Instrument::Piano => v1::Instrument::Piano,
        Instrument::BassDrum => v1::Instrument::BassDrum,
        Instrument::SnareDrum => v1::Instrument::SnareDrum,
        Instrument::Sticks => v1::Instrument::Sticks,
        Instrument::BassGuitar => v1::Instrument::BassGuitar,
        Instrument::Flute => v1::Instrument::Flute,
        Instrument::Bell => v1::Instrument::Bell,
        Instrument::Guitar => v1::Instrument::Guitar,
        Instrument::Chime => v1::Instrument::Chime,
        Instrument::Xylophone => v1::Instrument::Xylophone,
        Instrument::IronXylophone => v1::Instrument::IronXylophone,
        Instrument::CowBell => v1::Instrument::CowBell,
        Instrument::Didgeridoo => v1::Instrument::Didgeridoo,
        Instrument::Bit => v1::Instrument::Bit,
        Instrument::Banjo => v1::Instrument::Banjo,
        Instrument::Pling => v1::Instrument::Pling,
        Instrument::Zombie => v1::Instrument::Zombie,
        Instrument::Trumpet => v1::Instrument::Trumpet,
        Instrument::TrumpetExposed => v1::Instrument::TrumpetExposed,
        Instrument::TrumpetOxidized => v1::Instrument::TrumpetOxidized,
        Instrument::TrumpetWeathered => v1::Instrument::TrumpetWeathered,
        Instrument::Skeleton => v1::Instrument::Skeleton,
        Instrument::Creeper => v1::Instrument::Creeper,
        Instrument::Dragon => v1::Instrument::Dragon,
        Instrument::WitherSkeleton => v1::Instrument::WitherSkeleton,
        Instrument::Piglin => v1::Instrument::Piglin,
        Instrument::CustomHead => v1::Instrument::CustomHead,
    }
}

const fn inventory_action_to_wire(value: InventoryAction) -> v1::InventoryAction {
    match value {
        InventoryAction::Nothing => v1::InventoryAction::Nothing,
        InventoryAction::PickupAll => v1::InventoryAction::PickupAll,
        InventoryAction::PickupSome => v1::InventoryAction::PickupSome,
        InventoryAction::PickupHalf => v1::InventoryAction::PickupHalf,
        InventoryAction::PickupOne => v1::InventoryAction::PickupOne,
        InventoryAction::PlaceAll => v1::InventoryAction::PlaceAll,
        InventoryAction::PlaceSome => v1::InventoryAction::PlaceSome,
        InventoryAction::PlaceOne => v1::InventoryAction::PlaceOne,
        InventoryAction::SwapWithCursor => v1::InventoryAction::SwapWithCursor,
        InventoryAction::DropAllCursor => v1::InventoryAction::DropAllCursor,
        InventoryAction::DropOneCursor => v1::InventoryAction::DropOneCursor,
        InventoryAction::DropAllSlot => v1::InventoryAction::DropAllSlot,
        InventoryAction::DropOneSlot => v1::InventoryAction::DropOneSlot,
        InventoryAction::MoveToOtherInventory => v1::InventoryAction::MoveToOtherInventory,
        InventoryAction::HotbarMoveAndReadd => v1::InventoryAction::HotbarMoveAndReadd,
        InventoryAction::HotbarSwap => v1::InventoryAction::HotbarSwap,
        InventoryAction::CloneStack => v1::InventoryAction::CloneStack,
        InventoryAction::CollectToCursor => v1::InventoryAction::CollectToCursor,
        InventoryAction::Unknown => v1::InventoryAction::Unknown,
    }
}

const fn inventory_type_slot_type_to_wire(
    value: InventoryTypeSlotType,
) -> v1::InventoryTypeSlotType {
    match value {
        InventoryTypeSlotType::Result => v1::InventoryTypeSlotType::Result,
        InventoryTypeSlotType::Crafting => v1::InventoryTypeSlotType::Crafting,
        InventoryTypeSlotType::Armor => v1::InventoryTypeSlotType::Armor,
        InventoryTypeSlotType::Container => v1::InventoryTypeSlotType::Container,
        InventoryTypeSlotType::Quickbar => v1::InventoryTypeSlotType::Quickbar,
        InventoryTypeSlotType::Outside => v1::InventoryTypeSlotType::Outside,
        InventoryTypeSlotType::Fuel => v1::InventoryTypeSlotType::Fuel,
    }
}

const fn lightning_strike_cause_to_wire(value: LightningStrikeCause) -> v1::LightningStrikeCause {
    match value {
        LightningStrikeCause::Command => v1::LightningStrikeCause::Command,
        LightningStrikeCause::Custom => v1::LightningStrikeCause::Custom,
        LightningStrikeCause::Spawner => v1::LightningStrikeCause::Spawner,
        LightningStrikeCause::Trident => v1::LightningStrikeCause::Trident,
        LightningStrikeCause::Trap => v1::LightningStrikeCause::Trap,
        LightningStrikeCause::Weather => v1::LightningStrikeCause::Weather,
        LightningStrikeCause::Enchantment => v1::LightningStrikeCause::Enchantment,
        LightningStrikeCause::Unknown => v1::LightningStrikeCause::Unknown,
    }
}

const fn main_hand_to_wire(value: MainHand) -> v1::MainHand {
    match value {
        MainHand::Left => v1::MainHand::Left,
        MainHand::Right => v1::MainHand::Right,
    }
}

const fn player_animation_type_to_wire(value: PlayerAnimationType) -> v1::PlayerAnimationType {
    match value {
        PlayerAnimationType::ArmSwing => v1::PlayerAnimationType::ArmSwing,
        PlayerAnimationType::OffArmSwing => v1::PlayerAnimationType::OffArmSwing,
    }
}

const fn player_bed_enter_result_to_wire(value: PlayerBedEnterResult) -> v1::PlayerBedEnterResult {
    match value {
        PlayerBedEnterResult::Ok => v1::PlayerBedEnterResult::Ok,
        PlayerBedEnterResult::NotPossibleHere => v1::PlayerBedEnterResult::NotPossibleHere,
        PlayerBedEnterResult::NotPossibleNow => v1::PlayerBedEnterResult::NotPossibleNow,
        PlayerBedEnterResult::TooFarAway => v1::PlayerBedEnterResult::TooFarAway,
        PlayerBedEnterResult::NotSafe => v1::PlayerBedEnterResult::NotSafe,
        PlayerBedEnterResult::OtherProblem => v1::PlayerBedEnterResult::OtherProblem,
    }
}

const fn player_exp_cooldown_change_reason_to_wire(
    value: PlayerExpCooldownChangeReason,
) -> v1::PlayerExpCooldownChangeReason {
    match value {
        PlayerExpCooldownChangeReason::PickupOrb => v1::PlayerExpCooldownChangeReason::PickupOrb,
        PlayerExpCooldownChangeReason::Plugin => v1::PlayerExpCooldownChangeReason::Plugin,
    }
}

const fn player_fish_state_to_wire(value: PlayerFishState) -> v1::PlayerFishState {
    match value {
        PlayerFishState::Fishing => v1::PlayerFishState::Fishing,
        PlayerFishState::CaughtFish => v1::PlayerFishState::CaughtFish,
        PlayerFishState::CaughtEntity => v1::PlayerFishState::CaughtEntity,
        PlayerFishState::InGround => v1::PlayerFishState::InGround,
        PlayerFishState::FailedAttempt => v1::PlayerFishState::FailedAttempt,
        PlayerFishState::ReelIn => v1::PlayerFishState::ReelIn,
        PlayerFishState::Bite => v1::PlayerFishState::Bite,
    }
}

const fn player_login_result_to_wire(value: PlayerLoginResult) -> v1::PlayerLoginResult {
    match value {
        PlayerLoginResult::Allowed => v1::PlayerLoginResult::Allowed,
        PlayerLoginResult::KickFull => v1::PlayerLoginResult::KickFull,
        PlayerLoginResult::KickBanned => v1::PlayerLoginResult::KickBanned,
        PlayerLoginResult::KickWhitelist => v1::PlayerLoginResult::KickWhitelist,
        PlayerLoginResult::KickOther => v1::PlayerLoginResult::KickOther,
    }
}

fn player_login_result_from_wire(value: i32) -> Result<PlayerLoginResult, Malformed> {
    Ok(match v1::PlayerLoginResult::try_from(value) {
        Ok(v1::PlayerLoginResult::Allowed) => PlayerLoginResult::Allowed,
        Ok(v1::PlayerLoginResult::KickFull) => PlayerLoginResult::KickFull,
        Ok(v1::PlayerLoginResult::KickBanned) => PlayerLoginResult::KickBanned,
        Ok(v1::PlayerLoginResult::KickWhitelist) => PlayerLoginResult::KickWhitelist,
        Ok(v1::PlayerLoginResult::KickOther) => PlayerLoginResult::KickOther,
        Ok(v1::PlayerLoginResult::Unspecified) | Err(_) => {
            return Err(Malformed::Enum {
                field: "player_login_result",
                value,
            });
        }
    })
}

const fn player_recipe_book_settings_change_recipe_book_type_to_wire(
    value: PlayerRecipeBookSettingsChangeRecipeBookType,
) -> v1::PlayerRecipeBookSettingsChangeRecipeBookType {
    match value {
        PlayerRecipeBookSettingsChangeRecipeBookType::Crafting => {
            v1::PlayerRecipeBookSettingsChangeRecipeBookType::Crafting
        }
        PlayerRecipeBookSettingsChangeRecipeBookType::Furnace => {
            v1::PlayerRecipeBookSettingsChangeRecipeBookType::Furnace
        }
        PlayerRecipeBookSettingsChangeRecipeBookType::BlastFurnace => {
            v1::PlayerRecipeBookSettingsChangeRecipeBookType::BlastFurnace
        }
        PlayerRecipeBookSettingsChangeRecipeBookType::Smoker => {
            v1::PlayerRecipeBookSettingsChangeRecipeBookType::Smoker
        }
    }
}

const fn player_respawn_reason_to_wire(value: PlayerRespawnReason) -> v1::PlayerRespawnReason {
    match value {
        PlayerRespawnReason::Death => v1::PlayerRespawnReason::Death,
        PlayerRespawnReason::EndPortal => v1::PlayerRespawnReason::EndPortal,
        PlayerRespawnReason::Plugin => v1::PlayerRespawnReason::Plugin,
    }
}

const fn player_sign_open_cause_to_wire(value: PlayerSignOpenCause) -> v1::PlayerSignOpenCause {
    match value {
        PlayerSignOpenCause::Interact => v1::PlayerSignOpenCause::Interact,
        PlayerSignOpenCause::Place => v1::PlayerSignOpenCause::Place,
        PlayerSignOpenCause::Plugin => v1::PlayerSignOpenCause::Plugin,
        PlayerSignOpenCause::Unknown => v1::PlayerSignOpenCause::Unknown,
    }
}

const fn player_spawn_change_cause_to_wire(
    value: PlayerSpawnChangeCause,
) -> v1::PlayerSpawnChangeCause {
    match value {
        PlayerSpawnChangeCause::Command => v1::PlayerSpawnChangeCause::Command,
        PlayerSpawnChangeCause::Bed => v1::PlayerSpawnChangeCause::Bed,
        PlayerSpawnChangeCause::RespawnAnchor => v1::PlayerSpawnChangeCause::RespawnAnchor,
        PlayerSpawnChangeCause::Plugin => v1::PlayerSpawnChangeCause::Plugin,
        PlayerSpawnChangeCause::Reset => v1::PlayerSpawnChangeCause::Reset,
        PlayerSpawnChangeCause::Unknown => v1::PlayerSpawnChangeCause::Unknown,
    }
}

const fn player_teleport_cause_to_wire(value: PlayerTeleportCause) -> v1::PlayerTeleportCause {
    match value {
        PlayerTeleportCause::EnderPearl => v1::PlayerTeleportCause::EnderPearl,
        PlayerTeleportCause::Command => v1::PlayerTeleportCause::Command,
        PlayerTeleportCause::Plugin => v1::PlayerTeleportCause::Plugin,
        PlayerTeleportCause::NetherPortal => v1::PlayerTeleportCause::NetherPortal,
        PlayerTeleportCause::EndPortal => v1::PlayerTeleportCause::EndPortal,
        PlayerTeleportCause::Spectate => v1::PlayerTeleportCause::Spectate,
        PlayerTeleportCause::EndGateway => v1::PlayerTeleportCause::EndGateway,
        PlayerTeleportCause::ChorusFruit => v1::PlayerTeleportCause::ChorusFruit,
        PlayerTeleportCause::Dismount => v1::PlayerTeleportCause::Dismount,
        PlayerTeleportCause::ExitBed => v1::PlayerTeleportCause::ExitBed,
        PlayerTeleportCause::Unknown => v1::PlayerTeleportCause::Unknown,
    }
}

const fn portal_create_reason_to_wire(value: PortalCreateReason) -> v1::PortalCreateReason {
    match value {
        PortalCreateReason::Fire => v1::PortalCreateReason::Fire,
        PortalCreateReason::NetherPair => v1::PortalCreateReason::NetherPair,
        PortalCreateReason::EndPlatform => v1::PortalCreateReason::EndPlatform,
    }
}

const fn pose_to_wire(value: Pose) -> v1::Pose {
    match value {
        Pose::Standing => v1::Pose::Standing,
        Pose::FallFlying => v1::Pose::FallFlying,
        Pose::Sleeping => v1::Pose::Sleeping,
        Pose::Swimming => v1::Pose::Swimming,
        Pose::SpinAttack => v1::Pose::SpinAttack,
        Pose::Sneaking => v1::Pose::Sneaking,
        Pose::LongJumping => v1::Pose::LongJumping,
        Pose::Dying => v1::Pose::Dying,
        Pose::Croaking => v1::Pose::Croaking,
        Pose::UsingTongue => v1::Pose::UsingTongue,
        Pose::Sitting => v1::Pose::Sitting,
        Pose::Roaring => v1::Pose::Roaring,
        Pose::Sniffing => v1::Pose::Sniffing,
        Pose::Emerging => v1::Pose::Emerging,
        Pose::Digging => v1::Pose::Digging,
        Pose::Sliding => v1::Pose::Sliding,
        Pose::Shooting => v1::Pose::Shooting,
        Pose::Inhaling => v1::Pose::Inhaling,
    }
}

const fn raid_stop_reason_to_wire(value: RaidStopReason) -> v1::RaidStopReason {
    match value {
        RaidStopReason::Peace => v1::RaidStopReason::Peace,
        RaidStopReason::Timeout => v1::RaidStopReason::Timeout,
        RaidStopReason::Finished => v1::RaidStopReason::Finished,
        RaidStopReason::Unspawnable => v1::RaidStopReason::Unspawnable,
        RaidStopReason::NotInVillage => v1::RaidStopReason::NotInVillage,
        RaidStopReason::Plugin => v1::RaidStopReason::Plugin,
    }
}

const fn resource_pack_status_to_wire(value: ResourcePackStatus) -> v1::ResourcePackStatus {
    match value {
        ResourcePackStatus::SuccessfullyLoaded => v1::ResourcePackStatus::SuccessfullyLoaded,
        ResourcePackStatus::Declined => v1::ResourcePackStatus::Declined,
        ResourcePackStatus::FailedDownload => v1::ResourcePackStatus::FailedDownload,
        ResourcePackStatus::Accepted => v1::ResourcePackStatus::Accepted,
        ResourcePackStatus::Downloaded => v1::ResourcePackStatus::Downloaded,
        ResourcePackStatus::InvalidUrl => v1::ResourcePackStatus::InvalidUrl,
        ResourcePackStatus::FailedReload => v1::ResourcePackStatus::FailedReload,
        ResourcePackStatus::Discarded => v1::ResourcePackStatus::Discarded,
    }
}

const fn server_load_type_to_wire(value: ServerLoadType) -> v1::ServerLoadType {
    match value {
        ServerLoadType::Startup => v1::ServerLoadType::Startup,
        ServerLoadType::Reload => v1::ServerLoadType::Reload,
    }
}

const fn side_to_wire(value: Side) -> v1::Side {
    match value {
        Side::Front => v1::Side::Front,
        Side::Back => v1::Side::Back,
    }
}

const fn spellcaster_spell_to_wire(value: SpellcasterSpell) -> v1::SpellcasterSpell {
    match value {
        SpellcasterSpell::None => v1::SpellcasterSpell::None,
        SpellcasterSpell::SummonVex => v1::SpellcasterSpell::SummonVex,
        SpellcasterSpell::Fangs => v1::SpellcasterSpell::Fangs,
        SpellcasterSpell::Wololo => v1::SpellcasterSpell::Wololo,
        SpellcasterSpell::Disappear => v1::SpellcasterSpell::Disappear,
        SpellcasterSpell::Blindness => v1::SpellcasterSpell::Blindness,
    }
}

const fn statistic_to_wire(value: Statistic) -> v1::Statistic {
    match value {
        Statistic::DamageDealt => v1::Statistic::DamageDealt,
        Statistic::DamageTaken => v1::Statistic::DamageTaken,
        Statistic::Deaths => v1::Statistic::Deaths,
        Statistic::MobKills => v1::Statistic::MobKills,
        Statistic::PlayerKills => v1::Statistic::PlayerKills,
        Statistic::FishCaught => v1::Statistic::FishCaught,
        Statistic::AnimalsBred => v1::Statistic::AnimalsBred,
        Statistic::LeaveGame => v1::Statistic::LeaveGame,
        Statistic::Jump => v1::Statistic::Jump,
        Statistic::DropCount => v1::Statistic::DropCount,
        Statistic::Drop => v1::Statistic::Drop,
        Statistic::Pickup => v1::Statistic::Pickup,
        Statistic::PlayOneMinute => v1::Statistic::PlayOneMinute,
        Statistic::TotalWorldTime => v1::Statistic::TotalWorldTime,
        Statistic::WalkOneCm => v1::Statistic::WalkOneCm,
        Statistic::WalkOnWaterOneCm => v1::Statistic::WalkOnWaterOneCm,
        Statistic::FallOneCm => v1::Statistic::FallOneCm,
        Statistic::SneakTime => v1::Statistic::SneakTime,
        Statistic::ClimbOneCm => v1::Statistic::ClimbOneCm,
        Statistic::FlyOneCm => v1::Statistic::FlyOneCm,
        Statistic::WalkUnderWaterOneCm => v1::Statistic::WalkUnderWaterOneCm,
        Statistic::MinecartOneCm => v1::Statistic::MinecartOneCm,
        Statistic::BoatOneCm => v1::Statistic::BoatOneCm,
        Statistic::PigOneCm => v1::Statistic::PigOneCm,
        Statistic::HappyGhastOneCm => v1::Statistic::HappyGhastOneCm,
        Statistic::HorseOneCm => v1::Statistic::HorseOneCm,
        Statistic::SprintOneCm => v1::Statistic::SprintOneCm,
        Statistic::CrouchOneCm => v1::Statistic::CrouchOneCm,
        Statistic::AviateOneCm => v1::Statistic::AviateOneCm,
        Statistic::MineBlock => v1::Statistic::MineBlock,
        Statistic::UseItem => v1::Statistic::UseItem,
        Statistic::BreakItem => v1::Statistic::BreakItem,
        Statistic::CraftItem => v1::Statistic::CraftItem,
        Statistic::KillEntity => v1::Statistic::KillEntity,
        Statistic::EntityKilledBy => v1::Statistic::EntityKilledBy,
        Statistic::TimeSinceDeath => v1::Statistic::TimeSinceDeath,
        Statistic::TalkedToVillager => v1::Statistic::TalkedToVillager,
        Statistic::TradedWithVillager => v1::Statistic::TradedWithVillager,
        Statistic::CakeSlicesEaten => v1::Statistic::CakeSlicesEaten,
        Statistic::CauldronFilled => v1::Statistic::CauldronFilled,
        Statistic::CauldronUsed => v1::Statistic::CauldronUsed,
        Statistic::ArmorCleaned => v1::Statistic::ArmorCleaned,
        Statistic::BannerCleaned => v1::Statistic::BannerCleaned,
        Statistic::BrewingstandInteraction => v1::Statistic::BrewingstandInteraction,
        Statistic::BeaconInteraction => v1::Statistic::BeaconInteraction,
        Statistic::DropperInspected => v1::Statistic::DropperInspected,
        Statistic::HopperInspected => v1::Statistic::HopperInspected,
        Statistic::DispenserInspected => v1::Statistic::DispenserInspected,
        Statistic::NoteblockPlayed => v1::Statistic::NoteblockPlayed,
        Statistic::NoteblockTuned => v1::Statistic::NoteblockTuned,
        Statistic::FlowerPotted => v1::Statistic::FlowerPotted,
        Statistic::TrappedChestTriggered => v1::Statistic::TrappedChestTriggered,
        Statistic::EnderchestOpened => v1::Statistic::EnderchestOpened,
        Statistic::ItemEnchanted => v1::Statistic::ItemEnchanted,
        Statistic::RecordPlayed => v1::Statistic::RecordPlayed,
        Statistic::FurnaceInteraction => v1::Statistic::FurnaceInteraction,
        Statistic::CraftingTableInteraction => v1::Statistic::CraftingTableInteraction,
        Statistic::ChestOpened => v1::Statistic::ChestOpened,
        Statistic::SleepInBed => v1::Statistic::SleepInBed,
        Statistic::SleepInStrawBed => v1::Statistic::SleepInStrawBed,
        Statistic::ShulkerBoxOpened => v1::Statistic::ShulkerBoxOpened,
        Statistic::TimeSinceRest => v1::Statistic::TimeSinceRest,
        Statistic::SwimOneCm => v1::Statistic::SwimOneCm,
        Statistic::DamageDealtAbsorbed => v1::Statistic::DamageDealtAbsorbed,
        Statistic::DamageDealtResisted => v1::Statistic::DamageDealtResisted,
        Statistic::DamageBlockedByShield => v1::Statistic::DamageBlockedByShield,
        Statistic::DamageAbsorbed => v1::Statistic::DamageAbsorbed,
        Statistic::DamageResisted => v1::Statistic::DamageResisted,
        Statistic::CleanShulkerBox => v1::Statistic::CleanShulkerBox,
        Statistic::OpenBarrel => v1::Statistic::OpenBarrel,
        Statistic::InteractWithBlastFurnace => v1::Statistic::InteractWithBlastFurnace,
        Statistic::InteractWithSmoker => v1::Statistic::InteractWithSmoker,
        Statistic::InteractWithLectern => v1::Statistic::InteractWithLectern,
        Statistic::InteractWithCampfire => v1::Statistic::InteractWithCampfire,
        Statistic::InteractWithCartographyTable => v1::Statistic::InteractWithCartographyTable,
        Statistic::InteractWithLoom => v1::Statistic::InteractWithLoom,
        Statistic::InteractWithStonecutter => v1::Statistic::InteractWithStonecutter,
        Statistic::BellRing => v1::Statistic::BellRing,
        Statistic::RaidTrigger => v1::Statistic::RaidTrigger,
        Statistic::RaidWin => v1::Statistic::RaidWin,
        Statistic::InteractWithAnvil => v1::Statistic::InteractWithAnvil,
        Statistic::InteractWithGrindstone => v1::Statistic::InteractWithGrindstone,
        Statistic::TargetHit => v1::Statistic::TargetHit,
        Statistic::InteractWithSmithingTable => v1::Statistic::InteractWithSmithingTable,
        Statistic::StriderOneCm => v1::Statistic::StriderOneCm,
        Statistic::NautilusOneCm => v1::Statistic::NautilusOneCm,
    }
}

const fn tnt_prime_cause_to_wire(value: TNTPrimeCause) -> v1::TntPrimeCause {
    match value {
        TNTPrimeCause::Fire => v1::TntPrimeCause::Fire,
        TNTPrimeCause::Redstone => v1::TntPrimeCause::Redstone,
        TNTPrimeCause::Player => v1::TntPrimeCause::Player,
        TNTPrimeCause::Explosion => v1::TntPrimeCause::Explosion,
        TNTPrimeCause::Projectile => v1::TntPrimeCause::Projectile,
        TNTPrimeCause::BlockBreak => v1::TntPrimeCause::BlockBreak,
        TNTPrimeCause::Dispenser => v1::TntPrimeCause::Dispenser,
    }
}

const fn time_skip_reason_to_wire(value: TimeSkipReason) -> v1::TimeSkipReason {
    match value {
        TimeSkipReason::Command => v1::TimeSkipReason::Command,
        TimeSkipReason::Custom => v1::TimeSkipReason::Custom,
        TimeSkipReason::NightSkip => v1::TimeSkipReason::NightSkip,
    }
}

const fn tree_type_to_wire(value: TreeType) -> v1::TreeType {
    match value {
        TreeType::Tree => v1::TreeType::Tree,
        TreeType::BigTree => v1::TreeType::BigTree,
        TreeType::Redwood => v1::TreeType::Redwood,
        TreeType::TallRedwood => v1::TreeType::TallRedwood,
        TreeType::Birch => v1::TreeType::Birch,
        TreeType::Jungle => v1::TreeType::Jungle,
        TreeType::SmallJungle => v1::TreeType::SmallJungle,
        TreeType::CocoaTree => v1::TreeType::CocoaTree,
        TreeType::JungleBush => v1::TreeType::JungleBush,
        TreeType::RedMushroom => v1::TreeType::RedMushroom,
        TreeType::BrownMushroom => v1::TreeType::BrownMushroom,
        TreeType::Swamp => v1::TreeType::Swamp,
        TreeType::Acacia => v1::TreeType::Acacia,
        TreeType::DarkOak => v1::TreeType::DarkOak,
        TreeType::MegaRedwood => v1::TreeType::MegaRedwood,
        TreeType::MegaPine => v1::TreeType::MegaPine,
        TreeType::TallBirch => v1::TreeType::TallBirch,
        TreeType::ChorusPlant => v1::TreeType::ChorusPlant,
        TreeType::CrimsonFungus => v1::TreeType::CrimsonFungus,
        TreeType::WarpedFungus => v1::TreeType::WarpedFungus,
        TreeType::Azalea => v1::TreeType::Azalea,
        TreeType::Mangrove => v1::TreeType::Mangrove,
        TreeType::TallMangrove => v1::TreeType::TallMangrove,
        TreeType::Cherry => v1::TreeType::Cherry,
        TreeType::PaleOak => v1::TreeType::PaleOak,
        TreeType::PaleOakCreaking => v1::TreeType::PaleOakCreaking,
        TreeType::RedPoplar => v1::TreeType::RedPoplar,
        TreeType::OrangePoplar => v1::TreeType::OrangePoplar,
        TreeType::YellowPoplar => v1::TreeType::YellowPoplar,
    }
}

const fn villager_career_change_reason_to_wire(
    value: VillagerCareerChangeReason,
) -> v1::VillagerCareerChangeReason {
    match value {
        VillagerCareerChangeReason::LosingJob => v1::VillagerCareerChangeReason::LosingJob,
        VillagerCareerChangeReason::Employed => v1::VillagerCareerChangeReason::Employed,
    }
}
