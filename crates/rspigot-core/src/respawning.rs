use rspigot_blocks::StateFlags;
use rspigot_domain::bed::{self, RespawnConfig};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockStateId, Rotation, Vec3};

use crate::ports::ClientLink;
use crate::server::Server;
use crate::world_state::DimensionKey;

const FORCED_RESPAWN_LIFT: f64 = 0.1;

#[derive(Debug, Clone)]
pub(crate) struct RespawnSpot {
    pub(crate) position: Vec3,
    pub(crate) rotation: Rotation,
    pub(crate) dimension: DimensionKey,
    pub(crate) bed: bool,
    pub(crate) anchor: bool,
    pub(crate) missing: bool,
}

fn possible_to_respawn_in(state: BlockStateId) -> bool {
    let flags = state.flags();
    !flags.contains(StateFlags::LEGACY_SOLID) && !flags.has_fluid()
}

impl<L: ClientLink> Server<L> {
    fn find_respawn_here(
        &mut self,
        config: &RespawnConfig,
    ) -> Option<(Vec3, Rotation, bool, bool)> {
        self.load_chunks_now(&[config.pos.chunk()]);
        let state = self.block_or_air(config.pos);
        if let Some(kind) = bed::bed_of(state)
            && let Some(facing) = bed::facing_of(state)
        {
            let rule = bed::rule_of(
                &self.world.dimension.dimension_type,
                kind == rspigot_blocks::BlockKind::StrawBed,
            );
            if rule.can_set_spawn.test(self.dark_outside()) {
                return bed::find_stand_up(&self.world.chunks, config.pos, facing, config.yaw).map(
                    |spot| {
                        (
                            spot,
                            Rotation::new(bed::look_at_yaw(spot, config.pos), 0.0),
                            true,
                            false,
                        )
                    },
                );
            }
        }
        if state.kind() == Some(rspigot_blocks::BlockKind::RespawnAnchor)
            && (config.forced || crate::signal::level(state, "charges") > 0)
            && self.respawn_anchor_works()
        {
            let spot = self.anchor_stand_up(config.pos);
            if spot.is_some() {
                self.use_anchor_charge(config.pos, state);
            }
            return spot.map(|spot| {
                (
                    spot,
                    Rotation::new(bed::look_at_yaw(spot, config.pos), 0.0),
                    false,
                    true,
                )
            });
        }
        if !config.forced {
            return None;
        }
        let above = self.block_or_air(config.pos.offset(0, 1, 0));
        if !(possible_to_respawn_in(state) && possible_to_respawn_in(above)) {
            return None;
        }
        let spot = Vec3::new(
            f64::from(config.pos.x) + 0.5,
            f64::from(config.pos.y) + FORCED_RESPAWN_LIFT,
            f64::from(config.pos.z) + 0.5,
        );
        Some((spot, Rotation::new(config.yaw, config.pitch), false, false))
    }

    pub(crate) fn respawn_spot(&mut self, id: PlayerId) -> RespawnSpot {
        let current = self.world.dimension.key.clone();
        let suggestion = self.world.spawn.block();
        let default = RespawnSpot {
            position: self.find_spawn(suggestion),
            rotation: Rotation::new(self.world.level.spawn_angle, 0.0),
            dimension: current.clone(),
            bed: false,
            anchor: false,
            missing: false,
        };
        let Some(config) = self.players.get(&id).and_then(|p| p.respawn.clone()) else {
            return default;
        };
        let found = if config.dimension == current {
            self.find_respawn_here(&config)
        } else if self.dormant.contains_key(&config.dimension) {
            self.in_dimension(&config.dimension.clone(), |server| {
                server.find_respawn_here(&config)
            })
        } else {
            return default;
        };
        match found {
            Some((position, rotation, bed, anchor)) => RespawnSpot {
                position,
                rotation,
                dimension: config.dimension,
                bed,
                anchor,
                missing: false,
            },
            None => RespawnSpot {
                missing: true,
                ..default
            },
        }
    }
}
