use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use rspigot_domain::container_menu::{ContainerMenu, MenuKind};
use rspigot_domain::item::{Inventory, OFFHAND_SLOT, Slot};
use rspigot_domain::menu::{Clicker, MenuEffect};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;

use crate::containers::{next_window, sync_menu_view};
use crate::events::{ClientEvent, ContainerClick};
use crate::plugins::items::{spec_of, stack_of};
use crate::plugins::resolve::text;
use crate::plugins::views::Window;
use crate::plugins::{
    ActionError, ActionErrorKind, CustomInventoryOrder, CustomInventoryState, InventoryOwner,
    InventoryRef, InventoryView, PlayerSnapshot, Reply,
};
use crate::ports::ClientLink;
use crate::server::Server;

pub const MAX_PLUGIN_INVENTORIES: usize = 1024;
const ROWS: RangeInclusive<u8> = 1..=6;
const ROW_SLOTS: usize = 9;
const OFFHAND: i32 = 40;

type Key = (u64, u64);

struct PluginInventory {
    rows: u8,
    title: Component,
    locked: bool,
    items: Vec<Slot>,
}

#[derive(Default)]
pub(crate) struct PluginInventories {
    held: BTreeMap<Key, PluginInventory>,
}

impl PluginInventories {
    pub(super) fn contains(&self, connection: u64, inventory: u64) -> bool {
        self.held.contains_key(&(connection, inventory))
    }

    fn held_by(&self, connection: u64) -> usize {
        self.held
            .range((connection, 0)..=(connection, u64::MAX))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PluginMenu {
    pub window: i32,
    pub connection: u64,
    pub inventory: u64,
    pub menu: ContainerMenu,
    pub state_id: i32,
}

impl PluginMenu {
    pub(crate) const fn key(&self) -> Key {
        (self.connection, self.inventory)
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn create_plugin_inventory(
        &mut self,
        connection: u64,
        id: u64,
        rows: i32,
        title: &str,
        locked: bool,
    ) -> Result<(), ActionError> {
        let rows = u8::try_from(rows)
            .ok()
            .filter(|rows| ROWS.contains(rows))
            .ok_or_else(|| {
                ActionError::from(format!(
                    "rows {rows} is outside {}..={}",
                    ROWS.start(),
                    ROWS.end()
                ))
            })?;
        let inventories = &mut self.plugin_inventories;
        if inventories.held.contains_key(&(connection, id)) {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                format!("inventory {id} already exists"),
            ));
        }
        if inventories.held_by(connection) >= MAX_PLUGIN_INVENTORIES {
            return Err(ActionError::new(
                ActionErrorKind::LimitReached,
                format!("the plugin already holds {MAX_PLUGIN_INVENTORIES} inventories"),
            ));
        }
        inventories.held.insert(
            (connection, id),
            PluginInventory {
                rows,
                title: text(title),
                locked,
                items: vec![None; usize::from(rows) * ROW_SLOTS],
            },
        );
        Ok(())
    }

    pub(super) fn begin_plugin_open(
        &mut self,
        connection: u64,
        id: PlayerId,
        inventory: u64,
        reply: Option<Reply>,
    ) -> Result<(), ActionError> {
        if !self.players.contains_key(&id) {
            return Err(ActionError::player_gone());
        }
        if !self.plugin_inventories.contains(connection, inventory) {
            return Err(ActionError::new(
                ActionErrorKind::NotFound,
                format!("inventory {inventory} does not exist"),
            ));
        }
        self.close_open_menu(id, true);
        self.ask_plugin_open(id, connection, inventory, reply);
        Ok(())
    }

    pub(crate) fn show_plugin_menu(&mut self, connection: u64, id: PlayerId, inventory: u64) {
        let key = (connection, inventory);
        let (Some(held), Some(player)) = (
            self.plugin_inventories.held.get(&key),
            self.players.get_mut(&id),
        ) else {
            return;
        };
        let kind = MenuKind::Generic { rows: held.rows };
        let window = next_window(player);
        let mut open = PluginMenu {
            window,
            connection,
            inventory,
            menu: ContainerMenu::new(kind),
            state_id: 0,
        };
        player.link.send(ClientEvent::OpenScreen {
            window,
            menu: kind.protocol_id(),
            title: held.title.clone(),
        });
        sync_menu_view(
            &player.link,
            window,
            &mut open.state_id,
            &mut open.menu,
            &held.items,
            &player.inventory,
            true,
        );
        player.plugin_menu = Some(open);
    }

    pub(crate) fn apply_plugin_click(
        &mut self,
        id: PlayerId,
        click: &ContainerClick,
        act: bool,
        full: bool,
    ) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(open) = player.plugin_menu.as_mut() else {
            return;
        };
        let key = open.key();
        let Some(held) = self.plugin_inventories.held.get_mut(&key) else {
            return;
        };
        let effects = if act {
            let clicker = Clicker {
                infinite_materials: player.game_mode == GameMode::Creative,
                experience_level: player.vitals.experience.level,
            };
            open.menu.click(
                &mut held.items,
                &mut player.inventory,
                click.slot,
                click.button,
                click.input,
                clicker,
            )
        } else {
            Vec::new()
        };
        for (slot, hash) in &click.changed {
            open.menu.receive_remote(*slot, hash.clone());
        }
        open.menu.receive_remote_carried(click.carried.clone());
        let full = full
            || !act
            || click.state_id != open.state_id
            || effects.contains(&MenuEffect::FullResync);
        let drops = effects
            .into_iter()
            .filter(|effect| matches!(effect, MenuEffect::Drop { .. }))
            .collect();
        self.apply_menu_effects(id, drops);
        self.sync_plugin_viewer(id, full);
        if act {
            for viewer in self.plugin_viewers(|menu| menu.key() == key) {
                if viewer != id {
                    self.sync_plugin_viewer(viewer, false);
                }
            }
        }
        self.sync_inventory(id);
    }

