use rspigot_blocks::BlockKind;
use rspigot_domain::entity::Aabb;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

use crate::events::{ClientEvent, Hand};
use crate::plugins::events::PlayerTeleportCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::travel::Destination;
use crate::world_state::DimensionKey;

pub(crate) const PORTAL_COOLDOWN: i32 = 300;
const PORTAL_BLOCK_ENTITY: &str = "end_portal";
const EYE_PLACED_EVENT: i32 = 1503;
const PORTAL_OPENED_EVENT: i32 = 1038;
const SHAPE_MIN_Y: f64 = 0.375;
const SHAPE_MAX_Y: f64 = 0.75;
const INSIDE_DEFLATE: f64 = 9.999_999_747_378_752e-6;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const RING: [(i32, i32, i32, i32, &str); 4] = [
    (0, -2, 1, 0, "south"),
    (0, 2, 1, 0, "north"),
    (-2, 0, 0, 1, "east"),
    (2, 0, 0, 1, "west"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Traveler {
    Player(PlayerId),
    Entity(EntityId),
}

fn overworld() -> DimensionKey {
    Identifier::minecraft("overworld")
}

fn the_end() -> DimensionKey {
    Identifier::minecraft("the_end")
}

fn is_frame_with_eye(state: Option<BlockStateId>, facing: &str) -> bool {
    state.is_some_and(|state| {
        state.kind() == Some(BlockKind::EndPortalFrame)
            && state.property("eye") == Some("true")
            && state.property("facing") == Some(facing)
    })
}

pub(crate) fn find_portal(
    block: impl Fn(BlockPos) -> Option<BlockStateId>,
    near: BlockPos,
) -> Option<BlockPos> {
    for dx in -2..=2 {
        for dz in -2..=2 {
            let center = near.offset(dx, 0, dz);
            let complete = RING.iter().all(|&(ox, oz, sx, sz, facing)| {
                (-1..=1).all(|i| {
                    let at = center.offset(ox + sx * i, 0, oz + sz * i);
                    is_frame_with_eye(block(at), facing)
                })
            });
            if complete {
                return Some(center);
            }
        }
    }
    None
}

fn portal_shape(pos: BlockPos) -> Aabb {
    Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y) + SHAPE_MIN_Y,
        f64::from(pos.z),
        f64::from(pos.x) + 1.0,
        f64::from(pos.y) + SHAPE_MAX_Y,
        f64::from(pos.z) + 1.0,
    )
}

