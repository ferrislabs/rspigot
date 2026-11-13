use rspigot_ai::RideAction;
use rspigot_domain::entity::riding;
use rspigot_domain::entity::spawn::{self, Jockey};
use rspigot_domain::entity::species::{Species, strider};
use rspigot_domain::entity::{Aabb, Entity, EntityState, Pose, RemovalReason, fluid, save};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::spawn::SpawnGroupData;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};
use rspigot_nbt::Tag;
use std::num::NonZeroU8;

use crate::brain_mobs::PASSENGERS;

use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::mob_actions::new_mob;
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    CreatureSpawnReason, EntityDismount, EntityMount, VehicleEnter, VehicleExit,
};
use crate::plugins::steps::RideStep;
use crate::plugins::{Birth, Continuation, EventKind, EventPayload, Spawned};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const PORTALS: &str = "minecraft:portals";
const FREE_POSITION_RADIUS: f32 = 4.0;
const PIGLIN_STACK_DEPTH: i32 = 3;

struct JockeyRider {
    kind: Kind,
    group: Option<SpawnGroupData>,
    age: Option<i32>,
    held: Option<ItemKind>,
}

impl JockeyRider {
    const fn of(kind: Kind) -> Self {
        Self {
            kind,
            group: None,
            age: None,
            held: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Seat {
    pub kind: Kind,
    pub baby: bool,
    pub position: Vec3,
    pub yaw: f32,
    pub scale: f64,
    pub height: f32,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn is_passenger(&self, id: EntityId) -> bool {
        self.entities.rides.is_passenger(id)
    }

    pub(crate) fn is_vehicle(&self, id: EntityId) -> bool {
        self.entities.rides.is_vehicle(id)
    }

    fn is_player_entity(&self, id: EntityId) -> bool {
        self.players.values().any(|player| player.entity_id == id)
    }

    pub(crate) fn is_living_rider(&self, id: EntityId) -> bool {
        self.is_player_entity(id)
            || self
                .entities
                .store
                .get(id)
                .is_some_and(|entity| entity.living().is_some())
    }

    pub(crate) fn seat_of(&self, id: EntityId) -> Option<Seat> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(Seat {
                kind: Kind::Player,
                baby: false,
                position: player.position,
                yaw: player.rotation.yaw,
                scale: 1.0,
                height: player.height() as f32,
            });
        }
        let entity = self.entities.store.get(id)?;
        let base = &entity.base;
        let natural = base.kind.dimensions().height;
        let scale = if natural > 0.0 {
            f64::from(base.dimensions.height / natural)
        } else {
            1.0
        };
        Some(Seat {
            kind: base.kind,
            baby: entity.is_baby(),
            position: base.position,
            yaw: base.yaw,
            scale,
            height: base.dimensions.height,
        })
    }

    pub(crate) fn is_shifting(&self, id: EntityId) -> bool {
        self.players
            .values()
            .find(|player| player.entity_id == id)
            .is_some_and(|player| player.sneaking)
    }

    pub(crate) fn can_ride(&self, rider: EntityId) -> bool {
        !self.is_shifting(rider) && self.entities.rides.boarding_cooldown(rider) <= 0
    }

    pub(crate) fn can_add_passenger(&self, vehicle: EntityId, _rider: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(vehicle) else {
            return false;
        };
        let seated = self.entities.rides.passengers_of(vehicle).len();
        match entity.base.kind {
            Kind::Strider => seated == 0 && !self.eye_in_lava(vehicle),
            kind => seated < riding::max_passengers(kind),
        }
    }

    pub(crate) fn eye_in_lava(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let eye = Vec3::new(
            entity.base.position.x,
            entity.base.eye_y(),
            entity.base.position.z,
        );
        let pos = eye.block();
        let state = self.world.chunks.block(pos).unwrap_or_default();
        fluid::lava_height(&self.world.chunks, pos)
            .is_some_and(|height| state.flags().has_fluid() && eye.y < f64::from(pos.y) + height)
    }

    pub(crate) fn start_riding(
        &mut self,
        rider: EntityId,
        vehicle: EntityId,
        force: bool,
        notify: bool,
    ) -> bool {
        if self.entities.rides.vehicle_of(rider) == Some(vehicle) || rider == vehicle {
            return false;
        }
        let Some(kind) = self.kind_of(vehicle) else {
            return false;
        };
        if !force && !kind.can_serialize() {
            return false;
        }
        if self.entities.rides.has_indirect_passenger(rider, vehicle) {
            return false;
        }
        if !force && !(self.can_ride(rider) && self.can_add_passenger(vehicle, rider)) {
            return false;
        }
        let step = RideStep {
            rider,
            vehicle,
            force,
            notify,
        };
        if riding::is_bukkit_vehicle(kind) && self.is_living_rider(rider) {
            let lane = self.lane_for(self.player_of_entity(rider), rider);
            self.fire(lane, Continuation::Boarding(step), move |server| {
                Some(EventPayload::VehicleEnter(VehicleEnter {
                    entered: server.subject_view(rider)?,
                    vehicle: server.subject_view(vehicle)?,
                }))
            });
        } else {
            self.ask_mount(step);
        }
        true
    }

    pub(crate) fn finish_boarding(&mut self, step: RideStep, cancelled: bool) {
        if !cancelled {
            self.ask_mount(step);
        }
    }

    fn ask_mount(&mut self, step: RideStep) {
        let RideStep { rider, vehicle, .. } = step;
        let lane = self.lane_for(self.player_of_entity(rider), rider);
        self.fire(lane, Continuation::Mounting(step), move |server| {
            let entity = server.subject_view(rider)?;
            Some(EventPayload::EntityMount(EntityMount {
                entity_type: entity.kind.clone(),
                entity,
                mount: server.subject_view(vehicle)?,
            }))
        });
    }

    pub(crate) fn finish_mounting(&mut self, step: RideStep, cancelled: bool) {
        let RideStep {
            rider,
            vehicle,
            force,
            notify,
        } = step;
        if cancelled
            || self.entities.store.get(vehicle).is_none()
            || (self.entities.store.get(rider).is_none() && !self.is_player_entity(rider))
            || self.entities.rides.vehicle_of(rider) == Some(vehicle)
            || self.entities.rides.has_indirect_passenger(rider, vehicle)
            || (!force && !self.can_add_passenger(vehicle, rider))
        {
            return;
        }
        if self.is_passenger(rider) {
            self.stop_riding_forced(rider);
        }
        if let Some(entity) = self.entities.store.get_mut(rider) {
            entity.base.set_pose(Pose::Standing);
        }
        let players: Vec<EntityId> = self.players.values().map(|p| p.entity_id).collect();
        self.entities
            .rides
            .attach(vehicle, rider, |id| players.contains(&id));
        self.position_rider(vehicle, rider);
        self.send_passengers(vehicle);
        if notify && let Some(at) = self.entities.store.get(vehicle).map(|e| e.base.position) {
            self.game_event(
                GameEvent::EntityMount,
                at,
                GameEventContext::of(Some(rider)),
            );
        }
    }

    pub(crate) fn ride_action(&mut self, id: EntityId, action: RideAction) {
        match action {
            RideAction::Mount(target) => {
                let target = self.mount_override(id, target);
                self.start_riding(id, target, false, true);
            }
            RideAction::Dismount => self.stop_riding(id),
        }
    }

    fn mount_override(&self, rider: EntityId, target: EntityId) -> EntityId {
        let baby_piglin = self
            .entities
            .store
            .get(rider)
            .is_some_and(|entity| entity.base.kind == Kind::Piglin && entity.is_baby());
        if baby_piglin && self.kind_of(target) == Some(Kind::Hoglin) {
            self.top_passenger(target, PIGLIN_STACK_DEPTH)
        } else {
            target
        }
    }

    fn top_passenger(&self, vehicle: EntityId, counter: i32) -> EntityId {
        match self.entities.rides.first_passenger(vehicle) {
            Some(passenger) if counter != 1 => self.top_passenger(passenger, counter - 1),
            _ => vehicle,
        }
    }

    pub(crate) fn place_passengers(&mut self, vehicle: EntityId) {
        let Some(Tag::List(riders)) = self
            .entities
            .store
            .get_mut(vehicle)
            .and_then(|entity| entity.extra.remove(PASSENGERS))
        else {
            return;
        };
        for tag in riders {
            let Tag::Compound(nbt) = tag else {
                continue;
            };
            let id = self.allocate_entity_id();
            let uuid = self.random_uuid();
            let rider = match save::load_entity(&nbt, id, uuid) {
                Ok(rider) => rider,
                Err(error) => {
                    tracing::warn!(%error, "Skipping passenger that failed to load");
                    continue;
                }
            };
            if let Ok(rider) = self.place_entity(rider) {
                self.attach_loaded(vehicle, rider);
            }
        }
    }

    fn attach_loaded(&mut self, vehicle: EntityId, rider: EntityId) {
        self.entities.rides.attach(vehicle, rider, |_| false);
        self.position_rider(vehicle, rider);
        self.send_passengers(vehicle);
    }

    pub(crate) fn stop_riding(&mut self, rider: EntityId) {
        let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
            return;
        };
        let step = RideStep {
            rider,
            vehicle,
            force: false,
            notify: true,
        };
        let kind = self.kind_of(vehicle);
        if kind.is_some_and(riding::is_bukkit_vehicle) && self.is_living_rider(rider) {
            let lane = self.lane_for(self.player_of_entity(rider), rider);
            self.fire(lane, Continuation::Exiting(step), move |server| {
                Some(EventPayload::VehicleExit(VehicleExit {
                    exited: server.subject_view(rider)?,
                    vehicle: server.subject_view(vehicle)?,
                }))
            });
        } else {
            self.ask_dismount(step);
        }
    }

