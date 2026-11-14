use rspigot_domain::entity::boat;
use rspigot_domain::entity::raycast;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, fluid};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::Hand;
use crate::interact::Interaction;
use crate::plugins::events::{VehicleBlockCollision, VehicleMove, VehicleUpdate};
use crate::plugins::{EventKind, EventPayload, ray_end};
use crate::ports::ClientLink;
use crate::server::Server;

const BOAT_ITEM_CLASS: &str = "BoatItem";
const BLOCK_REACH: f64 = 4.5;
const CREATIVE_BLOCK_REACH: f64 = 5.0;
const SURFACE_HEIGHT: f64 = 8.0 / 9.0;
const PUSH_MARGIN: f64 = 0.2;
const PUSH_SHRINK: f64 = -0.01;
const NO_BOARDING: &str = "minecraft:cannot_be_pushed_onto_boats";
const VOID_DEPTH: f64 = 64.0;

static SURFACE: [rspigot_blocks::Aabb; 1] = [rspigot_blocks::Aabb::new(
    0.0,
    0.0,
    0.0,
    1.0,
    SURFACE_HEIGHT,
    1.0,
)];

fn aim_shape(state: rspigot_domain::BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    if state.flags().has_fluid() {
        &SURFACE
    } else {
        state.collision_shape()
    }
}

