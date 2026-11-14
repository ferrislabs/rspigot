use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_items::ItemKind;

use crate::events::Direction;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_end_crystal(
        &mut self,
        id: PlayerId,
        slot: usize,
        pos: BlockPos,
        face: Direction,
        clicked: BlockStateId,
    ) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let held = player.inventory.get(slot).cloned();
        if held.as_ref().map(|stack| stack.item) != Some(ItemKind::EndCrystal.id()) {
            return false;
        }
        if matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator) {
            return false;
        }
        if !matches!(
            clicked.kind(),
            Some(rspigot_blocks::BlockKind::Obsidian | rspigot_blocks::BlockKind::Bedrock)
        ) {
            return false;
        }
        let (dx, dy, dz) = face.offset();
        let above = pos.offset(dx, dy, dz);
        if !self.within_reach(id, pos) || self.is_protected(id, above) {
            return false;
        }
        let space = self
            .world
            .chunks
            .block(above)
            .is_some_and(BlockStateId::is_air)
            && self
                .world
                .chunks
                .block(above.offset(0, 1, 0))
                .is_some_and(BlockStateId::is_air);
        if !space {
            return false;
        }
        let area = rspigot_domain::entity::Aabb::new(
            f64::from(above.x),
            f64::from(above.y),
            f64::from(above.z),
            f64::from(above.x) + 1.0,
            f64::from(above.y) + 2.0,
            f64::from(above.z) + 1.0,
        );
        let blocked = self.entities.store.iter().any(|entity| {
            !entity.base.is_removed() && entity.base.bounding_box().intersects(&area)
        }) || self.intersects_player(above);
        if blocked {
            return false;
        }
        let at = rspigot_domain::Vec3::new(
            f64::from(above.x) + 0.5,
            f64::from(above.y),
            f64::from(above.z) + 0.5,
        );
        let entity = self.new_crystal(at);
        self.add_entity(entity, CreatureSpawnReason::Default);
        self.consume_end_crystal(id, slot);
        self.fight_try_respawn();
        true
    }

    fn consume_end_crystal(&mut self, id: PlayerId, slot: usize) {
        if self.infinite_materials(id) {
            return;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(held) = player.inventory.get(slot) else {
            return;
        };
        let remaining = held.count.get() - 1;
        let left =
            std::num::NonZeroU8::new(remaining).map(|count| rspigot_domain::item::ItemStack {
                count,
                ..held.clone()
            });
        player.inventory.set(slot, left);
    }
}
