use rspigot_domain::entity::species::{Species, strider};
use rspigot_domain::entity::{Entity, EntityBase, EntityState};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::Hand;
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_VEHICLE_STEP_SQUARED: f64 = 100.0;
const RIDDEN_PITCH_SCALE: f32 = 0.5;

fn steering_item(kind: Kind) -> Option<ItemKind> {
    match kind {
        Kind::Strider => Some(ItemKind::WarpedFungusOnAStick),
        Kind::Pig => Some(ItemKind::CarrotOnAStick),
        _ => None,
    }
}

fn strider_state(entity: &mut Entity) -> Option<(&mut EntityBase, &mut strider::StriderState)> {
    let EntityState::Living(state) = &mut entity.state else {
        return None;
    };
    match state.mob.as_mut().map(|mob| &mut mob.species) {
        Some(Species::Strider(steering)) => Some((&mut entity.base, steering)),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn controlling_player(&self, vehicle: EntityId) -> Option<PlayerId> {
        let entity = self.entities.store.get(vehicle)?;
        let item = steering_item(entity.base.kind)?;
        entity.living()?.equipment(EquipmentSlot::Saddle)?;
        let first = self.entities.rides.first_passenger(vehicle)?;
        let player = self.players.values().find(|p| p.entity_id == first)?;
        let holds = [Hand::Main, Hand::Off].into_iter().any(|hand| {
            self.held(player.id, hand)
                .is_some_and(|stack| stack.item == item.id())
        });
        holds.then_some(player.id)
    }

    pub(crate) fn tick_ridden(&mut self, vehicle: EntityId) {
        let Some(player) = self.controlling_player(vehicle) else {
            return;
        };
        let Some(rotation) = self.players.get(&player).map(|p| p.rotation) else {
            return;
        };
        let Some(entity) = self.entities.store.get_mut(vehicle) else {
            return;
        };
        let Some((base, steering)) = strider_state(entity) else {
            return;
        };
        base.set_rotation(rotation.yaw, rotation.pitch * RIDDEN_PITCH_SCALE);
        base.head_yaw = base.yaw;
        steering.tick_boost();
        let yaw = base.yaw;
        if let Some(living) = entity.living_mut() {
            living.body_yaw = yaw;
        }
    }

    pub(crate) fn use_steering_item(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(rider) = self.players.get(&id).map(|player| player.entity_id) else {
            return false;
        };
        let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
            return false;
        };
        let held = self.held(id, hand).map(|stack| stack.item);
        let item = self
            .kind_of(vehicle)
            .filter(|kind| *kind == Kind::Strider)
            .and_then(steering_item);
        if item.map(ItemKind::id) != held || self.controlling_player(vehicle) != Some(id) {
            return false;
        }
        let mut random = self.mob_random(vehicle);
        let boosted = self
            .entities
            .store
            .get_mut(vehicle)
            .and_then(strider_state)
            .is_some_and(|(base, steering)| steering.boost(base, &mut random));
        self.store_mob_random(vehicle, random);
        if boosted {
            self.hurt_held(id, hand, 1);
        }
        boosted
    }

    pub(crate) fn move_vehicle(
        &mut self,
        id: PlayerId,
        position: Vec3,
        rotation: Rotation,
        on_ground: bool,
    ) {
        let Some(rider) = self.players.get(&id).map(|p| p.entity_id) else {
            return;
        };
        let vehicle = self.entities.rides.root_vehicle(rider);
        if vehicle == rider || self.controlling_player(vehicle) != Some(id) {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(vehicle) else {
            return;
        };
        let base = &mut entity.base;
        if !position.is_finite()
            || base.position.distance_squared(position) > MAX_VEHICLE_STEP_SQUARED
        {
            return;
        }
        base.old_position = base.position;
        base.velocity = position - base.position;
        base.position = position;
        base.set_rotation(rotation.yaw, rotation.pitch);
        base.on_ground = on_ground;
        base.fall_distance = 0.0;
        base.needs_sync = true;
        self.entities.store.reindex(vehicle);
        self.position_riders(vehicle);
    }
}
