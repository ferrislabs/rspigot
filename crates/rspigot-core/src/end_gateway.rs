use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::dragon::fight::{GatewayBlock, gateway_blocks};
use rspigot_domain::dragon::gateway::{
    self as rules, BLOCK_ENTITY, COOLDOWN_EVENT, EXIT_HEIGHT, ISLAND_HEIGHT, ISLAND_SEARCH_RADIUS,
    Solidity,
};
use rspigot_domain::entity::Aabb;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};

use crate::chunks::Priority;
use crate::end_portal::{PORTAL_COOLDOWN, excluded};
use crate::events::ClientEvent;
use crate::plugins::events::PlayerTeleportCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const BLOCK_EVENT_RANGE: f64 = 64.0;
const SAVE_INTERVAL: i64 = 20;
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);
const INSIDE_DEFLATE: f64 = 1.0e-5;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const ISLAND_REACH: i32 = 7;
const FRAME_REACH: i32 = 2;

pub(crate) fn is_gateway(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::EndGateway)
}

fn full_collision(state: Option<BlockStateId>) -> bool {
    state.is_some_and(|state| {
        state.collision_shape().iter().any(|shape| {
            shape.min_x <= 0.0
                && shape.min_y <= 0.0
                && shape.min_z <= 0.0
                && shape.max_x >= 1.0
                && shape.max_y >= 1.0
                && shape.max_z >= 1.0
        })
    })
}

fn solidity(state: Option<BlockStateId>) -> Solidity {
    Solidity {
        full: full_collision(state),
        bedrock: state.is_some_and(|state| state.kind() == Some(BlockKind::Bedrock)),
    }
}

