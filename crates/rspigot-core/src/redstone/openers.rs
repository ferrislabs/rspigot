use rand::Rng;
use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, connected_direction, step};
use crate::doors::is_door_state;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const VOLUME: f32 = 1.0;
const PITCH_BASE: f32 = 0.9;
const PITCH_RANGE: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opener {
    Door,
    Trapdoor,
    Gate,
}

fn opener_of(state: BlockStateId) -> Option<Opener> {
    let name = state.kind()?.name();
    if name.ends_with("_door") && is_door_state(state) {
        Some(Opener::Door)
    } else if name.ends_with("_trapdoor") {
        Some(Opener::Trapdoor)
    } else if name.ends_with("_fence_gate") {
        Some(Opener::Gate)
    } else {
        None
    }
}

pub(super) fn is_opener(state: BlockStateId) -> bool {
    opener_of(state).is_some()
}

fn trapdoor_sound(name: &str, open: bool) -> &'static str {
    let (iron, copper, cherry, nether, bamboo) = (
        name.contains("iron"),
        name.contains("copper"),
        name.contains("cherry"),
        name.contains("crimson") || name.contains("warped"),
        name.contains("bamboo"),
    );
    match (open, iron, copper, cherry, nether, bamboo) {
        (true, true, ..) => "minecraft:block.iron_trapdoor.open",
        (false, true, ..) => "minecraft:block.iron_trapdoor.close",
        (true, _, true, ..) => "minecraft:block.copper_trapdoor.open",
        (false, _, true, ..) => "minecraft:block.copper_trapdoor.close",
        (true, _, _, true, ..) => "minecraft:block.cherry_wood_trapdoor.open",
        (false, _, _, true, ..) => "minecraft:block.cherry_wood_trapdoor.close",
        (true, _, _, _, true, _) => "minecraft:block.nether_wood_trapdoor.open",
        (false, _, _, _, true, _) => "minecraft:block.nether_wood_trapdoor.close",
        (true, _, _, _, _, true) => "minecraft:block.bamboo_wood_trapdoor.open",
        (false, _, _, _, _, true) => "minecraft:block.bamboo_wood_trapdoor.close",
        (true, ..) => "minecraft:block.wooden_trapdoor.open",
        (false, ..) => "minecraft:block.wooden_trapdoor.close",
    }
}

fn gate_sound(name: &str, open: bool) -> &'static str {
    let (cherry, nether, bamboo) = (
        name.contains("cherry"),
        name.contains("crimson") || name.contains("warped"),
        name.contains("bamboo"),
    );
    match (open, cherry, nether, bamboo) {
        (true, true, ..) => "minecraft:block.cherry_wood_fence_gate.open",
        (false, true, ..) => "minecraft:block.cherry_wood_fence_gate.close",
        (true, _, true, _) => "minecraft:block.nether_wood_fence_gate.open",
        (false, _, true, _) => "minecraft:block.nether_wood_fence_gate.close",
        (true, _, _, true) => "minecraft:block.bamboo_wood_fence_gate.open",
        (false, _, _, true) => "minecraft:block.bamboo_wood_fence_gate.close",
        (true, ..) => "minecraft:block.fence_gate.open",
        (false, ..) => "minecraft:block.fence_gate.close",
    }
}

impl<L: ClientLink> Server<L> {
    fn opener_signal(&self, pos: BlockPos, state: BlockStateId, opener: Opener) -> (bool, i32) {
        let mut powered = self.has_neighbor_signal(pos);
        let mut power = self.best_neighbor_signal(pos, true);
        if opener == Opener::Door {
            let other = step(
                pos,
                if state.property("half") == Some("lower") {
                    Face::Up
                } else {
                    Face::Down
                },
            );
            powered = powered || self.has_neighbor_signal(other);
            power = power.max(self.best_neighbor_signal(other, true));
        }
        (powered, power)
    }

    pub(crate) fn opener_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let Some(opener) = opener_of(state) else {
            return state;
        };
        if opener == Opener::Door && state.property("half") != Some("lower") {
            return state;
        }
        let (powered, _) = self.opener_signal(pos, state, opener);
        let value = if powered { "true" } else { "false" };
        state
            .with_property("powered", value)
            .and_then(|next| next.with_property("open", value))
            .unwrap_or(state)
    }

    pub(crate) fn opener_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(opener) = opener_of(state) else {
            return;
        };
        let (powered, power) = self.opener_signal(pos, state, opener);
        let old = match opener {
            Opener::Door => flag(state, "powered"),
            Opener::Trapdoor | Opener::Gate => flag(state, "open"),
        };
        let old = if old { MAX_POWER } else { 0 };
        if (old == 0) != (power == 0) {
            self.ask_redstone(RedstoneStep {
                pos,
                old,
                new: power,
                change: RedstoneChange::Opener,
            });
        } else {
            self.apply_opener(pos, powered);
        }
    }

    pub(crate) fn finish_opener(&mut self, pos: BlockPos, new: i32) {
        self.apply_opener(pos, new > 0);
    }

    fn apply_opener(&mut self, pos: BlockPos, powered: bool) {
        let state = self.block_or_air(pos);
        let (Some(opener), Some(kind)) = (opener_of(state), state.kind()) else {
            return;
        };
        if powered == flag(state, "powered") {
            return;
        }
        let value = if powered { "true" } else { "false" };
        let mut next = state;
        if powered != flag(state, "open") {
            if let Some(opened) = next.with_property("open", value) {
                next = opened;
            }
            self.play_opener_sound(pos, kind, opener, powered);
        }
        if let Some(lit) = next.with_property("powered", value) {
            next = lit;
        }
        self.set_block_quiet(pos, next);
    }

    fn play_opener_sound(&mut self, pos: BlockPos, kind: BlockKind, opener: Opener, open: bool) {
        match opener {
            Opener::Door => self.play_door_sound(None, pos, kind, open),
            Opener::Trapdoor | Opener::Gate => {
                let sound = if opener == Opener::Trapdoor {
                    trapdoor_sound(kind.name(), open)
                } else {
                    gate_sound(kind.name(), open)
                };
                let pitch = self.rng.random::<f32>() * PITCH_RANGE + PITCH_BASE;
                self.play_sound(
                    None,
                    pos.center(),
                    sound,
                    SoundSource::Blocks,
                    VOLUME,
                    pitch,
                );
            }
        }
    }

    pub(crate) fn attached_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        let toward = connected_direction(state).opposite();
        if toward != direction {
            return;
        }
        let support = self.block_or_air(step(pos, toward));
        if !support.is_face_sturdy(toward.opposite(), Support::Full) {
            self.destroy_block(pos, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(sound: &str) -> bool {
        rspigot_data::registries::static_index("minecraft:sound_event", sound).is_some()
    }

    #[test]
    fn every_trapdoor_and_gate_sound_is_a_registered_sound_event() {
        for name in [
            "oak_trapdoor",
            "iron_trapdoor",
            "copper_trapdoor",
            "cherry_trapdoor",
            "crimson_trapdoor",
            "bamboo_trapdoor",
            "oak_fence_gate",
            "cherry_fence_gate",
            "crimson_fence_gate",
            "bamboo_fence_gate",
        ] {
            for open in [true, false] {
                let sound = if name.ends_with("gate") {
                    gate_sound(name, open)
                } else {
                    trapdoor_sound(name, open)
                };
                assert!(registered(sound), "{name}: {sound}");
            }
        }
    }
}
