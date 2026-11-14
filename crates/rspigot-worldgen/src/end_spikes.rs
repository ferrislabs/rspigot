use rspigot_blocks::BlockKind;
use rspigot_domain::dragon::spikes::{self, EndSpike, SpikeBlock};
use rspigot_domain::entity::{Entity, EntityBase, EntityState, crystal, save};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId};
use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::buffer::ChunkBuffer;

const CAGE_MARGIN: i32 = 2;
const BARS_BITS: usize = 16;

#[derive(Debug, Clone)]
pub(crate) struct Pillars {
    spikes: Vec<EndSpike>,
    obsidian: BlockStateId,
    bedrock: BlockStateId,
    fire: BlockStateId,
    bars: [BlockStateId; BARS_BITS],
}

fn bool_name(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

impl Pillars {
    pub(crate) fn new(seed: i64) -> Self {
        Self {
            spikes: spikes::spikes_for_seed(seed),
            obsidian: BlockKind::Obsidian.default_state(),
            bedrock: BlockKind::Bedrock.default_state(),
            fire: BlockKind::Fire.default_state(),
            bars: std::array::from_fn(|bits| {
                BlockKind::IronBars
                    .state_with(&[
                        ("north", bool_name(bits & 1 != 0)),
                        ("south", bool_name(bits & 2 != 0)),
                        ("west", bool_name(bits & 4 != 0)),
                        ("east", bool_name(bits & 8 != 0)),
                    ])
                    .unwrap_or_else(|| BlockKind::IronBars.default_state())
            }),
        }
    }

    #[cfg(test)]
    fn is_bars(&self, state: BlockStateId) -> bool {
        self.bars.contains(&state)
    }

    #[cfg(test)]
    pub(crate) fn spikes(&self) -> &[EndSpike] {
        &self.spikes
    }

    fn touches(spike: &EndSpike, pos: ChunkPos) -> bool {
        let reach = spike.radius.max(CAGE_MARGIN);
        let (min_x, min_z) = (pos.x * 16, pos.z * 16);
        spike.center_x + reach >= min_x
            && spike.center_x - reach <= min_x + 15
            && spike.center_z + reach >= min_z
            && spike.center_z - reach <= min_z + 15
    }

    fn state_of(&self, block: SpikeBlock) -> BlockStateId {
        match block {
            SpikeBlock::Obsidian => self.obsidian,
            SpikeBlock::Air => BlockStateId::AIR,
            SpikeBlock::Bedrock => self.bedrock,
            SpikeBlock::Fire => self.fire,
            SpikeBlock::IronBars {
                north,
                south,
                west,
                east,
            } => {
                self.bars[usize::from(north)
                    | usize::from(south) << 1
                    | usize::from(west) << 2
                    | usize::from(east) << 3]
            }
        }
    }

    pub(crate) fn place(&self, buffer: &mut ChunkBuffer, pos: ChunkPos) {
        let min_y = buffer.height().min_y;
        for spike in self.spikes.iter().filter(|spike| Self::touches(spike, pos)) {
            let mut put = |at: BlockPos, block: SpikeBlock| {
                if at.chunk() == pos {
                    buffer.set(
                        (at.x & 15) as usize,
                        at.y,
                        (at.z & 15) as usize,
                        self.state_of(block),
                    );
                }
            };
            for (at, block) in spikes::spike_blocks(spike, min_y) {
                put(at, block);
            }
            let top = spike.top_block();
            put(top, SpikeBlock::Bedrock);
            put(top.offset(0, 1, 0), SpikeBlock::Fire);
        }
    }

    pub(crate) fn crystals(&self, pos: ChunkPos) -> Vec<Compound> {
        let origin = BlockPos::new(pos.x * 16, 0, pos.z * 16);
        self.spikes
            .iter()
            .filter(|spike| spike.is_center_within_chunk(origin))
            .filter_map(|spike| {
                let mut base = EntityBase::new(EntityId(0), Uuid::nil(), EntityKind::EndCrystal);
                let state = crystal::define(&mut base);
                let (x, y, z) = spike.crystal_spot();
                base.snap_to(rspigot_domain::Vec3::new(x, y, z), 0.0, 0.0);
                let mut nbt =
                    save::save_entity(&Entity::new(base, EntityState::Crystal(state)), None)?;
                nbt.remove("UUID");
                Some(nbt)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::world::Dimension;

    use rspigot_domain::chunk::ChunkColumn;

    use super::*;
    use crate::end::End;
    use crate::resources::Resources;

    const SEED: i64 = 4242;

    fn generate(end: &End, resources: &Resources, pos: ChunkPos) -> ChunkColumn {
        end.generate(resources, &Dimension::end(), pos)
    }

    #[test]
    fn ten_pillars_stand_at_the_vanilla_positions_of_the_seed() {
        let end = End::new(SEED as u64);
        let resources = Resources::resolve();
        let expected = spikes::spikes_for_seed(SEED);
        assert_eq!(end.pillars().spikes(), expected.as_slice());
        let obsidian = BlockKind::Obsidian.default_state();
        for spike in &expected {
            let chunk = BlockPos::new(spike.center_x, 0, spike.center_z).chunk();
            let column = generate(&end, &resources, chunk);
            assert_eq!(
                column.block(BlockPos::new(
                    spike.center_x + 1,
                    spike.height - 1,
                    spike.center_z
                )),
                Some(obsidian),
                "{spike:?}"
            );
            assert_eq!(
                column.block(BlockPos::new(spike.center_x, spike.height, spike.center_z)),
                Some(BlockKind::Bedrock.default_state())
            );
            assert_eq!(
                column.block(BlockPos::new(
                    spike.center_x,
                    spike.height + 1,
                    spike.center_z
                )),
                Some(BlockKind::Fire.default_state())
            );
        }
    }

    #[test]
    fn a_pillar_crossing_a_chunk_border_is_built_in_both_chunks() {
        let end = End::new(SEED as u64);
        let resources = Resources::resolve();
        let obsidian = BlockKind::Obsidian.default_state();
        for spike in end.pillars().spikes() {
            let mut seen = std::collections::HashSet::new();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let chunk = BlockPos::new(
                        spike.center_x + dx * spike.radius,
                        0,
                        spike.center_z + dz * spike.radius,
                    )
                    .chunk();
                    if !seen.insert((chunk.x, chunk.z)) {
                        continue;
                    }
                    let column = generate(&end, &resources, chunk);
                    for x in spike.center_x - spike.radius..=spike.center_x + spike.radius {
                        for z in spike.center_z - spike.radius..=spike.center_z + spike.radius {
                            let at = BlockPos::new(x, spike.height - 1, z);
                            let ddx = f64::from(x - spike.center_x);
                            let ddz = f64::from(z - spike.center_z);
                            let inside =
                                ddx * ddx + ddz * ddz <= f64::from(spike.radius * spike.radius + 1);
                            if at.chunk() == chunk && inside {
                                assert_eq!(column.block(at), Some(obsidian), "{spike:?} {at:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn guarded_pillars_wear_a_cage_of_bars_and_the_others_do_not() {
        let end = End::new(SEED as u64);
        let resources = Resources::resolve();
        let mut guarded = 0;
        for spike in end.pillars().spikes() {
            let chunk = BlockPos::new(spike.center_x, 0, spike.center_z).chunk();
            let column = generate(&end, &resources, chunk);
            let wall = BlockPos::new(spike.center_x, spike.height + 3, spike.center_z);
            let bars = column
                .block(wall)
                .is_some_and(|state| end.pillars().is_bars(state));
            assert_eq!(bars, spike.guarded, "{spike:?}");
            guarded += usize::from(bars);
        }
        assert_eq!(guarded, 2);
    }

    #[test]
    fn each_pillar_carries_one_crystal_on_top_in_its_own_chunk() {
        let end = End::new(SEED as u64);
        let mut total = 0;
        for spike in end.pillars().spikes() {
            let chunk = BlockPos::new(spike.center_x, 0, spike.center_z).chunk();
            let found = end.pillars().crystals(chunk);
            let (x, y, z) = spike.crystal_spot();
            let at = found
                .iter()
                .filter(|nbt| nbt.get_str("id") == Some("minecraft:end_crystal"))
                .filter(|nbt| {
                    save::entity_position(nbt) == Some(rspigot_domain::Vec3::new(x, y, z))
                })
                .count();
            assert_eq!(at, 1, "{spike:?}");
            assert!(found.iter().all(|nbt| nbt.get("UUID").is_none()));
            total += found.len();
        }
        assert_eq!(total, 10);
        assert!(end.pillars().crystals(ChunkPos::new(500, 500)).is_empty());
    }
}
