use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, crystal};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::dragon::CrystalFacts;
use crate::explosion::{BlockInteraction, fire_state_on};
use crate::health::DamageSource;
use crate::plugins::events::{
    BlockIgnite, BlockIgniteCause, EntityRemoveCause, ExplosionPrime, ExplosionPrimePatch,
};
use crate::plugins::steps::{CrystalBlastStep, KindleStep};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn new_crystal(&mut self, position: Vec3) -> Entity {
        let mut base = self.new_entity_base(Kind::EndCrystal);
        let state = crystal::define(&mut base);
        base.snap_to(position, 0.0, 0.0);
        Entity::new(base, EntityState::Crystal(state))
    }

    pub(crate) fn tick_crystal(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Crystal(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        state.time = state.time.wrapping_add(1);
        if self.entities.fight.is_none() {
            return;
        }
        let pos = base.position.block();
        let air = self
            .world
            .chunks
            .block(pos)
            .is_some_and(rspigot_domain::BlockStateId::is_air);
        if !air {
            return;
        }
        let below = self
            .world
            .chunks
            .block(pos.offset(0, -1, 0))
            .unwrap_or_default();
        let state = fire_state_on(below);
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        let step = KindleStep {
            crystal: id,
            pos,
            state,
        };
        self.fire(lane, Continuation::Kindle(step), move |_| {
            Some(EventPayload::BlockIgnite(BlockIgnite {
                block: pos,
                cause: BlockIgniteCause::EnderCrystal,
                igniting_block: None,
                igniting_entity: Some(view),
                player: None,
            }))
        });
    }

    pub(crate) fn finish_kindle(&mut self, step: KindleStep, cancelled: bool) {
        if !cancelled
            && self.entities.store.get(step.crystal).is_some()
            && self
                .world
                .chunks
                .block(step.pos)
                .is_some_and(rspigot_domain::BlockStateId::is_air)
        {
            self.set_block(step.pos, step.state);
        }
    }

    pub(crate) fn ask_hurt_crystal(
        &self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let entity = self.entities.store.get(id)?;
        if !matches!(entity.state, EntityState::Crystal(_)) || entity.base.is_removed() {
            return None;
        }
        if entity.base.invulnerable && !source.kind.is(DamageTag::BypassesInvulnerability) {
            return None;
        }
        if source.kind.is(DamageTag::IsFire) {
            return None;
        }
        if source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.kind == Kind::EnderDragon)
        {
            return None;
        }
        Some(amount)
    }

    pub(crate) fn apply_hurt_crystal(&mut self, id: EntityId, source: &DamageSource) -> bool {
        if self
            .entities
            .store
            .get(id)
            .is_none_or(|entity| entity.base.is_removed())
        {
            return false;
        }
        let facts = self.crystal_facts(id);
        if source.kind.in_tag("minecraft:is_explosion") {
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
            self.fight_crystal_destroyed(facts, source);
            return true;
        }
        let Some(view) = self.entity_view(id) else {
            return false;
        };
        let radius = crystal::EXPLOSION_RADIUS;
        let lane = self.lane_for(None, id);
        let step = CrystalBlastStep {
            crystal: id,
            source: source.clone(),
            facts,
            radius,
        };
        self.fire(
            lane,
            Continuation::CrystalBlast(Box::new(step)),
            move |_| {
                Some(EventPayload::ExplosionPrime(ExplosionPrime {
                    entity_type: view.kind.clone(),
                    entity: view,
                    fire: false,
                    radius,
                }))
            },
        );
        true
    }

    pub(crate) fn crystal_killed(&mut self, id: EntityId) {
        if self
            .entities
            .store
            .get(id)
            .is_none_or(|entity| entity.base.kind != Kind::EndCrystal)
        {
            return;
        }
        let facts = self.crystal_facts(id);
        self.fight_crystal_destroyed(facts, &DamageSource::new(DamageType::GenericKill));
    }

    fn crystal_facts(&self, id: EntityId) -> CrystalFacts {
        let entity = self.entities.store.get(id);
        CrystalFacts {
            id,
            uuid: entity.map(|entity| entity.base.uuid),
            pos: entity.map(|entity| entity.base.position.block()),
        }
    }

    pub(crate) fn finish_crystal_blast(
        &mut self,
        step: CrystalBlastStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        let CrystalBlastStep {
            crystal: id,
            source,
            facts,
            radius,
        } = step;
        if cancelled {
            return;
        }
        let Some(position) = self
            .entities
            .store
            .get(id)
            .filter(|entity| !entity.base.is_removed())
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let radius = patch.radius.unwrap_or(radius);
        let fire = patch.fire.unwrap_or(false);
        let blast = match (&source.causing, self.mob_source(id)) {
            (Some(causing), Some(direct)) => DamageSource {
                kind: DamageType::Explosion,
                causing: Some(causing.clone()),
                direct: Some(direct),
                position: None,
                knockback: None,
                direct_block: None,
            },
            (None, Some(direct)) => DamageSource::by(DamageType::Explosion, direct),
            _ => DamageSource::new(DamageType::Explosion),
        };
        self.explode(
            Some(id),
            &blast,
            position,
            radius,
            BlockInteraction::DestroyWithDecay,
            fire,
            Some(id),
        );
        self.fight_crystal_destroyed(facts, &source);
    }
}
