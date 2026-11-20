use rspigot_domain::BlockPos;
use rspigot_domain::cartography::{self, LOCK, MAP_ID, MAP_POST_PROCESSING, MapFacts, SCALE};
use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::item::{ItemStack, Slot, count};
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;

use crate::ports::ClientLink;
use crate::server::Server;

const TAKE_RESULT: &str = "minecraft:ui.cartography_table.take_result";

enum Place {
    Inventory(usize),
    Cursor,
}

pub(crate) fn both_inputs_used(before: &[Slot], after: &[Slot]) -> bool {
    [cartography::MAP_SLOT, cartography::MATERIAL_SLOT]
        .into_iter()
        .all(|index| count(&after.get(index).cloned().flatten()) < count(&before[index]))
        && before
            .get(cartography::RESULT_SLOT)
            .is_some_and(Option::is_some)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn map_facts_for(&mut self, id: PlayerId) -> Vec<(i32, MapFacts)> {
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
        };
        let Some(open) = player.open_menu.as_ref() else {
            return Vec::new();
        };
        if !matches!(
            open.menu.kind(),
            MenuKind::Crafting | MenuKind::CartographyTable
        ) {
            return Vec::new();
        }
        let mut ids: Vec<i32> = player
            .inventory
            .slots()
            .iter()
            .chain(open.local.iter())
            .chain(std::iter::once(open.menu.carried()))
            .flatten()
            .filter_map(cartography::map_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .filter_map(|map| Some((map, self.map_facts(map)?)))
            .collect()
    }

    fn post_processed(&mut self, stack: &ItemStack) -> ItemStack {
        let mut done = stack.clone();
        let Some(kind) = cartography::pending(stack).map(str::to_owned) else {
            return done;
        };
        done.components.remove(MAP_POST_PROCESSING);
        let Some(old) = cartography::map_id(stack) else {
            return done;
        };
        let fresh = match kind.as_str() {
            SCALE => self.derived_map(old, |data| Some(cartography::scaled(data))),
            LOCK => self.derived_map(old, cartography::locked_copy),
            _ => None,
        };
        if let Some(fresh) = fresh {
            done.components.insert(MAP_ID, fresh);
        }
        done
    }

    pub(crate) fn resolve_map_post_processing(&mut self, id: PlayerId) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let mut found: Vec<(Place, ItemStack)> = player
            .inventory
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| Some((Place::Inventory(slot), stack.clone()?)))
            .filter(|(_, stack)| cartography::pending(stack).is_some())
            .collect();
        if let Some(open) = player.open_menu.as_ref()
            && let Some(carried) = open.menu.carried().clone()
            && cartography::pending(&carried).is_some()
        {
            found.push((Place::Cursor, carried));
        }
        let mut cursor = false;
        for (place, stack) in found {
            let done = self.post_processed(&stack);
            let Some(player) = self.players.get_mut(&id) else {
                return cursor;
            };
            match place {
                Place::Inventory(slot) => {
                    player.inventory.set(slot, Some(done));
                }
                Place::Cursor => {
                    if let Some(open) = player.open_menu.as_mut() {
                        open.menu.set_carried(Some(done));
                        cursor = true;
                    }
                }
            }
        }
        cursor
    }

    pub(crate) fn cartography_taken(&mut self, pos: BlockPos) {
        self.play_sound(
            None,
            pos.center(),
            TAKE_RESULT,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }
}
