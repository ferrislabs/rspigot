use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::chunks::Priority;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const END_SPAWN_POINT: BlockPos = BlockPos::new(100, 50, 0);
pub(crate) const END_SPAWN_YAW: f32 = 90.0;
const PLATFORM_RADIUS: i32 = 2;
const PLATFORM_AIR_HEIGHT: i32 = 2;
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);

impl<L: ClientLink> Server<L> {
    pub(crate) fn make_end_platform(&mut self) {
        let origin = END_SPAWN_POINT.offset(0, -1, 0);
        let corners = [
            origin.offset(-PLATFORM_RADIUS, 0, -PLATFORM_RADIUS).chunk(),
            origin.offset(PLATFORM_RADIUS, 0, PLATFORM_RADIUS).chunk(),
        ];
        let mut held = Vec::new();
        for x in corners[0].x..=corners[1].x {
            for z in corners[0].z..=corners[1].z {
                let pos = rspigot_domain::ChunkPos::new(x, z);
                self.world
                    .chunks
                    .add_ticket(pos, Priority::URGENT, &self.workers);
                held.push(pos);
            }
        }
        self.wait_for_spawn(LOAD_TIMEOUT);
        let obsidian = BlockKind::Obsidian.default_state();
        for dx in -PLATFORM_RADIUS..=PLATFORM_RADIUS {
            for dz in -PLATFORM_RADIUS..=PLATFORM_RADIUS {
                for dy in -1..=PLATFORM_AIR_HEIGHT {
                    let wanted = if dy == -1 {
                        obsidian
                    } else {
                        BlockStateId::AIR
                    };
                    let pos = origin.offset(dx, dy, dz);
                    if self.world.chunks.block(pos) != Some(wanted) {
                        self.destroy_block_dropping(pos, None);
                        self.set_block(pos, wanted);
                    }
                }
            }
        }
        for pos in held {
            self.world.chunks.remove_ticket(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{Identifier, Vec3};

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::{ClientEvent, Inbound};
    use crate::plugins::events::PlayerTeleportCause;
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

    fn end() -> Identifier {
        Identifier::minecraft("the_end")
    }

    fn provide(server: &mut Server<Mute>, key: &Identifier) {
        if *key == end() {
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

    fn block(server: &Server<Mute>, x: i32, y: i32, z: i32) -> Option<BlockStateId> {
        server.dormant[&end()]
            .world
            .chunks
            .block(BlockPos::new(x, y, z))
    }

    #[test]
    fn an_end_portal_traveller_lands_on_a_fresh_obsidian_platform() {
        let (mut server, id) = arrive();
        server.change_dimension_now(id, crate::travel::Destination::end_spawn());
        server.deliver();
        let player = &server.players[&id];
        assert_eq!(player.dim, end());
        assert_eq!(player.position, Vec3::new(100.5, 50.0, 0.5));
        assert_eq!(player.rotation.yaw, 90.0);
        let obsidian = BlockKind::Obsidian.default_state();
        for dx in -2..=2 {
            for dz in -2..=2 {
                assert_eq!(block(&server, 100 + dx, 48, dz), Some(obsidian));
                for y in 49..=51 {
                    assert_eq!(block(&server, 100 + dx, y, dz), Some(BlockStateId::AIR));
                }
            }
        }
        assert_ne!(block(&server, 103, 48, 0), Some(obsidian));
    }

    #[test]
    fn change_dimension_by_end_portal_goes_to_the_spawn_whatever_the_position_asked() {
        let (mut server, id) = arrive();
        server.change_dimension(
            id,
            end(),
            Vec3::new(8.5, 70.0, 8.5),
            None,
            PlayerTeleportCause::EndPortal,
        );
        server.deliver();
        assert_eq!(server.players[&id].dim, end());
        assert_eq!(server.players[&id].position, Vec3::new(100.5, 50.0, 0.5));
        assert_eq!(
            block(&server, 100, 48, 0),
            Some(BlockKind::Obsidian.default_state())
        );
    }

    #[test]
    fn other_causes_keep_the_requested_position_and_build_nothing() {
        let (mut server, id) = arrive();
        server.change_dimension(
            id,
            end(),
            Vec3::new(8.5, 70.0, 8.5),
            None,
            PlayerTeleportCause::Plugin,
        );
        server.deliver();
        assert_eq!(server.players[&id].position, Vec3::new(8.5, 70.0, 8.5));
        assert_ne!(
            block(&server, 100, 48, 0),
            Some(BlockKind::Obsidian.default_state())
        );
    }
}