    pub(crate) fn finish_exiting(&mut self, step: RideStep, cancelled: bool) {
        if !cancelled {
            self.ask_dismount(step);
        }
    }

    fn ask_dismount(&mut self, step: RideStep) {
        let RideStep { rider, vehicle, .. } = step;
        let lane = self.lane_for(self.player_of_entity(rider), rider);
        self.fire(lane, Continuation::Dismounting(step), move |server| {
            let entity = server.subject_view(rider)?;
            Some(EventPayload::EntityDismount(EntityDismount {
                dismounted: server.subject_view(vehicle)?,
                entity_type: entity.kind.clone(),
                entity,
            }))
        });
    }

    pub(crate) fn finish_dismounting(&mut self, step: RideStep, cancelled: bool) {
        if cancelled || self.entities.rides.vehicle_of(step.rider) != Some(step.vehicle) {
            return;
        }
        self.detach_rider(step.rider, step.vehicle);
    }

    pub(crate) fn stop_riding_forced(&mut self, rider: EntityId) {
        let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
            return;
        };
        let lane = self.lane_for(self.player_of_entity(rider), rider);
        if self.kind_of(vehicle).is_some_and(riding::is_bukkit_vehicle)
            && self.is_living_rider(rider)
        {
            self.fire(
                lane,
                Continuation::Notice(EventKind::VehicleExit),
                move |server| {
                    Some(EventPayload::VehicleExit(VehicleExit {
                        exited: server.subject_view(rider)?,
                        vehicle: server.subject_view(vehicle)?,
                    }))
                },
            );
        }
        self.fire(
            lane,
            Continuation::Notice(EventKind::EntityDismount),
            move |server| {
                let entity = server.subject_view(rider)?;
                Some(EventPayload::EntityDismount(EntityDismount {
                    dismounted: server.subject_view(vehicle)?,
                    entity_type: entity.kind.clone(),
                    entity,
                }))
            },
        );
        self.detach_rider(rider, vehicle);
    }

    fn detach_rider(&mut self, rider: EntityId, vehicle: EntityId) {
        self.entities.rides.detach(vehicle, rider);
        self.send_passengers(vehicle);
        let at = self.entities.store.get(vehicle).map(|e| e.base.position);
        if let Some(at) = at {
            self.game_event(
                GameEvent::EntityDismount,
                at,
                GameEventContext::of(Some(rider)),
            );
        }
        if self.is_living_rider(rider) {
            self.dismount_vehicle(rider, vehicle);
        }
    }

    pub(crate) fn eject_passengers(&mut self, vehicle: EntityId) {
        let passengers: Vec<EntityId> = self.entities.rides.passengers_of(vehicle).to_vec();
        for passenger in passengers.into_iter().rev() {
            self.stop_riding_forced(passenger);
        }
    }

    pub(crate) fn unride(&mut self, id: EntityId) {
        if self.is_vehicle(id) {
            self.eject_passengers(id);
        }
        if self.is_passenger(id) {
            self.stop_riding_forced(id);
        }
    }

    pub(crate) fn release_riding(&mut self, id: EntityId, reason: RemovalReason) {
        if reason == RemovalReason::UnloadedToChunk {
            let passengers = self.entities.rides.indirect_passengers(id);
            for passenger in passengers.into_iter().rev() {
                if self.is_player_entity(passenger) {
                    self.stop_riding_forced(passenger);
                } else if self.entities.store.get(passenger).is_some() {
                    self.remove_entity(passenger, reason, None);
                }
            }
        } else {
            self.eject_passengers(id);
        }
        if reason.should_destroy() {
            self.stop_riding_forced(id);
        }
        self.entities.rides.forget(id);
        self.deliver();
    }

    pub(crate) fn send_passengers(&mut self, vehicle: EntityId) {
        let passengers = self.entities.rides.passengers_of(vehicle).to_vec();
        self.push(
            Audience::Trackers(vehicle),
            ClientEvent::Passengers {
                vehicle,
                passengers,
            },
        );
    }

    pub(crate) fn position_riders(&mut self, vehicle: EntityId) {
        let passengers = self.entities.rides.passengers_of(vehicle).to_vec();
        for passenger in passengers {
            self.position_rider(vehicle, passenger);
            self.position_riders(passenger);
        }
    }

    pub(crate) fn position_rider(&mut self, vehicle: EntityId, passenger: EntityId) {
        if !self.entities.rides.has_passenger(vehicle, passenger) {
            return;
        }
        let (Some(carrier), Some(rider)) = (self.seat_of(vehicle), self.seat_of(passenger)) else {
            return;
        };
        let index = self
            .entities
            .rides
            .passengers_of(vehicle)
            .iter()
            .position(|candidate| *candidate == passenger)
            .unwrap_or(0);
        let attach = riding::passenger_attachment(
            carrier.kind,
            carrier.baby,
            carrier.height,
            carrier.scale,
            carrier.yaw,
            index,
        );
        let own = riding::vehicle_attachment(rider.kind, rider.baby, rider.scale, rider.yaw);
        let target = carrier.position + attach - own;
        self.place_rider(passenger, target);
        if carrier.kind == Kind::Chicken {
            let body_yaw = self
                .entities
                .store
                .get(vehicle)
                .and_then(|e| e.living())
                .map(|living| living.body_yaw);
            if let (Some(body_yaw), Some(entity)) =
                (body_yaw, self.entities.store.get_mut(passenger))
                && let Some(living) = entity.living_mut()
            {
                living.body_yaw = body_yaw;
            }
        }
    }

    fn place_rider(&mut self, id: EntityId, position: Vec3) {
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == id) {
            player.position = position;
            player.on_ground = false;
            let chunk = position.chunk();
            if chunk != player.streamer.center() {
                player
                    .streamer
                    .move_to(chunk, &mut self.world.chunks, &self.workers, &player.link);
            }
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.position = position;
            entity.base.fall_distance = 0.0;
        }
        self.entities.store.reindex(id);
    }

    pub(crate) fn tick_passengers(&mut self, vehicle: EntityId) {
        let passengers = self.entities.rides.passengers_of(vehicle).to_vec();
        for passenger in passengers {
            if self.entities.rides.vehicle_of(passenger) != Some(vehicle) {
                continue;
            }
            if self.is_player_entity(passenger) {
                if self.is_shifting(passenger) {
                    self.stop_riding(passenger);
                    if !self.is_passenger(passenger) {
                        continue;
                    }
                }
                self.position_rider(vehicle, passenger);
                self.tick_passengers(passenger);
                continue;
            }
            if self.entities.store.get(passenger).is_none() {
                self.stop_riding_forced(passenger);
                continue;
            }
            self.tick_mob_passenger(passenger);
            self.position_rider(vehicle, passenger);
            self.tick_passengers(passenger);
        }
    }

    fn tick_mob_passenger(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.velocity = Vec3::ZERO;
        }
        self.tick_living_entity(id);
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.fall_distance = 0.0;
        }
        if let Some(vehicle) = self.entities.rides.vehicle_of(id)
            && self.controlling_mob(vehicle) == Some(id)
        {
            self.drive_vehicle(vehicle, id);
        }
    }

    pub(crate) fn controlling_mob(&self, vehicle: EntityId) -> Option<EntityId> {
        let entity = self.entities.store.get(vehicle)?;
        if entity.living().is_none() || rspigot_domain::entity::living::is_no_ai(&entity.base) {
            return None;
        }
        let first = self.entities.rides.first_passenger(vehicle)?;
        let rider = self.entities.store.get(first)?;
        rider.living()?.mob.as_ref()?;
        let controls =
            !rspigot_data::tags::tag("minecraft:entity_type", "minecraft:non_controlling_rider")
                .is_some_and(|ids| {
                    ids.binary_search(&u32::from(rider.base.kind.id().0))
                        .is_ok()
                });
        controls.then_some(first)
    }

    fn drive_vehicle(&mut self, vehicle: EntityId, rider: EntityId) {
        const SPEED: &str = rspigot_domain::entity::living::MOVEMENT_SPEED;
        let Some((yaw, head, inputs, modifier)) =
            self.entities.store.get(rider).and_then(|entity| {
                let living = entity.living()?;
                let attribute = rspigot_ai::attribute(entity.base.kind, living, SPEED);
                let modifier = if attribute > 0.0 {
                    f64::from(living.speed) / attribute
                } else {
                    0.0
                };
                Some((
                    entity.base.yaw,
                    entity.base.head_yaw,
                    (living.xxa, living.yya, living.zza, living.jumping),
                    modifier,
                ))
            })
        else {
            return;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(vehicle)
        else {
            return;
        };
        base.set_yaw(yaw);
        base.head_yaw = head;
        state.body_yaw = yaw;
        (state.xxa, state.yya, state.zza, state.jumping) = inputs;
        let own = rspigot_ai::attribute(base.kind, state, SPEED);
        state.speed = (modifier * own) as f32;
    }

    pub(crate) fn tick_ride_players(&mut self) {
        self.entities.rides.tick_cooldowns();
        let riders: Vec<EntityId> = self
            .players
            .values()
            .filter(|player| self.entities.rides.is_passenger(player.entity_id))
            .map(|player| player.entity_id)
            .collect();
        for rider in riders {
            let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
                continue;
            };
            if self.entities.store.get(vehicle).is_none() {
                self.stop_riding_forced(rider);
            }
        }
    }

    fn dismount_vehicle(&mut self, rider: EntityId, vehicle: EntityId) {
        let Some(seat) = self.seat_of(rider) else {
            return;
        };
        let vehicle_entity = self.entities.store.get(vehicle);
        let in_portal = vehicle_entity.is_some_and(|entity| {
            self.world
                .chunks
                .block(entity.base.position.block())
                .is_some_and(|state| rspigot_domain::entity::physics::has_tag(state, PORTALS))
        });
        let target = match vehicle_entity {
            Some(_) if !in_portal => self.dismount_location(vehicle, rider),
            _ => {
                let y = vehicle_entity.map_or(seat.position.y, |entity| {
                    entity.base.position.y.max(seat.position.y)
                });
                let base = Vec3::new(seat.position.x, y, seat.position.z);
                let dims = seat.kind.dimensions();
                if dims.width > FREE_POSITION_RADIUS || dims.height > FREE_POSITION_RADIUS {
                    base
                } else {
                    self.free_position(base, dims.width, dims.height)
                }
            }
        };
        self.dismount_to(rider, target);
    }

    fn free_position(&self, base: Vec3, width: f32, height: f32) -> Vec3 {
        let lifted = Vec3::new(base.x, base.y + f64::from(height) / 2.0, base.z);
        let area = Aabb::of_size(lifted, width, height);
        if fluid::is_free_of_collision(&self.world.chunks, &area) {
            base
        } else {
            for step in 1..=8 {
                let up = Vec3::new(base.x, base.y + f64::from(step) * 0.5, base.z);
                let area = Aabb::of_size(up, width, height);
                if fluid::is_free_of_collision(&self.world.chunks, &area) {
                    return up;
                }
            }
            base
        }
    }

    pub(crate) fn dismount_location(&self, vehicle: EntityId, rider: EntityId) -> Vec3 {
        let Some(entity) = self.entities.store.get(vehicle) else {
            return Vec3::ZERO;
        };
        let base = &entity.base;
        let default = Vec3::new(base.position.x, base.bounding_box().max_y, base.position.z);
        if base.kind.extends("Animal") {
            let dims = self
                .seat_of(rider)
                .map_or_else(|| Kind::Player.dimensions(), |seat| seat.kind.dimensions());
            if let Some(spot) = riding::animal_dismount(
                &self.world.chunks,
                base.position.block(),
                base.yaw,
                dims.width,
                dims.height,
            ) {
                return spot;
            }
        }
        default
    }

    pub(crate) fn dismount_to(&mut self, rider: EntityId, target: Vec3) {
        if let Some(id) = self.player_of_entity(rider) {
            self.teleport_player(id, target);
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(rider) {
            entity.base.position = target;
            entity.base.old_position = target;
            entity.base.needs_sync = true;
        }
        self.entities.store.reindex(rider);
        self.position_riders(rider);
    }

    pub(crate) fn run_jockey(&mut self, id: EntityId) {
        let plan = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            .and_then(|living| living.mob.as_mut())
            .and_then(|mob| mob.jockey.take());
        match plan {
            Some(Jockey::ExistingChicken) => self.mount_loose_chicken(id),
            Some(Jockey::NewChicken) => self.spawn_mount(id, Kind::Chicken, true),
            Some(Jockey::SkeletonRider) => self.spawn_rider_on(id, JockeyRider::of(Kind::Skeleton)),
            Some(Jockey::StriderPiglin { baby }) => self.spawn_rider_on(
                id,
                JockeyRider {
                    group: Some(SpawnGroupData::Zombie {
                        baby,
                        can_spawn_jockey: false,
                    }),
                    held: Some(ItemKind::WarpedFungusOnAStick),
                    ..JockeyRider::of(Kind::ZombifiedPiglin)
                },
            ),
            Some(Jockey::StriderBaby) => self.spawn_rider_on(
                id,
                JockeyRider {
                    age: Some(strider::BABY_AGE),
                    ..JockeyRider::of(Kind::Strider)
                },
            ),
            None => {}
        }
    }

    fn mount_loose_chicken(&mut self, rider: EntityId) {
        let Some(area) = self
            .entities
            .store
            .get(rider)
            .map(|entity| entity.base.bounding_box().inflate_by(5.0, 3.0, 5.0))
        else {
            return;
        };
        let chicken = self
            .entities
            .store
            .iter()
            .find(|entity| {
                entity.base.kind == Kind::Chicken
                    && !entity.base.is_removed()
                    && !self.entities.rides.is_vehicle(entity.base.id)
                    && !self.entities.rides.is_passenger(entity.base.id)
                    && entity.living().is_some_and(|living| {
                        !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
                    })
                    && entity.base.bounding_box().intersects(&area)
            })
            .map(|entity| entity.base.id);
        let Some(chicken) = chicken else {
            return;
        };
        if let Some(Species::Chicken(state)) = self
            .entities
            .store
            .get_mut(chicken)
            .and_then(|entity| entity.living_mut())
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            state.jockey = true;
        }
        self.start_riding(rider, chicken, false, false);
    }

    fn spawn_mount(&mut self, rider: EntityId, kind: Kind, chicken_jockey: bool) {
        let Some((position, yaw)) = self
            .entities
            .store
            .get(rider)
            .map(|entity| (entity.base.position, entity.base.yaw))
        else {
            return;
        };
        let mut mount = new_mob(self.new_entity_base(kind));
        mount.base.snap_to(position, yaw, 0.0);
        self.finalize_jockey_part(&mut mount, None);
        if chicken_jockey
            && let Some(Species::Chicken(state)) = mount
                .living_mut()
                .and_then(|living| living.mob.as_mut())
                .map(|mob| &mut mob.species)
        {
            state.jockey = true;
        }
        self.spawn_entity(
            mount,
            Birth::of(CreatureSpawnReason::Mount).then(Spawned::Mounted { rider }),
        );
    }

    fn spawn_rider_on(&mut self, vehicle: EntityId, rider: JockeyRider) {
        let Some((position, yaw)) = self
            .entities
            .store
            .get(vehicle)
            .map(|entity| (entity.base.position, entity.base.yaw))
        else {
            return;
        };
        let mut entity = new_mob(self.new_entity_base(rider.kind));
        entity.base.snap_to(position, yaw, 0.0);
        if let Some(age) = rider.age
            && let EntityState::Living(state) = &mut entity.state
            && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            rspigot_domain::entity::ageable::set_age(&mut entity.base, ageable, age);
        }
        self.finalize_jockey_part(&mut entity, rider.group);
        if let (Some(item), EntityState::Living(state)) = (rider.held, &mut entity.state) {
            spawn::set_item_slot(
                state,
                EquipmentSlot::Mainhand,
                Some(ItemStack::new(item.id(), NonZeroU8::MIN)),
            );
        }
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Jockey).then(Spawned::Rides { vehicle }),
        );
    }

    fn finalize_jockey_part(&mut self, entity: &mut Entity, group: Option<SpawnGroupData>) {
        let environment = self.spawn_environment(entity.base.position);
        let mut own = rspigot_domain::random::LegacyRandom::new(rand::Rng::random(&mut self.rng));
        spawn::finalize_spawn(
            entity,
            &environment,
            SpawnReason::Jockey,
            group,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
    }
}
