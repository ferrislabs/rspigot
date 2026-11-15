use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::Aabb;
use rspigot_domain::piston::{Limits, Reaction, Resolver, is_pushable, reaction_of};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::SoundSource;
use rustc_hash::FxHashMap;

use super::{facing, step};
use crate::plugins::events::{BlockPistonExtend, BlockPistonRetract};
use crate::plugins::{Continuation, EventKind, EventPayload, Lane, face_of};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const EXTEND_SOUND: &str = "minecraft:block.piston.extend";
const CONTRACT_SOUND: &str = "minecraft:block.piston.contract";
const SOUND_VOLUME: f32 = 0.5;
const MOVE_TICKS: u64 = 2;
const STEP_DISTANCE: f64 = 0.5;
const EVENT_LIMIT: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockEvent {
    pub(crate) pos: BlockPos,
    pub(crate) kind: BlockKind,
    pub(crate) id: u8,
    pub(crate) facing: Face,
}

#[derive(Debug, Clone)]
pub(crate) struct Moving {
    pub(crate) state: BlockStateId,
    pub(crate) direction: Face,
    pub(crate) extending: bool,
    pub(crate) source: bool,
    pub(crate) created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PistonAction {
    Extend,
    Pull,
    Retract(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PistonStep {
    pub(crate) piston: BlockPos,
    pub(crate) direction: Face,
    pub(crate) sticky: bool,
    pub(crate) action: PistonAction,
    pub(crate) blocks: Vec<BlockPos>,
}

impl PistonStep {
    pub(crate) fn kind(&self) -> EventKind {
        match self.action {
            PistonAction::Extend => EventKind::BlockPistonExtend,
            PistonAction::Pull | PistonAction::Retract(_) => EventKind::BlockPistonRetract,
        }
    }
}

fn is_piston_kind(kind: BlockKind) -> bool {
    matches!(kind, BlockKind::Piston | BlockKind::StickyPiston)
}

fn offset_by(pos: BlockPos, face: Face, count: i32) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx * count, dy * count, dz * count)
}

fn vector_of(face: Face) -> Vec3 {
    let (dx, dy, dz) = face.offset();
    Vec3::new(f64::from(dx), f64::from(dy), f64::from(dz))
}

fn piston_type(sticky: bool) -> &'static str {
    if sticky { "sticky" } else { "normal" }
}

fn moving_state(direction: Face, sticky: bool) -> BlockStateId {
    BlockKind::MovingPiston
        .default_state()
        .with_property("facing", direction.name())
        .and_then(|state| state.with_property("type", piston_type(sticky)))
        .unwrap_or(BlockKind::MovingPiston.default_state())
}

impl<L: ClientLink> Server<L> {
    fn piston_limits(&self) -> Limits {
        let height = self.world.dimension.height;
        Limits {
            min_y: height.min_y,
            max_y: height.max_y(),
        }
    }

    fn piston_powered(&self, pos: BlockPos, facing: Face) -> bool {
        for face in Face::ALL {
            if face != facing && self.has_signal(step(pos, face), face) {
                return true;
            }
        }
        if self.has_signal(pos, Face::Down) {
            return true;
        }
        let above = step(pos, Face::Up);
        Face::ALL
            .into_iter()
            .filter(|face| *face != Face::Down)
            .any(|face| self.has_signal(step(above, face), face))
    }

    fn resolves(
        &self,
        pos: BlockPos,
        direction: Face,
        extending: bool,
    ) -> Option<(Vec<BlockPos>, Vec<BlockPos>)> {
        let lookup = |at: BlockPos| self.block_or_air(at);
        let mut resolver = Resolver::new(&lookup, self.piston_limits(), pos, direction, extending);
        let resolved = resolver.resolve();
        resolved.then_some((resolver.to_push, resolver.to_destroy))
    }

    fn queue_block_event(&mut self, event: BlockEvent) {
        let events = &mut self.falls.redstone.events;
        if !events.contains(&event) {
            events.push(event);
        }
    }

