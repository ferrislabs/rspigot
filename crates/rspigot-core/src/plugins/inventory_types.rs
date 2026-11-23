use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::entity::trading::MerchantOffers;
use rspigot_domain::player::PlayerId;

use crate::events::ClientEvent;
use crate::plugins::menus::{Key, PluginInventory};
use crate::plugins::merchants::offer_of;
use crate::plugins::resolve::text;
use crate::plugins::{ActionError, ActionErrorKind, Trade};
use crate::ports::ClientLink;
use crate::server::Server;

const ROW_SLOTS: i32 = 9;
const MAX_ROWS: i32 = 6;
const MAX_TRADES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryType {
    Chest,
    Dispenser,
    Dropper,
    Furnace,
    BlastFurnace,
    Smoker,
    Workbench,
    Enchanting,
    Brewing,
    Anvil,
    Beacon,
    Hopper,
    ShulkerBox,
    Barrel,
    EnderChest,
    Grindstone,
    Loom,
    Cartography,
    Smithing,
    Stonecutter,
    Crafter,
    Merchant,
}

impl InventoryType {
    pub const ALL: [Self; 22] = [
        Self::Chest,
        Self::Dispenser,
        Self::Dropper,
        Self::Furnace,
        Self::BlastFurnace,
        Self::Smoker,
        Self::Workbench,
        Self::Enchanting,
        Self::Brewing,
        Self::Anvil,
        Self::Beacon,
        Self::Hopper,
        Self::ShulkerBox,
        Self::Barrel,
        Self::EnderChest,
        Self::Grindstone,
        Self::Loom,
        Self::Cartography,
        Self::Smithing,
        Self::Stonecutter,
        Self::Crafter,
        Self::Merchant,
    ];

    pub const fn bukkit_name(self) -> &'static str {
        match self {
            Self::Chest => "CHEST",
            Self::Dispenser => "DISPENSER",
            Self::Dropper => "DROPPER",
            Self::Furnace => "FURNACE",
            Self::BlastFurnace => "BLAST_FURNACE",
            Self::Smoker => "SMOKER",
            Self::Workbench => "WORKBENCH",
            Self::Enchanting => "ENCHANTING",
            Self::Brewing => "BREWING",
            Self::Anvil => "ANVIL",
            Self::Beacon => "BEACON",
            Self::Hopper => "HOPPER",
            Self::ShulkerBox => "SHULKER_BOX",
            Self::Barrel => "BARREL",
            Self::EnderChest => "ENDER_CHEST",
            Self::Grindstone => "GRINDSTONE",
            Self::Loom => "LOOM",
            Self::Cartography => "CARTOGRAPHY",
            Self::Smithing => "SMITHING",
            Self::Stonecutter => "STONECUTTER",
            Self::Crafter => "CRAFTER",
            Self::Merchant => "MERCHANT",
        }
    }

    const fn menu(self, rows: u8) -> MenuKind {
        match self {
            Self::Chest => MenuKind::Generic { rows },
            Self::Barrel | Self::EnderChest => MenuKind::Generic { rows: 3 },
            Self::Dispenser | Self::Dropper => MenuKind::Generic3x3,
            Self::Furnace => MenuKind::Furnace,
            Self::BlastFurnace => MenuKind::BlastFurnace,
            Self::Smoker => MenuKind::Smoker,
            Self::Workbench => MenuKind::Crafting,
            Self::Enchanting => MenuKind::Enchantment,
            Self::Brewing => MenuKind::BrewingStand,
            Self::Anvil => MenuKind::Anvil,
            Self::Beacon => MenuKind::Beacon,
            Self::Hopper => MenuKind::Hopper,
            Self::ShulkerBox => MenuKind::ShulkerBox,
            Self::Grindstone => MenuKind::Grindstone,
            Self::Loom => MenuKind::Loom,
            Self::Cartography => MenuKind::CartographyTable,
            Self::Smithing => MenuKind::Smithing,
            Self::Stonecutter => MenuKind::Stonecutter,
            Self::Crafter => MenuKind::Crafter,
            Self::Merchant => MenuKind::Merchant,
        }
    }

    pub fn size(self, slots: i32) -> Result<usize, String> {
        match self {
            Self::Chest => rows_of(slots).map(|rows| usize::from(rows) * ROW_SLOTS as usize),
            other => Ok(other.menu(1).container_size()),
        }
    }
}

