use std::collections::VecDeque;

use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;
use rustc_hash::FxHashSet;

use crate::loot::{self, LootContext};
use crate::plugins::events::SpongeAbsorb;
use crate::plugins::steps::SpongeStep;
use crate::plugins::{Continuation, EventPayload, Lane, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_DEPTH: u32 = 6;
const MAX_COUNT: usize = 65;
const ABSORB_SOUND: &str = "minecraft:block.sponge.absorb";
const DRIES_SOUND: &str = "minecraft:block.wet_sponge.dries";
const WATERLOGGED: &str = "waterlogged";
const NEIGHBOURS: [Face; 6] = [
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
    Face::West,
    Face::East,
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Absorbed {
    pub(crate) pos: BlockPos,
    pub(crate) before: BlockStateId,
    pub(crate) after: BlockStateId,
}

fn is_water_plant(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Kelp | BlockKind::KelpPlant | BlockKind::Seagrass | BlockKind::TallSeagrass
    )
}

fn absorbed_by_sponge(state: BlockStateId) -> Option<BlockStateId> {
    match state.kind()? {
        BlockKind::Water | BlockKind::BubbleColumn => Some(BlockStateId::AIR),
        kind if is_water_plant(kind) => Some(BlockStateId::AIR),
        _ if state.property(WATERLOGGED) == Some("true") => {
            state.with_property(WATERLOGGED, "false")
        }
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn wet_sponge_tick(&mut self, pos: BlockPos) {
        if !self.water_evaporates() {
            return;
        }
        self.set_block(pos, BlockKind::Sponge.default_state());
        let pitch = (1.0 + self.rng.random::<f32>() * 0.2) * 0.7;
        self.play_sound(
            None,
            pos.center(),
            DRIES_SOUND,
            SoundSource::Blocks,
            1.0,
            pitch,
        );
    }

    pub(crate) fn sponge_tick(&mut self, pos: BlockPos) {
        let Some(expected) = self.world.chunks.block(pos) else {
            return;
        };
        let changes = self.sponge_reach(pos);
        if changes.is_empty() {
            return;
        }
        let step = SpongeStep {
            pos,
            expected,
            changes: changes.clone(),
        };
        self.fire(
            Lane::Block(pos),
            Continuation::Sponge(Box::new(step)),
            move |_| {
                Some(EventPayload::SpongeAbsorb(SpongeAbsorb {
                    block: pos,
                    blocks: changes
                        .iter()
                        .map(|change| snapshot_of(change.pos, change.after))
                        .collect(),
                }))
            },
        );
    }

    fn sponge_reach(&self, start: BlockPos) -> Vec<Absorbed> {
        let mut changes = Vec::new();
        let mut visited: FxHashSet<BlockPos> = FxHashSet::default();
        let mut queue = VecDeque::from([(start, 0u32)]);
        let mut count = 0usize;
        while let Some((pos, depth)) = queue.pop_front() {
            if !visited.insert(pos) {
                continue;
            }
            if pos != start {
                let Some(before) = self.world.chunks.block(pos) else {
                    continue;
                };
                let Some(after) = absorbed_by_sponge(before) else {
                    continue;
                };
                changes.push(Absorbed { pos, before, after });
            }
            count += 1;
            if count >= MAX_COUNT {
                break;
            }
            if depth < MAX_DEPTH {
                for face in NEIGHBOURS {
                    let (dx, dy, dz) = face.offset();
                    queue.push_back((pos.offset(dx, dy, dz), depth + 1));
                }
            }
        }
        changes
    }

    pub(crate) fn finish_sponge(&mut self, step: SpongeStep, cancelled: bool) {
        if cancelled || self.world.chunks.block(step.pos) != Some(step.expected) {
            return;
        }
        for change in &step.changes {
            if self.world.chunks.block(change.pos) != Some(change.before) {
                continue;
            }
            if change.before.kind().is_some_and(is_water_plant) && self.config.rules.block_drops {
                let context = LootContext::block(change.before, None, None);
                for stack in loot::block_drops(&context, &mut self.rng) {
                    self.spawn_item(change.pos, stack);
                }
            }
            self.set_block(change.pos, change.after);
        }
        self.set_block(step.pos, BlockKind::WetSponge.default_state());
        self.play_sound(
            None,
            step.pos.center(),
            ABSORB_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }
}
