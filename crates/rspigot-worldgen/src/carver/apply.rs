use rspigot_domain::BlockStateId;

use super::mask::Mask;
use crate::buffer::ChunkBuffer;

fn named(state: BlockStateId) -> &'static str {
    state.kind().map_or("", rspigot_blocks::BlockKind::name)
}

pub(crate) fn apply<F>(mask: &Mask, buffer: &mut ChunkBuffer, origin: (i32, i32), mut substance: F)
where
    F: FnMut(i32, i32, i32) -> Option<BlockStateId>,
{
    let height = buffer.height();
    for x in 0..16usize {
        for z in 0..16usize {
            let (wx, wz) = (origin.0 + x as i32, origin.1 + z as i32);
            for (low, high) in mask.segments(x, z) {
                let mut grass = None;
                for y in (low.max(height.min_y)..=high.min(height.max_y())).rev() {
                    let current = buffer.get(x, y, z);
                    let name = named(current);
                    if name == "minecraft:bedrock" {
                        continue;
                    }
                    if name == "minecraft:grass_block" || name == "minecraft:mycelium" {
                        grass = Some(current);
                    }
                    let Some(new) = substance(wx, y, wz) else {
                        continue;
                    };
                    buffer.set(x, y, z, new);
                    if let Some(top) = grass
                        && y > height.min_y
                        && new.is_air()
                        && named(buffer.get(x, y - 1, z)) == "minecraft:dirt"
                    {
                        buffer.set(x, y - 1, z, top);
                    }
                }
            }
        }
    }
}
