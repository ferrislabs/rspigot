use rspigot_domain::particle::Particle;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::SoundSource;

use crate::behavior::{MemoryModuleType, MemoryValue};
use crate::world::Hand;
use crate::{
    BlazeAction, CatAction, CrossbowAction, EndermanAction, FoxAction, GhastAction,
    IronGolemAction, PandaAction, PolarBearAction, RaiderAction, ShulkerAction, SilverfishAction,
    WolfAction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetReason {
    ClosestPlayer,
    ClosestEntity,
    TargetAttackedEntity,
    TargetAttackedNearbyEntity,
    ReinforcementTarget,
    ForgotTarget,
    Tempt,
    TargetAttackedOwner,
    OwnerAttackedTarget,
    RandomTarget,
    DefendVillage,
    Collision,
    PigZombieTarget,
    FollowLeader,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetRequest {
    pub from: Option<EntityId>,
    pub to: Option<EntityId>,
    pub reason: TargetReason,
    pub tempt: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobAction {
    Swing(Hand),
    Attack(EntityId),
    RangedAttack {
        target: EntityId,
        power: f32,
    },
    Breed(EntityId),
    EntityEvent(i8),
    EatGrass,
    Alert {
        mob: EntityId,
        target: EntityId,
    },
    Particle(Particle),
    BlockSound {
        sound: &'static str,
        source: SoundSource,
        pos: BlockPos,
        volume: f32,
        pitch: f32,
    },
    RemoveTurtleEgg(BlockPos),
    LevelEvent {
        event: i32,
        pos: BlockPos,
        data: i32,
    },
    DestroyProgress {
        pos: BlockPos,
        stage: i32,
    },
    BreakDoor(BlockPos),
    RaidCarrot(BlockPos),
    SetSitting(bool),
    TeleportTo(Vec3),
    ForgetAndRefreshAnger(EntityId),
    SetMemory {
        mob: EntityId,
        memory: MemoryModuleType,
        value: Option<MemoryValue>,
    },
    Wolf(WolfAction),
    Cat(CatAction),
    Fox(FoxAction),
    Panda(PandaAction),
    PolarBear(PolarBearAction),
    Enderman(EndermanAction),
    Silverfish(SilverfishAction),
    IronGolem(IronGolemAction),
    Blaze(BlazeAction),
    Ghast(GhastAction),
    Shulker(ShulkerAction),
    Raider(RaiderAction),
    Crossbow(CrossbowAction),
    AmbientSound,
}
