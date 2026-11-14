use rspigot_blocks::BlockKind;
use rspigot_domain::container_menu::{ContainerMenu, MenuKind};
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::equine;
use rspigot_domain::item::Slot;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};
use rspigot_items::EquipmentSlot;

use crate::containers::{Backing, Lid, OpenMenu, next_window, sync_menu_view};
use crate::entities::wire_slot;
use crate::events::ClientEvent;
use crate::kinds::mount_can_use_slot;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const SADDLE: usize = equine::SADDLE_SLOT;
const BODY: usize = equine::BODY_SLOT;
const INTERACTION_RANGE_SQUARED: f64 = 16.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn open_mount_inventory(&mut self, player: PlayerId, mount: EntityId) {
        let Some(rider) = self.players.get(&player).map(|p| p.entity_id) else {
            return;
        };
        let Some(entity) = self.entities.store.get(mount) else {
            return;
        };
        let vehicle = self.entities.rides.is_vehicle(mount);
        if (vehicle && !self.entities.rides.has_passenger(mount, rider))
            || !equine::is_tamed(&entity.base)
        {
            return;
        }
        let position = entity.base.position.block();
        let menu = self.mount_menu_kind(mount);
        self.close_open_menu(player, true);
        let inventory = self.next_inventory_id();
        let open = OpenMenu {
            window: 0,
            inventory,
            title: "",
            menu: ContainerMenu::new(menu),
            backing: Backing {
                pos: position,
                second: None,
                entity: None,
                ender: false,
                block: BlockKind::Air,
                lid: Lid::None,
            },
            state_id: 0,
            local: Vec::new(),
            data: Vec::new(),
            trader: None,
            mount: Some(mount),
        };
        self.ask_open(player, position, open);
    }

    pub(crate) fn mount_menu_kind(&self, mount: EntityId) -> MenuKind {
        let Some(entity) = self.entities.store.get(mount) else {
            return MenuKind::Mount {
                columns: 0,
                saddle: false,
                armor: false,
                carpet: false,
            };
        };
        let columns =
            crate::kinds::mount_of(entity).map_or(0, |state| state.inventory_columns(&entity.base));
        MenuKind::Mount {
            columns: u8::try_from(columns).unwrap_or(0),
            saddle: mount_can_use_slot(entity, EquipmentSlot::Saddle)
                && rspigot_data::tags::tag("minecraft:entity_type", "minecraft:can_equip_saddle")
                    .is_some_and(|ids| {
                        ids.binary_search(&u32::from(entity.base.kind.id().0))
                            .is_ok()
                    }),
            carpet: entity.base.kind.extends("Llama"),
            armor: mount_can_use_slot(entity, EquipmentSlot::Body)
                && (entity.base.kind.extends("Llama")
                    || rspigot_data::tags::tag(
                        "minecraft:entity_type",
                        "minecraft:can_wear_horse_armor",
                    )
                    .is_some_and(|ids| {
                        ids.binary_search(&u32::from(entity.base.kind.id().0))
                            .is_ok()
                    })),
        }
    }

    pub(crate) fn mount_menu_items(&self, mount: EntityId, size: usize) -> Vec<Slot> {
        let mut items: Vec<Slot> = vec![None; size];
        let Some(entity) = self.entities.store.get(mount) else {
            return items;
        };
        if let Some(living) = entity.living() {
            if size > SADDLE {
                items[SADDLE] = living.equipment(EquipmentSlot::Saddle).cloned();
            }
            if size > BODY {
                items[BODY] = living.equipment(EquipmentSlot::Body).cloned();
            }
        }
        if let Some(state) = crate::kinds::mount_of(entity) {
            for (index, stack) in state.inventory.iter().enumerate() {
                if let Some(slot) = items.get_mut(equine::BODY_SLOT + 1 + index) {
                    slot.clone_from(stack);
                }
            }
        }
        items
    }

    pub(crate) fn write_mount_items(&mut self, mount: EntityId, items: &[Slot]) {
        let mut changed = Vec::new();
        if let Some(entity) = self.entities.store.get_mut(mount) {
            if let Some(living) = entity.living_mut() {
                for (slot, index) in [(EquipmentSlot::Saddle, SADDLE), (EquipmentSlot::Body, BODY)]
                {
                    let stack = items.get(index).cloned().flatten();
                    if living.equipment[living::slot_index(slot)] != stack {
                        living.equipment[living::slot_index(slot)] = stack.clone();
                        changed.push((wire_slot(slot), stack));
                    }
                }
            }
            if let Some(mount) = entity
                .living_mut()
                .and_then(|living| living.mob.as_mut())
                .and_then(|mob| mob.mount.as_deref_mut())
            {
                for (index, slot) in mount.inventory.iter_mut().enumerate() {
                    *slot = items.get(equine::BODY_SLOT + 1 + index).cloned().flatten();
                }
            }
        }
        if !changed.is_empty() {
            self.push(
                Audience::Trackers(mount),
                ClientEvent::EntityEquipment {
                    entity: mount,
                    slots: changed,
                },
            );
        }
    }

    pub(crate) fn mount_open_in_reach(&self, player: PlayerId, open: &OpenMenu) -> bool {
        let (Some(mount), Some(state)) = (open.mount, self.players.get(&player)) else {
            return false;
        };
        state.open_menu.is_none()
            && state.plugin_menu.is_none()
            && !state.vitals.is_dead()
            && self.mount_menu_valid_for(player, mount, open.menu.kind())
    }

    pub(crate) fn mount_menu_valid_for(
        &self,
        player: PlayerId,
        mount: EntityId,
        kind: MenuKind,
    ) -> bool {
        let Some(state) = self.players.get(&player) else {
            return false;
        };
        let Some(entity) = self.entities.store.get(mount) else {
            return false;
        };
        let alive = entity
            .living()
            .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living));
        let near = Vec3::new(
            entity.base.position.x,
            entity.base.position.y + f64::from(entity.base.dimensions.height) / 2.0,
            entity.base.position.z,
        )
        .distance_squared(state.position)
            <= INTERACTION_RANGE_SQUARED + f64::from(entity.base.dimensions.height);
        alive && near && self.mount_menu_kind(mount) == kind
    }

    pub(crate) fn show_mount_menu(&mut self, player: PlayerId, mut open: OpenMenu) {
        let Some(mount) = open.mount else {
            return;
        };
        let items = self.mount_menu_items(mount, open.menu.kind().container_size());
        let columns = match open.menu.kind() {
            MenuKind::Mount { columns, .. } => i32::from(columns),
            _ => 0,
        };
        let Some(state) = self.players.get_mut(&player) else {
            return;
        };
        let window = next_window(state);
        open.window = window;
        open.local = items.clone();
        state.link.send(ClientEvent::OpenMountScreen {
            window,
            columns,
            entity: mount,
        });
        sync_menu_view(
            &state.link,
            window,
            &mut open.state_id,
            &mut open.menu,
            &items,
            &state.inventory,
            true,
        );
        state.open_menu = Some(open);
    }
}
