use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::entity::Aabb;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, facing, step};
use crate::game_events::GameEventContext;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const REACH: i32 = 42;
const PULSE: u64 = 10;
const RELEASE_DELAY: u64 = 1;
const SIDES: [Face; 4] = [Face::North, Face::East, Face::South, Face::West];
const SEARCH: [Face; 2] = [Face::South, Face::West];
const SOUND_VOLUME: f32 = 0.4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookRun {
    facing: Face,
    was_attached: bool,
    was_powered: bool,
    attached: bool,
    powered: bool,
    length: i32,
    destroyed: bool,
    can_update: bool,
    wires: Vec<Option<BlockStateId>>,
}

fn with_flag(state: BlockStateId, name: &str, on: bool) -> BlockStateId {
    state
        .with_property(name, if on { "true" } else { "false" })
        .unwrap_or(state)
}

fn is_hook(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::TripwireHook)
}

fn is_wire(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Tripwire)
}

fn connects_to(state: BlockStateId, direction: Face) -> bool {
    is_wire(state) || (is_hook(state) && facing(state) == Some(direction.opposite()))
}

fn wire_bounds(pos: BlockPos, state: BlockStateId) -> Aabb {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    let (low, high) = if flag(state, "attached") {
        (0.0625, 0.156_25)
    } else {
        (0.0, 0.5)
    };
    Aabb::new(x, y + low, z, x + 1.0, y + high, z + 1.0)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tripwire_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        SIDES.into_iter().fold(state, |state, side| {
            let neighbour = self.block_or_air(step(pos, side));
            with_flag_named(state, side, connects_to(neighbour, side))
        })
    }

    pub(crate) fn tripwire_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        if !direction.is_horizontal() {
            return;
        }
        let neighbour = self.block_or_air(step(pos, direction));
        let next = with_flag_named(state, direction, connects_to(neighbour, direction));
        if next != state {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn hook_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        let Some(toward) = facing(state) else {
            return;
        };
        if direction.opposite() == toward && !self.hook_survives(pos, toward) {
            self.destroy_block(pos, state);
        }
    }

    fn hook_survives(&self, pos: BlockPos, toward: Face) -> bool {
        toward.is_horizontal()
            && self
                .block_or_air(step(pos, toward.opposite()))
                .is_face_sturdy(toward, Support::Full)
    }

    pub(crate) fn tripwire_will_destroy(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        tool: Option<&rspigot_domain::item::ItemStack>,
    ) {
        if is_wire(state) && tool.is_some_and(|stack| stack.item == ItemKind::Shears.id()) {
            self.set_block_quiet(pos, with_flag(state, "disarmed", true));
        }
    }

    pub(crate) fn tripwire_changed(&mut self, pos: BlockPos, old: BlockStateId, new: BlockStateId) {
        if is_wire(new) && old.kind() != new.kind() {
            self.wire_update_source(pos, new);
        }
        if is_wire(old) && old.kind() != new.kind() {
            self.wire_update_source(pos, with_flag(old, "powered", true));
        }
        if is_hook(new) && old.kind() != new.kind() {
            self.hook_calculate(pos, new, false, false, -1, None);
        }
        if is_hook(old) && old.kind() != new.kind() {
            self.hook_removed(pos, old);
        }
    }

    fn wire_update_source(&mut self, pos: BlockPos, state: BlockStateId) {
        for direction in SEARCH {
            for distance in 1..REACH {
                let at = step_by(pos, direction, distance);
                let found = self.block_or_air(at);
                if is_hook(found) {
                    if facing(found) == Some(direction.opposite()) {
                        self.hook_calculate(at, found, false, true, distance, Some(state));
                    }
                    break;
                }
                if !is_wire(found) {
                    break;
                }
            }
        }
    }

    fn hook_removed(&mut self, pos: BlockPos, old: BlockStateId) {
        let (attached, powered) = (flag(old, "attached"), flag(old, "powered"));
        if attached || powered {
            self.hook_calculate(pos, old, true, false, -1, None);
        }
        if powered && let Some(toward) = facing(old) {
            self.hook_notify(pos, toward);
        }
    }

    pub(crate) fn hook_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        self.hook_calculate(pos, state, false, true, -1, None);
    }

    fn hook_notify(&mut self, pos: BlockPos, toward: Face) {
        self.update_neighbors_at(pos);
        self.update_neighbors_at(step(pos, toward.opposite()));
    }

    fn hook_calculate(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        destroyed: bool,
        can_update: bool,
        wire_source: i32,
        wire_state: Option<BlockStateId>,
    ) {
        let Some(toward) = facing(state) else {
            return;
        };
        let was_attached = flag(state, "attached");
        let was_powered = flag(state, "powered");
        let mut attached = !destroyed;
        let mut powered = false;
        let mut length = 0;
        let mut wires: Vec<Option<BlockStateId>> = vec![None; REACH as usize];
        for distance in 1..REACH {
            let at = step_by(pos, toward, distance);
            let mut found = self.block_or_air(at);
            if is_hook(found) {
                if facing(found) == Some(toward.opposite()) {
                    length = distance;
                }
                break;
            }
            if is_wire(found) || distance == wire_source {
                if distance == wire_source {
                    found = wire_state.unwrap_or(found);
                }
                let armed = !flag(found, "disarmed");
                powered |= armed && flag(found, "powered");
                wires[distance as usize] = Some(found);
                if distance == wire_source {
                    self.schedule(pos, BlockKind::TripwireHook, PULSE);
                    attached &= armed;
                }
            } else {
                attached = false;
            }
        }
        attached &= length > 1;
        powered &= attached;
        let next = with_flag(with_flag(state, "attached", attached), "powered", powered);
        if length > 0 {
            let far = step_by(pos, toward, length);
            let opposite = toward.opposite();
            if let Some(far_state) = next.with_property("facing", opposite.name()) {
                self.set_block(far, far_state);
            }
            self.hook_notify(far, opposite);
            self.hook_emit(far, attached, powered, was_attached, was_powered);
        }
        self.ask_redstone(RedstoneStep {
            pos,
            old: MAX_POWER,
            new: 0,
            change: RedstoneChange::Hook(Box::new(HookRun {
                facing: toward,
                was_attached,
                was_powered,
                attached,
                powered,
                length,
                destroyed,
                can_update,
                wires,
            })),
        });
    }

    pub(crate) fn finish_hook(&mut self, pos: BlockPos, new: i32, run: HookRun) {
        if new > 0 {
            return;
        }
        let HookRun {
            facing: toward,
            was_attached,
            was_powered,
            attached,
            powered,
            length,
            destroyed,
            can_update,
            wires,
        } = run;
        let current = self.block_or_air(pos);
        if !is_hook(current) {
            return;
        }
        self.hook_emit(pos, attached, powered, was_attached, was_powered);
        let next = with_flag(with_flag(current, "attached", attached), "powered", powered);
        if !destroyed {
            if let Some(next) = next.with_property("facing", toward.name()) {
                self.set_block(pos, next);
            }
            if can_update {
                self.hook_notify(pos, toward);
            }
        }
        if was_attached != attached {
            for distance in 1..length {
                let at = step_by(pos, toward, distance);
                let Some(held) = wires[distance as usize] else {
                    continue;
                };
                let here = self.block_or_air(at);
                if is_wire(here) || is_hook(here) {
                    self.set_block(at, with_flag(held, "attached", attached));
                }
            }
        }
    }

    fn hook_emit(
        &mut self,
        pos: BlockPos,
        attached: bool,
        powered: bool,
        was_attached: bool,
        was_powered: bool,
    ) {
        let (sound, pitch, event) = if powered && !was_powered {
            (
                "minecraft:block.tripwire.click_on",
                0.6,
                Some(GameEvent::BlockActivate),
            )
        } else if !powered && was_powered {
            (
                "minecraft:block.tripwire.click_off",
                0.5,
                Some(GameEvent::BlockDeactivate),
            )
        } else if attached && !was_attached {
            (
                "minecraft:block.tripwire.attach",
                0.7,
                Some(GameEvent::BlockAttach),
            )
        } else if !attached && was_attached {
            let random: f32 = rand::Rng::random(&mut self.rng);
            (
                "minecraft:block.tripwire.detach",
                1.2 / (random * 0.2 + 0.9),
                Some(GameEvent::BlockDetach),
            )
        } else {
            return;
        };
        self.play_sound(
            None,
            pos.center(),
            sound,
            SoundSource::Blocks,
            SOUND_VOLUME,
            pitch,
        );
        if let Some(event) = event {
            self.game_event(event, pos.center(), GameEventContext::of(None));
        }
    }

    pub(crate) fn tripwire_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "powered") {
            self.tripwire_check(pos);
        }
    }

    pub(crate) fn tripwire_stepped_on(&mut self, pos: BlockPos, state: BlockStateId) {
        if !flag(state, "powered") && !self.has_scheduled(pos, BlockKind::Tripwire) {
            self.tripwire_check(pos);
        }
    }

    fn tripwire_check(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if !is_wire(state) {
            return;
        }
        let was = flag(state, "powered");
        let bounds = wire_bounds(pos, state);
        let pressed = self.count_touching(&bounds, false) > 0;
        if !was && pressed && flag(state, "attached") {
            return self.ask_touch(super::touch::TouchStep {
                pos,
                old: 0,
                what: super::touch::Touched::Wire,
                touchers: self.touchers_in(&bounds, false),
                at: 0,
                accepted: 0,
            });
        }
        self.tripwire_apply(pos, was, pressed);
    }

    pub(super) fn tripwire_apply(&mut self, pos: BlockPos, was: bool, pressed: bool) {
        let state = self.block_or_air(pos);
        if !is_wire(state) {
            return;
        }
        if was != pressed {
            let next = with_flag(state, "powered", pressed);
            self.set_block(pos, next);
            self.wire_update_source(pos, next);
        }
        if pressed {
            self.schedule(pos, BlockKind::Tripwire, PULSE);
        } else if was {
            self.schedule(pos, BlockKind::Tripwire, RELEASE_DELAY);
        }
    }
}

fn step_by(pos: BlockPos, face: Face, distance: i32) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx * distance, dy * distance, dz * distance)
}

fn with_flag_named(state: BlockStateId, side: Face, on: bool) -> BlockStateId {
    with_flag(state, side.name(), on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tripwire_connects_to_wires_and_to_a_hook_that_faces_it() {
        let wire = BlockKind::Tripwire.default_state();
        let hook = BlockKind::TripwireHook
            .default_state()
            .with_property("facing", "west")
            .unwrap();
        assert!(connects_to(wire, Face::North));
        assert!(connects_to(hook, Face::East));
        assert!(!connects_to(hook, Face::West));
        assert!(!connects_to(BlockKind::Stone.default_state(), Face::East));
    }

    #[test]
    fn an_attached_tripwire_is_a_thin_strip_and_a_loose_one_is_taller() {
        let loose = BlockKind::Tripwire.default_state();
        let strip = with_flag(loose, "attached", true);
        let at = BlockPos::new(0, 0, 0);
        assert!(wire_bounds(at, strip).max_y < wire_bounds(at, loose).max_y);
        assert!(wire_bounds(at, strip).min_y > 0.0);
    }
}
