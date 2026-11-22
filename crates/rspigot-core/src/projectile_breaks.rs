use rspigot_blocks::BlockKind;
use rspigot_domain::entity::EntityState;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use uuid::Uuid;

use crate::ports::ClientLink;
use crate::server::Server;

const CRACKED: &str = "cracked";

const fn is_impact_projectile(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Arrow
            | Kind::SpectralArrow
            | Kind::FireworkRocket
            | Kind::Snowball
            | Kind::Fireball
            | Kind::SmallFireball
            | Kind::Egg
            | Kind::Trident
            | Kind::DragonFireball
            | Kind::WitherSkull
            | Kind::WindCharge
            | Kind::BreezeWindCharge
    )
}

const fn breaks_to_projectiles(kind: BlockKind) -> bool {
    matches!(kind, BlockKind::ChorusFlower | BlockKind::DecoratedPot)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn projectile_hits_block(&mut self, projectile: EntityId, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if !state.kind().is_some_and(breaks_to_projectiles)
            || !self.projectile_may_interact(projectile)
            || !self.projectile_may_break(projectile)
        {
            return;
        }
        if state.kind() == Some(BlockKind::DecoratedPot)
            && let Some(cracked) = state.with_property(CRACKED, "true")
        {
            self.set_block(pos, cracked);
        }
        self.break_block_naturally(pos);
    }

    pub(crate) fn projectile_may_break(&self, projectile: EntityId) -> bool {
        self.config.rules.projectiles_can_break_blocks
            && self
                .entities
                .store
                .get(projectile)
                .is_some_and(|entity| is_impact_projectile(entity.base.kind))
    }

    fn projectile_may_interact(&self, projectile: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(projectile) else {
            return false;
        };
        let owner: Option<Uuid> = match &entity.state {
            EntityState::Arrow(state) => state.owner,
            EntityState::Thrown(state) => state.owner,
            EntityState::Hurting(state) => state.owner,
            _ => None,
        };
        match owner {
            None => true,
            Some(uuid) => {
                self.players
                    .values()
                    .any(|player| player.profile.id.0 == uuid)
                    || self.config.rules.mob_griefing
            }
        }
    }
}
