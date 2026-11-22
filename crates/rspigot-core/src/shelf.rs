use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::GameMode;
use rspigot_domain::shelf::{SLOTS, hit_slot};
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_entities::SoundSource;
use rspigot_nbt::Compound;

use crate::containers::write_items;
use crate::events::Direction;
use crate::plugins::steps::UseOnStep;
use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY: &str = "shelf";
const PLACE_SOUND: &str = "minecraft:block.shelf.place_item";
const SWAP_SOUND: &str = "minecraft:block.shelf.single_swap";

pub(crate) const fn is_shelf_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::AcaciaShelf
            | BlockKind::BambooShelf
            | BlockKind::BirchShelf
            | BlockKind::CherryShelf
            | BlockKind::CrimsonShelf
            | BlockKind::DarkOakShelf
            | BlockKind::JungleShelf
            | BlockKind::MangroveShelf
            | BlockKind::OakShelf
            | BlockKind::PaleOakShelf
            | BlockKind::PoplarShelf
            | BlockKind::SpruceShelf
            | BlockKind::WarpedShelf
    )
}

pub(crate) fn is_shelf(state: BlockStateId) -> bool {
    state.kind().is_some_and(is_shelf_kind)
}

const fn face_name(face: Direction) -> &'static str {
    match face {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_shelf_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_shelf(state) {
            return;
        }
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        if chunk.block_entity(pos).is_none() {
            let mut data = Compound::new();
            write_items(&mut data, &[]);
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(ENTITY),
                data,
            });
        }
    }

    fn shelf_items(&self, pos: BlockPos) -> Vec<Option<ItemStack>> {
        self.half_items(pos, SLOTS)
    }

    fn store_shelf(&mut self, pos: BlockPos, items: &[Option<ItemStack>]) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(pos)
            .map_or_else(Compound::new, |entity| entity.data.clone());
        write_items(&mut data, items);
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft(ENTITY),
            data,
        });
        self.update_neighbour_for_output_signal(pos);
    }

    pub(crate) fn shelf_items_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<ItemStack> {
        if !is_shelf(old) || old.kind() == new.kind() {
            return Vec::new();
        }
        self.shelf_items(pos).into_iter().flatten().collect()
    }

    pub(crate) fn use_shelf(&mut self, step: &UseOnStep) -> bool {
        let (id, hand, pos, clicked) = (step.player, step.hand, step.pos, step.clicked);
        let (face, cursor, sequence) = (step.face, step.cursor, step.sequence);
        if !is_shelf(clicked) || clicked.property("powered") == Some("true") {
            return false;
        }
        let Some(facing) = clicked.property("facing") else {
            return false;
        };
        let Some(slot) = hit_slot(facing, face_name(face), cursor.x, cursor.z) else {
            return false;
        };
        let allowed = self.players.get(&id).is_some_and(|player| {
            matches!(player.game_mode, GameMode::Survival | GameMode::Creative)
        });
        if !allowed {
            return false;
        }
        let mut items = self.shelf_items(pos);
        let held = self.held(id, hand);
        if held.is_none() && items[slot].is_none() {
            return false;
        }
        let placing = items[slot].is_none();
        let taken = std::mem::replace(&mut items[slot], held);
        self.set_held(id, hand, taken);
        self.store_shelf(pos, &items);
        let sound = if placing { PLACE_SOUND } else { SWAP_SOUND };
        self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        self.ack_block(id, sequence);
        true
    }
}
