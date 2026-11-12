use rspigot_ai::{AiWorld, TargetReason, valid_mob_target};
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::living;
use rspigot_domain::entity::neutral::{self, NeutralState};
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{Entity, LivingState};
use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use uuid::Uuid;

use crate::events::{ClientEvent, EntityStatus};
use crate::mobs::sky_darken;
use crate::plugins::events::EntityTame;
use crate::plugins::steps::TameStep;
use crate::plugins::{Continuation, EventPayload, OwnerRef};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const TAME_SUCCESS_EVENT: i8 = 7;
pub const TAME_FAILURE_EVENT: i8 = 6;
const FORGIVE_RANGE_XZ: f64 = 32.0;
const FORGIVE_RANGE_Y: f64 = 10.0;

fn mob_living_mut(entity: &mut Entity) -> Option<&mut LivingState> {
    entity.living_mut()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn set_sitting_pose(&mut self, id: EntityId, sitting: bool) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let tame = entity
            .living()
            .and_then(|state| state.mob.as_ref())
            .is_some_and(|mob| mob.tamable.is_some());
        if tame {
            tamable::set_in_sitting_pose(&mut entity.base, sitting);
        }
    }

    pub(crate) fn set_ordered_to_sit(&mut self, id: EntityId, ordered: bool) {
        if let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(mob_living_mut)
            .and_then(|state| state.mob.as_mut())
            .and_then(|mob| mob.tamable.as_mut())
        {
            state.ordered_to_sit = ordered;
        }
    }

    pub(crate) fn mob_entity_event(&mut self, id: EntityId, event: i8) {
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(event),
            },
        );
    }

    pub(crate) fn ask_tame(&mut self, player: PlayerId, target: EntityId) {
        let step = TameStep {
            target,
            owner: player,
        };
        self.fire(player, Continuation::Tame(step), move |server| {
            let entity = server.entity_view(target)?;
            Some(EventPayload::EntityTame(EntityTame {
                entity_type: entity.kind.clone(),
                entity,
                owner: OwnerRef::Player(server.snapshot_of(player)?),
            }))
        });
    }

    pub(crate) fn finish_tame(&mut self, step: TameStep, cancelled: bool) {
        let alive = self
            .entities
            .store
            .get(step.target)
            .and_then(|entity| {
                entity
                    .living()
                    .map(|state| !living::is_dead_or_dying(&entity.base, state))
            })
            .unwrap_or(false);
        if !alive {
            return;
        }
        if self
            .entities
            .store
            .get(step.target)
            .is_some_and(|entity| entity.base.kind == rspigot_entities::EntityKind::Ocelot)
        {
            self.finish_ocelot_trust(step, cancelled);
            return;
        }
        let Some(owner) = self.players.get(&step.owner).map(|p| p.profile.id.0) else {
            return;
        };
        if cancelled {
            self.mob_entity_event(step.target, TAME_FAILURE_EVENT);
            return;
        }
        self.tame_with(step.target, owner);
        self.mob_entity_event(step.target, TAME_SUCCESS_EVENT);
    }

    pub(crate) fn tame_with(&mut self, id: EntityId, owner: Uuid) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(state) = entity
            .living_mut()
            .and_then(|state| state.mob.as_mut())
            .and_then(|mob| mob.tamable.as_mut())
        else {
            return;
        };
        state.ordered_to_sit = true;
        tamable::tame(&mut entity.base, owner);
        self.species_tamed(id);
    }

    pub(crate) fn species_tamed(&mut self, id: EntityId) {
        if self.entities.store.get(id).map(|e| e.base.kind)
            == Some(rspigot_entities::EntityKind::Wolf)
        {
            self.wolf_tamed(id);
        }
        self.cat_tamed(id);
    }

    pub(crate) fn start_persistent_anger_timer(&mut self, id: EntityId) {
        if self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| entity.base.kind == Kind::ZombifiedPiglin)
        {
            self.ask_pig_zombie_anger(id);
            return;
        }
        let game_time = self.world.level.game_time;
        let mut random = self.mob_random(id);
        if let Some(entity) = self.entities.store.get_mut(id)
            && let Some(state) = entity
                .living_mut()
                .and_then(|state| state.mob.as_mut())
                .and_then(|mob| mob.neutral.as_mut())
        {
            state.start_timer(game_time, &mut random);
            let state = *state;
            neutral::sync(&mut entity.base, &state);
        }
        self.store_mob_random(id, random);
    }

    pub(crate) fn stop_being_angry(&mut self, id: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(mob_living_mut) {
            state.damage.last_hurt_by_mob = None;
            if let Some(anger) = state.mob.as_mut().and_then(|mob| mob.neutral.as_mut()) {
                anger.target = None;
            }
        }
        self.forget_target(id, TargetReason::ForgotTarget);
        if let Some(entity) = self.entities.store.get_mut(id)
            && let Some(state) = entity
                .living_mut()
                .and_then(|state| state.mob.as_mut())
                .and_then(|mob| mob.neutral.as_mut())
        {
            state.end_time = neutral::NO_ANGER_END_TIME;
            let state = *state;
            neutral::sync(&mut entity.base, &state);
        }
    }

    pub(crate) fn forgive_dead_player(&mut self, player: PlayerId) {
        if !self.config.rules.forgive_dead_players {
            return;
        }
        let Some(found) = self.players.get(&player) else {
            return;
        };
        let uuid = found.profile.id.0;
        let origin = found.position.block();
        let area = Aabb::new(
            f64::from(origin.x) - FORGIVE_RANGE_XZ,
            f64::from(origin.y) - FORGIVE_RANGE_Y,
            f64::from(origin.z) - FORGIVE_RANGE_XZ,
            f64::from(origin.x) + 1.0 + FORGIVE_RANGE_XZ,
            f64::from(origin.y) + 1.0 + FORGIVE_RANGE_Y,
            f64::from(origin.z) + 1.0 + FORGIVE_RANGE_XZ,
        );
        let angry: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity
                    .living()
                    .and_then(|state| state.mob.as_ref())
                    .and_then(|mob| mob.neutral)
                    .is_some_and(|anger| anger.target == Some(uuid))
                    && entity.base.bounding_box().intersects(&area)
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in angry {
            self.stop_being_angry(id);
        }
    }

    pub(crate) fn forget_and_refresh_anger(&mut self, id: EntityId) {
        self.stop_being_angry(id);
        self.start_persistent_anger_timer(id);
    }

    fn anger_of(&self, id: EntityId) -> Option<NeutralState> {
        self.entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| mob.neutral)
    }

    pub(crate) fn update_persistent_anger(
        &mut self,
        id: EntityId,
        stay_angry_if_target_present: bool,
    ) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(anger) = self.anger_of(id) else {
            return;
        };
        let reference = anger.target;
        let raw_target = self.mob_target(id);
        let peaceful = self.world.difficulty == Difficulty::Peaceful;
        let sky = sky_darken(&self.world.level);
        let (valid, raw, reference_player) = {
            let world = self.mob_world(sky);
            let raw = raw_target.and_then(|target| world.living(target));
            let valid = raw.and_then(|target| valid_mob_target(&view, &world, target));
            let reference_player = reference
                .and_then(|uuid| world.resolve(uuid))
                .and_then(|found| world.living(found))
                .filter(|living| living.player);
            (valid, raw, reference_player)
        };
        let raw_matches_reference = reference.is_some() && raw.map(|l| l.facts.uuid) == reference;
        if raw.is_some_and(|target| !target.alive && !target.player) && raw_matches_reference {
            self.stop_being_angry(id);
            return;
        }
        if let Some(target) = valid {
            let changed = reference != Some(target.facts.uuid);
            if changed {
                self.set_anger_target(id, Some(target.facts.uuid));
            }
            if changed || stay_angry_if_target_present {
                self.start_persistent_anger_timer(id);
            }
        }
        let game_time = self.world.level.game_time;
        let angry = self
            .anger_of(id)
            .is_some_and(|state| state.is_angry(game_time));
        let valid_player = valid.is_some_and(|target| {
            target.player && !target.creative && !target.spectator && !peaceful
        });
        if reference.is_some()
            && !angry
            && (valid.is_none() || !valid_player || !stay_angry_if_target_present)
        {
            self.stop_being_angry(id);
        }
        if reference_player.is_some_and(|player| player.creative || player.spectator || peaceful) {
            self.stop_being_angry(id);
        }
    }

    fn set_anger_target(&mut self, id: EntityId, target: Option<Uuid>) {
        if let Some(anger) = self
            .entities
            .store
            .get_mut(id)
            .and_then(mob_living_mut)
            .and_then(|state| state.mob.as_mut())
            .and_then(|mob| mob.neutral.as_mut())
        {
            anger.target = target;
        }
    }

    pub(crate) fn teleport_mob_to(&mut self, id: EntityId, position: Vec3) {
        self.relocate(id, position, false);
    }
}