fn chunks_around(center: BlockPos, reach: i32) -> Vec<ChunkPos> {
    let low = center.offset(-reach, 0, -reach).chunk();
    let high = center.offset(reach, 0, reach).chunk();
    let mut out = Vec::new();
    for x in low.x..=high.x {
        for z in low.z..=high.z {
            out.push(ChunkPos::new(x, z));
        }
    }
    out
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_gateway_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_gateway(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if present {
            return;
        }
        self.entities.gateways.remove(&pos);
        let gateway = rules::Gateway::default();
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(BLOCK_ENTITY),
                data: gateway.to_nbt(),
            });
        }
        self.entities.gateways.insert(pos, gateway);
    }

    fn gateway_state(&mut self, pos: BlockPos) -> Option<rules::Gateway> {
        if let Some(gateway) = self.entities.gateways.get(&pos) {
            return Some(*gateway);
        }
        let gateway = {
            let entity = self.world.chunks.get(pos.chunk())?.block_entity(pos)?;
            rules::Gateway::from_nbt(&entity.data)
        };
        self.entities.gateways.insert(pos, gateway);
        Some(gateway)
    }

    fn gateway_persist(&mut self, pos: BlockPos, gateway: &rules::Gateway) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(pos).map_or_else(
            || (Identifier::minecraft(BLOCK_ENTITY), Default::default()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        gateway.write(&mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    pub(crate) fn gateway_set_exit(&mut self, pos: BlockPos, exit: BlockPos, exact: bool) {
        let Some(mut gateway) = self.gateway_state(pos) else {
            return;
        };
        gateway.exit = Some(exit);
        gateway.exact = exact;
        self.entities.gateways.insert(pos, gateway);
        self.gateway_persist(pos, &gateway);
    }

    #[cfg(test)]
    pub(crate) fn gateway_exit_of(&mut self, pos: BlockPos) -> Option<(BlockPos, bool)> {
        self.gateway_state(pos)
            .and_then(|gateway| gateway.exit.map(|exit| (exit, gateway.exact)))
    }

    fn gateway_cooldown_event(&mut self, pos: BlockPos) {
        self.push(
            Audience::Nearby {
                position: pos.center(),
                range: BLOCK_EVENT_RANGE,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos,
                action: COOLDOWN_EVENT,
                data: 0,
                block: BlockKind::EndGateway.id().0,
            },
        );
    }

    pub(crate) fn tick_gateways(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:end_gateway")
                    .map(|entity| entity.pos),
            );
        }
        self.entities.gateways.retain(|pos, _| found.contains(pos));
        found.sort_unstable_by_key(|pos| (pos.x, pos.y, pos.z));
        for pos in found {
            self.tick_gateway(pos);
        }
    }

    fn tick_gateway(&mut self, pos: BlockPos) {
        if !self.world.chunks.block(pos).is_some_and(is_gateway) {
            return;
        }
        let Some(mut gateway) = self.gateway_state(pos) else {
            return;
        };
        let tick = gateway.tick();
        self.entities.gateways.insert(pos, gateway);
        if tick.cooldown_started {
            self.gateway_cooldown_event(pos);
        }
        if tick.changed || gateway.age % SAVE_INTERVAL == 0 {
            self.gateway_persist(pos, &gateway);
        }
    }

    fn load_chunks_now(&mut self, chunks: &[ChunkPos]) {
        for pos in chunks {
            self.world
                .chunks
                .add_ticket(*pos, Priority::URGENT, &self.workers);
        }
        self.wait_for_spawn(LOAD_TIMEOUT);
        for pos in chunks {
            self.world.chunks.remove_ticket(*pos);
        }
    }

    fn gateway_chunk_is_empty(&mut self, pos: ChunkPos) -> bool {
        self.load_chunks_now(&[pos]);
        self.world.chunks.get(pos).is_none_or(|chunk| {
            chunk
                .sections()
                .iter()
                .all(rspigot_domain::chunk::ChunkSection::is_empty)
        })
    }

    fn gateway_highest_section(&self, pos: ChunkPos) -> i32 {
        let min_y = self.world.dimension.height.min_y;
        self.world
            .chunks
            .get(pos)
            .and_then(|chunk| {
                chunk
                    .sections()
                    .iter()
                    .rposition(|section| !section.is_empty())
            })
            .map_or(min_y, |index| min_y + index as i32 * 16)
    }

    fn height_range(&self) -> (i32, i32) {
        let height = self.world.dimension.height;
        (height.min_y, height.min_y + height.height as i32 - 1)
    }

    fn gateway_find_exit(&mut self, exit: BlockPos) -> BlockPos {
        self.load_chunks_now(&chunks_around(exit, rules::EXIT_SEARCH_RADIUS));
        let (min_y, max_y) = self.height_range();
        let chunks = &self.world.chunks;
        rules::exit_position(exit, min_y, max_y, |pos| solidity(chunks.block(pos)))
    }

    fn gateway_create_exit(&mut self, gateway: BlockPos) -> BlockPos {
        let (x, z) = rules::tentative_position(gateway, |chunk| self.gateway_chunk_is_empty(chunk));
        let chunk = ChunkPos::new(
            (x / rules::TENTATIVE_STEP).floor() as i32,
            (z / rules::TENTATIVE_STEP).floor() as i32,
        );
        self.load_chunks_now(&[chunk]);
        let highest = self.gateway_highest_section(chunk);
        let found = {
            let chunks = &self.world.chunks;
            rules::find_valid_spawn(
                chunk,
                highest,
                |pos| chunks.block(pos).and_then(BlockStateId::kind) == Some(BlockKind::EndStone),
                |pos| full_collision(chunks.block(pos)),
            )
        };
        let spot = match found {
            Some(spot) => spot,
            None => {
                let island = BlockPos::new(
                    (x + 0.5).floor() as i32,
                    ISLAND_HEIGHT,
                    (z + 0.5).floor() as i32,
                );
                self.place_end_island(island);
                island
            }
        };
        self.load_chunks_now(&chunks_around(spot, ISLAND_SEARCH_RADIUS));
        let (min_y, max_y) = self.height_range();
        let chunks = &self.world.chunks;
        let ground =
            rules::find_tallest_block(spot, ISLAND_SEARCH_RADIUS, true, min_y, max_y, |pos| {
                solidity(chunks.block(pos))
            });
        let exit = ground.offset(0, EXIT_HEIGHT, 0);
        self.place_end_gateway(exit, Some(gateway));
        exit
    }

    fn place_end_island(&mut self, origin: BlockPos) {
        self.load_chunks_now(&chunks_around(origin, ISLAND_REACH));
        let mut random = LegacyRandom::new(rules::pos_seed(origin));
        for pos in rules::island_blocks(origin, &mut random) {
            self.set_block(pos, BlockKind::EndStone.default_state());
        }
    }

    pub(crate) fn place_end_gateway(&mut self, center: BlockPos, exit: Option<BlockPos>) {
        self.load_chunks_now(&chunks_around(center, FRAME_REACH));
        for (pos, kind) in gateway_blocks(center) {
            let state = match kind {
                GatewayBlock::Gateway => BlockKind::EndGateway.default_state(),
                GatewayBlock::Bedrock => BlockKind::Bedrock.default_state(),
                GatewayBlock::Air => BlockStateId::AIR,
            };
            self.set_block(pos, state);
        }
        if let Some(exit) = exit {
            self.gateway_set_exit(center, exit, false);
        }
    }

    fn gateway_destination(&mut self, pos: BlockPos) -> Option<Vec3> {
        let mut gateway = self.gateway_state(pos)?;
        if gateway.exit.is_none() && self.world.dimension.has_dragon_fight() {
            let exit = self.gateway_create_exit(pos);
            self.gateway_set_exit(pos, exit, gateway.exact);
            gateway = self.gateway_state(pos)?;
        }
        let exit = gateway.exit?;
        let target = if gateway.exact {
            exit
        } else {
            self.gateway_find_exit(exit)
        };
        Some(Vec3::new(
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        ))
    }

    pub(crate) fn tick_gateway_travelers(&mut self) {
        let mut gateways: Vec<BlockPos> = self.entities.gateways.keys().copied().collect();
        gateways.sort_unstable_by_key(|pos| (pos.x, pos.y, pos.z));
        for pos in gateways {
            self.gateway_visit(pos);
        }
    }

    fn gateway_cube(pos: BlockPos) -> Aabb {
        Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x) + 1.0,
            f64::from(pos.y) + 1.0,
            f64::from(pos.z) + 1.0,
        )
    }

    fn gateway_player_inside(&self, id: PlayerId, cube: &Aabb) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if player.vitals.is_dead()
            || player.won_game
            || player.game_mode == GameMode::Spectator
            || self.entities.rides.is_passenger(player.entity_id)
        {
            return false;
        }
        let at = player.position;
        Aabb::new(
            at.x - PLAYER_HALF_WIDTH,
            at.y,
            at.z - PLAYER_HALF_WIDTH,
            at.x + PLAYER_HALF_WIDTH,
            at.y + player.height(),
            at.z + PLAYER_HALF_WIDTH,
        )
        .deflate(INSIDE_DEFLATE)
        .intersects(cube)
    }

    fn gateway_entity_inside(&self, id: EntityId, cube: &Aabb) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let base = &entity.base;
        !(base.is_removed()
            || excluded(base.kind)
            || !entity.is_simulated()
            || self.entities.dragons.contains_key(&id)
            || self.entities.rides.is_passenger(id)
            || !self.entity_ticking(base.chunk()))
            && base.bounding_box().deflate(INSIDE_DEFLATE).intersects(cube)
    }

    fn gateway_visit(&mut self, pos: BlockPos) {
        let cube = Self::gateway_cube(pos);
        let mut players: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|id| self.gateway_player_inside(*id, &cube))
            .collect();
        players.sort_unstable_by_key(|id| id.0);
        for id in players {
            if !self.gateway_arm(pos) {
                return;
            }
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let waiting = player.portal_cooldown > 0;
            player.portal_cooldown = PORTAL_COOLDOWN;
            if waiting {
                continue;
            }
            if let Some(destination) = self.gateway_destination(pos) {
                self.ask_teleport(id, destination, None, PlayerTeleportCause::EndGateway, None);
            }
        }
        let mut entities: Vec<EntityId> = self
            .entities
            .store
            .ids()
            .into_iter()
            .filter(|id| self.gateway_entity_inside(*id, &cube))
            .collect();
        entities.sort_unstable();
        for id in entities {
            if !self.gateway_arm(pos) {
                return;
            }
            let Some(entity) = self.entities.store.get_mut(id) else {
                continue;
            };
            let waiting = entity.base.portal_cooldown > 0;
            entity.base.portal_cooldown = PORTAL_COOLDOWN;
            if waiting {
                continue;
            }
            if let Some(destination) = self.gateway_destination(pos) {
                self.relocate_tracked(id, destination, false, None);
            }
        }
    }

    fn gateway_arm(&mut self, pos: BlockPos) -> bool {
        let Some(mut gateway) = self.gateway_state(pos) else {
            return false;
        };
        if gateway.is_cooling_down() {
            return false;
        }
        gateway.trigger_cooldown();
        self.entities.gateways.insert(pos, gateway);
        self.gateway_cooldown_event(pos);
        true
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::dragon::gateway::Gateway;
    use rspigot_entities::EntityKind as Kind;

    use super::*;
    use crate::end_tests::{Tap, arrive, end, enter_end, run_until};
    use crate::mob_actions::new_mob;

    fn ready() -> (Server<Tap>, PlayerId, Tap) {
        let (mut server, id, link) = arrive();
        enter_end(&mut server, id);
        (server, id, link)
    }

    fn place(server: &mut Server<Tap>, center: BlockPos, exit: Option<BlockPos>) {
        server.in_dimension(&end(), |server| server.place_end_gateway(center, exit));
    }

    fn exact(server: &mut Server<Tap>, center: BlockPos, exit: BlockPos) {
        server.in_dimension(&end(), |server| server.gateway_set_exit(center, exit, true));
    }

    fn put_player(server: &mut Server<Tap>, id: PlayerId, at: Vec3) {
        if let Some(player) = server.players.get_mut(&id) {
            player.position = at;
            player.portal_cooldown = 0;
        }
    }

    fn gateway(server: &Server<Tap>, pos: BlockPos) -> Option<Gateway> {
        server.dormant[&end()].entities.gateways.get(&pos).copied()
    }

    fn data_of(server: &Server<Tap>, pos: BlockPos) -> Option<rspigot_nbt::Compound> {
        server.dormant[&end()]
            .world
            .chunks
            .get(pos.chunk())?
            .block_entity(pos)
            .map(|entity| entity.data.clone())
    }

    #[test]
    fn a_gateway_block_gets_a_block_entity_that_ages_and_saves_its_age() {
        let (mut server, _, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        let data = data_of(&server, center).expect("a block entity");
        assert_eq!(data.get_i64("Age"), Some(0));
        for _ in 0..40 {
            server.tick();
        }
        assert!(gateway(&server, center).unwrap().age >= 40);
        assert!(data_of(&server, center).unwrap().get_i64("Age").unwrap() >= 20);
    }

    #[test]
    fn a_known_exit_is_saved_with_the_exact_teleport_flag() {
        let (mut server, _, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, Some(BlockPos::new(5, 80, 5)));
        let data = data_of(&server, center).unwrap();
        assert_eq!(data.get_int_array("exit_portal"), Some(&[5, 80, 5][..]));
        assert!(data.get("ExactTeleport").is_none());
        exact(&mut server, center, BlockPos::new(6, 81, 6));
        let data = data_of(&server, center).unwrap();
        assert_eq!(data.get_bool("ExactTeleport"), Some(true));
    }

    #[test]
    fn a_gateway_reads_its_exit_and_age_back_from_its_block_entity_after_a_reload() {
        let (mut server, _, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        exact(&mut server, center, BlockPos::new(6, 81, 6));
        for _ in 0..25 {
            server.tick();
        }
        server
            .dormant
            .get_mut(&end())
            .unwrap()
            .entities
            .gateways
            .clear();
        let (exit, exact) = server
            .in_dimension(&end(), |server| server.gateway_exit_of(center))
            .expect("read back");
        assert_eq!((exit, exact), (BlockPos::new(6, 81, 6), true));
        assert!(gateway(&server, center).unwrap().age >= 20);
    }

    #[test]
    fn a_player_in_the_gateway_lands_on_the_exact_exit_and_the_gateway_cools_down() {
        let (mut server, id, link) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        exact(&mut server, center, BlockPos::new(110, 80, 0));
        link.take();
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].position, Vec3::new(110.5, 80.0, 0.5));
        assert!(gateway(&server, center).unwrap().is_cooling_down());
        assert!(link.take().iter().any(|event| matches!(
            event,
            ClientEvent::BlockEvent { pos, action: 1, .. } if *pos == center
        )));
    }

    #[test]
    fn an_inexact_exit_lands_on_the_tallest_block_beside_the_exit_portal() {
        let (mut server, id, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, Some(BlockPos::new(300, 80, 0)));
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        server.deliver();
        let at = server.players[&id].position;
        assert_eq!(at, Vec3::new(295.5, 63.0, -4.5));
    }

    #[test]
    fn the_gateway_lets_nobody_through_while_it_cools_down() {
        let (mut server, id, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        exact(&mut server, center, BlockPos::new(200, 80, 0));
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        server.deliver();
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        assert_eq!(server.players[&id].position, Vec3::new(96.5, 75.0, 0.5));
        put_player(&mut server, id, Vec3::new(100.5, 70.0, 0.5));
        for _ in 0..40 {
            server.tick();
        }
        assert!(!gateway(&server, center).unwrap().is_cooling_down());
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].position, Vec3::new(200.5, 80.0, 0.5));
    }

    #[test]
    fn a_player_on_the_portal_cooldown_does_not_travel_but_still_arms_the_gateway() {
        let (mut server, id, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        exact(&mut server, center, BlockPos::new(200, 80, 0));
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.players.get_mut(&id).unwrap().portal_cooldown = 100;
        server.tick();
        assert_eq!(server.players[&id].position, Vec3::new(96.5, 75.0, 0.5));
        assert!(gateway(&server, center).unwrap().is_cooling_down());
    }

    #[test]
    fn a_mob_in_the_gateway_is_relocated_to_the_exit() {
        let (mut server, id, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        exact(&mut server, center, BlockPos::new(200, 80, 0));
        put_player(&mut server, id, Vec3::new(100.5, 70.0, 0.5));
        let pig = server.in_dimension(&end(), |server| {
            let mut base = server.new_entity_base(Kind::Pig);
            base.position = Vec3::new(96.5, 75.0, 0.5);
            server.place_entity(new_mob(base)).expect("placed")
        });
        run_until(&mut server, 40, |server| {
            server.dormant[&end()]
                .entities
                .store
                .get(pig)
                .is_some_and(|entity| entity.base.position.x > 150.0)
        });
        let position = server.dormant[&end()]
            .entities
            .store
            .get(pig)
            .map(|entity| entity.base.position)
            .expect("pig");
        assert_eq!((position.x, position.z), (200.5, 0.5));
    }

    #[test]
    fn a_gateway_without_exit_creates_a_return_gateway_far_out_and_links_both() {
        let (mut server, id, _) = ready();
        let center = BlockPos::new(96, 75, 0);
        place(&mut server, center, None);
        put_player(&mut server, id, Vec3::new(96.5, 75.0, 0.5));
        server.tick();
        server.deliver();
        let (exit, exact) = server
            .in_dimension(&end(), |server| server.gateway_exit_of(center))
            .expect("an exit was created");
        assert!(!exact);
        assert!(exit.x > 500);
        let back = server
            .in_dimension(&end(), |server| server.gateway_exit_of(exit))
            .expect("the return gateway leads back");
        assert_eq!(back, (center, false));
        let data = data_of(&server, exit).expect("a block entity on the return gateway");
        assert_eq!(
            data.get_int_array("exit_portal"),
            Some(&[center.x, center.y, center.z][..])
        );
        let at = server.players[&id].position;
        assert_eq!(at.y, 63.0);
        assert!((at.x - f64::from(exit.x)).abs() <= 5.5);
    }

    #[test]
    fn gateways_do_not_exist_in_the_overworld_and_leave_nothing_to_tick() {
        let (mut server, _, _) = arrive();
        server.tick();
        assert!(server.entities.gateways.is_empty());
    }
}
