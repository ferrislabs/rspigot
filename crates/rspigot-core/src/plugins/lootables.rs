use rspigot_domain::{BlockPos, EntityId};

use crate::plugins::resolve::within;
use crate::plugins::{Answer, LookupAnswer};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::vehicles::{cargo_mut, cargo_of};

const LOOT_TABLE: &str = "LootTable";
const LOOT_TABLE_SEED: &str = "LootTableSeed";
const LOOTABLE_BLOCKS: [&str; 9] = [
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:barrel",
    "minecraft:hopper",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:shulker_box",
    "minecraft:decorated_pot",
    "minecraft:suspicious_sand",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootTarget {
    Block(BlockPos),
    Entity(EntityId),
}

fn checked(table: Option<String>) -> Result<Option<String>, String> {
    let Some(table) = table else {
        return Ok(None);
    };
    let name = if table.contains(':') {
        table
    } else {
        format!("minecraft:{table}")
    };
    if rspigot_data::loot::table(&name).is_none() {
        return Err(format!("unknown loot table {name}"));
    }
    Ok(Some(name))
}

fn lootable_block(name: &str) -> bool {
    LOOTABLE_BLOCKS.contains(&name) || name.ends_with("shulker_box")
}

impl<L: ClientLink> Server<L> {
    pub(super) fn set_block_loot_table(
        &mut self,
        pos: BlockPos,
        table: Option<String>,
        seed: i64,
    ) -> Result<(), String> {
        let height = self.world.dimension.height;
        within("y", pos.y, height.min_y..=height.max_y())?;
        let table = checked(table)?;
        let chunk = self
            .world
            .chunks
            .chunk_mut(pos.chunk())
            .ok_or_else(|| "the chunk is not loaded".to_owned())?;
        let mut entity = chunk
            .block_entity(pos)
            .filter(|entity| lootable_block(&entity.kind.to_string()))
            .cloned()
            .ok_or_else(|| "the block holds no loot".to_owned())?;
        match table {
            Some(name) => {
                entity.data.insert(LOOT_TABLE, name);
                entity.data.insert(LOOT_TABLE_SEED, seed);
            }
            None => {
                entity.data.remove(LOOT_TABLE);
                entity.data.remove(LOOT_TABLE_SEED);
            }
        }
        chunk.set_block_entity(entity);
        Ok(())
    }

    pub(super) fn set_entity_loot_table(
        &mut self,
        id: EntityId,
        table: Option<String>,
        seed: i64,
    ) -> Result<(), String> {
        let table = checked(table)?;
        let entity = self
            .entities
            .store
            .get_mut(id)
            .ok_or_else(|| "the entity is gone".to_owned())?;
        if let Some(cargo) = cargo_mut(entity) {
            cargo.loot_seed = if table.is_some() { seed } else { 0 };
            cargo.loot_table = table;
            return Ok(());
        }
        let mob = entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .ok_or_else(|| "the entity has no loot".to_owned())?;
        mob.death_loot_table_seed = if table.is_some() { seed } else { 0 };
        mob.death_loot_table = table;
        Ok(())
    }

    pub(super) fn lootable_answer(&self, target: LootTarget) -> Answer {
        let found = match target {
            LootTarget::Block(pos) => self
                .world
                .chunks
                .get(pos.chunk())
                .and_then(|chunk| chunk.block_entity(pos))
                .filter(|entity| lootable_block(&entity.kind.to_string()))
                .map(|entity| {
                    (
                        entity.data.get_str(LOOT_TABLE).map(str::to_owned),
                        entity.data.get_i64(LOOT_TABLE_SEED).unwrap_or(0),
                    )
                }),
            LootTarget::Entity(id) => self.entities.store.get(id).and_then(|entity| {
                if let Some(cargo) = cargo_of(entity) {
                    return Some((cargo.loot_table.clone(), cargo.loot_seed));
                }
                let mob = entity.living()?.mob.as_ref()?;
                Some((mob.death_loot_table.clone(), mob.death_loot_table_seed))
            }),
        };
        match found {
            Some((table, seed)) => Answer::Lookup(LookupAnswer::Lootable { table, seed }),
            None => Answer::NotFound,
        }
    }
}
