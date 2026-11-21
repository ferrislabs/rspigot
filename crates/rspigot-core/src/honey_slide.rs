use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::honey_slide::{self, EFFECT_ODDS, PARTICLE_EVENT, SLIDE_SOUND};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::SoundSource;

use crate::advancements::AdvancementEvent;
use crate::events::{ClientEvent, EntityStatus};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const HALF_WIDTH: f64 = 0.3;
const HEIGHT: f64 = 1.8;
const EDGE: f64 = 1.0e-5;

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_honey_slide(&mut self, id: PlayerId, velocity_y: f64, on_ground: bool) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if on_ground || velocity_y >= honey_slide::MIN_FALL_SPEED || player.vitals.is_dead() {
            return;
        }
        let at = player.position;
        let entity = player.entity_id;
        let velocity = Vec3::new(0.0, velocity_y, 0.0);
        let sliding = self.honey_blocks_around(at).into_iter().find(|pos| {
            honey_slide::is_sliding_down(*pos, at, HALF_WIDTH * 2.0, on_ground, velocity)
        });
        let Some(pos) = sliding else {
            return;
        };
        if let Some(player) = self.players.get_mut(&id) {
            player.vitals.fall_distance = 0.0;
        }
        if honey_slide::achievement_due(self.current_tick()) {
            let mut event = AdvancementEvent::named("minecraft:slide_down_block");
            event.at = Some(pos);
            self.trigger_event(id, event);
        }
        if self.rng.random_range(0..EFFECT_ODDS) == 0 {
            self.play_entity_sound(entity, SLIDE_SOUND, SoundSource::Blocks, 1.0, 1.0);
        }
        if self.rng.random_range(0..EFFECT_ODDS) == 0 {
            self.push(
                Audience::One(id),
                ClientEvent::EntityStatus {
                    entity,
                    status: EntityStatus::Event(PARTICLE_EVENT),
                },
            );
            self.push(
                Audience::Trackers(entity),
                ClientEvent::EntityStatus {
                    entity,
                    status: EntityStatus::Event(PARTICLE_EVENT),
                },
            );
        }
    }

    fn honey_blocks_around(&self, at: Vec3) -> Vec<BlockPos> {
        let low = |value: f64| (value - HALF_WIDTH + EDGE).floor() as i32;
        let high = |value: f64| (value + HALF_WIDTH - EDGE).floor() as i32;
        let mut found = Vec::new();
        for x in low(at.x)..=high(at.x) {
            for y in at.y.floor() as i32..=((at.y + HEIGHT - EDGE).floor() as i32) {
                for z in low(at.z)..=high(at.z) {
                    let pos = BlockPos::new(x, y, z);
                    if self
                        .world
                        .chunks
                        .block(pos)
                        .is_some_and(|state| state.kind() == Some(BlockKind::HoneyBlock))
                    {
                        found.push(pos);
                    }
                }
            }
        }
        found
    }
}