fn rows_of(slots: i32) -> Result<u8, String> {
    if slots % ROW_SLOTS != 0 || !(ROW_SLOTS..=ROW_SLOTS * MAX_ROWS).contains(&slots) {
        return Err(format!(
            "a chest has a multiple of {ROW_SLOTS} slots up to {}, got {slots}",
            ROW_SLOTS * MAX_ROWS
        ));
    }
    u8::try_from(slots / ROW_SLOTS).map_err(|_| format!("{slots} slots is too many"))
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn offers_of(trades: &[Trade]) -> Result<MerchantOffers, ActionError> {
    if trades.len() > MAX_TRADES {
        return Err(ActionError::new(
            ActionErrorKind::LimitReached,
            format!("a merchant takes at most {MAX_TRADES} trades"),
        ));
    }
    let mut offers = MerchantOffers::new();
    for trade in trades {
        offers.push(offer_of(trade)?);
    }
    Ok(offers)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerchantProgress {
    pub level: i32,
    pub xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

impl Default for MerchantProgress {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            show_progress: false,
            can_restock: false,
        }
    }
}

impl MerchantProgress {
    fn check(self) -> Result<Self, ActionError> {
        if !(1..=5).contains(&self.level) {
            return Err(invalid(format!(
                "merchant level {} is outside 1..=5",
                self.level
            )));
        }
        if self.xp < 0 {
            return Err(invalid(format!("merchant xp {} is negative", self.xp)));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventorySpec {
    pub progress: Option<MerchantProgress>,
    pub ty: InventoryType,
    pub slots: i32,
    pub title: String,
    pub locked: bool,
    pub trades: Vec<Trade>,
}

impl<L: ClientLink> Server<L> {
    pub(super) fn create_typed_inventory(
        &mut self,
        connection: u64,
        id: u64,
        spec: &InventorySpec,
    ) -> Result<(), ActionError> {
        let InventorySpec {
            ty,
            slots,
            title,
            locked,
            trades,
            progress,
        } = spec;
        let progress = progress.map(MerchantProgress::check).transpose()?;
        let (ty, locked) = (*ty, *locked);
        let size = ty.size(*slots).map_err(invalid)?;
        if ty != InventoryType::Merchant && !trades.is_empty() {
            return Err(invalid("only a merchant inventory takes trades"));
        }
        let offers = offers_of(trades)?;
        self.reserve_inventory(connection, id)?;
        let rows = u8::try_from(size / ROW_SLOTS as usize).unwrap_or(1);
        self.plugin_inventories.held.insert(
            (connection, id),
            PluginInventory {
                kind: ty.menu(rows),
                ty,
                title: text(title),
                locked,
                items: vec![None; size],
                offers,
                progress: progress.unwrap_or_default(),
            },
        );
        Ok(())
    }

    pub(super) fn set_inventory_trades(
        &mut self,
        key: Key,
        trades: &[Trade],
    ) -> Result<(), String> {
        let offers = offers_of(trades).map_err(|error| error.reason)?;
        let held = self
            .plugin_inventories
            .held
            .get_mut(&key)
            .ok_or_else(|| format!("there is no inventory {}", key.1))?;
        if held.ty != InventoryType::Merchant {
            return Err("only a merchant inventory takes trades".to_owned());
        }
        held.offers = offers;
        self.refresh_merchant_viewers(key);
        Ok(())
    }

    pub(super) fn set_inventory_progress(
        &mut self,
        key: Key,
        progress: MerchantProgress,
    ) -> Result<(), String> {
        let progress = progress.check().map_err(|error| error.reason)?;
        let held = self
            .plugin_inventories
            .held
            .get_mut(&key)
            .ok_or_else(|| format!("there is no inventory {}", key.1))?;
        if held.ty != InventoryType::Merchant {
            return Err("only a merchant inventory has a progress".to_owned());
        }
        held.progress = progress;
        self.refresh_merchant_viewers(key);
        Ok(())
    }

    pub(super) fn refresh_merchant_viewers(&mut self, key: Key) {
        let viewers: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| {
                player
                    .plugin_menu
                    .as_ref()
                    .is_some_and(|menu| menu.key() == key)
            })
            .map(|(id, _)| *id)
            .collect();
        for viewer in viewers {
            let (Some(held), Some(player)) = (
                self.plugin_inventories.held.get_mut(&key),
                self.players.get_mut(&viewer),
            ) else {
                continue;
            };
            let Some(open) = player.plugin_menu.as_mut() else {
                continue;
            };
            open.menu
                .set_offers(&mut held.items, &mut player.inventory, held.offers.clone());
            player.link.send(ClientEvent::MerchantOffers {
                window: open.window,
                offers: held.offers.iter().cloned().collect(),
                level: held.progress.level,
                xp: held.progress.xp,
                show_progress: held.progress.show_progress,
                can_restock: held.progress.can_restock,
            });
            self.sync_plugin_viewer(viewer, true);
        }
    }

    pub(crate) fn plugin_trades_taken(&mut self, id: PlayerId, traded: &[usize]) {
        if traded.is_empty() {
            return;
        }
        let Some(key) = self
            .players
            .get(&id)
            .and_then(|player| player.plugin_menu.as_ref())
            .map(|menu| menu.key())
        else {
            return;
        };
        let Some(held) = self.plugin_inventories.held.get_mut(&key) else {
            return;
        };
        for index in traded {
            if let Some(offer) = held.offers.get_mut(*index) {
                offer.increase_uses();
            }
        }
        self.refresh_merchant_viewers(key);
    }

    pub(crate) fn plugin_select_trade(&mut self, id: PlayerId, index: i32) -> bool {
        let merchant = self
            .players
            .get(&id)
            .and_then(|player| player.plugin_menu.as_ref())
            .and_then(|menu| self.plugin_inventories.held.get(&menu.key()))
            .is_some_and(|held| held.ty == InventoryType::Merchant);
        if !merchant {
            return false;
        }
        self.with_plugin_menu(id, |menu, items, inventory| {
            menu.select_trade(items, inventory, index)
        });
        self.sync_plugin_viewer(id, false);
        self.sync_inventory(id);
        true
    }
}
