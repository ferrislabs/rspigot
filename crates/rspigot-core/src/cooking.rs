use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::furnace::{self, Answers, Furnace, Tick};
use rspigot_domain::item::Slot;
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_items::CookingKind;
use rspigot_nbt::{Compound, Tag};

use crate::containers::{read_items, write_items};
use crate::plugins::steps::ExpStep;
use crate::ports::ClientLink;
use crate::server::Server;

const COOKING_TIME_SPENT: &str = "cooking_time_spent";
const COOKING_TOTAL_TIME: &str = "cooking_total_time";
const LIT_TIME_REMAINING: &str = "lit_time_remaining";
const LIT_TOTAL_TIME: &str = "lit_total_time";
const SPEED_MULTIPLIER: &str = "speed_multiplier";
const RECIPES_USED: &str = "RecipesUsed";
const LIT: &str = "lit";
const FURNACE_SLOTS: usize = 3;

pub(crate) fn cooking_of(block: BlockKind) -> Option<CookingKind> {
    match block {
        BlockKind::Furnace => Some(CookingKind::Smelting),
        BlockKind::BlastFurnace => Some(CookingKind::Blasting),
        BlockKind::Smoker => Some(CookingKind::Smoking),
        _ => None,
    }
}

fn cooking_of_entity(kind: &Identifier) -> Option<CookingKind> {
    match kind.as_str() {
        "minecraft:furnace" => Some(CookingKind::Smelting),
        "minecraft:blast_furnace" => Some(CookingKind::Blasting),
        "minecraft:smoker" => Some(CookingKind::Smoking),
        _ => None,
    }
}

pub(crate) fn read_furnace(data: &Compound) -> Furnace {
    let mut items = read_items(data, FURNACE_SLOTS).into_iter();
    let mut furnace = Furnace {
        items: std::array::from_fn(|_| items.next().flatten()),
        lit_time_remaining: data.get_i32(LIT_TIME_REMAINING).unwrap_or(0),
        lit_total_time: data.get_i32(LIT_TOTAL_TIME).unwrap_or(0),
        cooking_time_spent: data.get_i32(COOKING_TIME_SPENT).unwrap_or(0),
        cooking_total_time: data.get_i32(COOKING_TOTAL_TIME).unwrap_or(0),
        ..Furnace::default()
    };
    if let Some(speed) = data.get_f64(SPEED_MULTIPLIER) {
        furnace.speed_multiplier = speed as f32;
    }
    if let Some(used) = data.get_compound(RECIPES_USED) {
        for (id, uses) in used.iter() {
            if let Some(uses) = uses.as_i64().and_then(|uses| u32::try_from(uses).ok()) {
                furnace.recipes_used.insert(id.to_owned(), uses);
            }
        }
    }
    furnace
}

pub(crate) fn write_furnace(furnace: &Furnace, data: &mut Compound) {
    write_items(data, &furnace.items);
    data.insert(LIT_TIME_REMAINING, furnace.lit_time_remaining);
    data.insert(LIT_TOTAL_TIME, furnace.lit_total_time);
    data.insert(COOKING_TIME_SPENT, furnace.cooking_time_spent);
    data.insert(COOKING_TOTAL_TIME, furnace.cooking_total_time);
    data.insert(SPEED_MULTIPLIER, Tag::Float(furnace.speed_multiplier));
    let mut used = Compound::new();
    for (id, uses) in &furnace.recipes_used {
        used.insert(id.clone(), i32::try_from(*uses).unwrap_or(i32::MAX));
    }
    data.insert(RECIPES_USED, used);
}

fn same_content(before: &Slot, after: &Slot) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            rspigot_domain::item::same_item_same_components(before, after)
        }
        _ => false,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn furnace_at(&self, pos: BlockPos) -> Option<Furnace> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| read_furnace(&entity.data))
    }

    pub(crate) fn store_furnace(&mut self, pos: BlockPos, furnace: &Furnace) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let Some(existing) = chunk.block_entity(pos) else {
            return;
        };
        let (kind, mut data) = (existing.kind.clone(), existing.data.clone());
        write_furnace(furnace, &mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    pub(crate) fn furnace_data(&self, pos: BlockPos) -> Vec<i16> {
        self.furnace_at(pos)
            .map_or_else(|| vec![0; 4], |furnace| furnace.data().to_vec())
    }

    pub(crate) fn tick_furnaces(&mut self) {
        let mut found: Vec<(BlockPos, CookingKind)> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| cooking_of_entity(kind).is_some())
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(chunk.block_entities.iter().filter_map(|entity| {
                cooking_of_entity(&entity.kind).map(|kind| (entity.pos, kind))
            }));
        }
        for (pos, kind) in found {
            self.tick_furnace(pos, kind);
        }
    }

    fn tick_furnace(&mut self, pos: BlockPos, kind: CookingKind) {
        if self.furnace_asking(pos) {
            return;
        }
        let Some(furnace) = self.cooking_furnace(pos, kind) else {
            return;
        };
        self.run_furnace(pos, kind, &furnace, Answers::default());
    }

    pub(crate) fn cooking_furnace(&self, pos: BlockPos, kind: CookingKind) -> Option<Furnace> {
        let state = self.world.chunks.block(pos)?;
        if state.kind().and_then(cooking_of) != Some(kind) {
            return None;
        }
        self.furnace_at(pos)
    }

    pub(crate) fn furnace_ticked(&mut self, pos: BlockPos, furnace: &Furnace, tick: Tick) {
        self.store_furnace(pos, furnace);
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if tick.was_lit != tick.is_lit
            && let Some(lit) = state.with_property(LIT, if tick.is_lit { "true" } else { "false" })
        {
            self.set_block(pos, lit);
        }
    }

    pub(crate) fn furnace_clicked(
        &mut self,
        pos: BlockPos,
        kind: CookingKind,
        before: &[Slot],
        after: &[Slot],
    ) {
        let Some(mut furnace) = self.furnace_at(pos) else {
            return;
        };
        furnace.items = std::array::from_fn(|slot| after.get(slot).cloned().flatten());
        let input = furnace::INPUT_SLOT;
        let old = before.get(input).cloned().flatten();
        if !same_content(&old, &furnace.items[input]) {
            furnace.input_replaced(kind);
        }
        self.store_furnace(pos, &furnace);
    }

    pub(crate) fn furnace_experience_lost_by(
        &mut self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<i32> {
        if old.kind() == new.kind() || old.kind().and_then(cooking_of).is_none() {
            return Vec::new();
        }
        let Some(furnace) = self.furnace_at(pos) else {
            return Vec::new();
        };
        let rng = &mut self.rng;
        furnace.owed(&mut || rng.random::<f32>())
    }

    pub(crate) fn pop_furnace_experience(&mut self, pos: BlockPos, amounts: Vec<i32>) {
        for amount in amounts {
            self.ask_block_exp(ExpStep {
                pos,
                amount,
                at: pos.center(),
            });
        }
    }
}