fn excluded(kind: Kind) -> bool {
    matches!(kind, Kind::EnderDragon | Kind::Wither | Kind::EndCrystal)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_portal_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.kind() != Some(BlockKind::EndPortal) {
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
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(rspigot_domain::chunk::BlockEntity {
                pos,
                kind: Identifier::minecraft(PORTAL_BLOCK_ENTITY),
                data: rspigot_nbt::Compound::new(),
            });
        }
    }

    pub(crate) fn use_ender_eye(
        &mut self,
        id: PlayerId,
        hand: Hand,
        slot: usize,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        if clicked.kind() != Some(BlockKind::EndPortalFrame)
            || clicked.property("eye") != Some("false")
        {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let holds = player
            .inventory
            .get(slot)
            .is_some_and(|stack| stack.item == ItemKind::EnderEye.id());
        if !holds || player.game_mode == GameMode::Spectator {
            return false;
        }
        if !self.within_reach(id, pos) || self.is_protected(id, pos) {
            return false;
        }
        if !self.fill_frame(pos, clicked) {
            return false;
        }
        self.consume_held(id, hand, 1);
        true
    }

    pub(crate) fn fill_frame(&mut self, pos: BlockPos, frame: BlockStateId) -> bool {
        let Some(filled) = frame.with_property("eye", "true") else {
            return false;
        };
        self.set_block(pos, filled);
        self.level_event(EYE_PLACED_EVENT, pos, 0);
        let chunks = &self.world.chunks;
        let Some(center) = find_portal(|at| chunks.block(at), pos) else {
            return true;
        };
        let portal = BlockKind::EndPortal.default_state();
        for dx in -1..=1 {
            for dz in -1..=1 {
                let at = center.offset(dx, 0, dz);
                self.set_block(at, portal);
                self.ensure_portal_entity(at, portal);
            }
        }
        let key = self.world.dimension.key.clone();
        self.push(
            Audience::Dimension(key),
            ClientEvent::GlobalLevelEvent {
                event: PORTAL_OPENED_EVENT,
                pos: center,
                data: 0,
            },
        );
        true
    }

    fn touches_end_portal(&self, bounds: &Aabb) -> bool {
        let bounds = bounds.deflate(INSIDE_DEFLATE);
        let floor = |value: f64| value.floor() as i32;
        for x in floor(bounds.min_x)..=floor(bounds.max_x) {
            for y in floor(bounds.min_y)..=floor(bounds.max_y) {
                for z in floor(bounds.min_z)..=floor(bounds.max_z) {
                    let pos = BlockPos::new(x, y, z);
                    let portal = self
                        .world
                        .chunks
                        .block(pos)
                        .is_some_and(|state| state.kind() == Some(BlockKind::EndPortal));
                    if portal && portal_shape(pos).intersects(&bounds) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn tick_portals(&mut self) {
        let key = self.world.dimension.key.clone();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in players {
            self.tick_player_portal(id, &key);
        }
        for id in self.entities.store.ids() {
            self.tick_entity_portal(id, &key);
        }
    }

    fn tick_player_portal(&mut self, id: PlayerId, key: &DimensionKey) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let cooldown = (player.portal_cooldown - 1).max(0);
        let alive = !player.vitals.is_dead() && player.game_mode != GameMode::Spectator;
        let riding = self.entities.rides.is_passenger(player.entity_id);
        let at = player.position;
        let height = player.height();
        let bounds = Aabb::new(
            at.x - PLAYER_HALF_WIDTH,
            at.y,
            at.z - PLAYER_HALF_WIDTH,
            at.x + PLAYER_HALF_WIDTH,
            at.y + height,
            at.z + PLAYER_HALF_WIDTH,
        );
        let inside = alive && !riding && self.touches_end_portal(&bounds);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.portal_cooldown = cooldown;
        if !inside {
            return;
        }
        if cooldown > 0 {
            player.portal_cooldown = PORTAL_COOLDOWN;
            return;
        }
        player.portal_cooldown = PORTAL_COOLDOWN;
        self.portal_queue.push((key.clone(), Traveler::Player(id)));
    }

    fn tick_entity_portal(&mut self, id: EntityId, key: &DimensionKey) {
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
        let cooldown = (base.portal_cooldown - 1).max(0);
        let inside = self.touches_end_portal(&base.bounding_box());
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        entity.base.portal_cooldown = cooldown;
        if !inside {
            return;
        }
        if cooldown > 0 {
            entity.base.portal_cooldown = PORTAL_COOLDOWN;
            return;
        }
        entity.base.portal_cooldown = PORTAL_COOLDOWN;
        self.portal_queue.push((key.clone(), Traveler::Entity(id)));
    }

    fn overworld_spawn(&self) -> Option<Vec3> {
        let key = overworld();
        if self.world.dimension.key == key {
            Some(self.world.spawn)
        } else {
            self.dormant.get(&key).map(|state| state.world.spawn)
        }
    }

    pub(crate) fn run_portal_queue(&mut self) {
        for (source, traveler) in std::mem::take(&mut self.portal_queue) {
            let destination = if source == the_end() {
                let Some(spawn) = self.overworld_spawn() else {
                    continue;
                };
                Destination {
                    dimension: overworld(),
                    position: spawn,
                    rotation: None,
                    platform: false,
                }
            } else {
                Destination::end_spawn()
            };
            match traveler {
                Traveler::Player(id) => self.change_dimension(
                    id,
                    destination.dimension,
                    destination.position,
                    destination.rotation,
                    PlayerTeleportCause::EndPortal,
                ),
                Traveler::Entity(id) => {
                    self.in_dimension(&source, |server| {
                        server.ask_entity_portal(id, destination);
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_domain::item::ItemStack;
    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::Inbound;
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
        if *key == the_end() {
            server.load_dimension(
                key.clone(),
                Dimension::end(),
                Arc::new(Quiet),
                Arc::new(Quiet),
            );
        }
    }

    fn arrive() -> (Server<Mute>, PlayerId) {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
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
        (server, id)
    }

    fn frame(facing: &str, eye: bool) -> BlockStateId {
        BlockKind::EndPortalFrame
            .default_state()
            .with_property("facing", facing)
            .and_then(|state| state.with_property("eye", if eye { "true" } else { "false" }))
            .unwrap()
    }

    fn ring(server: &mut Server<Mute>, center: BlockPos, eyes: bool) {
        for i in -1..=1 {
            server.set_block(center.offset(i, 0, -2), frame("south", eyes));
            server.set_block(center.offset(i, 0, 2), frame("north", eyes));
            server.set_block(center.offset(-2, 0, i), frame("east", eyes));
            server.set_block(center.offset(2, 0, i), frame("west", eyes));
        }
    }

    fn center() -> BlockPos {
        BlockPos::new(8, 64, 8)
    }

    fn block(server: &Server<Mute>, pos: BlockPos) -> Option<BlockStateId> {
        server.world.chunks.block(pos)
    }

    fn is_open(server: &Server<Mute>) -> bool {
        (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                block(server, center().offset(dx, 0, dz))
                    == Some(BlockKind::EndPortal.default_state())
            })
        })
    }

    fn open(server: &mut Server<Mute>) {
        ring(server, center(), true);
        server.fill_frame(center().offset(0, 0, -2), frame("south", false));
    }

    fn walk_in(server: &mut Server<Mute>, id: PlayerId) {
        server.players.get_mut(&id).unwrap().position = Vec3::new(8.5, 64.0, 8.5);
    }

    #[test]
    fn the_twelfth_eye_opens_the_three_by_three_interior() {
        let (mut server, _) = arrive();
        ring(&mut server, center(), true);
        server.set_block(center().offset(1, 0, 2), frame("north", false));
        assert!(server.fill_frame(center().offset(1, 0, 2), frame("north", false)));
        assert!(is_open(&server));
        assert_eq!(
            block(&server, center().offset(1, 0, 2)),
            Some(frame("north", true))
        );
        let entity = server
            .world
            .chunks
            .get(center().chunk())
            .and_then(|chunk| chunk.block_entity(center()))
            .map(|entity| entity.kind.clone());
        assert_eq!(entity, Some(Identifier::minecraft("end_portal")));
    }

    #[test]
    fn an_incomplete_ring_stays_closed() {
        let (mut server, _) = arrive();
        ring(&mut server, center(), true);
        server.set_block(center().offset(-1, 0, -2), frame("south", false));
        server.set_block(center().offset(1, 0, 2), frame("north", false));
        assert!(server.fill_frame(center().offset(1, 0, 2), frame("north", false)));
        assert_eq!(
            block(&server, center()),
            Some(BlockStateId::AIR),
            "one frame still has no eye"
        );
        assert_eq!(
            block(&server, center().offset(1, 0, 2)),
            Some(frame("north", true))
        );
    }

    #[test]
    fn frames_facing_away_do_not_form_a_portal() {
        let (mut server, _) = arrive();
        ring(&mut server, center(), true);
        server.set_block(center().offset(0, 0, -2), frame("north", true));
        server.set_block(center().offset(1, 0, 2), frame("north", false));
        server.fill_frame(center().offset(1, 0, 2), frame("north", false));
        assert!(!is_open(&server));
    }

    #[test]
    fn breaking_a_frame_leaves_the_portal_open() {
        let (mut server, _) = arrive();
        open(&mut server);
        server.set_block(center().offset(0, 0, -2), BlockStateId::AIR);
        server.deliver();
        assert!(is_open(&server));
    }

    #[test]
    fn an_ender_eye_in_hand_fills_a_frame_and_is_consumed_out_of_creative() {
        let (mut server, id) = arrive();
        ring(&mut server, center(), true);
        let target = center().offset(0, 0, 2);
        server.set_block(target, frame("north", false));
        server.players.get_mut(&id).unwrap().position = Vec3::new(8.5, 64.0, 5.5);
        let held = ItemStack::new(
            ItemKind::EnderEye.id(),
            std::num::NonZeroU8::new(2).unwrap(),
        );
        let slot = rspigot_domain::item::HOTBAR_START;
        let player = server.players.get_mut(&id).unwrap();
        player.game_mode = GameMode::Survival;
        player.inventory.set(slot, Some(held));
        let clicked = block(&server, target).unwrap();
        assert!(server.use_ender_eye(id, Hand::Main, slot, target, clicked));
        assert!(is_open(&server));
        assert_eq!(
            server.players[&id]
                .inventory
                .get(slot)
                .map(|s| s.count.get()),
            Some(1)
        );
        let again = block(&server, target).unwrap();
        assert!(!server.use_ender_eye(id, Hand::Main, slot, target, again));
    }

    #[test]
    fn a_player_in_the_portal_goes_to_the_end_then_back_to_the_world_spawn() {
        let (mut server, id) = arrive();
        open(&mut server);
        walk_in(&mut server, id);
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, the_end());
        assert_eq!(server.players[&id].position, Vec3::new(100.5, 50.0, 0.5));
        assert_eq!(server.players[&id].portal_cooldown, PORTAL_COOLDOWN);

        let exit = BlockPos::new(100, 70, 0);
        let end = the_end();
        server.in_dimension(&end, |server| {
            server.set_block(exit, BlockKind::EndPortal.default_state());
        });
        server.players.get_mut(&id).unwrap().position = Vec3::new(100.5, 70.0, 0.5);
        server.players.get_mut(&id).unwrap().portal_cooldown = 1;
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, overworld());
        assert_eq!(server.players[&id].position, server.world.spawn);
    }

    #[test]
    fn standing_in_the_portal_keeps_the_cooldown_up_so_the_player_does_not_bounce() {
        let (mut server, id) = arrive();
        open(&mut server);
        walk_in(&mut server, id);
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, the_end());
        let end = the_end();
        server.in_dimension(&end, |server| {
            server.set_block(
                BlockPos::new(100, 50, 0),
                BlockKind::EndPortal.default_state(),
            );
        });
        for _ in 0..5 {
            server.tick();
        }
        server.deliver();
        assert_eq!(server.players[&id].dim, the_end());
        assert_eq!(server.players[&id].portal_cooldown, PORTAL_COOLDOWN);
    }

    #[test]
    fn a_player_above_the_thin_shape_does_not_travel() {
        let (mut server, id) = arrive();
        open(&mut server);
        server.players.get_mut(&id).unwrap().position = Vec3::new(8.5, 64.8, 8.5);
        server.tick();
        server.deliver();
        assert_eq!(server.players[&id].dim, overworld());
    }

    #[test]
    fn an_item_in_the_portal_travels_to_the_end_with_its_data() {
        let (mut server, _) = arrive();
        open(&mut server);
        server.spawn_item_entity(
            Vec3::new(8.5, 64.4, 8.5),
            Vec3::ZERO,
            0,
            ItemStack::new(ItemKind::Diamond.id(), std::num::NonZeroU8::new(3).unwrap()),
        );
        assert_eq!(server.entities_of(Kind::Item).len(), 1);
        server.tick();
        server.deliver();
        assert!(server.entities_of(Kind::Item).is_empty());
        let end = the_end();
        let arrived = server.in_dimension(&end, |server| server.entities_of(Kind::Item));
        assert_eq!(arrived.len(), 1);
    }
}
