use rspigot_domain::EntityId;
use rspigot_domain::entity::{Entity, EntityState};

use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityShootBow, EntityShootBowPatch, Face, ProjectileHit};
use crate::plugins::objects::slot_of;
use crate::plugins::steps::{Draw, Shooter, ShotStep};
use crate::plugins::{EventPayload, spec_of, view_of_entity};
use crate::ports::ClientLink;
use crate::server::Server;

fn face_of(face: rspigot_blocks::Face) -> Face {
    match face {
        rspigot_blocks::Face::Down => Face::Down,
        rspigot_blocks::Face::Up => Face::Up,
        rspigot_blocks::Face::North => Face::North,
        rspigot_blocks::Face::South => Face::South,
        rspigot_blocks::Face::West => Face::West,
        rspigot_blocks::Face::East => Face::East,
    }
}

impl<L: ClientLink> Server<L> {
    fn shooter_entity(&self, shooter: Shooter) -> Option<EntityId> {
        match shooter {
            Shooter::Player(id) => self.players.get(&id).map(|player| player.entity_id),
            Shooter::Mob(id) => Some(id),
        }
    }

    pub(crate) fn shoot_bow(&mut self, shooter: Shooter, draw: Draw, arrow: Entity) {
        let Some(entity) = self.shooter_entity(shooter) else {
            return;
        };
        let projectile = view_of_entity(&arrow);
        let Draw {
            weapon,
            ammo,
            hand,
            force,
        } = draw;
        let lane = self.lane_for(None, entity);
        let step = ShotStep {
            shooter,
            force,
            arrow,
        };
        self.fire(lane, Continuation::Shoot(Box::new(step)), move |server| {
            let entity = server.subject_view(entity)?;
            Some(EventPayload::EntityShootBow(EntityShootBow {
                bow: weapon.as_ref().map(spec_of),
                consumable: ammo.as_ref().map(spec_of),
                consume_item: true,
                entity_type: entity.kind.clone(),
                entity,
                force,
                hand: slot_of(hand),
                projectile,
            }))
        });
    }

    pub(super) fn finish_shot(
        &mut self,
        step: ShotStep,
        cancelled: bool,
        patch: EntityShootBowPatch,
    ) {
        let ShotStep {
            shooter,
            force,
            arrow,
        } = step;
        let Some(entity) = self.shooter_entity(shooter) else {
            return;
        };
        let present = match shooter {
            Shooter::Player(id) => self
                .players
                .get(&id)
                .is_some_and(|player| !player.vitals.is_dead()),
            Shooter::Mob(_) => self.entities.store.get(entity).is_some(),
        };
        if !present {
            return;
        }
        if cancelled {
            if let Shooter::Player(id) = shooter {
                self.bow_sound(id, force);
            }
            return;
        }
        let replaced = patch
            .projectile
            .is_some_and(|projectile| projectile.id != arrow.base.id);
        if !replaced {
            self.add_entity(arrow, crate::plugins::events::CreatureSpawnReason::Default);
        }
        match shooter {
            Shooter::Player(id) => self.bow_sound(id, force),
            Shooter::Mob(id) => self.skeleton_sound(id),
        }
    }

    pub(crate) fn ask_projectile_hit(&mut self, arrow: EntityId, target: EntityId) {
        let lane = self.lane_for(None, target);
        self.fire(
            lane,
            Continuation::ProjectileHit { arrow, target },
            move |server| {
                let entity = server.entity_view(arrow)?;
                Some(EventPayload::ProjectileHit(ProjectileHit {
                    entity_type: entity.kind.clone(),
                    entity,
                    hit_block: None,
                    hit_block_face: None,
                    hit_entity: Some(server.subject_view(target)?),
                }))
            },
        );
    }

    pub(crate) fn notice_block_hit(
        &mut self,
        arrow: EntityId,
        pos: rspigot_domain::BlockPos,
        face: rspigot_blocks::Face,
    ) {
        let lane = self.lane_for(None, arrow);
        self.fire(
            lane,
            Continuation::Notice(crate::plugins::EventKind::ProjectileHit),
            move |server| {
                let entity = server.entity_view(arrow)?;
                Some(EventPayload::ProjectileHit(ProjectileHit {
                    entity_type: entity.kind.clone(),
                    entity,
                    hit_block: Some(pos),
                    hit_block_face: Some(face_of(face)),
                    hit_entity: None,
                }))
            },
        );
    }

    pub(crate) fn projectile_waits(&self, projectile: EntityId) -> bool {
        self.decisions.anywhere(|then| match then {
            Continuation::ProjectileHit { arrow: waiting, .. } => *waiting == projectile,
            Continuation::Hurt(step) | Continuation::Strike(step) => match step.then {
                crate::plugins::AfterHurt::Arrow(follow) => follow.arrow == projectile,
                crate::plugins::AfterHurt::Thrown(follow) => follow.projectile == projectile,
                _ => false,
            },
            _ => false,
        })
    }

    pub(super) fn finish_projectile_hit(
        &mut self,
        arrow: EntityId,
        target: EntityId,
        cancelled: bool,
    ) {
        if cancelled {
            return;
        }
        let Some(entity) = self.entities.store.get(arrow) else {
            return;
        };
        let thrown = matches!(entity.state, EntityState::Thrown(_));
        if thrown {
            self.thrown_hit_entity(arrow, target);
        } else if self.living_entity_alive(target) {
            self.arrow_hit_entity(arrow, target);
        }
    }
}
