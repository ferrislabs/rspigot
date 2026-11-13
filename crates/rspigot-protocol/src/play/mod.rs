pub mod chat;
pub mod chunk;
pub mod combat;
pub mod commands;
pub mod effect;
pub mod entity;
pub mod inventory;
pub mod metadata;
pub mod player;
pub mod serverbound;
pub mod slot;
pub mod types;
pub mod wire;
pub mod world;

pub use serverbound::ServerboundPlay;

pub mod clientbound {
    pub use super::chat::{
        ClearTitles, DisguisedChat, PlayerChat, SetSubtitleText, SetTitleText, SetTitlesAnimation,
        SystemChat,
    };
    pub use super::chunk::{ForgetLevelChunk, LevelChunkWithLight, LightUpdate};
    pub use super::combat::{DamageEvent, EntityEvent, HurtAnimation, PlayerCombatKill};
    pub use super::commands::{CommandSuggestions, Commands};
    pub use super::effect::{RemoveMobEffect, UpdateMobEffect};
    pub use super::entity::{
        AddEntity, Animate, EntityPositionSync, MoveEntityPos, MoveEntityPosRot, MoveEntityRot,
        RemoveEntities, RotateHead, SetEntityMotion, SetEquipment, SwingAnimation, TakeItemEntity,
        TeleportEntity, UpdateAttributes,
    };
    pub use super::inventory::{
        ContainerClose, ContainerSetContent, ContainerSetData, ContainerSetSlot, MerchantOffers,
        OpenScreen, RecipeBookSettings, SetCursorItem, SetPlayerInventory, TradeCost, TradeOffer,
        UpdateRecipes,
    };
    pub use super::metadata::SetEntityData;
    pub use super::player::{
        BundleDelimiter, ChangeDifficulty, Login, PlayerAbilities, PlayerInfoRemove,
        PlayerInfoUpdate, PlayerPosition, PlayerRotation, Respawn, ServerData,
        SetDefaultSpawnPosition, SetExperience, SetHealth, SetHeldSlot, StartConfiguration,
        TabList, TickingState, TickingStep,
    };
    pub use super::world::{
        BlockChangedAck, BlockDestruction, BlockEntityData, BlockEvent, BlockUpdate,
        ChunkBatchFinished, ChunkBatchStart, Explode, ExplosionParticle, GameEvent, GameRuleValues,
        InitializeBorder, LevelEvent, LevelParticles, ParticleOptions, ParticleRandomization,
        SectionBlocksUpdate, SetChunkCacheCenter, SetChunkCacheRadius, SetSimulationDistance,
        SetTime, Sound,
    };
}

#[cfg(test)]
mod tests {
    use super::clientbound::*;
    use crate::Packet;
    use crate::ids::play::clientbound as id;

