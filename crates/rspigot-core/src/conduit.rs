use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_nbt::Compound;

use crate::health::DamageSource;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY: &str = "minecraft:conduit";
const CHECK_INTERVAL: i64 = 40;
const AMBIENT_INTERVAL: i64 = 80;
const MIN_ACTIVE_BLOCKS: usize = 16;
const HUNTING_BLOCKS: usize = 42;
const RANGE_PER_SEVEN: i32 = 16;
const BLOCKS_PER_RANGE: usize = 7;
const EFFECT_TICKS: u32 = 260;
const HUNT_RANGE: f64 = 8.0;
const HUNT_DAMAGE: f32 = 4.0;
const HALF_WIDTH: f64 = 0.3;
const PLAYER_HEIGHT: f64 = 1.8;
const AMBIENT_SOUND: &str = "minecraft:block.conduit.ambient";
const ATTACK_SOUND: &str = "minecraft:block.conduit.attack.target";
const ENEMY_CLASSES: [&str; 7] = [
    "Monster",
    "Slime",
    "Ghast",
    "Phantom",
    "Hoglin",
    "Shulker",
    "EnderDragon",
];

pub(crate) fn is_conduit(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Conduit)
}

fn is_frame(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(
            BlockKind::Prismarine
                | BlockKind::PrismarineBricks
                | BlockKind::SeaLantern
                | BlockKind::DarkPrismarine
        )
    )
}

pub(crate) fn frame_blocks(
    center: BlockPos,
    water: impl Fn(BlockPos) -> bool,
    block: impl Fn(BlockPos) -> BlockStateId,
) -> Vec<BlockPos> {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if !water(center.offset(dx, dy, dz)) {
                    return Vec::new();
                }
            }
        }
    }
    let mut found = Vec::new();
    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            for dz in -2i32..=2 {
                let (ax, ay, az) = (dx.abs(), dy.abs(), dz.abs());
                let outside = ax > 1 || ay > 1 || az > 1;
                let on_ring = dx == 0 && (ay == 2 || az == 2)
                    || dy == 0 && (ax == 2 || az == 2)
                    || dz == 0 && (ax == 2 || ay == 2);
                let at = center.offset(dx, dy, dz);
                if outside && on_ring && is_frame(block(at)) {
                    found.push(at);
                }
            }
        }
    }
    found
}

pub(crate) fn is_active(frame: usize) -> bool {
    frame >= MIN_ACTIVE_BLOCKS
}

pub(crate) fn effect_range(frame: usize) -> i32 {
    i32::try_from(frame / BLOCKS_PER_RANGE).unwrap_or(0) * RANGE_PER_SEVEN
}

