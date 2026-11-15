use rspigot_domain::entity::Aabb;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, EntityId};

use super::controls::PLAYER_WIDTH;
use crate::plugins::events::{Action, EntityInteract, EventResult, Face, PlayerInteract};
use crate::plugins::{Continuation, EventKind, EventPatch, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Toucher {
    Player(PlayerId),
    Entity(EntityId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touched {
    Plate { weighted: bool, limit: i32 },
    Wire,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TouchStep {
    pub(crate) pos: BlockPos,
    pub(crate) old: i32,
    pub(crate) what: Touched,
    pub(crate) touchers: Vec<Toucher>,
    pub(crate) at: usize,
    pub(crate) accepted: i32,
}

impl TouchStep {
    pub(crate) fn kind(&self) -> EventKind {
        match self.touchers.get(self.at) {
            Some(Toucher::Entity(_)) => EventKind::EntityInteract,
            _ => EventKind::PlayerInteract,
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn touchers_in(&self, bounds: &Aabb, living_only: bool) -> Vec<Toucher> {
        let mut found: Vec<Toucher> = self
            .players
            .values()
            .filter(|player| {
                player.game_mode != GameMode::Spectator
                    && !player.vitals.is_dead()
                    && Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32)
                        .intersects(bounds)
            })
            .map(|player| Toucher::Player(player.id))
            .collect();
        let mut others: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && (!living_only || entity.living().is_some())
                    && entity.base.bounding_box().intersects(bounds)
            })
            .map(|entity| entity.base.id)
            .collect();
        others.sort_by_key(|id| id.0);
        found.extend(others.into_iter().map(Toucher::Entity));
        found
    }

    pub(super) fn ask_touch(&mut self, step: TouchStep) {
        let Some(toucher) = step.touchers.get(step.at).copied() else {
            return self.settle_touch(step);
        };
        let pos = step.pos;
        self.fire(
            Lane::Block(pos),
            Continuation::Touch(Box::new(step)),
            move |server| match toucher {
                Toucher::Player(id) => Some(EventPayload::PlayerInteract(PlayerInteract {
                    action: Action::Physical,
                    block: true,
                    block_face: Face::Itself,
                    block_in_hand: false,
                    clicked_block: Some(pos),
                    clicked_position: None,
                    hand: None,
                    has_item: false,
                    item: None,
                    material: "minecraft:air".to_owned(),
                    player: server.snapshot_of(id)?,
                    use_interacted_block: EventResult::Default,
                    use_item_in_hand: EventResult::Default,
                })),
                Toucher::Entity(id) => {
                    let entity = server.entity_view(id)?;
                    Some(EventPayload::EntityInteract(EntityInteract {
                        block: pos,
                        entity_type: entity.kind.clone(),
                        entity,
                    }))
                }
            },
        );
    }

    pub(crate) fn finish_touch(
        &mut self,
        mut step: TouchStep,
        cancelled: bool,
        patch: Option<EventPatch>,
    ) {
        let denied = match (step.touchers.get(step.at), patch) {
            (Some(Toucher::Player(_)), Some(EventPatch::PlayerInteract(patch))) => {
                patch.use_interacted_block == Some(EventResult::Deny)
            }
            _ => false,
        };
        if !(cancelled || denied) {
            step.accepted += 1;
        }
        step.at += 1;
        let first_is_enough = !matches!(step.what, Touched::Plate { weighted: true, .. });
        if first_is_enough && step.accepted > 0 {
            step.at = step.touchers.len();
        }
        self.ask_touch(step);
    }

    fn settle_touch(&mut self, step: TouchStep) {
        match step.what {
            Touched::Plate { weighted, limit } => {
                let new = if weighted {
                    plate_weight(step.accepted, limit)
                } else if step.accepted > 0 {
                    super::MAX_POWER
                } else {
                    0
                };
                self.plate_settled(step.pos, step.old, new);
            }
            Touched::Wire => {
                if step.accepted > 0 {
                    self.tripwire_apply(step.pos, false, true);
                }
            }
        }
    }
}

pub(super) fn plate_weight(count: i32, limit: i32) -> i32 {
    let count = count.min(limit);
    if count < 1 {
        return 0;
    }
    ((count as f32 / limit as f32) * 15.0).ceil() as i32
}
