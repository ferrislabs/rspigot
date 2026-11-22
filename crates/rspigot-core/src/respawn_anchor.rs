use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::bed::{self, RespawnConfig};
use rspigot_domain::damage::DamageType;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::explosion::BlockInteraction;
use crate::health::DamageSource;
use crate::plugins::events::PlayerSpawnChangeCause;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::level;

const FULL: i32 = 4;
const EXPLOSION_POWER: f32 = 5.0;
const CHARGE_SOUND: &str = "minecraft:block.respawn_anchor.charge";
const SET_SPAWN_SOUND: &str = "minecraft:block.respawn_anchor.set_spawn";
const DEPLETE_SOUND: &str = "minecraft:block.respawn_anchor.deplete";
const HORIZONTAL_OFFSETS: [(i32, i32); 8] = [
    (0, -1),
    (-1, 0),
    (0, 1),
    (1, 0),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];
const HORIZONTAL: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];

fn stand_offsets() -> Vec<(i32, i32, i32)> {
    let mut offsets = Vec::with_capacity(25);
    for dy in [0, -1, 1] {
        offsets.extend(HORIZONTAL_OFFSETS.iter().map(|(dx, dz)| (*dx, dy, *dz)));
    }
    offsets.push((0, 1, 0));
    offsets
}

fn is_fuel(stack: &ItemStack) -> bool {
    stack.item == ItemKind::Glowstone.id()
}

impl<L: ClientLink> Server<L> {
    fn anchor_center(pos: BlockPos) -> Vec3 {
        Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        )
    }

    pub(crate) fn charge_anchor(&mut self, pos: BlockPos, state: BlockStateId) {
        let next = (level(state, "charges") + 1).to_string();
        if let Some(charged) = state.with_property("charges", &next) {
            self.set_block(pos, charged);
        }
        self.play_sound(
            None,
            Self::anchor_center(pos),
            CHARGE_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }

    pub(crate) fn use_respawn_anchor(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        if state.kind() != Some(BlockKind::RespawnAnchor) {
            return false;
        }
        let charges = level(state, "charges");
        if charges < FULL {
            if held.is_some_and(is_fuel) {
                self.charge_anchor(pos, state);
                self.consume_held(id, hand, 1);
                return true;
            }
            if hand == Hand::Main && self.held(id, Hand::Off).as_ref().is_some_and(is_fuel) {
                return true;
            }
        }
        if charges == 0 {
            return false;
        }
        if !self.respawn_anchor_works() {
            self.explode_anchor(pos);
            return true;
        }
        let here = self.world.dimension.key.clone();
        let already = self
            .players
            .get(&id)
            .and_then(|player| player.respawn.as_ref())
            .is_some_and(|config| config.dimension == here && config.pos == pos);
        if !already {
            self.set_respawn_position(
                id,
                Some(RespawnConfig {
                    dimension: here,
                    pos,
                    yaw: 0.0,
                    pitch: 0.0,
                    forced: false,
                }),
                true,
                PlayerSpawnChangeCause::RespawnAnchor,
            );
            self.play_sound(
                None,
                Self::anchor_center(pos),
                SET_SPAWN_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        true
    }

    fn water_would_flow(&self, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        if state.property("waterlogged") == Some("true") {
            return true;
        }
        if state.kind() != Some(BlockKind::Water) {
            return false;
        }
        let flow = level(state, "level");
        if flow == 0 {
            return true;
        }
        let amount = if flow >= 8 { 8 } else { 8 - flow };
        if amount < 2 {
            return false;
        }
        !self
            .world
            .chunks
            .block(pos.offset(0, -1, 0))
            .is_some_and(|below| below.kind() == Some(BlockKind::Water))
    }

    pub(crate) fn explode_anchor(&mut self, pos: BlockPos) {
        self.set_block(pos, BlockStateId::AIR);
        let near = HORIZONTAL.iter().any(|face| {
            let (dx, _, dz) = face.offset();
            self.water_would_flow(pos.offset(dx, 0, dz))
        });
        let above = self
            .world
            .chunks
            .block(pos.offset(0, 1, 0))
            .is_some_and(|state| {
                state.kind() == Some(BlockKind::Water)
                    || state.property("waterlogged") == Some("true")
            });
        let interaction = if near || above {
            BlockInteraction::Keep
        } else {
            self.block_block_interaction()
        };
        let damage = DamageSource::new(DamageType::BadRespawnPoint);
        self.explode(
            None,
            &damage,
            Self::anchor_center(pos),
            EXPLOSION_POWER,
            interaction,
            true,
            None,
        );
    }

    pub(crate) fn anchor_stand_up(&self, pos: BlockPos) -> Option<Vec3> {
        let offsets = stand_offsets();
        [true, false].into_iter().find_map(|dangerous| {
            offsets.iter().find_map(|(dx, dy, dz)| {
                bed::safe_stand_location(&self.world.chunks, pos.offset(*dx, *dy, *dz), dangerous)
            })
        })
    }

    pub(crate) fn use_anchor_charge(&mut self, pos: BlockPos, state: BlockStateId) {
        let left = (level(state, "charges") - 1).to_string();
        if let Some(next) = state.with_property("charges", &left) {
            self.set_block(pos, next);
        }
        self.play_sound(
            None,
            Self::anchor_center(pos),
            DEPLETE_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }
}
