use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::EntityId;
use rspigot_domain::entity::raycast::BlockHit;
use rspigot_entities::SoundSource;

use crate::ports::ClientLink;
use crate::server::Server;

const AMETHYST_HIT: &str = "minecraft:block.amethyst_block.hit";
const AMETHYST_CHIME: &str = "minecraft:block.amethyst_block.chime";
const PITCH_BASE: f32 = 0.5;
const PITCH_SPREAD: f32 = 1.2;

fn is_amethyst(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::AmethystBlock
            | BlockKind::BuddingAmethyst
            | BlockKind::SmallAmethystBud
            | BlockKind::MediumAmethystBud
            | BlockKind::LargeAmethystBud
            | BlockKind::AmethystCluster
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn projectile_block_effects(&mut self, projectile: EntityId, hit: BlockHit) {
        let state = self.block_or_air(hit.pos);
        let Some(kind) = state.kind() else {
            return;
        };
        match kind {
            BlockKind::Bell => {
                self.bell_hit_by_projectile(hit.pos, state, hit.face, hit.location.y)
            }
            BlockKind::BigDripleaf => self.leaf_projectile_hit(hit.pos),
            BlockKind::PointedDripstone => self.trident_breaks_dripstone(projectile, hit.pos),
            _ if is_amethyst(kind) => {
                for sound in [AMETHYST_HIT, AMETHYST_CHIME] {
                    let pitch = PITCH_BASE + self.rng.random::<f32>() * PITCH_SPREAD;
                    self.play_sound(
                        None,
                        hit.pos.center(),
                        sound,
                        SoundSource::Blocks,
                        1.0,
                        pitch,
                    );
                }
            }
            _ => {}
        }
    }
}
