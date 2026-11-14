use rspigot_ai::{CaravanFacts, LlamaAction, LlamaFacts, MobView};
use rspigot_domain::entity::species::llama::{self, LlamaState};
use rspigot_domain::entity::species::{Species, equine};
use rspigot_domain::entity::thrown::{self, ThrownState};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, ageable};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;

use crate::mob_actions::new_mob;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::spawning::LEASH;

const SPIT_SOUND: &str = "minecraft:entity.llama.spit";
const AMBIENT_SOUND: &str = "minecraft:entity.llama.ambient";
const ANGRY_SOUND: &str = "minecraft:entity.llama.angry";
const EAT_SOUND: &str = "minecraft:entity.llama.eat";
const CHEST_SOUND: &str = "minecraft:entity.llama.chest";
const BODY_YAW_RADIANS: f32 = 0.017_453_292;
const SPIT_TARGET_HEIGHT: f64 = 0.333_333_333_333_333_3;
const SPIT_DISTANCE_SCALE: f64 = 0.200_000_002_980_232_24;
const SPIT_MOUTH_OFFSET: f32 = 1.0;
const FALL_DAMAGE_THRESHOLD: f64 = 6.0;

pub(crate) fn llama_state(entity: &Entity) -> Option<&LlamaState> {
    match &entity.living()?.mob.as_ref()?.species {
        Species::Llama(state) => Some(state),
        _ => None,
    }
}

fn llama_state_mut(entity: &mut Entity) -> Option<&mut LlamaState> {
    match &mut entity.living_mut()?.mob.as_mut()?.species {
        Species::Llama(state) => Some(state),
        _ => None,
    }
}

pub(crate) fn ambient_sound() -> &'static str {
    AMBIENT_SOUND
}

pub(crate) fn angry_sound() -> &'static str {
    ANGRY_SOUND
}

pub(crate) fn eating_sound() -> &'static str {
    EAT_SOUND
}

pub(crate) fn chest_sound() -> &'static str {
    CHEST_SOUND
}

pub(crate) fn caravan_facts_of(entity: &Entity) -> CaravanFacts {
    let Some(state) = llama_state(entity) else {
        return CaravanFacts::default();
    };
    CaravanFacts {
        in_caravan: state.in_caravan(),
        head: state.caravan_head,
        has_tail: state.has_caravan_tail(),
        leashed: entity.extra.contains_key(LEASH),
    }
}

