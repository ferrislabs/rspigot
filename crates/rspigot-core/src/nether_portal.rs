use std::time::Duration;

use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::Aabb;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::portal::{Axis, PortalShape, Rectangle, Terrain, largest_rectangle_around};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Rotation, Vec3};

use crate::chunks::Priority;
use crate::end_portal::{PORTAL_COOLDOWN, excluded};
use crate::input::is_replaceable;
use crate::plugins::events::{PlayerTeleportCause, PortalCreateReason};
use crate::plugins::steps::{PlatformStep, PlatformTraveler};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::travel::Destination;
use crate::world_state::DimensionKey;

const DECAY: i32 = 4;
const SEARCH_TO_NETHER: i32 = 16;
const SEARCH_TO_OVERWORLD: i32 = 128;
const CREATION_RADIUS: i32 = 16;
const LOAD_RADIUS: i32 = 32;
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const NETHER_LOGICAL_HEIGHT: i32 = 128;
const INSIDE_DEFLATE: f64 = 9.999_999_747_378_752e-6;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const MAX_PORTAL_SIZE: i32 = 21;

type Placed = Vec<(BlockPos, BlockStateId)>;
const NETHER_SCALE: f64 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NetherExit {
    pub(crate) entry_axis: Axis,
    pub(crate) relative: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) search_radius: i32,
    pub(crate) can_create: bool,
    pub(crate) creation_radius: i32,
    pub(crate) spectator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetherTraveler {
    Player(PlayerId),
    Entity(EntityId),
}

#[derive(Debug, Clone)]
pub(crate) struct NetherTrip {
    pub(crate) source: DimensionKey,
    pub(crate) traveler: NetherTraveler,
    pub(crate) entry: BlockPos,
}

pub(crate) fn nether() -> DimensionKey {
    Identifier::minecraft("the_nether")
}

fn overworld() -> DimensionKey {
    Identifier::minecraft("overworld")
}

fn portal_axis(state: BlockStateId) -> Option<Axis> {
    if state.kind() != Some(BlockKind::NetherPortal) {
        return None;
    }
    state.property("axis").and_then(Axis::from_name)
}

fn portal_shape(pos: BlockPos, axis: Axis) -> Aabb {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    match axis {
        Axis::X => Aabb::new(x, y, z + 0.375, x + 1.0, y + 1.0, z + 0.625),
        Axis::Z => Aabb::new(x + 0.375, y, z, x + 0.625, y + 1.0, z + 1.0),
    }
}

fn inverse_lerp(value: f64, low: f64, high: f64) -> f64 {
    (value - low) / (high - low)
}

pub(crate) fn relative_position(
    rectangle: &Rectangle,
    axis: Axis,
    position: Vec3,
    width: f64,
    height: f64,
) -> Vec3 {
    let free_width = f64::from(rectangle.axis1_size) - width;
    let free_height = f64::from(rectangle.axis2_size) - height;
    let corner = rectangle.min_corner;
    let (along, across, corner_along, corner_across) = match axis {
        Axis::X => (position.x, position.z, corner.x, corner.z),
        Axis::Z => (position.z, position.x, corner.z, corner.x),
    };
    let x = if free_width > 0.0 {
        inverse_lerp(
            along - (f64::from(corner_along) + width / 2.0),
            0.0,
            free_width,
        )
        .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let y = if free_height > 0.0 {
        inverse_lerp(position.y - f64::from(corner.y), 0.0, free_height).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let z = across - (f64::from(corner_across) + 0.5);
    Vec3::new(x, y, z)
}

fn is_liquid(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava))
}

impl<L: ClientLink> Server<L> {
    fn in_portal_dimension(&self) -> bool {
        let key = &self.world.dimension.key;
        *key == overworld() || *key == nether()
    }

    fn portal_enabled(&self) -> bool {
        self.config.allow_nether
            && self.config.rules.allow_entering_nether_using_portals
            && self.in_portal_dimension()
    }