    pub(crate) fn plugin_menu_locked(&self, id: PlayerId) -> bool {
        self.players
            .get(&id)
            .and_then(|player| player.plugin_menu.as_ref())
            .and_then(|open| self.plugin_inventories.held.get(&open.key()))
            .is_none_or(|held| held.locked)
    }

    pub(crate) fn plugin_menu_items(&self, id: PlayerId) -> Option<&[Slot]> {
        let open = self.players.get(&id)?.plugin_menu.as_ref()?;
        self.plugin_inventories
            .held
            .get(&open.key())
            .map(|held| held.items.as_slice())
    }

    pub(crate) fn with_plugin_menu<T>(
        &mut self,
        id: PlayerId,
        change: impl FnOnce(&mut ContainerMenu, &mut Vec<Slot>, &mut Inventory) -> T,
    ) -> Option<T> {
        let player = self.players.get_mut(&id)?;
        let open = player.plugin_menu.as_mut()?;
        let held = self.plugin_inventories.held.get_mut(&open.key())?;
        Some(change(
            &mut open.menu,
            &mut held.items,
            &mut player.inventory,
        ))
    }

    pub(crate) fn sync_plugin_viewers_of(&mut self, id: PlayerId) {
        let Some(key) = self
            .players
            .get(&id)
            .and_then(|player| player.plugin_menu.as_ref())
            .map(PluginMenu::key)
        else {
            return;
        };
        for viewer in self.plugin_viewers(|menu| menu.key() == key) {
            if viewer != id {
                self.sync_plugin_viewer(viewer, false);
            }
        }
    }

    pub(crate) fn menu_view(
        &self,
        key: Key,
        player: &PlayerSnapshot,
    ) -> (InventoryRef, InventoryView, Vec<PlayerSnapshot>) {
        let held = self.plugin_inventories.held.get(&key);
        let top = InventoryRef {
            owner: InventoryOwner::Custom {
                plugin: self.plugin_name(key.0),
                id: key.1,
            },
            size: held.map_or(0, |held| {
                i32::try_from(held.items.len()).unwrap_or(i32::MAX)
            }),
            kind: "CHEST".to_owned(),
            id: key.1,
        };
        let bottom = self.player_inventory(player.id);
        let view = InventoryView {
            top: top.clone(),
            bottom,
            title: held.map(|held| held.title.plain_text()).unwrap_or_default(),
            player: player.clone(),
        };
        let mut viewers: Vec<PlayerSnapshot> = self
            .plugin_viewers(|menu| menu.key() == key)
            .into_iter()
            .filter_map(|viewer| self.snapshot_of(viewer))
            .collect();
        viewers.sort_by_key(|viewer| viewer.entity_id);
        (top, view, viewers)
    }

