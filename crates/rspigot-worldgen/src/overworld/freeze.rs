use rspigot_blocks::{Face, Support};
use rspigot_domain::{BlockStateId, ChunkPos};

use super::surface_rules::SurfaceSystem;
use crate::buffer::ChunkBuffer;
use crate::classify::{Classifier, MOTION_BLOCKING};
use crate::resources::{BlockKind, Palette};

pub(crate) struct FreezeContext<'a> {
    pub(crate) system: &'a SurfaceSystem,
    pub(crate) classifier: &'a Classifier,
    pub(crate) palette: &'a Palette,
    pub(crate) sea_level: i32,
    pub(crate) scan_from: i32,
}

fn cannot_hold_snow(state: BlockStateId) -> bool {
    matches!(
        state.kind().map(rspigot_blocks::BlockKind::name),
        Some("minecraft:ice" | "minecraft:packed_ice" | "minecraft:blue_ice" | "minecraft:barrier")
    )
}

fn can_hold_snow(state: BlockStateId) -> bool {
    if cannot_hold_snow(state) {
        return false;
    }
    if state.kind().map(rspigot_blocks::BlockKind::name) == Some("minecraft:snow") {
        return state.property("layers") == Some("8");
    }
    state.is_face_sturdy(Face::Up, Support::Full)
        || matches!(
            state.kind().map(rspigot_blocks::BlockKind::name),
            Some("minecraft:soul_sand" | "minecraft:mud" | "minecraft:honey_block")
        )
}

pub(crate) fn freeze_top_layer(
    buffer: &mut ChunkBuffer,
    context: &FreezeContext<'_>,
    pos: ChunkPos,
    biome_at: impl Fn(usize, i32, usize) -> u8,
) {
    let height = buffer.height();
    let water = context.palette[BlockKind::Water];
    for z in 0..16usize {
        for x in 0..16usize {
            let mut free = context.scan_from.min(height.max_y());
            while free > height.min_y
                && context.classifier.flags(buffer.get(x, free - 1, z)) & MOTION_BLOCKING == 0
            {
                free -= 1;
            }
            if free <= height.min_y {
                continue;
            }
            let below = free - 1;
            let (wx, wz) = (pos.x * 16 + x as i32, pos.z * 16 + z as i32);
            let biome = biome_at(x, free, z);
            let cold = !context
                .system
                .warm_enough_to_rain(biome, wx, free, wz, context.sea_level);
            if !cold {
                continue;
            }
            let ground = buffer.get(x, below, z);
            if ground == water {
                buffer.set(x, below, z, context.palette[BlockKind::Ice]);
            }
            let at = buffer.get(x, free, z);
            let snow = context.palette[BlockKind::SnowLayer];
            let below_state = buffer.get(x, below, z);
            if (at.is_air() || at == snow) && can_hold_snow(below_state) {
                buffer.set(x, free, z, snow);
                if let Some(snowy) = below_state.with_property("snowy", "true") {
                    buffer.set(x, below, z, snowy);
                }
            }
        }
    }
}
