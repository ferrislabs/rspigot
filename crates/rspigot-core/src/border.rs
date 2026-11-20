use std::sync::Arc;

use rspigot_domain::damage::DamageType;
use rspigot_domain::player::PlayerId;
use rspigot_domain::world_border::{BorderView, WorldBorder};
use rspigot_nbt::Compound;

use crate::chunks::{ChunkJob, Priority};
use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const BORDER_DATA: &str = "world_border";

impl<L: ClientLink> Server<L> {
    pub(crate) fn border_event(&self) -> ClientEvent {
        ClientEvent::Border(self.world.border.view(self.world.level.game_time))
    }

    pub(crate) fn broadcast_border_change(&mut self, before: &BorderView, after: &BorderView) {
        let mut events = Vec::new();
        if (before.center_x, before.center_z) != (after.center_x, after.center_z) {
            events.push(ClientEvent::BorderCenter {
                x: after.center_x,
                z: after.center_z,
            });
        }
        if (before.old_size, before.new_size, before.lerp_time)
            != (after.old_size, after.new_size, after.lerp_time)
        {
            events.push(if after.lerp_time > 0 {
                ClientEvent::BorderLerp {
                    old_size: after.old_size,
                    new_size: after.new_size,
                    time: after.lerp_time,
                }
            } else {
                ClientEvent::BorderSize(after.new_size)
            });
        }
        if before.warning_time != after.warning_time {
            events.push(ClientEvent::BorderWarningDelay(after.warning_time));
        }
        if before.warning_blocks != after.warning_blocks {
            events.push(ClientEvent::BorderWarningDistance(after.warning_blocks));
        }
        for event in events {
            self.push(Audience::Dimension(self.world.dimension.key.clone()), event);
        }
    }

    pub(crate) fn load_world_border(&mut self) {
        let Some(nbt) = self.world.level.world_border.take() else {
            return;
        };
        self.world.border = WorldBorder::from_nbt(&nbt, self.world.level.game_time);
        self.world.level.world_border = Some(nbt);
    }

    pub(crate) fn save_world_border(&mut self) {
        let nbt: Compound = self.world.border.to_nbt(self.world.level.game_time);
        self.world.level.world_border = Some(nbt);
    }

    pub(crate) fn save_world_border_data(&mut self) {
        self.save_world_border();
        if let Some(data) = &self.world.level.world_border {
            self.workers.submit(
                Priority::BACKGROUND,
                ChunkJob::SaveData {
                    dimension: Arc::clone(&self.world.dimension),
                    name: BORDER_DATA,
                    data: Arc::new(data.clone()),
                },
            );
        }
    }

    pub(crate) fn tick_border(&mut self) {
        let now = self.world.level.game_time;
        self.world.border.tick(now);
        let damage_per_block = self.world.border.damage_per_block();
        if damage_per_block <= 0.0 {
            return;
        }
        let safe_zone = self.world.border.safe_zone();
        let hurt: Vec<(PlayerId, f32)> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter_map(|player| {
                let outside =
                    self.world
                        .border
                        .distance_to_border(player.position.x, player.position.z, now)
                        + safe_zone;
                (outside < 0.0).then(|| {
                    let amount = (-outside * damage_per_block).floor().max(1.0);
                    (player.id, amount as f32)
                })
            })
            .collect();
        for (id, amount) in hurt {
            self.hurt(id, &DamageSource::new(DamageType::OutsideBorder), amount);
        }
    }
}