    pub(crate) fn close_plugin_menu(
        &mut self,
        id: PlayerId,
        notify_client: bool,
        by_player: bool,
    ) -> bool {
        let Some(window) = self
            .players
            .get(&id)
            .and_then(|player| player.plugin_menu.as_ref())
            .map(|open| open.window)
        else {
            return false;
        };
        self.notify_close(id, Window::Plugin(window), by_player);
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let Some(mut open) = player.plugin_menu.take() else {
            return false;
        };
        let before = Self::equipment_of(player);
        let effects = open.menu.close(&mut player.inventory);
        if notify_client {
            player.link.send(ClientEvent::ContainerClose(open.window));
        }
        self.apply_menu_effects(id, effects);
        self.broadcast_equipment_changes(id, before);
        true
    }

    pub(crate) fn forget_plugin_inventories(&mut self, connection: u64) {
        for viewer in self.plugin_viewers(|menu| menu.connection == connection) {
            self.close_plugin_menu(viewer, true, false);
        }
        self.plugin_inventories
            .held
            .retain(|(owner, _), _| *owner != connection);
    }

    pub(super) fn apply_custom_inventory_order(
        &mut self,
        connection: u64,
        inventory: u64,
        order: CustomInventoryOrder,
    ) {
        let key = (connection, inventory);
        let name = order.name();
        if let Err(reason) = self.custom_inventory_order(key, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn custom_inventory_order(
        &mut self,
        key: Key,
        order: CustomInventoryOrder,
    ) -> Result<(), String> {
        let Some(held) = self.plugin_inventories.held.get_mut(&key) else {
            return Err(format!("there is no inventory {}", key.1));
        };
        match order {
            CustomInventoryOrder::Set { slot, item } => {
                let size = held.items.len();
                let index = usize::try_from(slot)
                    .ok()
                    .filter(|index| *index < size)
                    .ok_or_else(|| format!("slot {slot} is outside 0..={}", size - 1))?;
                held.items[index] = match item {
                    Some(spec) => stack_of(&spec)?,
                    None => None,
                };
                self.sync_plugin_viewers(key);
            }
            CustomInventoryOrder::Clear => {
                held.items.fill(None);
                self.sync_plugin_viewers(key);
            }
            CustomInventoryOrder::Close => self.close_plugin_viewers(key),
            CustomInventoryOrder::Discard => {
                self.close_plugin_viewers(key);
                self.plugin_inventories.held.remove(&key);
            }
        }
        Ok(())
    }

    pub(super) fn custom_inventory_state(
        &self,
        connection: u64,
        inventory: u64,
    ) -> Option<CustomInventoryState> {
        let key = (connection, inventory);
        let held = self.plugin_inventories.held.get(&key)?;
        let mut viewers: Vec<PlayerSnapshot> = self
            .plugin_viewers(|menu| menu.key() == key)
            .into_iter()
            .filter_map(|viewer| self.snapshot_of(viewer))
            .collect();
        viewers.sort_by_key(|viewer| viewer.entity_id);
        Some(CustomInventoryState {
            items: held
                .items
                .iter()
                .map(|slot| slot.as_ref().map(spec_of))
                .collect(),
            viewers,
        })
    }

    fn plugin_viewers(&self, viewing: impl Fn(&PluginMenu) -> bool) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter(|(_, player)| player.plugin_menu.as_ref().is_some_and(&viewing))
            .map(|(id, _)| *id)
            .collect()
    }

    fn sync_plugin_viewers(&mut self, key: Key) {
        for viewer in self.plugin_viewers(|menu| menu.key() == key) {
            self.sync_plugin_viewer(viewer, false);
        }
    }

    fn close_plugin_viewers(&mut self, key: Key) {
        for viewer in self.plugin_viewers(|menu| menu.key() == key) {
            self.close_plugin_menu(viewer, true, false);
        }
    }

    pub(crate) fn sync_plugin_viewer(&mut self, id: PlayerId, full: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(open) = player.plugin_menu.as_mut() else {
            return;
        };
        let Some(held) = self.plugin_inventories.held.get(&open.key()) else {
            return;
        };
        sync_menu_view(
            &player.link,
            open.window,
            &mut open.state_id,
            &mut open.menu,
            &held.items,
            &player.inventory,
            full,
        );
    }

    pub(crate) fn resend_offhand(&self, id: PlayerId) {
        if let Some(player) = self.players.get(&id) {
            player.link.send(ClientEvent::PlayerInventorySlot {
                slot: OFFHAND,
                stack: player.inventory.get(OFFHAND_SLOT).cloned(),
            });
        }
    }
}
