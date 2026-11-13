use rspigot_blocks::BlockKind;
use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::player::PlayerId;

use crate::console::english_title;
use crate::containers::OpenMenu;
use crate::plugins::events::InventoryTypeSlotType;
use crate::plugins::{InventoryOwner, InventoryRef, InventoryView, PlayerSnapshot};
use crate::ports::ClientLink;
use crate::server::Server;

const PLAYER_SLOTS: i32 = 41;
const PLAYER_GRID: i32 = 5;
const ARMOR_END: i32 = 9;
const STORAGE_END: i32 = 35;
const MAIN_SLOTS: i32 = 27;
const MAIN_START: i32 = 9;
const ARMOR_TOP: i32 = 39;
const CRAFTING_TITLE: &str = "container.crafting";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Window {
    Inventory,
    Container(i32),
    Plugin(i32),
}

impl Window {
    pub(crate) const fn id(self) -> i32 {
        match self {
            Self::Inventory => 0,
            Self::Container(window) | Self::Plugin(window) => window,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TopType {
    Crafting,
    Chest,
    EnderChest,
    Barrel,
    Dispenser,
    Dropper,
    Hopper,
    ShulkerBox,
    Workbench,
    Furnace,
    BlastFurnace,
    Smoker,
    Anvil,
    Smithing,
    Grindstone,
    Stonecutter,
    Cartography,
    Loom,
    Lectern,
    Beacon,
    Merchant,
}

impl TopType {
    pub(crate) fn of(open: &OpenMenu) -> Self {
        match open.menu.kind() {
            MenuKind::Generic { .. } if open.backing.ender => Self::EnderChest,
            MenuKind::Generic { .. } if open.backing.block == BlockKind::Barrel => Self::Barrel,
            MenuKind::Generic { .. } => Self::Chest,
            MenuKind::Generic3x3 if open.backing.block == BlockKind::Dropper => Self::Dropper,
            MenuKind::Generic3x3 => Self::Dispenser,
            MenuKind::Hopper => Self::Hopper,
            MenuKind::ShulkerBox => Self::ShulkerBox,
            MenuKind::Crafting => Self::Workbench,
            MenuKind::Furnace => Self::Furnace,
            MenuKind::BlastFurnace => Self::BlastFurnace,
            MenuKind::Smoker => Self::Smoker,
            MenuKind::Anvil => Self::Anvil,
            MenuKind::Smithing => Self::Smithing,
            MenuKind::Grindstone => Self::Grindstone,
            MenuKind::Stonecutter => Self::Stonecutter,
            MenuKind::CartographyTable => Self::Cartography,
            MenuKind::Loom => Self::Loom,
            MenuKind::Lectern => Self::Lectern,
            MenuKind::Beacon => Self::Beacon,
            MenuKind::Merchant => Self::Merchant,
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Crafting => "CRAFTING",
            Self::Chest => "CHEST",
            Self::EnderChest => "ENDER_CHEST",
            Self::Barrel => "BARREL",
            Self::Dispenser => "DISPENSER",
            Self::Dropper => "DROPPER",
            Self::Hopper => "HOPPER",
            Self::ShulkerBox => "SHULKER_BOX",
            Self::Workbench => "WORKBENCH",
            Self::Furnace => "FURNACE",
            Self::BlastFurnace => "BLAST_FURNACE",
            Self::Smoker => "SMOKER",
            Self::Anvil => "ANVIL",
            Self::Smithing => "SMITHING",
            Self::Grindstone => "GRINDSTONE",
            Self::Stonecutter => "STONECUTTER",
            Self::Cartography => "CARTOGRAPHY",
            Self::Loom => "LOOM",
            Self::Lectern => "LECTERN",
            Self::Beacon => "BEACON",
            Self::Merchant => "MERCHANT",
        }
    }

    fn top_slot(self, raw: i32) -> InventoryTypeSlotType {
        let result_at = match self {
            Self::Furnace | Self::BlastFurnace | Self::Smoker => {
                return match raw {
                    2 => InventoryTypeSlotType::Result,
                    1 => InventoryTypeSlotType::Fuel,
                    _ => InventoryTypeSlotType::Crafting,
                };
            }
            Self::Workbench | Self::Crafting => 0,
            Self::Anvil | Self::Cartography | Self::Grindstone | Self::Merchant => 2,
            Self::Stonecutter => 1,
            Self::Loom | Self::Smithing => 3,
            Self::Beacon => return InventoryTypeSlotType::Crafting,
            _ => return InventoryTypeSlotType::Container,
        };
        if raw == result_at {
            InventoryTypeSlotType::Result
        } else {
            InventoryTypeSlotType::Crafting
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Described {
    pub(crate) top: InventoryRef,
    pub(crate) view: InventoryView,
    pub(crate) viewers: Vec<PlayerSnapshot>,
    pub(crate) kind: TopType,
}

impl Described {
    pub(crate) const fn top_size(&self) -> i32 {
        self.top.size
    }

    pub(crate) fn slot_type(&self, raw: i32) -> InventoryTypeSlotType {
        if raw < 0 {
            return InventoryTypeSlotType::Outside;
        }
        let size = self.top_size();
        if raw < size {
            return self.kind.top_slot(raw);
        }
        if self.kind == TopType::Crafting {
            if raw < ARMOR_END {
                return InventoryTypeSlotType::Armor;
            }
            if raw > STORAGE_END {
                return InventoryTypeSlotType::Quickbar;
            }
            return InventoryTypeSlotType::Container;
        }
        if raw >= size + MAIN_SLOTS {
            InventoryTypeSlotType::Quickbar
        } else {
            InventoryTypeSlotType::Container
        }
    }

    pub(crate) fn convert_slot(&self, raw: i32) -> i32 {
        let size = self.top_size();
        if raw < size {
            return raw;
        }
        let mut slot = raw - size;
        if self.kind == TopType::Crafting {
            if slot < 4 {
                return ARMOR_TOP - slot;
            }
            if slot > ARMOR_TOP {
                return slot;
            }
            slot -= 4;
        }
        if slot >= MAIN_SLOTS {
            slot - MAIN_SLOTS
        } else {
            slot + MAIN_START
        }
    }

    pub(crate) fn clicked_inventory(&self, raw: i32) -> Option<InventoryRef> {
        if raw < 0 {
            None
        } else if raw < self.top_size() {
            Some(self.top.clone())
        } else {
            Some(self.view.bottom.clone())
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn player_inventory(&self, id: PlayerId) -> InventoryRef {
        InventoryRef {
            owner: InventoryOwner::Player(id),
            size: PLAYER_SLOTS,
            kind: "PLAYER".to_owned(),
            id: self
                .players
                .get(&id)
                .map_or(0, |player| player.inventory_ids[0]),
        }
    }

    pub(crate) fn window_of(&self, id: PlayerId) -> Option<Window> {
        let player = self.players.get(&id)?;
        Some(if let Some(open) = &player.plugin_menu {
            Window::Plugin(open.window)
        } else if let Some(open) = &player.open_menu {
            Window::Container(open.window)
        } else {
            Window::Inventory
        })
    }

    pub(crate) fn describe_window(&self, id: PlayerId, window: Window) -> Option<Described> {
        let player = self.snapshot_of(id)?;
        let state = self.players.get(&id)?;
        match window {
            Window::Inventory => {
                let top = InventoryRef {
                    owner: InventoryOwner::Player(id),
                    size: PLAYER_GRID,
                    kind: TopType::Crafting.name().to_owned(),
                    id: state.inventory_ids[1],
                };
                Some(self.described(
                    top,
                    CRAFTING_TITLE.to_owned(),
                    TopType::Crafting,
                    player,
                    vec![id],
                ))
            }
            Window::Container(_) => self.describe_open(id, state.open_menu.as_ref()?),
            Window::Plugin(_) => {
                let key = state.plugin_menu.as_ref()?.key();
                let (top, view, viewers) = self.menu_view(key, &player);
                Some(Described {
                    top,
                    view,
                    viewers,
                    kind: TopType::Chest,
                })
            }
        }
    }

    pub(crate) fn describe_open(&self, id: PlayerId, open: &OpenMenu) -> Option<Described> {
        let player = self.snapshot_of(id)?;
        let kind = TopType::of(open);
        let owner = if let Some(trader) = open.trader {
            InventoryOwner::Entity(trader)
        } else if open.backing.ender {
            InventoryOwner::Player(id)
        } else {
            InventoryOwner::Block(open.backing.pos)
        };
        let top = InventoryRef {
            owner,
            size: i32::try_from(open.menu.kind().container_size()).unwrap_or(i32::MAX),
            kind: kind.name().to_owned(),
            id: open.inventory,
        };
        let backing = open.backing;
        let mut viewers: Vec<PlayerId> = if backing.entity.is_some() {
            self.players
                .iter()
                .filter(|(_, other)| {
                    other.open_menu.as_ref().is_some_and(|seen| {
                        seen.backing.entity.is_some() && seen.backing.pos == backing.pos
                    })
                })
                .map(|(viewer, _)| *viewer)
                .collect()
        } else {
            Vec::new()
        };
        if !viewers.contains(&id) {
            viewers.push(id);
        }
        Some(self.described(top, open.title.to_owned(), kind, player, viewers))
    }

    fn described(
        &self,
        top: InventoryRef,
        title: String,
        kind: TopType,
        player: PlayerSnapshot,
        viewers: Vec<PlayerId>,
    ) -> Described {
        let title = english_title(&title);
        let mut viewers: Vec<PlayerSnapshot> = viewers
            .into_iter()
            .filter_map(|viewer| self.snapshot_of(viewer))
            .collect();
        viewers.sort_by_key(|viewer| viewer.entity_id);
        Described {
            view: InventoryView {
                top: top.clone(),
                bottom: self.player_inventory(player.id),
                title,
                player,
            },
            top,
            viewers,
            kind,
        }
    }
}