    pub(crate) fn piston_check(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind().filter(|kind| is_piston_kind(*kind)) else {
            return;
        };
        let Some(direction) = facing(state) else {
            return;
        };
        let powered = self.piston_powered(pos, direction);
        let extended = flag(state, "extended");
        if powered && !extended {
            if self.resolves(pos, direction, true).is_some() {
                self.queue_block_event(BlockEvent {
                    pos,
                    kind,
                    id: 0,
                    facing: direction,
                });
            }
            return;
        }
        if powered || !extended {
            return;
        }
        let ahead = step(step(pos, direction), direction);
        let id = match self.falls.redstone.moving.get(&ahead) {
            Some(moving)
                if moving.direction == direction
                    && moving.extending
                    && self.current_tick().saturating_sub(moving.created) < 1 =>
            {
                2
            }
            _ => 1,
        };
        let step = PistonStep {
            piston: pos,
            direction,
            sticky: kind == BlockKind::StickyPiston,
            action: PistonAction::Retract(id),
            blocks: Vec::new(),
        };
        if kind == BlockKind::StickyPiston {
            self.queue_block_event(BlockEvent {
                pos,
                kind,
                id,
                facing: direction,
            });
        } else if !self.falls.redstone.events.contains(&BlockEvent {
            pos,
            kind,
            id,
            facing: direction,
        }) {
            self.ask_piston(step);
        }
    }

    pub(crate) fn ask_piston(&mut self, step: PistonStep) {
        let (piston, direction, sticky) = (step.piston, step.direction, step.sticky);
        let blocks = step.blocks.clone();
        let extending = step.action == PistonAction::Extend;
        self.fire(
            Lane::Block(piston),
            Continuation::Piston(Box::new(step)),
            move |_| {
                Some(if extending {
                    EventPayload::BlockPistonExtend(BlockPistonExtend {
                        block: piston,
                        blocks,
                        direction: face_of(direction),
                        sticky,
                    })
                } else {
                    EventPayload::BlockPistonRetract(BlockPistonRetract {
                        block: piston,
                        blocks,
                        direction: face_of(direction),
                        sticky,
                    })
                })
            },
        );
    }

    pub(crate) fn finish_piston(&mut self, step: PistonStep) {
        let state = self.block_or_air(step.piston);
        if step.action != PistonAction::Pull && !state.kind().is_some_and(is_piston_kind) {
            return;
        }
        match step.action {
            PistonAction::Extend => {
                let Some((pushed, destroyed)) = self.resolves(step.piston, step.direction, true)
                else {
                    return;
                };
                self.carry_blocks(
                    step.piston,
                    step.direction,
                    true,
                    step.sticky,
                    &pushed,
                    &destroyed,
                );
                if let Some(extended) = state.with_property("extended", "true") {
                    self.set_block(step.piston, extended);
                }
                let pitch = self.rng_unit() * 0.25 + 0.6;
                self.play_sound(
                    None,
                    step.piston.center(),
                    EXTEND_SOUND,
                    SoundSource::Blocks,
                    SOUND_VOLUME,
                    pitch,
                );
            }
            PistonAction::Pull => {
                if let Some((pushed, destroyed)) = self.resolves(step.piston, step.direction, false)
                {
                    self.carry_blocks(
                        step.piston,
                        step.direction,
                        false,
                        step.sticky,
                        &pushed,
                        &destroyed,
                    );
                }
            }
            PistonAction::Retract(id) => {
                self.queue_block_event(BlockEvent {
                    pos: step.piston,
                    kind: state.kind().unwrap_or(BlockKind::Piston),
                    id,
                    facing: step.direction,
                });
            }
        }
    }

    fn rng_unit(&mut self) -> f32 {
        use rand::Rng;
        self.rng.random::<f32>()
    }

    pub(crate) fn tick_block_events(&mut self) {
        let mut rounds = 0;
        while !self.falls.redstone.events.is_empty() && rounds < EVENT_LIMIT {
            rounds += 1;
            let batch = std::mem::take(&mut self.falls.redstone.events);
            for event in batch {
                let state = self.block_or_air(event.pos);
                if state.kind() == Some(event.kind) {
                    self.piston_event(event, state);
                }
            }
        }
    }

    fn piston_event(&mut self, event: BlockEvent, state: BlockStateId) {
        let BlockEvent {
            pos,
            id,
            facing: direction,
            ..
        } = event;
        let sticky = event.kind == BlockKind::StickyPiston;
        let powered = self.piston_powered(pos, direction);
        if powered && id != 0 {
            if let Some(extended) = state.with_property("extended", "true") {
                self.set_block_quiet(pos, extended);
            }
            return;
        }
        if !powered && id == 0 {
            return;
        }
        if id == 0 {
            if let Some((pushed, destroyed)) = self.resolves(pos, direction, true) {
                let blocks = pushed.into_iter().chain(destroyed).collect();
                self.ask_piston(PistonStep {
                    piston: pos,
                    direction,
                    sticky,
                    action: PistonAction::Extend,
                    blocks,
                });
            }
            return;
        }
        self.retract_piston(pos, direction, sticky, id);
    }

