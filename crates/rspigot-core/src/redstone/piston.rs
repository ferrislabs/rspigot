use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::piston::motion::STEP_PER_TICK;
use rspigot_domain::piston::{Limits, Reaction, Resolver, is_pushable, reaction_of};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;
use rustc_hash::FxHashMap;

use super::{face_code, facing, step};
use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::plugins::events::{BlockPistonExtend, BlockPistonRetract};
use crate::plugins::{Continuation, EventKind, EventPayload, Lane, face_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::signal::flag;

const EXTEND_SOUND: &str = "minecraft:block.piston.extend";
const CONTRACT_SOUND: &str = "minecraft:block.piston.contract";
const SOUND_VOLUME: f32 = 0.5;
const EVENT_LIMIT: usize = 4096;
const BLOCK_EVENT_RANGE: f64 = 64.0;

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
    pub(crate) progress: f32,
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
                self.announce_block_event(step.piston, state, 0, step.direction);
                if let Some(extended) = state.with_property("extended", "true") {
                    self.game_event(
                        GameEvent::BlockActivate,
                        step.piston.center(),
                        GameEventContext::block(extended),
                    );
                }
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
        self.set_block_unsent(pos, moving_state(direction, sticky));
        let created = self.current_tick();
        self.falls.redstone.moving.insert(
            pos,
            Moving {
                state: returning,
                direction,
                extending: false,
                source: true,
                created,
                progress: 0.0,
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
        self.announce_block_event(pos, returning, id, direction);
        let extended = returning
            .with_property("extended", "true")
            .unwrap_or(returning);
        self.game_event(
            GameEvent::BlockDeactivate,
            pos.center(),
            GameEventContext::block(extended),
        );
    }

    fn announce_block_event(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        id: u8,
        direction: Face,
    ) {
        let Some(kind) = state.kind() else {
            return;
        };
        self.push(
            Audience::Nearby {
                position: pos.center(),
                range: BLOCK_EVENT_RANGE,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos,
                action: id,
                data: face_code(direction),
                block: kind.id().0,
            },
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
            self.set_block_unsent(target, moving_state(direction, false));
            self.falls.redstone.moving.insert(
                target,
                Moving {
                    state: states[index],
                    direction,
                    extending,
                    source: false,
                    created,
                    progress: 0.0,
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
            self.set_block_unsent(front, moving_state(direction, sticky));
            self.falls.redstone.moving.insert(
                front,
                Moving {
                    state: head,
                    direction,
                    extending: true,
                    source: true,
                    created,
                    progress: 0.0,
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
            self.moving_tick(at);
        }
    }

    pub(crate) fn moving_tick(&mut self, pos: BlockPos) {
        let Some(moving) = self.falls.redstone.moving.get(&pos).cloned() else {
            return;
        };
        if moving.progress >= 1.0 {
            self.finish_moving(pos, false);
            self.forget_old_pushes();
            return;
        }
        let new_progress = moving.progress + STEP_PER_TICK;
        self.move_collided_entities(pos, &moving, new_progress);
        self.move_stuck_entities(pos, &moving, new_progress);
        if let Some(stored) = self.falls.redstone.moving.get_mut(&pos) {
            stored.progress = new_progress.min(1.0);
        }
        self.schedule(pos, BlockKind::MovingPiston, 1);
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

    fn fitting_base(
        &self,
        head: BlockPos,
        state: BlockStateId,
    ) -> Option<(BlockPos, BlockStateId)> {
        let direction = facing(state)?;
        let base = step(head, direction.opposite());
        let candidate = self.block_or_air(base);
        let expected = if state.property("type") == Some("sticky") {
            BlockKind::StickyPiston
        } else {
            BlockKind::Piston
        };
        (candidate.kind() == Some(expected)
            && flag(candidate, "extended")
            && facing(candidate) == Some(direction))
        .then_some((base, candidate))
    }

    pub(crate) fn piston_head_removed(&mut self, pos: BlockPos, state: BlockStateId) {
        if let Some((base, candidate)) = self.fitting_base(pos, state) {
            self.destroy_block(base, candidate);
        }
    }

    pub(crate) fn piston_will_destroy(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        creative: bool,
    ) {
        match state.kind() {
            Some(BlockKind::PistonHead) if creative => {
                if let Some((base, _)) = self.fitting_base(pos, state) {
                    self.set_block(base, BlockStateId::AIR);
                }
            }
            Some(BlockKind::MovingPiston) => {
                let Some(direction) = facing(state) else {
                    return;
                };
                let base = step(pos, direction.opposite());
                let candidate = self.block_or_air(base);
                if candidate.kind().is_some_and(is_piston_kind) && flag(candidate, "extended") {
                    self.set_block(base, BlockStateId::AIR);
                }
            }
            _ => {}
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
