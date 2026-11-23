use std::sync::atomic::{AtomicU64, Ordering};

use rspigot_plugin_protocol::v1::{
    self, answer, custom_inventory_order, from_plugin, question, server_order,
};

use crate::contract::Trade;
use crate::link::Link;
use crate::{Error, ItemStack, Player};

static NEXT_INVENTORY: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

const KINDS: [(InventoryType, v1::InventoryKind); 22] = [
    (InventoryType::Chest, v1::InventoryKind::Chest),
    (InventoryType::Dispenser, v1::InventoryKind::Dispenser),
    (InventoryType::Dropper, v1::InventoryKind::Dropper),
    (InventoryType::Furnace, v1::InventoryKind::Furnace),
    (InventoryType::BlastFurnace, v1::InventoryKind::BlastFurnace),
    (InventoryType::Smoker, v1::InventoryKind::Smoker),
    (InventoryType::Workbench, v1::InventoryKind::Workbench),
    (InventoryType::Enchanting, v1::InventoryKind::Enchanting),
    (InventoryType::Brewing, v1::InventoryKind::Brewing),
    (InventoryType::Anvil, v1::InventoryKind::Anvil),
    (InventoryType::Beacon, v1::InventoryKind::Beacon),
    (InventoryType::Hopper, v1::InventoryKind::Hopper),
    (InventoryType::ShulkerBox, v1::InventoryKind::ShulkerBox),
    (InventoryType::Barrel, v1::InventoryKind::Barrel),
    (InventoryType::EnderChest, v1::InventoryKind::EnderChest),
    (InventoryType::Grindstone, v1::InventoryKind::Grindstone),
    (InventoryType::Loom, v1::InventoryKind::Loom),
    (InventoryType::Cartography, v1::InventoryKind::Cartography),
    (InventoryType::Smithing, v1::InventoryKind::Smithing),
    (InventoryType::Stonecutter, v1::InventoryKind::Stonecutter),
    (InventoryType::Crafter, v1::InventoryKind::Crafter),
    (InventoryType::Merchant, v1::InventoryKind::Merchant),
];

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

    fn to_wire(self) -> v1::InventoryKind {
        KINDS
            .iter()
            .find(|(kind, _)| *kind == self)
            .map_or(v1::InventoryKind::Chest, |(_, wire)| *wire)
    }

    fn from_wire(value: i32) -> Result<Self, Error> {
        let wire = v1::InventoryKind::try_from(value)
            .map_err(|_| Error::Malformed(format!("an unknown inventory kind {value}")))?;
        if wire == v1::InventoryKind::Unspecified {
            return Ok(Self::Chest);
        }
        KINDS
            .iter()
            .find(|(_, known)| *known == wire)
            .map(|(kind, _)| *kind)
            .ok_or_else(|| Error::Malformed(format!("an unknown inventory kind {value}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerchantProgress {
    pub level: i32,
    pub xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

impl MerchantProgress {
    const fn to_wire(self) -> v1::MerchantProgress {
        v1::MerchantProgress {
            level: self.level,
            xp: self.xp,
            show_progress: self.show_progress,
            can_restock: self.can_restock,
        }
    }

    const fn from_wire(wire: v1::MerchantProgress) -> Self {
        Self {
            level: wire.level,
            xp: wire.xp,
            show_progress: wire.show_progress,
            can_restock: wire.can_restock,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomInventoryState {
    pub progress: Option<MerchantProgress>,
    pub items: Vec<Option<ItemStack>>,
    pub viewers: Vec<Player>,
    pub kind: InventoryType,
    pub trades: Vec<Trade>,
}

impl CustomInventoryState {
    fn from_wire(state: v1::CustomInventoryState, link: &Link) -> Result<Self, Error> {
        let items = (0..)
            .zip(state.slots)
            .map(|(index, slot)| {
                if slot.slot != index {
                    return Err(Error::Malformed(format!(
                        "slot {} where slot {index} was expected",
                        slot.slot
                    )));
                }
                slot.item.map(ItemStack::from_wire).transpose()
            })
            .collect::<Result<_, Error>>()?;
        let viewers = state
            .viewers
            .into_iter()
            .map(|viewer| Player::from_wire(Some(viewer), link))
            .collect::<Result<_, Error>>()?;
        let trades = state
            .trades
            .into_iter()
            .map(|trade| Trade::from_wire(trade, link))
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            progress: state.progress.map(MerchantProgress::from_wire),
            items,
            viewers,
            kind: InventoryType::from_wire(state.kind)?,
            trades,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CustomInventory {
    pub id: u64,
    pub plugin: String,
    generation: u64,
    link: Link,
}

impl PartialEq for CustomInventory {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.plugin == other.plugin && self.generation == other.generation
    }
}

impl Eq for CustomInventory {}

impl CustomInventory {
    pub(crate) async fn create(
        link: &Link,
        rows: u8,
        title: String,
        locked: bool,
    ) -> Result<Self, Error> {
        let inventory = Self::new(NEXT_INVENTORY.fetch_add(1, Ordering::Relaxed), link);
        link.act_from(
            inventory.generation,
            from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::CreateInventory(v1::CreateInventory {
                    id: inventory.id,
                    rows: i32::from(rows),
                    title,
                    locked,
                    ..Default::default()
                })),
            }),
        )
        .await?;
        Ok(inventory)
    }

    pub(crate) async fn create_typed(
        link: &Link,
        kind: InventoryType,
        slots: u16,
        title: String,
        locked: bool,
        trades: &[Trade],
        progress: Option<MerchantProgress>,
    ) -> Result<Self, Error> {
        let inventory = Self::new(NEXT_INVENTORY.fetch_add(1, Ordering::Relaxed), link);
        link.act_from(
            inventory.generation,
            from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::CreateInventory(v1::CreateInventory {
                    id: inventory.id,
                    title,
                    locked,
                    kind: kind.to_wire() as i32,
                    slots: i32::from(slots),
                    trades: trades.iter().map(Trade::to_wire).collect(),
                    progress: progress.map(MerchantProgress::to_wire),
                    ..Default::default()
                })),
            }),
        )
        .await?;
        Ok(inventory)
    }

    pub fn set_progress(&self, progress: MerchantProgress) {
        self.order(custom_inventory_order::Order::SetProgress(
            progress.to_wire(),
        ));
    }

    pub fn set_trades(&self, trades: &[Trade]) {
        self.order(custom_inventory_order::Order::SetTrades(v1::TradeList {
            trades: trades.iter().map(Trade::to_wire).collect(),
        }));
    }

    pub(crate) fn new(id: u64, link: &Link) -> Self {
        Self::owned_by(id, link.name(), link)
    }

    pub(crate) fn owned_by(id: u64, plugin: String, link: &Link) -> Self {
        Self {
            id,
            plugin,
            generation: link.generation(),
            link: link.clone(),
        }
    }

    pub fn is_mine(&self) -> bool {
        self.plugin == self.link.name()
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn set(&self, slot: u16, item: Option<ItemStack>) {
        self.order(custom_inventory_order::Order::Set(v1::SetSlot {
            slot: i32::from(slot),
            item: item.map(ItemStack::into_wire),
        }));
    }

    pub fn clear(&self) {
        self.order(custom_inventory_order::Order::Clear(v1::Empty {}));
    }

    pub fn close(&self) {
        self.order(custom_inventory_order::Order::Close(v1::Empty {}));
    }

    pub fn discard(&self) {
        self.order(custom_inventory_order::Order::Discard(v1::Empty {}));
    }

    pub async fn get_state(&self) -> Result<CustomInventoryState, Error> {
        if self.link.generation() != self.generation || !self.is_mine() {
            return Err(Error::NotFound);
        }
        let state = self
            .link
            .ask(
                question::Question::CustomInventory(self.id),
                "a custom inventory",
                |body| match body {
                    answer::Body::CustomInventory(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        CustomInventoryState::from_wire(state, &self.link)
    }

    fn order(&self, order: custom_inventory_order::Order) {
        if !self.is_mine() {
            tracing::warn!(
                "Dropped an order on inventory {} of plugin {}",
                self.id,
                self.plugin
            );
            return;
        }
        self.link.order_from(
            self.generation,
            from_plugin::Body::CustomInventory(v1::CustomInventoryOrder {
                inventory: self.id,
                order: Some(order),
            }),
        );
    }
}