    #[test]
    fn clientbound_ids_match_packets_report() {
        let table = [
            (<Login as Packet>::ID, 50),
            (<Respawn as Packet>::ID, 84),
            (<GameEvent as Packet>::ID, 39),
            (<PlayerPosition as Packet>::ID, 73),
            (<PlayerRotation as Packet>::ID, 74),
            (<SetChunkCacheCenter as Packet>::ID, 96),
            (<SetChunkCacheRadius as Packet>::ID, 97),
            (<SetSimulationDistance as Packet>::ID, 113),
            (<ChunkBatchStart as Packet>::ID, 12),
            (<ChunkBatchFinished as Packet>::ID, 11),
            (<LevelChunkWithLight as Packet>::ID, 46),
            (<ForgetLevelChunk as Packet>::ID, 38),
            (<LightUpdate as Packet>::ID, 49),
            (<SetDefaultSpawnPosition as Packet>::ID, 99),
            (<PlayerAbilities as Packet>::ID, 65),
            (<SetHeldSlot as Packet>::ID, 107),
            (<ChangeDifficulty as Packet>::ID, 10),
            (<SetTime as Packet>::ID, 115),
            (<SystemChat as Packet>::ID, 124),
            (<DisguisedChat as Packet>::ID, 33),
            (<PlayerChat as Packet>::ID, 66),
            (<PlayerInfoUpdate as Packet>::ID, 71),
            (<PlayerInfoRemove as Packet>::ID, 70),
            (<TabList as Packet>::ID, 125),
            (<SetTitleText as Packet>::ID, 116),
            (<SetSubtitleText as Packet>::ID, 114),
            (<SetTitlesAnimation as Packet>::ID, 117),
            (<ClearTitles as Packet>::ID, 14),
            (<AddEntity as Packet>::ID, 1),
            (<MoveEntityPos as Packet>::ID, 54),
            (<MoveEntityPosRot as Packet>::ID, 55),
            (<MoveEntityRot as Packet>::ID, 57),
            (<EntityPositionSync as Packet>::ID, 35),
            (<TeleportEntity as Packet>::ID, 128),
            (<RotateHead as Packet>::ID, 85),
            (<Animate as Packet>::ID, 2),
            (<SetEntityData as Packet>::ID, 101),
            (<SetEntityMotion as Packet>::ID, 103),
            (<SetEquipment as Packet>::ID, 104),
            (<RemoveEntities as Packet>::ID, 78),
            (<BlockUpdate as Packet>::ID, 8),
            (<BlockEvent as Packet>::ID, 7),
            (<BlockChangedAck as Packet>::ID, 4),
            (<BlockDestruction as Packet>::ID, 5),
            (<SectionBlocksUpdate as Packet>::ID, 86),
            (<BlockEntityData as Packet>::ID, 6),
            (<ContainerSetContent as Packet>::ID, 18),
            (<ContainerSetData as Packet>::ID, 19),
            (<ContainerSetSlot as Packet>::ID, 20),
            (<SetCursorItem as Packet>::ID, 98),
            (<SetPlayerInventory as Packet>::ID, 110),
            (<SetHealth as Packet>::ID, 106),
            (<SetExperience as Packet>::ID, 105),
            (<Commands as Packet>::ID, 16),
            (<CommandSuggestions as Packet>::ID, 15),
            (<TakeItemEntity as Packet>::ID, 127),
            (<StartConfiguration as Packet>::ID, 120),
            (<ServerData as Packet>::ID, 88),
            (<TickingState as Packet>::ID, 130),
            (<TickingStep as Packet>::ID, 131),
            (<InitializeBorder as Packet>::ID, 44),
            (<GameRuleValues as Packet>::ID, 40),
            (<Sound as Packet>::ID, 119),
            (<LevelEvent as Packet>::ID, 47),
            (<LevelParticles as Packet>::ID, 48),
            (<OpenScreen as Packet>::ID, 60),
            (<MerchantOffers as Packet>::ID, 53),
            (<ContainerClose as Packet>::ID, 17),
            (<UpdateAttributes as Packet>::ID, 134),
            (<UpdateRecipes as Packet>::ID, 136),
            (<RecipeBookSettings as Packet>::ID, 77),
            (<BundleDelimiter as Packet>::ID, 0),
            (<SwingAnimation as Packet>::ID, 123),
            (<DamageEvent as Packet>::ID, 25),
            (<EntityEvent as Packet>::ID, 34),
            (<HurtAnimation as Packet>::ID, 43),
            (<PlayerCombatKill as Packet>::ID, 69),
        ];
        for (actual, expected) in table {
            assert_eq!(actual, expected);
        }
        let mut ids: Vec<i32> = table.iter().map(|(actual, _)| *actual).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), table.len());
    }

    #[test]
    fn write_packet_prefixes_the_id() {
        let mut out = Vec::new();
        crate::write_packet(&SetChunkCacheRadius { radius: 8 }, &mut out);
        assert_eq!(out, vec![id::SET_CHUNK_CACHE_RADIUS as u8, 8]);
    }
}
