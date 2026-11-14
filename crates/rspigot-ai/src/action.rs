use rspigot_domain::particle::Particle;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::SoundSource;

use crate::world::Hand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetReason {
    ClosestPlayer,
    ClosestEntity,
    TargetAttackedEntity,
    TargetAttackedNearbyEntity,
    ReinforcementTarget,
    ForgotTarget,
    Tempt,
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
}
