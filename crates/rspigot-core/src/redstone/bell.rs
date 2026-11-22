use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::effect::{EffectDuration, EffectInstance, EffectKind};
use rspigot_domain::entity::{Aabb, living};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::{EntityKind, SoundSource};

use super::{face_code, facing};
use crate::events::ClientEvent;
use crate::plugins::events::{BellResonate, BellRing, EntityPotionEffectCause};
use crate::plugins::{Continuation, EventKind, EventPayload, Lane, face_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::signal::flag;

const RING_SOUND: &str = "minecraft:block.bell.use";
const RESONATE_SOUND: &str = "minecraft:block.bell.resonate";
const RING_VOLUME: f32 = 2.0;
const RING_EVENT: u8 = 1;
const RING_TICKS: i32 = 50;
const RESONATE_FROM: i32 = 5;
const RESONATION_TICKS: i32 = 40;
const SCAN_RANGE: f64 = 48.0;
const RAIDER_RANGE: f64 = 32.0;
const GLOW_TICKS: u32 = 60;
const HIT_HEIGHT: f64 = 0.8124;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Ringing {
    ticks: i32,
    resonation: i32,
    resonating: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BellStep {
    pub(crate) pos: BlockPos,
    pub(crate) entity: Option<EntityId>,
    pub(crate) direction: Face,
}

fn axis_of(face: Face) -> u8 {
    match face {
        Face::West | Face::East => 0,
        Face::Down | Face::Up => 1,
        Face::North | Face::South => 2,
    }
}

fn is_raider(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Pillager
            | EntityKind::Vindicator
            | EntityKind::Ravager
            | EntityKind::Evoker
            | EntityKind::Witch
            | EntityKind::Illusioner
    )
}

fn proper_hit(state: BlockStateId, clicked: Face, height: f64) -> bool {
    if matches!(clicked, Face::Up | Face::Down) || height > HIT_HEIGHT {
        return false;
    }
    let Some(own) = facing(state) else {
        return false;
    };
    match state.property("attachment") {
        Some("floor") => axis_of(own) == axis_of(clicked),
        Some("single_wall" | "double_wall") => axis_of(own) != axis_of(clicked),
        Some("ceiling") => true,
        _ => false,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn bell_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let powered = self.has_neighbor_signal(pos);
        if powered == flag(state, "powered") {
            return;
        }
        if powered {
            let direction = facing(state).unwrap_or(Face::North);
            self.ring_bell(pos, None, direction);
        }
        if let Some(next) = state.with_property("powered", if powered { "true" } else { "false" }) {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn use_bell(
        &mut self,
        id: rspigot_domain::player::PlayerId,
        pos: BlockPos,
        state: BlockStateId,
        clicked: Face,
        height: f64,
    ) -> bool {
        if state.kind() != Some(BlockKind::Bell) {
            return false;
        }
        if proper_hit(state, clicked, height)
            && let Some(entity) = self.players.get(&id).map(|player| player.entity_id)
        {
            self.ring_bell(pos, Some(entity), clicked);
        }
        true
    }

    pub(crate) fn bell_hit_by_projectile(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        clicked: Face,
        hit_y: f64,
    ) {
        if proper_hit(state, clicked, hit_y - f64::from(pos.y)) {
            self.ring_bell(pos, None, clicked);
        }
    }

    pub(crate) fn ring_bell(&mut self, pos: BlockPos, entity: Option<EntityId>, direction: Face) {
        let step = BellStep {
            pos,
            entity,
            direction,
        };
        self.fire(Lane::Block(pos), Continuation::Bell(step), move |server| {
            Some(EventPayload::BellRing(BellRing {
                block: pos,
                direction: face_of(direction),
                entity: entity.and_then(|id| server.subject_view(id)),
            }))
        });
    }

    pub(crate) fn finish_bell(&mut self, bell: BellStep, cancelled: bool) {
        if cancelled || self.block_or_air(bell.pos).kind() != Some(BlockKind::Bell) {
            return;
        }
        let ringing = self.falls.redstone.bells.entry(bell.pos).or_default();
        ringing.ticks = 0;
        self.count_bell_ringer(bell.entity);
        self.push(
            Audience::Nearby {
                position: bell.pos.center(),
                range: 64.0,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos: bell.pos,
                action: RING_EVENT,
                data: face_code(bell.direction),
                block: BlockKind::Bell.id().0,
            },
        );
        self.play_sound(
            None,
            bell.pos.center(),
            RING_SOUND,
            SoundSource::Blocks,
            RING_VOLUME,
            1.0,
        );
    }

    fn raiders_near(&self, pos: BlockPos) -> Vec<EntityId> {
        let center = pos.center();
        let area = Aabb::new(
            center.x - SCAN_RANGE,
            center.y - SCAN_RANGE,
            center.z - SCAN_RANGE,
            center.x + SCAN_RANGE,
            center.y + SCAN_RANGE,
            center.z + SCAN_RANGE,
        );
        let mut found: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && is_raider(entity.base.kind)
                    && entity
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
                    && entity.base.bounding_box().intersects(&area)
                    && entity.base.position.distance_squared(center) < RAIDER_RANGE * RAIDER_RANGE
            })
            .map(|entity| entity.base.id)
            .collect();
        found.sort_by_key(|id| id.0);
        found
    }

    pub(crate) fn tick_bells(&mut self) {
        let positions: Vec<BlockPos> = self.falls.redstone.bells.keys().copied().collect();
        for pos in positions {
            if self.block_or_air(pos).kind() != Some(BlockKind::Bell) {
                self.falls.redstone.bells.remove(&pos);
                continue;
            }
            let Some(mut ringing) = self.falls.redstone.bells.get(&pos).copied() else {
                continue;
            };
            ringing.ticks += 1;
            let mut finished = false;
            if ringing.ticks >= RING_TICKS {
                finished = true;
            }
            if ringing.ticks >= RESONATE_FROM
                && ringing.resonation == 0
                && !ringing.resonating
                && !self.raiders_near(pos).is_empty()
            {
                ringing.resonating = true;
                self.play_sound(
                    None,
                    pos.center(),
                    RESONATE_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
            if ringing.resonating {
                if ringing.resonation < RESONATION_TICKS {
                    ringing.resonation += 1;
                } else {
                    self.make_raiders_glow(pos);
                    ringing.resonating = false;
                }
            }
            if finished && !ringing.resonating {
                self.falls.redstone.bells.remove(&pos);
            } else {
                self.falls.redstone.bells.insert(pos, ringing);
            }
        }
    }

    fn make_raiders_glow(&mut self, pos: BlockPos) {
        let raiders = self.raiders_near(pos);
        let views: Vec<_> = raiders
            .iter()
            .filter_map(|id| self.entity_view(*id))
            .collect();
        self.fire(
            Lane::Block(pos),
            Continuation::Notice(EventKind::BellResonate),
            move |_| {
                Some(EventPayload::BellResonate(BellResonate {
                    block: pos,
                    resonated_entities: views,
                }))
            },
        );
        for raider in raiders {
            self.add_effect_to(
                raider,
                EffectInstance::new(
                    EffectKind::Glowing.id(),
                    EffectDuration::Ticks(GLOW_TICKS),
                    0,
                    true,
                ),
                EntityPotionEffectCause::Unknown,
            );
        }
    }
}