#[derive(Debug, Clone, Copy)]
struct Neighbour {
    id: EntityId,
    player: bool,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_boat(&mut self, id: PlayerId, hand: Hand, rotation: Rotation) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        let Some(kind) = stack
            .item
            .kind()
            .filter(|item| item.class() == BOAT_ITEM_CLASS)
            .and_then(|item| Kind::from_name(item.name()))
        else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if player.game_mode == GameMode::Spectator {
            return false;
        }
        let reach = if player.game_mode == GameMode::Creative {
            CREATIVE_BLOCK_REACH
        } else {
            BLOCK_REACH
        };
        let from = player.eye_position();
        let yaw = player.rotation.yaw;
        let Some(hit) = raycast::clip_blocks_with(
            &self.world.chunks,
            from,
            ray_end(from, rotation, reach),
            aim_shape,
        ) else {
            return true;
        };
        let mut base = self.new_entity_base(kind);
        let state = boat::define(&mut base);
        boat::set_initial_position(&mut base, hit.location);
        base.set_yaw(yaw);
        if !fluid::is_free_of_collision(&self.world.chunks, &base.bounding_box()) {
            return true;
        }
        let entity = Entity::new(base, EntityState::Boat(Box::new(state)));
        self.place_vehicle(id, hand, entity, (hit.pos, hit.face));
        true
    }

    pub(crate) fn boat_interact(&mut self, id: PlayerId, target: EntityId) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let EntityState::Boat(state) = &entity.state else {
            return None;
        };
        let rider = self.players.get(&id)?.entity_id;
        let sneaking = self.is_shifting(rider);
        let free = state.out_of_control_ticks < boat::OUT_OF_CONTROL_LIMIT;
        if !sneaking && free && self.start_riding(rider, target, false, true) {
            return Some(Interaction::Predicted);
        }
        Some(Interaction::Pass)
    }

    pub(crate) fn paddle_boat(&mut self, id: PlayerId, left: bool, right: bool) {
        let Some(rider) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
            return;
        };
        if self.entities.rides.first_passenger(vehicle) != Some(rider) {
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(vehicle)
            && matches!(entity.state, EntityState::Boat(_))
        {
            boat::set_paddles(&mut entity.base, left, right);
        }
    }

    pub(crate) fn boat_seat_offset(
        &self,
        vehicle: EntityId,
        rider: Kind,
        index: usize,
    ) -> Option<Vec3> {
        let entity = self.entities.store.get(vehicle)?;
        if !matches!(entity.state, EntityState::Boat(_)) {
            return None;
        }
        let seated = self.entities.rides.passengers_of(vehicle).len();
        Some(boat::seat_offset(
            &entity.base,
            index,
            seated,
            rider.extends("Animal"),
        ))
    }

    pub(crate) fn boat_can_take(&self, vehicle: EntityId) -> Option<bool> {
        let entity = self.entities.store.get(vehicle)?;
        if !matches!(entity.state, EntityState::Boat(_)) {
            return None;
        }
        let seated = self.entities.rides.passengers_of(vehicle).len();
        Some(seated < boat::max_passengers(&entity.base) && !entity.base.eye_in_water)
    }

    pub(crate) fn tick_boat(&mut self, id: EntityId) {
        let first = self.entities.rides.first_passenger(id);
        let piloted = first.is_some_and(|rider| self.is_player_entity(rider));
        let floor = f64::from(self.world.dimension.height.min_y) - VOID_DEPTH;
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Boat(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let from = (base.position, Rotation::new(base.yaw, base.pitch));
        fluid::update(base, chunks);
        let outcome = boat::tick(base, state, chunks, piloted, piloted, piloted);
        let to = (base.position, Rotation::new(base.yaw, base.pitch));
        base.needs_sync |= from != to;
        if base.position.y < floor {
            self.remove_entity(id, RemovalReason::Discarded, None);
            return;
        }
        self.entities.store.reindex(id);
        if outcome.eject {
            self.eject_passengers(id);
        }
        if let Some(block) = outcome.block_collision {
            self.vehicle_block_collision(id, block);
        }
        self.vehicle_updated(id);
        if from != to {
            self.vehicle_moved_by_tick(id, from, to);
        }
        self.crowd_boat(id, piloted);
    }

    fn vehicle_block_collision(&mut self, id: EntityId, block: BlockPos) {
        if !self.listened(EventKind::VehicleBlockCollision) {
            return;
        }
        let Some(vehicle) = self.subject_view(id) else {
            return;
        };
        self.notify_now(EventPayload::VehicleBlockCollision(VehicleBlockCollision {
            block,
            vehicle,
        }));
    }

    pub(crate) fn vehicle_updated(&mut self, id: EntityId) {
        if !self.listened(EventKind::VehicleUpdate) {
            return;
        }
        let Some(vehicle) = self.subject_view(id) else {
            return;
        };
        self.notify_now(EventPayload::VehicleUpdate(VehicleUpdate { vehicle }));
    }

    pub(crate) fn vehicle_moved_by_tick(
        &mut self,
        id: EntityId,
        from: (Vec3, Rotation),
        to: (Vec3, Rotation),
    ) {
        if !self.listened(EventKind::VehicleMove) {
            return;
        }
        let Some(vehicle) = self.subject_view(id) else {
            return;
        };
        let payload = EventPayload::VehicleMove(VehicleMove {
            from: self.location_of(from.0, from.1),
            to: self.location_of(to.0, to.1),
            vehicle,
        });
        self.notify_now(payload);
    }

    fn crowd_boat(&mut self, id: EntityId, piloted: bool) {
        let neighbours = self.boat_neighbours(id);
        for Neighbour { id: other, player } in neighbours {
            if self.entities.rides.has_passenger(other, id)
                || self.entities.rides.is_passenger_of_same_vehicle(id, other)
            {
                continue;
            }
            if !piloted && !player && self.try_board(id, other) {
                continue;
            }
            self.crash_into(id, other, player);
        }
    }

    fn try_board(&mut self, boat: EntityId, other: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(other) else {
            return false;
        };
        let (Some(carrier), true) = (self.entities.store.get(boat), entity.living().is_some())
        else {
            return false;
        };
        let roomy = self.boat_can_take(boat).unwrap_or(false)
            && !self.entities.rides.is_passenger(other)
            && entity.base.dimensions.width < carrier.base.dimensions.width
            && !rspigot_data::tags::tag("minecraft:entity_type", NO_BOARDING).is_some_and(|ids| {
                ids.binary_search(&u32::from(entity.base.kind.id().0))
                    .is_ok()
            });
        roomy && self.start_riding(other, boat, false, true)
    }

    fn crash_into(&mut self, boat: EntityId, other: EntityId, player: bool) {
        let Some(this) = self.entities.store.get(boat).map(|e| e.base.bounding_box()) else {
            return;
        };
        let (floor, sibling) = if player {
            let Some(body) = self
                .players
                .values()
                .find(|candidate| candidate.entity_id == other)
            else {
                return;
            };
            (body.position.y, false)
        } else {
            let Some(that) = self.entities.store.get(other) else {
                return;
            };
            (
                that.base.bounding_box().min_y,
                matches!(that.state, EntityState::Boat(_)),
            )
        };
        let reaches = if sibling {
            floor < this.max_y
        } else {
            floor <= this.min_y
        };
        if reaches {
            self.push_vehicle(boat, other);
        }
    }

    fn boat_neighbours(&self, id: EntityId) -> Vec<Neighbour> {
        let Some(entity) = self.entities.store.get(id) else {
            return Vec::new();
        };
        let area = entity
            .base
            .bounding_box()
            .inflate_by(PUSH_MARGIN, PUSH_SHRINK, PUSH_MARGIN);
        let low = ChunkPos::from_block(area.min_x, area.min_z);
        let high = ChunkPos::from_block(area.max_x, area.max_z);
        let mut found = Vec::new();
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                for other in self.entities.store.in_chunk(ChunkPos::new(x, z)) {
                    let Some(candidate) = self.entities.store.get(*other) else {
                        continue;
                    };
                    let pushable = matches!(candidate.state, EntityState::Boat(_))
                        || candidate.living().is_some_and(|living| {
                            !rspigot_domain::entity::living::is_dead_or_dying(
                                &candidate.base,
                                living,
                            )
                        });
                    if *other != id
                        && pushable
                        && !candidate.base.is_removed()
                        && candidate.base.bounding_box().intersects(&area)
                    {
                        found.push(Neighbour {
                            id: *other,
                            player: false,
                        });
                    }
                }
            }
        }
        for player in self.players.values() {
            let width = 0.6;
            let body = Aabb::of_size(player.position, width, player.height() as f32);
            if player.game_mode != GameMode::Spectator
                && !player.vitals.is_dead()
                && body.intersects(&area)
            {
                found.push(Neighbour {
                    id: player.entity_id,
                    player: true,
                });
            }
        }
        found
    }
}