    pub(crate) fn try_light_portal(&mut self, pos: BlockPos) {
        if !self.portal_enabled() {
            return;
        }
        let min_y = self.world.dimension.height.min_y;
        let chunks = &self.world.chunks;
        let Some(shape) = PortalShape::find_empty(&|at| chunks.block(at), min_y, pos, Axis::X)
        else {
            return;
        };
        let portal = BlockKind::NetherPortal
            .state_with(&[("axis", shape.axis.name())])
            .unwrap_or_else(|| BlockKind::NetherPortal.default_state());
        let interior = shape.interior();
        let placed: Vec<(BlockPos, BlockStateId)> =
            interior.iter().map(|cell| (*cell, portal)).collect();
        let obsidian = BlockKind::Obsidian.default_state();
        let listing: Vec<(BlockPos, BlockStateId)> = shape
            .blocks_touched()
            .into_iter()
            .map(|at| {
                let state = if interior.contains(&at) {
                    portal
                } else {
                    obsidian
                };
                (at, state)
            })
            .collect();
        let key = self.world.dimension.key.clone();
        if self.listened(crate::plugins::EventKind::PortalCreate) {
            let step = PlatformStep {
                traveler: PlatformTraveler::Ignition(key.clone()),
                destination: Destination::here(key, pos.center()),
                placed,
                reason: PortalCreateReason::Fire,
            };
            self.ask_platform(step, listing);
        } else {
            self.apply_portal_blocks(&placed);
        }
    }

    pub(crate) fn apply_portal_blocks(&mut self, placed: &[(BlockPos, BlockStateId)]) {
        let held = self.hold_chunks_of(placed.iter().map(|(pos, _)| pos.chunk()));
        for (pos, state) in placed {
            self.set_block(*pos, *state);
        }
        for pos in held {
            self.world.chunks.remove_ticket(pos);
        }
    }

    fn hold_chunks_of(&mut self, chunks: impl Iterator<Item = ChunkPos>) -> Vec<ChunkPos> {
        let mut held: Vec<ChunkPos> = chunks.collect();
        held.sort_unstable_by_key(|pos| (pos.x, pos.z));
        held.dedup();
        for pos in &held {
            self.world
                .chunks
                .add_ticket(*pos, Priority::URGENT, &self.workers);
        }
        self.wait_for_spawn(LOAD_TIMEOUT);
        held
    }

