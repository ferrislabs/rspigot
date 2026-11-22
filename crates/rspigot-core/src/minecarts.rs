use rspigot_domain::entity::minecart::{self, exits_of};
use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, fluid};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::{Direction, Hand};
use crate::interact::Interaction;
use crate::plugins::events::Face;
use crate::ports::ClientLink;
use crate::server::Server;

const MINECART_ITEM_CLASS: &str = "MinecartItem";
const CUSTOM_NAME: &str = "minecraft:custom_name";
const RAIL_LIFT: f64 = 0.0625;
const SLOPE_LIFT: f64 = 0.5;
const PUSH_MARGIN: f64 = 0.2;
const RIDE_SPEED_SQUARED: f64 = 0.01;
const VOID_DEPTH: f64 = 64.0;

const fn face_of(direction: Direction) -> Face {
    match direction {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}

#[derive(Debug, Clone, Copy)]
struct Neighbour {
    id: EntityId,
    player: bool,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_minecart(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        face: Direction,
        clicked: BlockStateId,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        let Some(kind) = stack
            .item
            .kind()
            .filter(|item| item.class() == MINECART_ITEM_CLASS)
            .and_then(|item| Kind::from_name(item.name()))
        else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if matches!(player.game_mode, GameMode::Spectator | GameMode::Adventure)
            || !minecart::is_rail(clicked)
        {
            return true;
        }
        let slope = clicked
            .property("shape")
            .and_then(exits_of)
            .is_some_and(|exits| exits.slope);
        let lift = RAIL_LIFT + if slope { SLOPE_LIFT } else { 0.0 };
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + lift,
            f64::from(pos.z) + 0.5,
        );
        let mut base = self.new_entity_base(kind);
        let state = minecart::define(&mut base);
        minecart::set_initial_position(&mut base, at);
        if let Some(name) = stack
            .components
            .get(CUSTOM_NAME)
            .and_then(component_from_tag)
        {
            base.set_custom_name(Some(name));
        }
        let entity = Entity::new(base, EntityState::Minecart(Box::new(state)));
        self.place_vehicle(id, hand, entity, (pos, face_of(face)));
        true
    }

    pub(crate) fn minecart_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let EntityState::Minecart(state) = &entity.state else {
            return None;
        };
        let kind = entity.base.kind;
        let chest = state.cargo.is_some();
        if kind == Kind::CommandBlockMinecart {
            return Some(if self.command_minecart_interact(id, target) {
                Interaction::Predicted
            } else {
                Interaction::Pass
            });
        }
        let rider = self.players.get(&id)?.entity_id;
        if chest {
            return Some(if self.open_cargo(id, target) {
                Interaction::ServerOnly
            } else {
                Interaction::Pass
            });
        }
        if !minecart::is_rideable(kind)
            || self.is_shifting(rider)
            || self.entities.rides.is_vehicle(target)
        {
            return Some(Interaction::Pass);
        }
        Some(if self.start_riding(rider, target, false, true) {
            Interaction::ServerOnly
        } else {
            Interaction::Predicted
        })
    }

    pub(crate) fn cart_can_take(&self, vehicle: EntityId) -> Option<bool> {
        let entity = self.entities.store.get(vehicle)?;
        if !matches!(entity.state, EntityState::Minecart(_)) {
            return None;
        }
        Some(minecart::is_rideable(entity.base.kind) && !self.entities.rides.is_vehicle(vehicle))
    }

    pub(crate) fn tick_minecart(&mut self, id: EntityId) {
        let occupied = self.entities.rides.is_vehicle(id);
        let speed_rule = self.config.rules.max_minecart_speed;
        let floor = f64::from(self.world.dimension.height.min_y) - VOID_DEPTH;
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Minecart(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let from = (base.position, Rotation::new(base.yaw, base.pitch));
        fluid::update(base, chunks);
        let outcome = minecart::tick(base, state, chunks, occupied, speed_rule);
        let to = (base.position, Rotation::new(base.yaw, base.pitch));
        base.needs_sync = true;
        if base.position.y < floor {
            self.remove_entity(id, RemovalReason::Discarded, None);
            return;
        }
        self.entities.store.reindex(id);
        if let Some(block) = outcome.block_collision {
            self.vehicle_block_collision(id, block);
        }
        self.vehicle_updated(id);
        if from != to {
            self.vehicle_moved_by_tick(id, from, to);
        }
        self.crowd_minecart(id);
        self.tick_command_minecart(id);
    }

    fn crowd_minecart(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let rides = minecart::is_rideable(entity.base.kind)
            && entity.base.velocity.horizontal_distance_sqr() >= RIDE_SPEED_SQUARED;
        let occupied = self.entities.rides.is_vehicle(id);
        for Neighbour { id: other, player } in self.cart_neighbours(id, !rides) {
            if self.entities.rides.has_passenger(id, other)
                || self.entities.rides.is_passenger_of_same_vehicle(id, other)
            {
                continue;
            }
            let Some(kind) = self.subject_kind(other, player) else {
                continue;
            };
            let boards = rides
                && !player
                && !kind.extends("AbstractMinecart")
                && kind != Kind::IronGolem
                && !occupied
                && !self.entities.rides.is_passenger(other);
            if rides || kind.extends("AbstractMinecart") {
                self.push_vehicle(id, other, boards);
            }
        }
    }

    fn subject_kind(&self, id: EntityId, player: bool) -> Option<Kind> {
        if player {
            return Some(Kind::Player);
        }
        self.entities.store.get(id).map(|entity| entity.base.kind)
    }

    fn cart_neighbours(&self, id: EntityId, carts_only: bool) -> Vec<Neighbour> {
        let Some(entity) = self.entities.store.get(id) else {
            return Vec::new();
        };
        let area = entity
            .base
            .bounding_box()
            .inflate_by(PUSH_MARGIN, 0.0, PUSH_MARGIN);
        let low = ChunkPos::from_block(area.min_x, area.min_z);
        let high = ChunkPos::from_block(area.max_x, area.max_z);
        let mut found = Vec::new();
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                for other in self.entities.store.in_chunk(ChunkPos::new(x, z)) {
                    let Some(candidate) = self.entities.store.get(*other) else {
                        continue;
                    };
                    let cart = matches!(candidate.state, EntityState::Minecart(_));
                    let pushable = cart
                        || candidate.living().is_some_and(|living| {
                            !rspigot_domain::entity::living::is_dead_or_dying(
                                &candidate.base,
                                living,
                            )
                        });
                    if *other != id
                        && pushable
                        && (cart || !carts_only)
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
        if !carts_only {
            for player in self.players.values() {
                let body = Aabb::of_size(player.position, 0.6, player.height() as f32);
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
        }
        found
    }
}
