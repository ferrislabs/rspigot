use rspigot_domain::entity::raycast::BlockHit;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};

use crate::events::Hand;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::SculkBloomPatch;
use crate::plugins::events::{BlockIgnite, BlockIgniteCause, WorldUnload};
use crate::plugins::{ActionError, ActionErrorKind, EventKind, EventPayload, Reply};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug)]
pub(crate) struct BloomStep {
    pub(crate) catalyst: BlockPos,
    pub(crate) at: Vec3,
    pub(crate) experience: i32,
    pub(crate) killer: Option<PlayerId>,
    pub(crate) victim: EntityId,
}

#[derive(Debug)]
pub(crate) enum GapStep {
    Brush {
        player: PlayerId,
        hand: Hand,
        hit: BlockHit,
    },
    Bloom(BloomStep),
    Ignite {
        pos: BlockPos,
        state: BlockStateId,
    },
    Unload {
        world: Identifier,
        files: bool,
        reply: Reply,
    },
}

impl GapStep {
    pub(crate) fn kind(&self) -> EventKind {
        match self {
            Self::Brush { .. } => EventKind::BlockBrush,
            Self::Bloom(_) => EventKind::SculkBloom,
            Self::Unload { .. } => EventKind::WorldUnload,
            Self::Ignite { .. } => EventKind::BlockIgnite,
        }
    }

    pub(crate) fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Brush { hit, .. }, Self::Brush { hit: other, .. }) => hit.pos == other.pos,
            (Self::Ignite { pos, .. }, Self::Ignite { pos: other, .. }) => pos == other,
            _ => false,
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn fire_gap(
        &mut self,
        lane: impl Into<Lane>,
        step: GapStep,
        describe: impl FnOnce(&Self) -> Option<EventPayload>,
    ) {
        self.fire(lane, Continuation::Gap(Box::new(step)), describe);
    }

    pub(crate) fn ignite_by(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        cause: BlockIgniteCause,
        igniter: Option<EntityId>,
    ) {
        self.fire_gap(
            Lane::Block(pos),
            GapStep::Ignite { pos, state },
            move |server| {
                Some(EventPayload::BlockIgnite(BlockIgnite {
                    block: pos,
                    cause,
                    igniting_block: None,
                    igniting_entity: igniter.and_then(|id| server.subject_view(id)),
                    player: None,
                }))
            },
        );
    }

    pub(crate) fn ask_world_unload(
        &mut self,
        world: Identifier,
        files: bool,
        reply: Reply,
    ) -> Result<(), ActionError> {
        self.check_world_deletable(&world)?;
        let name = world.to_string();
        self.fire_gap(
            Lane::World,
            GapStep::Unload {
                world,
                files,
                reply,
            },
            move |_| Some(EventPayload::WorldUnload(WorldUnload { world: name })),
        );
        Ok(())
    }

    pub(crate) fn finish_gap(
        &mut self,
        step: GapStep,
        cancelled: bool,
        patch: Option<crate::plugins::EventPatch>,
    ) {
        if let GapStep::Unload {
            world,
            files,
            reply,
        } = &step
        {
            let result = if cancelled {
                Err(ActionError::new(
                    ActionErrorKind::Cancelled,
                    format!("a plugin cancelled the unload of the world {world}"),
                ))
            } else {
                self.delete_world(world, *files)
            };
            self.answer_action(*reply, result);
            return;
        }
        if cancelled {
            return;
        }
        match step {
            GapStep::Brush { player, hand, hit } => self.brush_block(player, hand, hit),
            GapStep::Unload { .. } => {}
            GapStep::Ignite { pos, state } => {
                if self
                    .world
                    .chunks
                    .block(pos)
                    .is_some_and(BlockStateId::is_air)
                {
                    self.set_block(pos, state);
                }
            }
            GapStep::Bloom(step) => {
                let patch = match patch {
                    Some(crate::plugins::EventPatch::SculkBloom(patch)) => patch,
                    _ => SculkBloomPatch::default(),
                };
                let experience = patch.charge.unwrap_or(step.experience);
                self.finish_catalyst_death(BloomStep { experience, ..step });
            }
        }
    }
}