    fn hold_square(&mut self, center: BlockPos, radius: i32) -> Vec<ChunkPos> {
        let (low, high) = (
            center.offset(-radius, 0, -radius).chunk(),
            center.offset(radius, 0, radius).chunk(),
        );
        let mut cells = Vec::new();
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                cells.push(ChunkPos::new(x, z));
            }
        }
        self.hold_chunks_of(cells.into_iter())
    }

    pub(crate) fn portal_neighbour_changed(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Face,
        neighbour: BlockStateId,
    ) {
        let Some(axis) = portal_axis(state) else {
            return;
        };
        let along = direction.is_horizontal() && {
            let (dx, _, _) = direction.offset();
            (dx != 0) == (axis == Axis::X)
        };
        let across_axis = direction.is_horizontal() && !along;
        if across_axis || neighbour.kind() == Some(BlockKind::NetherPortal) {
            return;
        }
        let min_y = self.world.dimension.height.min_y;
        let chunks = &self.world.chunks;
        let shape = PortalShape::find_any(&|at| chunks.block(at), min_y, pos, axis);
        if !shape.is_complete() {
            self.set_block(pos, BlockStateId::AIR);
        }
    }

    fn touched_nether_portal(&self, bounds: &Aabb) -> Option<BlockPos> {
        let bounds = bounds.deflate(INSIDE_DEFLATE);
        let floor = |value: f64| value.floor() as i32;
        for x in floor(bounds.min_x)..=floor(bounds.max_x) {
            for y in floor(bounds.min_y)..=floor(bounds.max_y) {
                for z in floor(bounds.min_z)..=floor(bounds.max_z) {
                    let pos = BlockPos::new(x, y, z);
                    let axis = self.world.chunks.block(pos).and_then(portal_axis);
                    if axis.is_some_and(|axis| portal_shape(pos, axis).intersects(&bounds)) {
                        return Some(pos);
                    }
                }
            }
        }
        None
    }

    fn transition_time(&self, player: Option<PlayerId>) -> i32 {
        let Some(player) = player.and_then(|id| self.players.get(&id)) else {
            return 0;
        };
        let rules = &self.config.rules;
        if player.game_mode == GameMode::Creative {
            rules.players_nether_portal_creative_delay
        } else {
            rules.players_nether_portal_default_delay
        }
    }

    pub(crate) fn tick_nether_portals(&mut self) {
        if !self.portal_enabled() {
            self.nether_time.clear();
            return;
        }
        let key = self.world.dimension.key.clone();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in players {
            self.tick_player_nether(id, &key);
        }
        for id in self.ticking_entities() {
            self.tick_entity_nether(id, &key);
        }
    }

    fn tick_player_nether(&mut self, id: PlayerId, key: &DimensionKey) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let alive = !player.vitals.is_dead() && player.game_mode != GameMode::Spectator;
        let riding = self.entities.rides.is_passenger(entity);
        let at = player.position;
        let bounds = Aabb::new(
            at.x - PLAYER_HALF_WIDTH,
            at.y,
            at.z - PLAYER_HALF_WIDTH,
            at.x + PLAYER_HALF_WIDTH,
            at.y + player.height(),
            at.z + PLAYER_HALF_WIDTH,
        );
        let touched = if alive && !riding {
            self.touched_nether_portal(&bounds)
        } else {
            None
        };
        let delay = self.transition_time(Some(id));
        let ready = self.step_portal_time(entity, touched.is_some(), delay);
        if let Some(entry) = touched {
            self.notify_portal_enter(entity, vec![entry]);
            if ready {
                self.nether_queue.push(NetherTrip {
                    source: key.clone(),
                    traveler: NetherTraveler::Player(id),
                    entry,
                });
            }
        }
    }

    fn tick_entity_nether(&mut self, id: EntityId, key: &DimensionKey) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        if base.is_removed()
            || excluded(base.kind)
            || !entity.is_simulated()
            || self.entities.dragons.contains_key(&id)
            || self.entities.rides.is_passenger(id)
            || !self.entity_ticking(base.chunk())
        {
            return;
        }
        let touched = self.touched_nether_portal(&base.bounding_box());
        let ready = self.step_portal_time(id, touched.is_some(), 0);
        if let Some(entry) = touched {
            self.notify_portal_enter(id, vec![entry]);
            if ready {
                self.nether_queue.push(NetherTrip {
                    source: key.clone(),
                    traveler: NetherTraveler::Entity(id),
                    entry,
                });
            }
        }
    }

    fn portal_cooldown_of(&self, id: EntityId) -> i32 {
        if let Some(player) = self.players.values().find(|player| player.entity_id == id) {
            return player.portal_cooldown;
        }
        self.entities
            .store
            .get(id)
            .map_or(0, |entity| entity.base.portal_cooldown)
    }

    fn step_portal_time(&mut self, id: EntityId, inside: bool, delay: i32) -> bool {
        if !inside {
            if let Some(time) = self.nether_time.get_mut(&id) {
                *time = (*time - DECAY).max(0);
                if *time == 0 {
                    self.nether_time.remove(&id);
                }
            }
            return false;
        }
        if self.portal_cooldown_of(id) > 0 {
            self.set_portal_cooldown(id);
            return false;
        }
        let time = self.nether_time.entry(id).or_insert(0);
        let ready = *time >= delay;
        *time += 1;
        if !ready {
            return false;
        }
        self.nether_time.remove(&id);
        self.set_portal_cooldown(id);
        true
    }

    fn set_portal_cooldown(&mut self, id: EntityId) {
        if let Some(player) = self
            .players
            .values_mut()
            .find(|player| player.entity_id == id)
        {
            player.portal_cooldown = PORTAL_COOLDOWN;
        } else if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.portal_cooldown = PORTAL_COOLDOWN;
        }
    }

    pub(crate) fn run_nether_queue(&mut self) {
        for trip in std::mem::take(&mut self.nether_queue) {
            let source = trip.source.clone();
            let Some((traveler, destination, exit)) =
                self.in_dimension(&source, |server| server.start_nether_trip(&trip))
            else {
                continue;
            };
            match traveler {
                NetherTraveler::Player(id) => self.ask_portal_with(
                    id,
                    destination,
                    PlayerTeleportCause::NetherPortal,
                    Some(exit),
                ),
                NetherTraveler::Entity(id) => {
                    self.ask_entity_portal_with(id, destination, Some(exit));
                }
            }
        }
    }

    fn start_nether_trip(
        &mut self,
        trip: &NetherTrip,
    ) -> Option<(NetherTraveler, Destination, NetherExit)> {
        let entry_state = self.block_or_air(trip.entry);
        let (axis, rectangle) = match portal_axis(entry_state) {
            Some(axis) => {
                let chunks = &self.world.chunks;
                let rectangle = largest_rectangle_around(
                    trip.entry,
                    axis,
                    MAX_PORTAL_SIZE,
                    MAX_PORTAL_SIZE,
                    |at| chunks.block(at) == Some(entry_state),
                );
                (axis, Some(rectangle))
            }
            None => (Axis::X, None),
        };
        let (position, rotation, bounds, spectator) = match trip.traveler {
            NetherTraveler::Player(id) => {
                let player = self.players.get(&id)?;
                let at = player.position;
                let bounds = Aabb::new(
                    at.x - PLAYER_HALF_WIDTH,
                    at.y,
                    at.z - PLAYER_HALF_WIDTH,
                    at.x + PLAYER_HALF_WIDTH,
                    at.y + player.height(),
                    at.z + PLAYER_HALF_WIDTH,
                );
                (
                    at,
                    player.rotation,
                    bounds,
                    player.game_mode == GameMode::Spectator,
                )
            }
            NetherTraveler::Entity(id) => {
                let entity = self.entities.store.get(id)?;
                let base = &entity.base;
                (
                    base.position,
                    Rotation::new(base.yaw, base.pitch),
                    base.bounding_box(),
                    false,
                )
            }
        };
        let width = bounds.max_x - bounds.min_x;
        let height = bounds.max_y - bounds.min_y;
        let relative = rectangle.map_or(Vec3::new(0.5, 0.0, 0.0), |rectangle| {
            relative_position(&rectangle, axis, position, width, height)
        });
        let to_nether = self.world.dimension.key == overworld();
        let target = if to_nether { nether() } else { overworld() };
        let scale = if to_nether {
            1.0 / NETHER_SCALE
        } else {
            NETHER_SCALE
        };
        let approx = BlockPos::new(
            (position.x * scale).floor() as i32,
            position.y.floor() as i32,
            (position.z * scale).floor() as i32,
        );
        let exit = NetherExit {
            entry_axis: axis,
            relative,
            yaw: rotation.yaw,
            pitch: rotation.pitch,
            width,
            height,
            search_radius: if to_nether {
                SEARCH_TO_NETHER
            } else {
                SEARCH_TO_OVERWORLD
            },
            can_create: true,
            creation_radius: CREATION_RADIUS,
            spectator,
        };
        let destination = Destination::here(
            target,
            Vec3::new(
                f64::from(approx.x),
                f64::from(approx.y),
                f64::from(approx.z),
            ),
        );
        Some((trip.traveler, destination, exit))
    }

    pub(crate) fn travel_through_nether(
        &mut self,
        id: PlayerId,
        destination: Destination,
        exit: &NetherExit,
    ) {
        let Some(source) = self.players.get(&id).map(|player| player.dim.clone()) else {
            return;
        };
        let Some((arrival, placed)) = self.resolve_nether_arrival(&destination, exit) else {
            return;
        };
        self.travel_player_with(id, source, arrival, placed);
    }

    pub(crate) fn entity_through_nether(
        &mut self,
        id: EntityId,
        destination: Destination,
        exit: &NetherExit,
    ) {
        if destination.dimension == self.world.dimension.key {
            return;
        }
        let Some((arrival, placed)) = self.resolve_nether_arrival(&destination, exit) else {
            return;
        };
        if !placed.is_empty() && self.listened(crate::plugins::EventKind::PortalCreate) {
            let listing = placed.clone();
            let step = PlatformStep {
                traveler: PlatformTraveler::Entity(id),
                destination: arrival,
                placed,
                reason: PortalCreateReason::NetherPair,
            };
            self.ask_platform(step, listing);
            return;
        }
        self.travel_entity(id, arrival, true, &placed);
    }

    pub(crate) fn resolve_nether_arrival(
        &mut self,
        destination: &Destination,
        exit: &NetherExit,
    ) -> Option<(Destination, Vec<(BlockPos, BlockStateId)>)> {
        if !self.ensure_dimension(&destination.dimension) {
            return None;
        }
        let target = destination.dimension.clone();
        let approx = BlockPos::new(
            destination.position.x.floor() as i32,
            destination.position.y.floor() as i32,
            destination.position.z.floor() as i32,
        );
        let exit = exit.clone();
        let to_nether = target == nether();
        self.in_dimension(&target, |server| {
            server.find_or_create_exit(approx, &exit, to_nether)
        })
        .map(|(position, yaw, placed)| {
            (
                Destination::new(target, position, Some(Rotation::new(yaw, exit.pitch))),
                placed,
            )
        })
    }

    fn find_or_create_exit(
        &mut self,
        approx: BlockPos,
        exit: &NetherExit,
        to_nether: bool,
    ) -> Option<(Vec3, f32, Placed)> {
        let held = self.hold_square(approx, exit.search_radius.min(LOAD_RADIUS));
        let found = self.closest_portal(approx, exit.search_radius);
        let mut placed = Vec::new();
        let (rectangle, exit_axis) = if let Some(found) = found {
            let state = self.block_or_air(found);
            let axis = portal_axis(state).unwrap_or(Axis::X);
            let chunks = &self.world.chunks;
            let rectangle =
                largest_rectangle_around(found, axis, MAX_PORTAL_SIZE, MAX_PORTAL_SIZE, |at| {
                    chunks.block(at) == Some(state)
                });
            (rectangle, axis)
        } else if exit.can_create && !exit.spectator {
            let creation =
                self.plan_created_portal(approx, exit.entry_axis, exit.creation_radius, to_nether);
            let Some(created) = creation else {
                for pos in held {
                    self.world.chunks.remove_ticket(pos);
                }
                return None;
            };
            placed = created.changes;
            (
                Rectangle {
                    min_corner: created.exit,
                    axis1_size: 2,
                    axis2_size: 3,
                },
                exit.entry_axis,
            )
        } else {
            for pos in held {
                self.world.chunks.remove_ticket(pos);
            }
            return None;
        };
        for pos in held {
            self.world.chunks.remove_ticket(pos);
        }
        let turned = exit_axis != exit.entry_axis;
        let yaw = exit.yaw + if turned { 90.0 } else { 0.0 };
        let along = f64::from(rectangle.axis1_size);
        let free = along - exit.width;
        let x = exit.width / 2.0 + free * exit.relative.x;
        let y = (f64::from(rectangle.axis2_size) - exit.height) * exit.relative.y;
        let across = 0.5 + exit.relative.z;
        let corner = rectangle.min_corner;
        let position = if exit_axis == Axis::X {
            Vec3::new(
                f64::from(corner.x) + x,
                f64::from(corner.y) + y,
                f64::from(corner.z) + across,
            )
        } else {
            Vec3::new(
                f64::from(corner.x) + across,
                f64::from(corner.y) + y,
                f64::from(corner.z) + x,
            )
        };
        Some((position, yaw, placed))
    }

    fn closest_portal(&self, approx: BlockPos, radius: i32) -> Option<BlockPos> {
        let found = self.world.chunks.poi().in_square(
            |kind| kind == PoiType::NetherPortal,
            approx,
            radius,
            Occupancy::Any,
        );
        let distance = |pos: BlockPos| {
            let (dx, dy, dz) = (
                i64::from(pos.x - approx.x),
                i64::from(pos.y - approx.y),
                i64::from(pos.z - approx.z),
            );
            dx * dx + dy * dy + dz * dz
        };
        found
            .into_iter()
            .map(|record| record.pos())
            .filter(|pos| {
                self.world
                    .chunks
                    .block(*pos)
                    .is_some_and(|state| portal_axis(state).is_some())
            })
            .min_by_key(|pos| (distance(*pos), pos.y))
    }

    fn plan_created_portal(
        &mut self,
        approx: BlockPos,
        axis: Axis,
        radius: i32,
        to_nether: bool,
    ) -> Option<rspigot_domain::portal::CreatedPortal> {
        let held = self.hold_square(approx, radius + 2);
        let height = self.world.dimension.height;
        let top = if to_nether {
            height.max_y().min(height.min_y + NETHER_LOGICAL_HEIGHT - 1)
        } else {
            height.max_y()
        };
        let chunks = &self.world.chunks;
        let block = |at: BlockPos| chunks.block(at);
        let replaceable = |state: BlockStateId| is_replaceable(state) && !is_liquid(state);
        let solid = |state: BlockStateId| {
            !state.is_air() && !is_liquid(state) && !state.collision_shape().is_empty()
        };
        let surface = |x: i32, z: i32| {
            let mut y = height.max_y();
            while y >= height.min_y {
                let state = chunks.block(BlockPos::new(x, y, z));
                if state.is_some_and(|state| {
                    !state.is_air() && (is_liquid(state) || !is_replaceable(state))
                }) {
                    return y + 1;
                }
                y -= 1;
            }
            height.min_y
        };
        let inside = |_: i32, _: i32| true;
        let identity = |at: BlockPos| at;
        let terrain = Terrain {
            block: &block,
            replaceable: &replaceable,
            solid: &solid,
            motion_blocking_height: &surface,
            within_border: &inside,
            clamp_to_border: &identity,
            min_y: height.min_y,
            top_y: top,
        };
        let created = terrain.create_portal(approx, axis, radius);
        for pos in held {
            self.world.chunks.remove_ticket(pos);
        }
        created
    }

    pub(crate) fn restore_login_dimension(&mut self, id: PlayerId, saved: &DimensionKey) {
        if *saved == self.world.dimension.key || !self.ensure_dimension(saved) {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let destination = Destination::new(saved.clone(), player.position, Some(player.rotation));
        self.change_dimension_now(id, destination);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::{ClientEvent, Inbound};
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    #[derive(Clone)]
    struct Mute;

    impl ClientLink for Mute {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn provide(server: &mut Server<Mute>, key: &Identifier) {
        let dimension = match key.as_str() {
            "minecraft:the_nether" => Dimension::nether(),
            "minecraft:the_end" => Dimension::end(),
            _ => return,
        };
        server.load_dimension(key.clone(), dimension, Arc::new(Quiet), Arc::new(Quiet));
    }

    fn arrive(allow_nether: bool, mode: GameMode) -> (Server<Mute>, PlayerId) {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            allow_nether,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5)
        .with_dimension_provider(provide);
        server.wait_for_spawn(Duration::from_secs(10));
        let id = PlayerId::offline("Alice");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Mute,
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        if let Some(player) = server.players.get_mut(&id) {
            player.game_mode = mode;
        }
        (server, id)
    }

    const ORIGIN: BlockPos = BlockPos::new(8, 64, 8);

    fn build_frame(server: &mut Server<Mute>) {
        let obsidian = BlockKind::Obsidian.default_state();
        for x in -1..=2 {
            server.set_block(ORIGIN.offset(x, -1, 0), obsidian);
            server.set_block(ORIGIN.offset(x, 3, 0), obsidian);
        }
        for y in 0..3 {
            server.set_block(ORIGIN.offset(-1, y, 0), obsidian);
            server.set_block(ORIGIN.offset(2, y, 0), obsidian);
        }
    }

    fn portal_blocks(server: &Server<Mute>, center: BlockPos, radius: i32) -> usize {
        let mut count = 0;
        for x in -radius..=radius {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    let at = center.offset(x, y, z);
                    if server
                        .world
                        .chunks
                        .block(at)
                        .and_then(portal_axis)
                        .is_some()
                    {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn stand_in_portal(server: &mut Server<Mute>, id: PlayerId) {
        let player = server.players.get_mut(&id).expect("player");
        player.position = Vec3::new(9.0, 64.0, 8.5);
        player.portal_cooldown = 0;
    }

    fn lit() -> (Server<Mute>, PlayerId) {
        let (mut server, id) = arrive(true, GameMode::Survival);
        build_frame(&mut server);
        server.set_block(ORIGIN, BlockKind::Fire.default_state());
        server.deliver();
        (server, id)
    }

    #[test]
    fn fire_inside_an_obsidian_frame_lights_the_portal() {
        let (server, _) = lit();
        assert_eq!(portal_blocks(&server, ORIGIN.offset(0, 1, 0), 4), 6);
        let state = server
            .world
            .chunks
            .block(ORIGIN.offset(1, 2, 0))
            .expect("block");
        assert_eq!(state.kind(), Some(BlockKind::NetherPortal));
        assert_eq!(state.property("axis"), Some("x"));
    }

    #[test]
    fn nothing_lights_when_the_nether_is_disabled() {
        let (mut server, _) = arrive(false, GameMode::Survival);
        build_frame(&mut server);
        server.set_block(ORIGIN, BlockKind::Fire.default_state());
        server.deliver();
        assert_eq!(portal_blocks(&server, ORIGIN.offset(0, 1, 0), 4), 0);
    }

    #[test]
    fn breaking_a_frame_block_collapses_the_portal() {
        let (mut server, _) = lit();
        server.set_block(ORIGIN.offset(2, 1, 0), BlockStateId::AIR);
        for _ in 0..4 {
            server.tick();
            server.deliver();
        }
        assert_eq!(portal_blocks(&server, ORIGIN.offset(0, 1, 0), 4), 0);
    }

    #[test]
    fn a_survival_player_waits_eighty_ticks_then_arrives_in_a_new_nether_portal() {
        let (mut server, id) = lit();
        stand_in_portal(&mut server, id);
        for _ in 0..80 {
            server.tick();
            server.deliver();
        }
        assert_eq!(server.players[&id].dim, Identifier::minecraft("overworld"));
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, nether());
        let at = server.players[&id].position;
        assert!(
            (at.x - 1.5).abs() < 2.5 && (at.z - 1.0).abs() < 2.5,
            "{at:?}"
        );
        let found = server.in_dimension(&nether(), |nether| {
            portal_blocks(nether, BlockPos::new(1, 64, 1), 5)
        });
        assert_eq!(found, 6);
    }

    #[test]
    fn a_creative_player_travels_on_the_first_tick_with_the_default_creative_delay() {
        let (mut server, id) = arrive(true, GameMode::Creative);
        build_frame(&mut server);
        server.set_block(ORIGIN, BlockKind::Fire.default_state());
        server.deliver();
        stand_in_portal(&mut server, id);
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, nether());
    }

    #[test]
    fn the_creative_delay_rule_adds_ticks_before_a_creative_player_travels() {
        let (mut server, id) = arrive(true, GameMode::Creative);
        server.config.rules.players_nether_portal_creative_delay = 2;
        build_frame(&mut server);
        server.set_block(ORIGIN, BlockKind::Fire.default_state());
        server.deliver();
        stand_in_portal(&mut server, id);
        for _ in 0..2 {
            server.tick();
            server.deliver();
        }
        assert_eq!(server.players[&id].dim, Identifier::minecraft("overworld"));
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, nether());
    }

    #[test]
    fn the_default_delay_rule_shortens_a_survival_wait() {
        let (mut server, id) = lit();
        server.config.rules.players_nether_portal_default_delay = 3;
        stand_in_portal(&mut server, id);
        for _ in 0..3 {
            server.tick();
            server.deliver();
        }
        assert_eq!(server.players[&id].dim, Identifier::minecraft("overworld"));
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, nether());
    }

    #[test]
    fn the_nether_entry_rule_off_stops_portals_from_lighting() {
        let (mut server, _) = arrive(true, GameMode::Survival);
        server.config.rules.allow_entering_nether_using_portals = false;
        build_frame(&mut server);
        server.set_block(ORIGIN, BlockKind::Fire.default_state());
        server.deliver();
        assert_eq!(portal_blocks(&server, ORIGIN.offset(0, 1, 0), 4), 0);
    }

    #[test]
    fn the_way_back_uses_the_nether_portal_and_lands_in_the_overworld_frame() {
        let (mut server, id) = lit();
        stand_in_portal(&mut server, id);
        for _ in 0..81 {
            server.tick();
            server.deliver();
        }
        assert_eq!(server.players[&id].dim, nether());
        server.players.get_mut(&id).expect("player").portal_cooldown = 0;
        for _ in 0..82 {
            server.tick();
            server.deliver();
        }
        assert_eq!(server.players[&id].dim, Identifier::minecraft("overworld"));
        let at = server.players[&id].position;
        assert!(
            at.x > 7.5 && at.x < 11.0 && (at.z - 8.5).abs() < 1.5,
            "{at:?}"
        );
        assert_eq!(portal_blocks(&server, ORIGIN.offset(0, 1, 0), 6), 6);
    }

    #[test]
    fn the_relative_position_inside_the_portal_is_kept_through_the_rectangle() {
        let rectangle = Rectangle {
            min_corner: BlockPos::new(10, 64, 10),
            axis1_size: 2,
            axis2_size: 3,
        };
        let relative =
            relative_position(&rectangle, Axis::X, Vec3::new(11.0, 64.0, 10.5), 0.6, 1.8);
        assert!((relative.x - 0.5).abs() < 1e-9);
        assert!(relative.y.abs() < 1e-9);
        assert!(relative.z.abs() < 1e-9);
    }
}
