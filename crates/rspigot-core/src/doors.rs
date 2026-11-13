use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::SoundSource;

use crate::plugins::events::EntityInteract;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const DOOR_VOLUME: f32 = 1.0;
const DOOR_PITCH_BASE: f32 = 0.9;
const DOOR_PITCH_RANGE: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DoorSounds {
    pub open: &'static str,
    pub close: &'static str,
}

const WOODEN: DoorSounds = DoorSounds {
    open: "minecraft:block.wooden_door.open",
    close: "minecraft:block.wooden_door.close",
};
const IRON: DoorSounds = DoorSounds {
    open: "minecraft:block.iron_door.open",
    close: "minecraft:block.iron_door.close",
};
const COPPER: DoorSounds = DoorSounds {
    open: "minecraft:block.copper_door.open",
    close: "minecraft:block.copper_door.close",
};
const CHERRY: DoorSounds = DoorSounds {
    open: "minecraft:block.cherry_wood_door.open",
    close: "minecraft:block.cherry_wood_door.close",
};
const NETHER: DoorSounds = DoorSounds {
    open: "minecraft:block.nether_wood_door.open",
    close: "minecraft:block.nether_wood_door.close",
};
const BAMBOO: DoorSounds = DoorSounds {
    open: "minecraft:block.bamboo_wood_door.open",
    close: "minecraft:block.bamboo_wood_door.close",
};

pub(crate) fn door_sounds(kind: BlockKind) -> DoorSounds {
    match kind {
        BlockKind::IronDoor => IRON,
        BlockKind::CherryDoor => CHERRY,
        BlockKind::CrimsonDoor | BlockKind::WarpedDoor => NETHER,
        BlockKind::BambooDoor => BAMBOO,
        BlockKind::CopperDoor
        | BlockKind::ExposedCopperDoor
        | BlockKind::WeatheredCopperDoor
        | BlockKind::OxidizedCopperDoor
        | BlockKind::WaxedCopperDoor
        | BlockKind::WaxedExposedCopperDoor
        | BlockKind::WaxedWeatheredCopperDoor
        | BlockKind::WaxedOxidizedCopperDoor => COPPER,
        _ => WOODEN,
    }
}

pub(crate) fn is_door_state(state: BlockStateId) -> bool {
    state.property("half").is_some()
        && state.property("open").is_some()
        && state.property("hinge").is_some()
}

fn other_half(pos: BlockPos, state: BlockStateId) -> BlockPos {
    if state.property("half") == Some("lower") {
        pos.offset(0, 1, 0)
    } else {
        pos.offset(0, -1, 0)
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn mob_door_action(
        &mut self,
        id: EntityId,
        pos: BlockPos,
        open: bool,
        interact: bool,
    ) {
        if open && interact {
            self.mob_interact_with_door(id, pos);
        } else {
            self.set_door_open(pos, open);
        }
    }

    fn mob_interact_with_door(&mut self, id: EntityId, pos: BlockPos) {
        let Some(mob) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::DoorOpen { mob: id, pos }, move |_| {
            Some(EventPayload::EntityInteract(EntityInteract {
                block: pos,
                entity_type: mob.kind.clone(),
                entity: mob,
            }))
        });
    }

    pub(crate) fn finish_door_open(&mut self, mob: EntityId, pos: BlockPos) {
        if self.entities.store.get(mob).is_some() {
            self.set_door_open(pos, true);
        }
    }

    pub(crate) fn set_door_open(&mut self, pos: BlockPos, open: bool) -> bool {
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| is_door_state(*state))
        else {
            return false;
        };
        let Some(kind) = state.kind() else {
            return false;
        };
        if (state.property("open") == Some("true")) == open {
            return false;
        }
        let value = if open { "true" } else { "false" };
        let Some(changed) = state.with_property("open", value) else {
            return false;
        };
        self.set_block(pos, changed);
        let other = other_half(pos, state);
        if let Some(other_state) = self.world.chunks.block(other)
            && other_state.kind() == Some(kind)
            && let Some(other_changed) = other_state.with_property("open", value)
        {
            self.set_block(other, other_changed);
        }
        self.play_door_sound(None, pos, kind, open);
        true
    }

    pub(crate) fn play_door_sound(
        &mut self,
        except: Option<PlayerId>,
        pos: BlockPos,
        kind: BlockKind,
        open: bool,
    ) {
        let sounds = door_sounds(kind);
        let sound = if open { sounds.open } else { sounds.close };
        let pitch = self.rng.random::<f32>() * DOOR_PITCH_RANGE + DOOR_PITCH_BASE;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(except, at, sound, SoundSource::Blocks, DOOR_VOLUME, pitch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(sound: &str) -> bool {
        rspigot_data::registries::static_index("minecraft:sound_event", sound).is_some()
    }

    #[test]
    fn door_sounds_follow_the_block_set_type() {
        assert_eq!(door_sounds(BlockKind::OakDoor), WOODEN);
        assert_eq!(door_sounds(BlockKind::PoplarDoor), WOODEN);
        assert_eq!(door_sounds(BlockKind::IronDoor), IRON);
        assert_eq!(door_sounds(BlockKind::CherryDoor), CHERRY);
        assert_eq!(door_sounds(BlockKind::CrimsonDoor), NETHER);
        assert_eq!(door_sounds(BlockKind::WarpedDoor), NETHER);
        assert_eq!(door_sounds(BlockKind::BambooDoor), BAMBOO);
        assert_eq!(door_sounds(BlockKind::WaxedExposedCopperDoor), COPPER);
    }

    #[test]
    fn every_door_sound_is_a_registered_sound_event() {
        for sounds in [WOODEN, IRON, COPPER, CHERRY, NETHER, BAMBOO] {
            assert!(registered(sounds.open), "{}", sounds.open);
            assert!(registered(sounds.close), "{}", sounds.close);
        }
    }

    #[test]
    fn only_two_block_doors_count_as_doors() {
        assert!(is_door_state(BlockKind::OakDoor.default_state()));
        assert!(!is_door_state(BlockKind::OakTrapdoor.default_state()));
        assert!(!is_door_state(BlockKind::OakFenceGate.default_state()));
        assert!(!is_door_state(BlockKind::Stone.default_state()));
    }
}
