use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, fangs, living};
use rspigot_entities::EntityKind as Kind;

use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::mob_life::type_in_tag;
use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ILLAGER_FRIENDS: &str = "minecraft:illager_friends";

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_fangs(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Fangs(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        let step = state.tick();
        if step.strike {
            self.fangs_strike(id);
        }
        if step.send_attack_event {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::Event(fangs::ATTACK_EVENT),
                },
            );
        }
        if step.discard {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }

    fn fangs_strike(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Fangs(state) = &entity.state else {
            return;
        };
        let owner_uuid = state.owner;
        let area = entity.base.bounding_box().inflate_by(
            fangs::HIT_INFLATE_X,
            fangs::HIT_INFLATE_Y,
            fangs::HIT_INFLATE_Z,
        );
        let owner = self.owner_entity(owner_uuid);
        let victims = self.living_in(&area);
        for victim in victims {
            self.fangs_hit(id, owner, victim);
        }
    }

    fn living_in(&self, area: &Aabb) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter(|player| {
                Aabb::of_size(player.position, 0.6, player.height() as f32).intersects(area)
            })
            .map(|player| player.entity_id)
            .collect();
        found.extend(
            self.entities
                .store
                .iter()
                .filter(|other| !other.base.is_removed())
                .filter(|other| other.base.bounding_box().intersects(area))
                .filter(|other| {
                    other
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&other.base, state))
                })
                .map(|other| other.base.id),
        );
        found
    }

    fn fangs_hit(&mut self, fangs: EntityId, owner: Option<EntityId>, victim: EntityId) {
        if !self.living_entity_alive(victim) || self.entity_invulnerable(victim) {
            return;
        }
        if owner == Some(victim) {
            return;
        }
        let Some(direct) = self.mob_source(fangs) else {
            return;
        };
        let source = match owner {
            None => DamageSource {
                kind: DamageType::Magic,
                causing: None,
                direct: Some(direct),
                position: None,
                knockback: None,
                direct_block: None,
            },
            Some(owner) => {
                if self.is_allied_with_evoker(owner, victim) {
                    return;
                }
                DamageSource {
                    kind: DamageType::IndirectMagic,
                    causing: self.source_for(owner),
                    direct: Some(direct),
                    position: None,
                    knockback: None,
                    direct_block: None,
                }
            }
        };
        self.hurt_entity(victim, &source, fangs::DAMAGE);
    }

    fn entity_invulnerable(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .is_some_and(|entity| living::is_invulnerable(&entity.base))
    }

    pub(crate) fn is_allied_with_evoker(&self, owner: EntityId, other: EntityId) -> bool {
        let Some(owner_kind) = self.kind_of(owner) else {
            return false;
        };
        let other_kind = self.kind_of(other).or_else(|| {
            self.players
                .values()
                .any(|player| player.entity_id == other)
                .then_some(Kind::Player)
        });
        let Some(other_kind) = other_kind else {
            return false;
        };
        if owner_kind != Kind::Evoker {
            return false;
        }
        if type_in_tag(other_kind, ILLAGER_FRIENDS) || other_kind.extends("AbstractIllager") {
            return true;
        }
        if other_kind != Kind::Vex {
            return false;
        }
        let vex_owner = self
            .entities
            .store
            .get(other)
            .and_then(Entity::living)
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| match mob.species {
                rspigot_domain::entity::species::Species::Vex(vex) => vex.owner,
                _ => None,
            })
            .and_then(|uuid| self.entities.store.by_uuid(uuid));
        vex_owner.is_some_and(|root| {
            root == owner
                || self
                    .kind_of(root)
                    .is_some_and(|kind| type_in_tag(kind, ILLAGER_FRIENDS))
        })
    }
}
