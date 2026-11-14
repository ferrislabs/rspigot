use rspigot_domain::entity::BlockView;
use rspigot_domain::entity::leash::{self, Body};
use rspigot_domain::entity::living;
use rspigot_domain::entity::physics;
use rspigot_domain::entity::save::read_uuid;
use rspigot_domain::item::ItemStack;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::ports::ClientLink;
use crate::server::Server;
use crate::spawning::LEASH;

const LEAD_BREAK_SOUND: &str = "minecraft:item.lead.break";
const FALL_DISTANCE_CAP: f64 = 1.0;
const FALL_MOVEMENT_FLOOR: f64 = -0.5;

fn holder_uuid(extra: &Compound) -> Option<Uuid> {
    match extra.get(LEASH)? {
        Tag::Compound(holder) => read_uuid(holder.get("UUID")?),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn leash_holder_of(
        &self,
        entity: &rspigot_domain::entity::Entity,
    ) -> Option<EntityId> {
        let uuid = holder_uuid(&entity.extra)?;
        self.entities.store.by_uuid(uuid)
    }

    pub(crate) fn leashed_to_block(&self, entity: &rspigot_domain::entity::Entity) -> bool {
        matches!(entity.extra.get(LEASH), Some(Tag::IntArray(pos)) if pos.len() == 3)
    }

    pub(crate) fn remove_leash(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.extra.remove(LEASH);
        }
        self.entities.leash_spin.remove(&id);
    }

    pub(crate) fn drop_leash(&mut self, id: EntityId) {
        let leashed = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| entity.extra.contains_key(LEASH));
        self.remove_leash(id);
        if leashed && self.config.rules.mob_drops {
            let height = self
                .entities
                .store
                .get(id)
                .map_or(0.0, |entity| f64::from(entity.base.dimensions.height));
            self.drop_at_location(
                id,
                height,
                ItemStack::new(ItemKind::Lead.id(), std::num::NonZeroU8::MIN),
                Vec3::ZERO,
            );
        }
    }

    pub(crate) fn tick_leash(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(uuid) = holder_uuid(&entity.extra) else {
            return;
        };
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        let Some(holder) = self.entities.store.by_uuid(uuid) else {
            if entity.base.tick_count > leash::LOOSE_TICKS {
                self.drop_leash(id);
            }
            return;
        };
        let holder_alive = self.entities.store.get(holder).is_some_and(|other| {
            other
                .living()
                .is_none_or(|state| !living::is_dead_or_dying(&other.base, state))
        });
        if !alive || !holder_alive {
            self.drop_leash(id);
            return;
        }
        let (Some(entity), Some(other)) =
            (self.entities.store.get(id), self.entities.store.get(holder))
        else {
            return;
        };
        let distance = leash::center_distance(
            entity.base.bounding_box().center(),
            other.base.bounding_box().center(),
        );
        let entity_width = entity.base.dimensions.width;
        let holder_width = other.base.dimensions.width;
        if leash::snaps(distance) {
            let at = other.base.position;
            self.play_sound(None, at, LEAD_BREAK_SOUND, SoundSource::Neutral, 1.0, 1.0);
            self.drop_leash(id);
            return;
        }
        if leash::must_pull(distance, entity_width, holder_width) {
            self.pull_on_leash(id, holder);
        }
        self.spin_on_leash(id);
    }

    fn pull_on_leash(&mut self, id: EntityId, holder: EntityId) {
        let (Some(entity), Some(other)) =
            (self.entities.store.get(id), self.entities.store.get(holder))
        else {
            return;
        };
        let body = |entity: &rspigot_domain::entity::Entity| Body {
            position: entity.base.position,
            yaw: entity.base.yaw,
            width: entity.base.dimensions.width,
            height: entity.base.dimensions.height,
        };
        let holder_movement = if other
            .living()
            .is_some_and(|_| living::is_no_ai(&other.base))
        {
            Vec3::ZERO
        } else {
            other.base.velocity
        };
        let movement = entity.base.velocity;
        let Some(pull) = leash::elastic_pull(&body(entity), &body(other), movement) else {
            return;
        };
        let push = leash::impulse(&pull, holder_movement, movement);
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
            if entity.base.velocity.y > FALL_MOVEMENT_FLOOR
                && entity.base.fall_distance > FALL_DISTANCE_CAP
            {
                entity.base.fall_distance = FALL_DISTANCE_CAP;
            }
        }
        *self.entities.leash_spin.entry(id).or_insert(0.0) += leash::TORQUE_FACTOR * pull.torque;
    }

    fn spin_on_leash(&mut self, id: EntityId) {
        let spin = self.entities.leash_spin.get(&id).copied().unwrap_or(0.0);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let below = physics::movement_affecting_pos(&entity.base, &self.world.chunks);
        let friction = physics::friction(self.world.chunks.block_or_air(below));
        let friction = leash::angular_friction(
            entity.base.on_ground,
            friction,
            entity.base.fluid.in_liquid(),
        );
        if let Some(entity) = self.entities.store.get_mut(id) {
            let yaw = (f64::from(entity.base.yaw) - spin) as f32;
            entity.base.set_yaw(yaw);
        }
        self.entities
            .leash_spin
            .insert(id, spin * f64::from(friction));
    }
}