pub(crate) fn fall_hurts(distance: f64) -> bool {
    distance >= FALL_DAMAGE_THRESHOLD
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn llama_view(&self, entity: &Entity, view: &mut MobView) {
        self.equine_view(entity, view);
        let Some(state) = llama_state(entity) else {
            return;
        };
        let holder = self.leash_holder_of(entity);
        let trader = holder.filter(|holder| self.kind_of(*holder) == Some(Kind::WanderingTrader));
        let knot = holder.is_some_and(|holder| self.kind_of(holder) == Some(Kind::LeashKnot))
            || self.leashed_to_block(entity);
        view.species.llama = LlamaFacts {
            strength: llama::strength(&entity.base),
            did_spit: state.did_spit,
            caravan: caravan_facts_of(entity),
            leashed_to_knot: knot,
            leashed_to_trader: trader,
        };
    }

    pub(crate) fn llama_action(&mut self, id: EntityId, action: LlamaAction) {
        match action {
            LlamaAction::JoinCaravan { head } => self.llama_join_caravan(id, head),
            LlamaAction::LeaveCaravan => self.llama_leave_caravan(id),
            LlamaAction::ClearSpit => {
                if let Some(state) = self.entities.store.get_mut(id).and_then(llama_state_mut) {
                    state.did_spit = false;
                }
            }
        }
    }

    fn llama_join_caravan(&mut self, id: EntityId, head: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(llama_state_mut) {
            state.caravan_head = Some(head);
        }
        if let Some(state) = self.entities.store.get_mut(head).and_then(llama_state_mut) {
            state.caravan_tail = Some(id);
        }
    }

    fn llama_leave_caravan(&mut self, id: EntityId) {
        let head = self
            .entities
            .store
            .get_mut(id)
            .and_then(llama_state_mut)
            .and_then(|state| state.caravan_head.take());
        if let Some(head) = head
            && let Some(state) = self.entities.store.get_mut(head).and_then(llama_state_mut)
        {
            state.caravan_tail = None;
        }
    }

    pub(crate) fn llama_removed(&mut self, id: EntityId) {
        let (head, tail) = match self.entities.store.get(id).and_then(llama_state) {
            Some(state) => (state.caravan_head, state.caravan_tail),
            None => return,
        };
        if let Some(head) = head
            && let Some(state) = self.entities.store.get_mut(head).and_then(llama_state_mut)
        {
            state.caravan_tail = None;
        }
        if let Some(tail) = tail
            && let Some(state) = self.entities.store.get_mut(tail).and_then(llama_state_mut)
        {
            state.caravan_head = None;
        }
    }

    pub(crate) fn llama_spit_at(&mut self, shooter: EntityId, target: EntityId) {
        let Some((_, height, bounds)) = self.entity_box(target) else {
            return;
        };
        let Some(llama) = self.entities.store.get(shooter) else {
            return;
        };
        let Some(living) = llama.living() else {
            return;
        };
        let body = living.body_yaw;
        let width = llama.base.dimensions.width;
        let angle = f64::from(body * BODY_YAW_RADIANS);
        let reach = f64::from(width + SPIT_MOUTH_OFFSET) * 0.5;
        let origin = Vec3::new(
            llama.base.position.x - reach * f64::from(mth::sin(angle)),
            llama.base.eye_y() - f64::from(thrown::SPIT_OFFSET as f32),
            llama.base.position.z + reach * f64::from(mth::cos(angle)),
        );
        let owner = llama.base.uuid;
        let llama_position = llama.base.position;
        let center_x = (bounds.min_x + bounds.max_x) / 2.0;
        let center_z = (bounds.min_z + bounds.max_z) / 2.0;
        let dx = center_x - llama_position.x;
        let dy = bounds.min_y + height * SPIT_TARGET_HEIGHT - origin.y;
        let dz = center_z - llama_position.z;
        let lift = (dx * dx + dz * dz).sqrt() * SPIT_DISTANCE_SCALE;
        let mut base = self.new_entity_base(Kind::LlamaSpit);
        base.position = origin;
        base.old_position = origin;
        let seed = rand::Rng::random::<i64>(&mut self.rng);
        let mut state = ThrownState::new(&mut base, seed);
        state.owner = Some(owner);
        thrown::shoot(
            &mut base,
            &mut state,
            Vec3::new(dx, dy + lift, dz),
            thrown::SPIT_POWER,
            thrown::SPIT_UNCERTAINTY,
        );
        let spit = Entity::new(base, EntityState::Thrown(Box::new(state)));
        self.add_entity(spit, CreatureSpawnReason::Default);
        let mut random = self.mob_random(shooter);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.store_mob_random(shooter, random);
        self.entity_sound(shooter, SPIT_SOUND, 1.0, pitch);
        if let Some(state) = self
            .entities
            .store
            .get_mut(shooter)
            .and_then(llama_state_mut)
        {
            state.did_spit = true;
        }
    }

    pub(crate) fn llama_offspring(
        &mut self,
        parent: EntityId,
        partner: EntityId,
        random: &mut LegacyRandom,
    ) -> Option<Entity> {
        let parent_entity = self.entities.store.get(parent)?;
        let partner_entity = self.entities.store.get(partner)?;
        if !llama::is_llama(partner_entity.base.kind) {
            return None;
        }
        let trader = llama::is_trader(parent_entity.base.kind);
        let own_strength = llama::strength(&parent_entity.base);
        let other_strength = llama::strength(&partner_entity.base);
        let own_variant = llama::variant(&parent_entity.base);
        let other_variant = llama::variant(&partner_entity.base);
        let mut own = parent_entity.living()?.attributes.clone();
        let mut other = partner_entity.living()?.attributes.clone();
        let kind = if trader {
            Kind::TraderLlama
        } else {
            Kind::Llama
        };
        let mut child = new_mob(self.new_entity_base(kind));
        if let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = &mut child
        {
            equine::set_offspring_attributes(&mut own, &mut other, &mut living.attributes, random);
            equine::clamp_health(base, living);
            let strength = llama::offspring_strength(own_strength, other_strength, random);
            llama::set_strength(base, strength);
            let variant = llama::offspring_variant(own_variant, other_variant, random);
            llama::set_variant(base, variant);
            if trader && let Some(mob) = living.mob.as_mut() {
                mob.persistence_required = true;
            }
        }
        Some(child)
    }

    pub(crate) fn is_llama(&self, id: EntityId) -> bool {
        self.kind_of(id).is_some_and(llama::is_llama)
    }

    pub(crate) fn llama_land_hurts(&self, id: EntityId, distance: f64) -> bool {
        match self.kind_of(id) {
            Some(kind) if llama::is_llama(kind) => fall_hurts(distance),
            _ => true,
        }
    }

    pub(crate) fn trader_llama_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let tamed = equine::is_tamed(&entity.base);
        let age_locked = ageable::is_age_locked(&entity.base);
        let persistent = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .is_some_and(|mob| mob.persistence_required);
        let holder = self.leash_holder_of(entity);
        let leashed = entity.extra.contains_key(LEASH);
        let leashed_to_trader =
            holder.is_some_and(|holder| self.kind_of(holder) == Some(Kind::WanderingTrader));
        let one_player = self.has_exactly_one_player_passenger(id);
        let can_despawn =
            !(tamed || (leashed && !leashed_to_trader) || one_player || age_locked || persistent);
        if !can_despawn {
            return;
        }
        let trader_delay = holder
            .filter(|_| leashed_to_trader)
            .and_then(|holder| self.entities.store.get(holder))
            .and_then(|trader| match trader.living()?.mob.as_ref()?.species {
                Species::WanderingTrader(state) => Some(state.despawn_delay),
                _ => None,
            });
        let Some(state) = self.entities.store.get_mut(id).and_then(llama_state_mut) else {
            return;
        };
        let delay = trader_delay.unwrap_or(state.despawn_delay) - 1;
        state.despawn_delay = delay;
        if delay < 1 {
            self.drop_leash(id);
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }

    fn has_exactly_one_player_passenger(&self, id: EntityId) -> bool {
        let passengers = self.entities.rides.passengers_of(id);
        passengers.len() == 1
            && self
                .players
                .values()
                .any(|player| player.entity_id == passengers[0])
    }

    pub(crate) fn trader_llama_blocks_ride(&self, id: EntityId) -> bool {
        self.kind_of(id) == Some(Kind::TraderLlama)
            && self
                .entities
                .store
                .get(id)
                .and_then(|entity| self.leash_holder_of(entity))
                .is_some_and(|holder| self.kind_of(holder) == Some(Kind::WanderingTrader))
    }
}