pub(crate) fn can_hunt(frame: usize) -> bool {
    frame >= HUNTING_BLOCKS
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_conduit_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_conduit(state) {
            return;
        }
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        if chunk.block_entity(pos).is_none() {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft("conduit"),
                data: Compound::new(),
            });
        }
    }

    pub(crate) fn tick_conduits(&mut self) {
        let now = self.world.level.game_time;
        let due_check = now % CHECK_INTERVAL == 0;
        let due_ambient = now % AMBIENT_INTERVAL == 0;
        if !due_check && !due_ambient {
            return;
        }
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.as_str() == ENTITY)
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == ENTITY)
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            if !is_conduit(self.block_or_air(pos)) {
                continue;
            }
            let frame = self.conduit_frame(pos);
            if !is_active(frame.len()) {
                continue;
            }
            if due_check {
                self.conduit_power(pos, frame.len());
                if can_hunt(frame.len()) {
                    self.conduit_hunt(pos);
                }
            }
            if due_ambient {
                self.play_sound(
                    None,
                    pos.center(),
                    AMBIENT_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }

    pub(crate) fn conduit_frame(&self, pos: BlockPos) -> Vec<BlockPos> {
        let chunks = &self.world.chunks;
        frame_blocks(
            pos,
            |at| {
                chunks
                    .block(at)
                    .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::WATER))
            },
            |at| chunks.block(at).unwrap_or(BlockStateId::AIR),
        )
    }

    fn conduit_power(&mut self, pos: BlockPos, frame: usize) {
        let range = f64::from(effect_range(frame));
        let low = Vec3::new(
            f64::from(pos.x) - range,
            f64::from(pos.y) - range,
            f64::from(pos.z) - range,
        );
        let high = Vec3::new(
            f64::from(pos.x) + 1.0 + range,
            f64::from(pos.y) + 1.0 + range,
            f64::from(pos.z) + 1.0 + range,
        );
        let Some(effect) = EffectId::from_name("minecraft:conduit_power") else {
            return;
        };
        let wet: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                let at = player.position;
                !player.vitals.is_dead()
                    && at.x + HALF_WIDTH > low.x
                    && at.x - HALF_WIDTH < high.x
                    && at.z + HALF_WIDTH > low.z
                    && at.z - HALF_WIDTH < high.z
                    && at.y + PLAYER_HEIGHT > low.y
                    && at.y < high.y
                    && (self.in_water(at) || self.raining_at(at.block()))
            })
            .map(|player| player.id)
            .collect();
        for id in wet {
            let instance = EffectInstance {
                ambient: true,
                ..EffectInstance::new(effect, EffectDuration::Ticks(EFFECT_TICKS), 0, true)
            };
            self.apply_effect(id, instance, Some(EntityPotionEffectCause::Conduit));
        }
    }

    fn conduit_hunt(&mut self, pos: BlockPos) {
        let center = pos.center();
        let targets: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                let base = &entity.base;
                let (dx, dy, dz) = (
                    base.position.x - center.x,
                    base.position.y - center.y,
                    base.position.z - center.z,
                );
                !base.is_removed()
                    && dx * dx + dy * dy + dz * dz < HUNT_RANGE * HUNT_RANGE
                    && ENEMY_CLASSES.iter().any(|class| base.kind.extends(class))
                    && entity.living().is_some_and(|living| {
                        !rspigot_domain::entity::living::is_dead_or_dying(base, living)
                    })
                    && (self.in_water(base.position) || self.is_in_rain(base))
            })
            .map(|entity| entity.base.id)
            .collect();
        if targets.is_empty() {
            return;
        }
        let target = targets[self.rng.random_range(0..targets.len())];
        self.play_sound(
            None,
            pos.center(),
            ATTACK_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.hurt_entity(target, &DamageSource::new(DamageType::Magic), HUNT_DAMAGE);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const CENTER: BlockPos = BlockPos::new(0, 64, 0);

    fn frame_world(blocks: &[(BlockPos, BlockKind)]) -> HashMap<BlockPos, BlockStateId> {
        blocks
            .iter()
            .map(|(pos, kind)| (*pos, kind.default_state()))
            .collect()
    }

    fn full_frame() -> Vec<(BlockPos, BlockKind)> {
        let mut blocks = Vec::new();
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                for dz in -2i32..=2 {
                    let (ax, ay, az) = (dx.abs(), dy.abs(), dz.abs());
                    let outside = ax > 1 || ay > 1 || az > 1;
                    let ring = dx == 0 && (ay == 2 || az == 2)
                        || dy == 0 && (ax == 2 || az == 2)
                        || dz == 0 && (ax == 2 || ay == 2);
                    if outside && ring {
                        blocks.push((CENTER.offset(dx, dy, dz), BlockKind::Prismarine));
                    }
                }
            }
        }
        blocks
    }

    fn count(blocks: &[(BlockPos, BlockKind)], water: bool) -> usize {
        let world = frame_world(blocks);
        frame_blocks(
            CENTER,
            |_| water,
            |at| world.get(&at).copied().unwrap_or(BlockStateId::AIR),
        )
        .len()
    }

    #[test]
    fn the_full_cage_has_forty_two_blocks() {
        assert_eq!(full_frame().len(), 42);
        assert_eq!(count(&full_frame(), true), 42);
    }

    #[test]
    fn a_dry_conduit_has_no_frame() {
        assert_eq!(count(&full_frame(), false), 0);
    }

    #[test]
    fn only_prismarine_blocks_count() {
        let mut blocks = full_frame();
        blocks[0].1 = BlockKind::Stone;
        blocks[1].1 = BlockKind::SeaLantern;
        blocks[2].1 = BlockKind::DarkPrismarine;
        blocks[3].1 = BlockKind::PrismarineBricks;
        assert_eq!(count(&blocks, true), 41);
    }

    #[test]
    fn activity_hunting_and_range_follow_the_frame_size() {
        assert!(!is_active(15));
        assert!(is_active(16));
        assert!(!can_hunt(41));
        assert!(can_hunt(42));
        assert_eq!(effect_range(16), 32);
        assert_eq!(effect_range(42), 96);
    }
}