    fn retract_piston(&mut self, pos: BlockPos, direction: Face, sticky: bool, id: u8) {
        let front = step(pos, direction);
        if self.falls.redstone.moving.contains_key(&front) {
            self.finish_moving(front, true);
        }
        let base = if sticky {
            BlockKind::StickyPiston
        } else {
            BlockKind::Piston
        };
        let returning = base
            .default_state()
            .with_property("facing", direction.name())
            .unwrap_or(base.default_state());
        self.set_block_quiet(pos, moving_state(direction, sticky));
        let created = self.current_tick();
        self.falls.redstone.moving.insert(
            pos,
            Moving {
                state: returning,
                direction,
                extending: false,
                source: true,
                created,
            },
        );
        self.schedule(pos, BlockKind::MovingPiston, 1);
        self.update_neighbors_at(pos);
        if sticky {
            self.pull_with_sticky(pos, direction, id);
        } else {
            self.set_block(front, BlockStateId::AIR);
        }
        let pitch = self.rng_unit() * 0.15 + 0.6;
        self.play_sound(
            None,
            pos.center(),
            CONTRACT_SOUND,
            SoundSource::Blocks,
            SOUND_VOLUME,
            pitch,
        );
    }

    fn pull_with_sticky(&mut self, pos: BlockPos, direction: Face, id: u8) {
        let front = step(pos, direction);
        let beyond = step(front, direction);
        let state = self.block_or_air(beyond);
        let mut handled = false;
        if state.kind() == Some(BlockKind::MovingPiston)
            && self
                .falls
                .redstone
                .moving
                .get(&beyond)
                .is_some_and(|moving| moving.direction == direction && moving.extending)
        {
            self.finish_moving(beyond, true);
            handled = true;
        }
        if handled {
            return;
        }
        let pullable = id == 1
            && !state.is_air()
            && is_pushable(
                state,
                self.piston_limits(),
                beyond,
                direction.opposite(),
                false,
                direction,
            )
            && (reaction_of(state) == Reaction::Normal || state.kind().is_some_and(is_piston_kind));
        if pullable {
            if self.block_or_air(front).kind() == Some(BlockKind::PistonHead) {
                self.set_block(front, BlockStateId::AIR);
            }
            if let Some((pushed, destroyed)) = self.resolves(pos, direction, false) {
                let blocks = pushed.into_iter().chain(destroyed).collect();
                self.ask_piston(PistonStep {
                    piston: pos,
                    direction,
                    sticky: true,
                    action: PistonAction::Pull,
                    blocks,
                });
            }
        } else {
            self.set_block(front, BlockStateId::AIR);
        }
    }

    fn carry_blocks(
        &mut self,
        pos: BlockPos,
        direction: Face,
        extending: bool,
        sticky: bool,
        pushed: &[BlockPos],
        destroyed: &[BlockPos],
    ) {
        let push = if extending {
            direction
        } else {
            direction.opposite()
        };
        let states: Vec<BlockStateId> = pushed.iter().map(|at| self.block_or_air(*at)).collect();
        let mut vacated: FxHashMap<BlockPos, BlockStateId> =
            pushed.iter().copied().zip(states.iter().copied()).collect();
        let created = self.current_tick();
        let mut destroyed_states = Vec::new();
        for at in destroyed.iter().rev() {
            let state = self.block_or_air(*at);
            self.destroy_block(*at, state);
            destroyed_states.push(*at);
        }
        let mut arrivals = Vec::new();
        for (index, at) in pushed.iter().enumerate().rev() {
            let target = step(*at, push);
            vacated.remove(&target);
            self.set_block_quiet(target, moving_state(direction, false));
            self.falls.redstone.moving.insert(
                target,
                Moving {
                    state: states[index],
                    direction,
                    extending,
                    source: false,
                    created,
                },
            );
            arrivals.push(target);
        }
        if extending {
            let head = BlockKind::PistonHead
                .default_state()
                .with_property("facing", direction.name())
                .and_then(|state| state.with_property("type", piston_type(sticky)))
                .unwrap_or(BlockKind::PistonHead.default_state());
            let front = step(pos, direction);
            vacated.remove(&front);
            self.set_block_quiet(front, moving_state(direction, sticky));
            self.falls.redstone.moving.insert(
                front,
                Moving {
                    state: head,
                    direction,
                    extending: true,
                    source: true,
                    created,
                },
            );
            arrivals.push(front);
        }
        let mut cleared: Vec<BlockPos> = vacated.keys().copied().collect();
        cleared.sort_by_key(|at| (at.x, at.y, at.z));
        for at in cleared {
            self.set_block_quiet(at, BlockStateId::AIR);
        }
        for at in destroyed_states {
            self.update_neighbors_at(at);
        }
        for at in pushed.iter().rev() {
            self.update_neighbors_at(*at);
        }
        if extending {
            self.update_neighbors_at(step(pos, direction));
        }
        for at in arrivals {
            self.schedule(at, BlockKind::MovingPiston, 1);
        }
    }

