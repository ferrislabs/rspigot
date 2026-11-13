use rspigot_domain::BlockPos;
use rspigot_domain::player::PlayerId;

use crate::containers::OpenMenu;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{InventoryClose, InventoryOpen};
use crate::plugins::views::Window;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug)]
pub(crate) struct OpenStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) open: OpenMenu,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PluginOpenStep {
    pub(crate) player: PlayerId,
    pub(crate) connection: u64,
    pub(crate) inventory: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClosePacket {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ButtonPacket {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
    pub(crate) button: i32,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ask_open(&mut self, id: PlayerId, pos: BlockPos, open: OpenMenu) {
        let payload = self.listened(EventKind::InventoryOpen).then(|| {
            let described = self.describe_open(id, &open)?;
            Some(EventPayload::InventoryOpen(InventoryOpen {
                inventory: described.top,
                player: described.view.player.clone(),
                view: described.view,
                viewers: described.viewers,
            }))
        });
        let step = OpenStep {
            player: id,
            pos,
            open,
        };
        self.fire(
            Lane::Player(id),
            Continuation::Open(Box::new(step)),
            move |_| payload.flatten(),
        );
    }

    pub(crate) fn finish_open(&mut self, step: OpenStep, cancelled: bool) {
        if step.open.trader.is_some() {
            if cancelled || !self.trader_open_in_reach(step.player, &step.open) {
                self.cancel_trading_open(&step.open);
                return;
            }
            self.show_trader_menu(step.player, step.open);
            return;
        }
        if step.open.mount.is_some() {
            if !cancelled && self.mount_open_in_reach(step.player, &step.open) {
                self.show_mount_menu(step.player, step.open);
            }
            return;
        }
        if cancelled || !self.opening_in_reach(step.player, step.pos, &step.open) {
            return;
        }
        self.show_block_menu(step.player, step.pos, step.open);
    }

    pub(crate) fn ask_plugin_open(&mut self, id: PlayerId, connection: u64, inventory: u64) {
        let step = PluginOpenStep {
            player: id,
            connection,
            inventory,
        };
        self.fire(
            Lane::Player(id),
            Continuation::OpenPlugin(step),
            move |server| {
                let player = server.snapshot_of(id)?;
                let (top, view, mut viewers) = server.menu_view((connection, inventory), &player);
                if !viewers.iter().any(|viewer| viewer.id == id) {
                    viewers.push(player.clone());
                    viewers.sort_by_key(|viewer| viewer.entity_id);
                }
                Some(EventPayload::InventoryOpen(InventoryOpen {
                    inventory: top,
                    player,
                    view,
                    viewers,
                }))
            },
        );
    }

    pub(crate) fn finish_plugin_open(&mut self, step: PluginOpenStep, cancelled: bool) {
        let free = self.players.get(&step.player).is_some_and(|player| {
            !player.vitals.is_dead() && player.open_menu.is_none() && player.plugin_menu.is_none()
        });
        if cancelled || !free {
            return;
        }
        self.show_plugin_menu(step.connection, step.player, step.inventory);
    }

    pub(crate) fn notify_close(&mut self, id: PlayerId, window: Window, by_player: bool) {
        self.fire(
            Lane::Player(id),
            Continuation::Notice(EventKind::InventoryClose),
            move |server| {
                let described = server.describe_window(id, window)?;
                Some(EventPayload::InventoryClose(InventoryClose {
                    inventory: described.top,
                    player: described.view.player.clone(),
                    view: described.view,
                    viewers: described.viewers,
                    by_player,
                }))
            },
        );
    }

    pub(crate) fn press_button(&mut self, id: PlayerId, window: i32, button: i32) {
        self.fire_deferred(
            id,
            Continuation::Button(ButtonPacket {
                player: id,
                window,
                button,
            }),
        );
    }

    pub(crate) fn handle_container_close(&mut self, id: PlayerId, window: i32) {
        self.fire_deferred(id, Continuation::Close(ClosePacket { player: id, window }));
    }

    pub(crate) fn close_at_head(&mut self, packet: ClosePacket) {
        let ClosePacket { player: id, window } = packet;
        if self.handle_open_menu_close(id, window) {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() {
            return;
        }
        self.notify_close(id, Window::Inventory, true);
        self.close_inventory_menu(id);
    }
}
