use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_items::ItemKind;

use rspigot_domain::entity::crystal;
use rspigot_domain::game_event::GameEvent;

use crate::events::Direction;
use crate::game_events::GameEventContext;
use crate::health::OFFHAND_SLOT;
use crate::plugins::events::{CreatureSpawnReason, EntityPlace, EquipmentSlot, Face};
use crate::plugins::steps::CrystalPlaceStep;
use crate::plugins::{Continuation, EventPayload};
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
        let above = pos.offset(0, 1, 0);
        if !self.within_reach(id, pos) || self.is_protected(id, above) {
            return false;
        }
        if !self
            .world
            .chunks
            .block(above)
            .is_some_and(BlockStateId::is_air)
        {
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
        let mut entity = self.new_crystal(at);
        crystal::set_shows_bottom(&mut entity.base, false);
        let view = crate::plugins::view_of_entity(&entity);
        let step = CrystalPlaceStep {
            player: id,
            slot,
            pos,
            entity,
        };
        let lane = self.lane_for(Some(id), view.id);
        self.fire(
            lane,
            Continuation::PlaceCrystal(Box::new(step)),
            move |server| {
                Some(EventPayload::EntityPlace(EntityPlace {
                    block: pos,
                    block_face: place_face(face),
                    entity_type: view.kind.clone(),
                    entity: view,
                    hand: if slot == OFFHAND_SLOT {
                        EquipmentSlot::OffHand
                    } else {
                        EquipmentSlot::Hand
                    },
                    player: server.snapshot_of(id),
                }))
            },
        );
        true
    }

    pub(crate) fn finish_crystal_place(&mut self, step: CrystalPlaceStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let CrystalPlaceStep {
            player,
            slot,
            pos,
            entity,
            ..
        } = step;
        let at = pos.offset(0, 1, 0);
        self.add_entity(entity, CreatureSpawnReason::Default);
        self.consume_end_crystal(player, slot);
        let placer = self.players.get(&player).map(|p| p.entity_id);
        self.game_event(
            GameEvent::EntityPlace,
            rspigot_domain::Vec3::new(f64::from(at.x), f64::from(at.y), f64::from(at.z)),
            GameEventContext::of(placer),
        );
        self.fight_try_respawn();
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

const fn place_face(face: Direction) -> Face {
    match face {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}
