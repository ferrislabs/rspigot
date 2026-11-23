use rspigot_domain::{BiomeId, BlockPos};

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(super) fn set_block_biome(&mut self, pos: BlockPos, biome: &str) -> Result<(), String> {
        let name = if biome.contains(':') {
            biome.to_owned()
        } else {
            format!("minecraft:{biome}")
        };
        let id = rspigot_data::registries::biome_id(&name)
            .ok_or_else(|| format!("unknown biome {name}"))?;
        let min_y = self.world.dimension.height.min_y;
        let chunk = pos.chunk();
        let column = self
            .world
            .chunks
            .chunk_mut(chunk)
            .ok_or_else(|| "the chunk is not loaded".to_owned())?;
        let relative = usize::try_from(pos.y - min_y)
            .map_err(|_| format!("y {} is below the world", pos.y))?;
        let section = column
            .sections_mut()
            .get_mut(relative >> 4)
            .ok_or_else(|| format!("y {} is above the world", pos.y))?;
        let cell = |value: usize| (value & 15) >> 2;
        section.set_biome(
            cell(usize::try_from(pos.x & 15).unwrap_or(0)),
            cell(relative),
            cell(usize::try_from(pos.z & 15).unwrap_or(0)),
            BiomeId(id),
        );
        for player in self.players.values_mut() {
            player.streamer.resend(chunk);
        }
        Ok(())
    }
}
