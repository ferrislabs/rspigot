use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::grindstone;
use rspigot_domain::menu::Clicker;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;

const ANVIL_DAMAGE_CHANCE: f32 = 0.12;
const GRINDSTONE_USED_EVENT: i32 = 1042;
const ANVIL_USED_EVENT: i32 = 1030;
const ANVIL_DESTROYED_EVENT: i32 = 1029;

impl<L: ClientLink> Server<L> {
    pub(crate) fn rename_item(&mut self, id: PlayerId, name: &str) {
        let Some(window) = self.window_of(id) else {
            return;
        };
        let anvil = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .is_some_and(|open| open.menu.kind() == MenuKind::Anvil);
        if !anvil {
            return;
        }
        let grid = self.inputs_before(id, window);
        if !self.rename_in_menu(id, name) {
            if grid.held {
                self.release_result(id, window);
            }
            return;
        }
        self.sync_open_menu(id, false);
        self.sync_menu_data();
        let after = self.crafting_inputs(id, window);
        let carried = match after {
            Some(after) => self.fire_prepare(id, window, after, None, grid.held),
            None => false,
        };
        if grid.held && !carried {
            self.release_result(id, window);
        }
    }

    fn rename_in_menu(&mut self, id: PlayerId, name: &str) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(open) = player.open_menu.as_ref() else {
            return false;
        };
        let clicker = Clicker {
            infinite_materials: player.game_mode == GameMode::Creative,
            experience_level: player.vitals.experience.level,
        };
        let mut items = self.open_items(open);
        let before = items.clone();
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let Some(open) = player.open_menu.as_mut() else {
            return false;
        };
        let changed = open
            .menu
            .rename(&mut items, &mut player.inventory, name, clicker);
        if changed {
            self.store_open_items(id, &before, items);
        }
        changed
    }

    pub(crate) fn grindstone_used(&mut self, pos: BlockPos, base: i32) {
        let rng = &mut self.rng;
        let amount = grindstone::award(base, |bound| rng.random_range(0..bound));
        self.award_experience(pos.center(), amount);
        self.level_event(GRINDSTONE_USED_EVENT, pos, 0);
    }

    pub(crate) fn anvil_used(&mut self, id: PlayerId, pos: BlockPos, levels: i32, wear: bool) {
        if levels > 0
            && let Some(player) = self.players.get_mut(&id)
        {
            let tick_count = player.vitals.tick_count as i32;
            player
                .vitals
                .experience
                .give_levels(levels.saturating_neg(), tick_count);
        }
        let state = self.world.chunks.block(pos);
        let is_anvil = state.is_some_and(|state| {
            matches!(
                state.kind(),
                Some(BlockKind::Anvil | BlockKind::ChippedAnvil | BlockKind::DamagedAnvil)
            )
        });
        if wear
            && is_anvil
            && let Some(state) = state
            && self.rng.random::<f32>() < ANVIL_DAMAGE_CHANCE
        {
            match state.damaged_anvil() {
                Some(damaged) => {
                    self.set_block(pos, damaged);
                    self.level_event(ANVIL_USED_EVENT, pos, 0);
                }
                None => {
                    self.set_block(pos, BlockStateId::AIR);
                    self.level_event(ANVIL_DESTROYED_EVENT, pos, 0);
                }
            }
            return;
        }
        self.level_event(ANVIL_USED_EVENT, pos, 0);
    }
}
