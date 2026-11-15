use super::place::Run;
use super::prov::Pos;
use crate::datapack::state_named;
use rspigot_domain::BlockStateId;

#[derive(Debug, Clone)]
pub(crate) struct EndIsland {
    stone: BlockStateId,
}

impl EndIsland {
    pub(crate) fn compile() -> Self {
        Self {
            stone: state_named("minecraft:end_stone", &[]).unwrap_or(BlockStateId::AIR),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        7
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let mut radius = run.rng.below(3) as f32 + 4.0;
        let mut dy = 0;
        while radius > 0.5 {
            let low = (-radius).floor() as i32;
            let high = radius.ceil() as i32;
            for dx in low..=high {
                for dz in low..=high {
                    if ((dx * dx + dz * dz) as f32) <= (radius + 1.0) * (radius + 1.0) {
                        run.region
                            .set(origin.0 + dx, origin.1 + dy, origin.2 + dz, self.stone);
                    }
                }
            }
            radius -= run.rng.below(2) as f32 + 0.5;
            dy -= 1;
        }
        true
    }
}

#[cfg(test)]
mod tests;