    pub(crate) fn moving_tick(&mut self, pos: BlockPos) {
        let now = self.current_tick();
        let Some(moving) = self.falls.redstone.moving.get(&pos).cloned() else {
            if self.block_or_air(pos).kind() == Some(BlockKind::MovingPiston) {
                self.set_block(pos, BlockStateId::AIR);
            }
            return;
        };
        let elapsed = now.saturating_sub(moving.created).min(MOVE_TICKS);
        let direction = if moving.extending {
            moving.direction
        } else {
            moving.direction.opposite()
        };
        let before = (elapsed.saturating_sub(1)) as f64 * STEP_DISTANCE;
        self.push_entities(pos, direction, before);
        if elapsed >= MOVE_TICKS {
            self.finish_moving(pos, false);
        } else {
            self.schedule(pos, BlockKind::MovingPiston, 1);
        }
    }

    pub(crate) fn finish_moving(&mut self, pos: BlockPos, forced: bool) {
        let Some(moving) = self.falls.redstone.moving.remove(&pos) else {
            return;
        };
        if self.block_or_air(pos).kind() != Some(BlockKind::MovingPiston) {
            return;
        }
        let state = if forced && moving.source {
            BlockStateId::AIR
        } else if moving.state.property("waterlogged") == Some("true") {
            moving
                .state
                .with_property("waterlogged", "false")
                .unwrap_or(moving.state)
        } else {
            moving.state
        };
        self.set_block(pos, state);
        self.neighbor_changed(pos, pos);
    }

    fn push_entities(&mut self, pos: BlockPos, direction: Face, progress: f64) {
        let delta = vector_of(direction).scale(STEP_DISTANCE);
        let start = offset_by(pos, direction.opposite(), 1);
        let base = Aabb::block(start).move_by(vector_of(direction).scale(progress));
        let region = base.expand_towards(delta);
        let ids: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none() && entity.base.bounding_box().intersects(&region)
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in ids {
            if let Some(entity) = self.entities.store.get_mut(id) {
                entity.base.position = entity.base.position + delta;
                entity.base.needs_sync = true;
            }
            self.entities.store.reindex(id);
        }
        let players: Vec<(rspigot_domain::player::PlayerId, Vec3)> = self
            .players
            .iter()
            .filter(|(_, player)| {
                Aabb::of_size(player.position, 0.6, player.height() as f32).intersects(&region)
            })
            .map(|(id, player)| (*id, player.position + delta))
            .collect();
        for (id, position) in players {
            self.teleport_player(id, position);
        }
    }

    pub(crate) fn piston_head_survives(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(direction) = facing(state) else {
            return false;
        };
        let base = self.block_or_air(step(pos, direction.opposite()));
        let expected = if state.property("type") == Some("sticky") {
            BlockKind::StickyPiston
        } else {
            BlockKind::Piston
        };
        base.kind() == Some(expected) && flag(base, "extended") && facing(base) == Some(direction)
    }

    pub(crate) fn piston_head_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.piston_head_survives(pos, state)
            && let Some(direction) = facing(state)
        {
            let base = step(pos, direction.opposite());
            self.neighbor_changed(base, pos);
        }
    }

    pub(crate) fn piston_head_shape(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Face,
    ) {
        if facing(state).map(Face::opposite) == Some(direction)
            && !self.piston_head_survives(pos, state)
        {
            self.set_block(pos, BlockStateId::AIR);
        }
    }

    pub(crate) fn piston_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        if !self.falls.redstone.moving.contains_key(&pos) {
            self.piston_check(pos, state);
        }
    }
}
