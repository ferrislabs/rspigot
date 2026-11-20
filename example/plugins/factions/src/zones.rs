use std::sync::{Arc, RwLock};

use rspigot_plugin::prelude::*;

const RADIUS: i32 = 8;

#[derive(Debug, Clone, Copy)]
struct Claim {
    center: BlockPos,
}

impl Claim {
    const fn contains(self, pos: BlockPos) -> bool {
        (pos.x - self.center.x).abs() <= RADIUS && (pos.z - self.center.z).abs() <= RADIUS
    }
}

#[derive(Debug, Clone, Default)]
pub struct Zones {
    claims: Arc<RwLock<Vec<Claim>>>,
}

impl Zones {
    pub fn claim(&self, center: BlockPos) {
        if let Ok(mut claims) = self.claims.write() {
            claims.push(Claim { center });
        }
    }

    pub fn clear(&self) -> usize {
        self.claims
            .write()
            .map_or(0, |mut claims| std::mem::take(&mut *claims).len())
    }

    pub fn count(&self) -> usize {
        self.claims.read().map_or(0, |claims| claims.len())
    }

    pub fn protects(&self, pos: BlockPos) -> bool {
        self.claims
            .read()
            .is_ok_and(|claims| claims.iter().any(|claim| claim.contains(pos)))
    }
}

#[listeners]
impl Zones {
    async fn on_block_break(&self, event: &mut BlockBreak) {
        if self.protects(event.block().pos) {
            event.player().message.send("This land is claimed");
            event.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_protects_the_square_around_its_center_and_nothing_else() {
        let zones = Zones::default();
        let center = BlockPos {
            x: 10,
            y: 64,
            z: -4,
        };
        zones.claim(center);
        assert!(zones.protects(BlockPos {
            x: 18,
            y: 1,
            z: -12
        }));
        assert!(!zones.protects(BlockPos {
            x: 19,
            y: 64,
            z: -4
        }));
        assert_eq!(zones.clear(), 1);
        assert!(!zones.protects(center));
    }

    #[test]
    fn a_clone_shares_the_claims() {
        let zones = Zones::default();
        let shared = zones.clone();
        shared.claim(BlockPos { x: 0, y: 0, z: 0 });
        assert_eq!(zones.count(), 1);
    }
}
