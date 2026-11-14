use rspigot_domain::anvil;
use rspigot_domain::click::{Classified, ClickAction, ClickType, Drag};
use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::crafting;
use rspigot_domain::grindstone;
use rspigot_domain::item::{Slot, stacks_match};
use rspigot_domain::menu::{ClickInput, Clicker, InventoryMenu, OUTSIDE, RESULT_SLOT};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::smithing;

use crate::events::ContainerClick;
use crate::menu::{MENU_SLOTS, Move};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    ClickKind, CraftItem, DragType, EventResult, InventoryAction, InventoryClick, InventoryDrag,
    InventoryTypeSlotType, SmithItem,
};
use crate::plugins::prepare::Inputs;
use crate::plugins::views::{Described, Window};
use crate::plugins::{EventKind, EventPatch, EventPayload, ItemSpec, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;

const OFFHAND_BUTTON: i8 = 40;
const NO_SLOT: i16 = -1;
const CRAFT_GRID: std::ops::RangeInclusive<usize> = 1..=9;

#[derive(Debug)]
pub(crate) struct WindowPacket {
    pub(crate) player: PlayerId,
    pub(crate) click: ContainerClick,
}

#[derive(Debug)]
pub(crate) struct WindowClick {
    pub(crate) player: PlayerId,
    pub(crate) window: Window,
    pub(crate) click: ContainerClick,
    pub(crate) classified: Classified,
    pub(crate) result: EventResult,
    pub(crate) game_mode: GameMode,
    pub(crate) current: Slot,
    pub(crate) cursor: Slot,
    pub(crate) recipe: Option<&'static str>,
    pub(crate) smithed: Slot,
}

#[derive(Debug)]
pub(crate) struct DragStep {
    pub(crate) player: PlayerId,
    pub(crate) window: Window,
    pub(crate) click: ContainerClick,
    pub(crate) drag: Drag,
}

#[derive(Debug, Default)]
pub(crate) struct Writes {
    pub(crate) current_item: Option<Option<ItemSpec>>,
    pub(crate) cursor: Option<Option<ItemSpec>>,
    pub(crate) result: Option<EventResult>,
}

impl Writes {
    pub(crate) fn of(patch: Option<EventPatch>) -> Self {
        match patch {
            Some(EventPatch::InventoryClick(patch)) => Self {
                current_item: patch.current_item,
                cursor: patch.cursor,
                result: patch.result,
            },
            Some(EventPatch::CraftItem(patch)) => Self {
                current_item: patch.current_item,
                cursor: patch.cursor,
                result: patch.result,
            },
            Some(EventPatch::SmithItem(patch)) => Self {
                current_item: patch.current_item,
                cursor: patch.cursor,
                result: patch.result,
            },
            Some(EventPatch::InventoryCreative(patch)) => Self {
                current_item: patch.current_item,
                cursor: patch.cursor,
                result: patch.result,
            },
            Some(EventPatch::InventoryDrag(patch)) => Self {
                current_item: None,
                cursor: patch.cursor,
                result: patch.result,
            },
            _ => Self::default(),
        }
    }

    const fn writes(&self) -> bool {
        self.current_item.is_some() || self.cursor.is_some()
    }
}

pub(crate) fn written(spec: Option<ItemSpec>) -> Option<Slot> {
    match spec {
        None => Some(None),
        Some(spec) => stack_of(&spec).ok(),
    }
}

const fn click_kind(click: ClickType) -> ClickKind {
    match click {
        ClickType::Left => ClickKind::Left,
        ClickType::Right => ClickKind::Right,
        ClickType::ShiftLeft => ClickKind::ShiftLeft,
        ClickType::ShiftRight => ClickKind::ShiftRight,
        ClickType::NumberKey => ClickKind::NumberKey,
        ClickType::SwapOffhand => ClickKind::SwapOffhand,
        ClickType::Drop => ClickKind::Drop,
        ClickType::ControlDrop => ClickKind::ControlDrop,
        ClickType::DoubleClick => ClickKind::DoubleClick,
        ClickType::Middle => ClickKind::Middle,
        ClickType::Unknown => ClickKind::Unknown,
    }
}

const fn action_of(action: ClickAction) -> InventoryAction {
    match action {
        ClickAction::Nothing => InventoryAction::Nothing,
        ClickAction::PickupAll => InventoryAction::PickupAll,
        ClickAction::PickupSome => InventoryAction::PickupSome,
        ClickAction::PickupHalf => InventoryAction::PickupHalf,
        ClickAction::PickupOne => InventoryAction::PickupOne,
        ClickAction::PlaceAll => InventoryAction::PlaceAll,
        ClickAction::PlaceSome => InventoryAction::PlaceSome,
        ClickAction::PlaceOne => InventoryAction::PlaceOne,
        ClickAction::SwapWithCursor => InventoryAction::SwapWithCursor,
        ClickAction::DropAllCursor => InventoryAction::DropAllCursor,
        ClickAction::DropOneCursor => InventoryAction::DropOneCursor,
        ClickAction::DropAllSlot => InventoryAction::DropAllSlot,
        ClickAction::DropOneSlot => InventoryAction::DropOneSlot,
        ClickAction::MoveToOtherInventory => InventoryAction::MoveToOtherInventory,
        ClickAction::HotbarMoveAndReadd => InventoryAction::HotbarMoveAndReadd,
        ClickAction::HotbarSwap => InventoryAction::HotbarSwap,
        ClickAction::CloneStack => InventoryAction::CloneStack,
        ClickAction::CollectToCursor => InventoryAction::CollectToCursor,
        ClickAction::Unknown => InventoryAction::Unknown,
    }
}

pub(crate) const fn left_click(kind: ClickKind) -> bool {
    matches!(
        kind,
        ClickKind::Left | ClickKind::ShiftLeft | ClickKind::DoubleClick | ClickKind::Creative
    )
}

pub(crate) const fn right_click(kind: ClickKind) -> bool {
    matches!(kind, ClickKind::Right | ClickKind::ShiftRight)
}

pub(crate) const fn shift_click(kind: ClickKind) -> bool {
    matches!(kind, ClickKind::ShiftLeft | ClickKind::ShiftRight)
}

fn menu_index(slot: i16) -> Option<usize> {
    usize::try_from(slot).ok()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_container_click(&mut self, id: PlayerId, click: ContainerClick) {
        self.fire_deferred(
            id,
            Continuation::Window(Box::new(WindowPacket { player: id, click })),
        );
    }

    fn window_slots(&self, id: PlayerId, window: Window) -> Option<usize> {
        let player = self.players.get(&id)?;
        Some(match window {
            Window::Inventory => usize::from(MENU_SLOTS.unsigned_abs()),
            Window::Container(_) => player.open_menu.as_ref()?.menu.kind().slot_count(),
            Window::Plugin(_) => player.plugin_menu.as_ref()?.menu.kind().slot_count(),
        })
    }

    fn window_valid(&self, id: PlayerId, window: Window) -> bool {
        match window {
            Window::Inventory => true,
            Window::Container(_) => self.open_menu_is_valid(id),
            Window::Plugin(_) => self.plugin_menu_items(id).is_some(),
        }
    }

    fn window_contents(&self, id: PlayerId, window: Window) -> Vec<Slot> {
        match window {
            Window::Inventory => Vec::new(),
            Window::Container(_) => self.open_menu_items(id).unwrap_or_default(),
            Window::Plugin(_) => self
                .plugin_menu_items(id)
                .map(<[Slot]>::to_vec)
                .unwrap_or_default(),
        }
    }

    fn window_item(&self, id: PlayerId, window: Window, slot: i16, contents: &[Slot]) -> Slot {
        let index = menu_index(slot)?;
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => player.inventory.get(index).cloned(),
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                open.menu.item(index, contents, &player.inventory)
            }
            Window::Plugin(_) => {
                let open = player.plugin_menu.as_ref()?;
                open.menu.item(index, contents, &player.inventory)
            }
        }
    }

    pub(crate) fn window_cursor(&self, id: PlayerId, window: Window) -> Slot {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => player.menu.carried().clone(),
            Window::Container(_) => player.open_menu.as_ref()?.menu.carried().clone(),
            Window::Plugin(_) => player.plugin_menu.as_ref()?.menu.carried().clone(),
        }
    }

    fn quick_crafting(&self, id: PlayerId, window: Window) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        match window {
            Window::Inventory => player.menu.quick_crafting(),
            Window::Container(_) => player
                .open_menu
                .as_ref()
                .is_some_and(|open| open.menu.quick_crafting()),
            Window::Plugin(_) => player
                .plugin_menu
                .as_ref()
                .is_some_and(|open| open.menu.quick_crafting()),
        }
    }

    fn classify_click(
        &self,
        id: PlayerId,
        window: Window,
        click: &ContainerClick,
        contents: &[Slot],
    ) -> Option<Classified> {
        let player = self.players.get(&id)?;
        let clicker = Clicker {
            infinite_materials: player.game_mode == GameMode::Creative,
            experience_level: player.vitals.experience.level,
        };
        Some(match window {
            Window::Inventory => player.menu.classify(
                &player.inventory,
                click.slot,
                click.button,
                click.input,
                clicker,
            ),
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                open.menu.classify(
                    contents,
                    &player.inventory,
                    click.slot,
                    click.button,
                    click.input,
                    clicker,
                )
            }
            Window::Plugin(_) => {
                let open = player.plugin_menu.as_ref()?;
                open.menu.classify(
                    contents,
                    &player.inventory,
                    click.slot,
                    click.button,
                    click.input,
                    clicker,
                )
            }
        })
    }

    fn pending_drag(&self, id: PlayerId, window: Window, button: i8) -> Option<Drag> {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => player.menu.pending_drag(&player.inventory, button),
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                open.menu
                    .pending_drag(&self.open_items(open), &player.inventory, button)
            }
            Window::Plugin(_) => {
                let open = player.plugin_menu.as_ref()?;
                open.menu
                    .pending_drag(self.plugin_menu_items(id)?, &player.inventory, button)
            }
        }
    }

    fn window_recipe(
        &self,
        id: PlayerId,
        window: Window,
        contents: &[Slot],
    ) -> Option<crafting::Recipe> {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => {
                crafting::recipe(2, 2, &InventoryMenu::crafting_grid(&player.inventory))
            }
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                (open.menu.kind() == MenuKind::Crafting)
                    .then(|| crafting::recipe(3, 3, contents.get(CRAFT_GRID)?))
                    .flatten()
            }
            Window::Plugin(_) => None,
        }
    }

    fn smithed(&self, id: PlayerId, window: Window, contents: &[Slot]) -> Slot {
        let player = self.players.get(&id)?;
        let open = player
            .open_menu
            .as_ref()
            .filter(|_| matches!(window, Window::Container(_)))?;
        if open.menu.kind() != MenuKind::Smithing {
            return None;
        }
        contents.get(smithing::RESULT).cloned().flatten()
    }

    fn at_lectern(&self, id: PlayerId, window: Window) -> bool {
        matches!(window, Window::Container(_))
            && self
                .players
                .get(&id)
                .and_then(|player| player.open_menu.as_ref())
                .is_some_and(|open| open.menu.kind() == MenuKind::Lectern)
    }

    fn click_heard(&self) -> bool {
        [
            EventKind::InventoryClick,
            EventKind::CraftItem,
            EventKind::SmithItem,
        ]
        .into_iter()
        .any(|kind| self.listened(kind))
    }

    pub(crate) fn crafting_recipe(&self, id: PlayerId, window: Window) -> Option<crafting::Recipe> {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => {
                crafting::recipe(2, 2, &InventoryMenu::crafting_grid(&player.inventory))
            }
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                if open.menu.kind() != MenuKind::Crafting {
                    return None;
                }
                let items = self.open_items(open);
                crafting::recipe(3, 3, items.get(CRAFT_GRID)?)
            }
            Window::Plugin(_) => None,
        }
    }

    pub(crate) fn click_at_head(&mut self, packet: WindowPacket) -> Option<Continuation> {
        let WindowPacket { player: id, click } = packet;
        let window = self.window_of(id)?;
        if click.container != window.id() {
            return None;
        }
        let player = self.players.get(&id)?;
        if player.vitals.is_dead() {
            self.resend_window(id, window);
            return None;
        }
        if !self.window_valid(id, window) {
            return None;
        }
        let total = i16::try_from(self.window_slots(id, window)?).unwrap_or(i16::MAX);
        let slot = click.slot;
        if slot != NO_SLOT && slot != OUTSIDE && !(0..total).contains(&slot) {
            if window == Window::Inventory && slot >= 0 {
                tracing::debug!(
                    "Player {} clicked invalid slot index: {slot}, available slots: {MENU_SLOTS}",
                    self.players
                        .get(&id)
                        .map_or("", |player| player.profile.name.as_str())
                );
            }
            if window == Window::Inventory
                && slot < 0
                && let Some(player) = self.players.get_mut(&id)
            {
                player.menu.suppress_remote_updates();
            }
            return None;
        }
        let spectator = self
            .players
            .get(&id)
            .is_some_and(|player| player.game_mode == GameMode::Spectator);
        let locked = matches!(window, Window::Plugin(_)) && self.plugin_menu_locked(id);
        if click.input == ClickInput::QuickCraft || self.quick_crafting(id, window) {
            let refused = locked || (spectator && !matches!(window, Window::Inventory));
            if !refused
                && click.input == ClickInput::QuickCraft
                && let Some(drag) = self.pending_drag(id, window, click.button)
            {
                return Some(Continuation::Drag(Box::new(DragStep {
                    player: id,
                    window,
                    click,
                    drag,
                })));
            }
            self.apply_window_click(id, window, &click, !refused, false);
            return None;
        }
        let refused = spectator || locked;
        if !self.click_heard() {
            self.apply_click(id, window, &click, !refused, false);
            return None;
        }
        let contents = self.window_contents(id, window);
        let recipe = (slot == 0)
            .then(|| self.window_recipe(id, window, &contents))
            .flatten();
        let smithed = (slot == i16::try_from(smithing::RESULT).unwrap_or(i16::MAX))
            .then(|| self.smithed(id, window, &contents))
            .flatten();
        let kind = if recipe.is_some() {
            EventKind::CraftItem
        } else if smithed.is_some() {
            EventKind::SmithItem
        } else {
            EventKind::InventoryClick
        };
        if !self.listened(kind) {
            self.apply_click(id, window, &click, !refused, false);
            return None;
        }
        let classified = self.classify_click(id, window, &click, &contents)?;
        let result = if refused {
            EventResult::Deny
        } else {
            EventResult::Default
        };
        let game_mode = self.players.get(&id)?.game_mode;
        let step = WindowClick {
            player: id,
            window,
            current: self.window_item(id, window, slot, &contents),
            cursor: self.window_cursor(id, window),
            game_mode,
            click,
            classified,
            result,
            recipe: recipe.map(|recipe| recipe.id),
            smithed,
        };
        if locked {
            let payload = self
                .listened(kind)
                .then(|| self.window_click_payload(kind, &step))
                .flatten();
            self.fire_refused(id, Continuation::Click(kind, Box::new(step)), move |_| {
                payload
            });
            return None;
        }
        Some(Continuation::Click(kind, Box::new(step)))
    }

    pub(crate) fn window_click_payload(
        &self,
        kind: EventKind,
        step: &WindowClick,
    ) -> Option<EventPayload> {
        let described = self.describe_window(step.player, step.window)?;
        let raw = i32::from(step.click.slot);
        let click = click_kind(step.classified.click);
        let slot_type = described.slot_type(raw);
        let current_item = if slot_type == InventoryTypeSlotType::Outside {
            None
        } else {
            step.current.as_ref().map(spec_of)
        };
        let hotbar_button = if click == ClickKind::NumberKey {
            i32::from(step.click.button)
        } else {
            -1
        };
        let Described {
            top, view, viewers, ..
        } = &described;
        let event = InventoryClick {
            action: action_of(step.classified.action),
            click,
            clicked_inventory: described.clicked_inventory(raw),
            current_item,
            cursor: step.cursor.as_ref().map(spec_of),
            hotbar_button,
            inventory: top.clone(),
            left_click: left_click(click),
            raw_slot: raw,
            result: step.result,
            right_click: right_click(click),
            shift_click: shift_click(click),
            slot: described.convert_slot(raw),
            slot_type,
            view: view.clone(),
            viewers: viewers.clone(),
            who_clicked: view.player.clone(),
        };
        Some(match kind {
            EventKind::CraftItem => EventPayload::CraftItem(CraftItem {
                action: event.action,
                click: event.click,
                clicked_inventory: event.clicked_inventory,
                current_item: event.current_item,
                cursor: event.cursor,
                hotbar_button: event.hotbar_button,
                inventory: event.inventory,
                left_click: event.left_click,
                raw_slot: event.raw_slot,
                recipe: step.recipe?.to_owned(),
                result: event.result,
                right_click: event.right_click,
                shift_click: event.shift_click,
                slot: event.slot,
                slot_type: event.slot_type,
                view: event.view,
                viewers: event.viewers,
                who_clicked: event.who_clicked,
            }),
            EventKind::SmithItem => EventPayload::SmithItem(SmithItem {
                action: event.action,
                click: event.click,
                clicked_inventory: event.clicked_inventory,
                current_item: event.current_item,
                cursor: event.cursor,
                hotbar_button: event.hotbar_button,
                inventory: event.inventory,
                left_click: event.left_click,
                raw_slot: event.raw_slot,
                result: event.result,
                right_click: event.right_click,
                shift_click: event.shift_click,
                slot: event.slot,
                slot_type: event.slot_type,
                view: event.view,
                viewers: event.viewers,
                who_clicked: event.who_clicked,
            }),
            _ => EventPayload::InventoryClick(event),
        })
    }

    pub(crate) fn finish_click(&mut self, kind: EventKind, step: WindowClick, writes: Writes) {
        let id = step.player;
        let window = step.window;
        if self.window_of(id) != Some(window) || !self.window_valid(id, window) {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() {
            return;
        }
        let mode_kept = player.game_mode == step.game_mode;
        let result = writes.result.unwrap_or(step.result);
        let mut act = result != EventResult::Deny;
        let heard = self.listened(kind);
        let mut full = heard && matches!(kind, EventKind::CraftItem | EventKind::SmithItem);
        let contents = self.window_contents(id, window);
        let current = self.window_item(id, window, step.click.slot, &contents);
        let cursor = self.window_cursor(id, window);
        let unchanged =
            stacks_match(&current, &step.current) && stacks_match(&cursor, &step.cursor);
        let same_result = match kind {
            EventKind::CraftItem => {
                self.window_recipe(id, window, &contents)
                    .map(|recipe| recipe.id)
                    == step.recipe
            }
            EventKind::SmithItem => {
                stacks_match(&self.smithed(id, window, &contents), &step.smithed)
            }
            _ => true,
        };
        if !mode_kept || !same_result || (writes.writes() && !unchanged) {
            act = false;
            full = true;
        } else {
            if let (Some(item), Some(slot), false) = (
                writes.current_item.and_then(written),
                menu_index(step.click.slot),
                self.at_lectern(id, window),
            ) {
                self.write_window_slot(id, window, slot, item);
            }
            if let Some(item) = writes.cursor.and_then(written) {
                self.write_window_cursor(id, window, item);
            }
        }
        self.apply_click(id, window, &step.click, act, full);
        let resent = matches!(window, Window::Plugin(_))
            || (heard && matches!(window, Window::Container(_)));
        if !act
            && resent
            && step.click.input == ClickInput::Swap
            && step.click.button == OFFHAND_BUTTON
        {
            self.resend_offhand(id);
        }
    }

    fn apply_click(
        &mut self,
        id: PlayerId,
        window: Window,
        click: &ContainerClick,
        act: bool,
        full: bool,
    ) {
        if !(act && self.shift_result(id, window, click, full)) {
            self.apply_window_click(id, window, click, act, full);
        }
    }

    pub(crate) fn apply_window_click(
        &mut self,
        id: PlayerId,
        window: Window,
        click: &ContainerClick,
        act: bool,
        full: bool,
    ) {
        let grid = self.inputs_before(id, window);
        let act = act.then_some(Move::Click);
        self.apply_window_move(id, window, click, act, full);
        self.inputs_changed(id, window, grid);
    }

    pub(crate) fn apply_window_move(
        &mut self,
        id: PlayerId,
        window: Window,
        click: &ContainerClick,
        act: Option<Move>,
        full: bool,
    ) -> Slot {
        match window {
            Window::Inventory => self.apply_inventory_click(id, click, act, full),
            Window::Container(_) => self.apply_container_click(id, click, act, full),
            Window::Plugin(_) => {
                self.apply_plugin_click(id, click, act.is_some(), full);
                None
            }
        }
    }

    pub(crate) fn write_window_slot(
        &mut self,
        id: PlayerId,
        window: Window,
        slot: usize,
        stack: Slot,
    ) {
        let grid = self.inputs_before(id, window);
        match window {
            Window::Inventory => {
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                let before = Self::equipment_of(player);
                let effects = player
                    .menu
                    .set_by_player(&mut player.inventory, slot, stack);
                self.apply_menu_effects(id, effects);
                self.broadcast_equipment_changes(id, before);
            }
            Window::Container(_) => {
                let Some(mut items) = self.open_menu_items(id) else {
                    return;
                };
                let before = items.clone();
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                let Some(open) = player.open_menu.as_mut() else {
                    return;
                };
                open.menu
                    .set_slot(&mut items, &mut player.inventory, slot, stack);
                self.store_open_items(id, &before, items);
            }
            Window::Plugin(_) => {
                self.with_plugin_menu(id, |menu, items, inventory| {
                    menu.set_slot(items, inventory, slot, stack)
                });
            }
        }
        self.inputs_changed(id, window, grid);
    }

    pub(crate) fn write_window_cursor(&mut self, id: PlayerId, window: Window, stack: Slot) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        match window {
            Window::Inventory => player.menu.set_carried(stack.clone()),
            Window::Container(_) => {
                if let Some(open) = player.open_menu.as_mut() {
                    open.menu.set_carried(stack.clone());
                }
            }
            Window::Plugin(_) => {
                if let Some(open) = player.plugin_menu.as_mut() {
                    open.menu.set_carried(stack.clone());
                }
            }
        }
        player
            .link
            .send(crate::events::ClientEvent::CursorItem(stack));
    }

    pub(crate) fn resend_window(&mut self, id: PlayerId, window: Window) {
        match window {
            Window::Inventory => self.send_inventory(id),
            Window::Container(_) => self.sync_open_menu(id, true),
            Window::Plugin(_) => self.sync_plugin_viewer(id, true),
        }
    }

    pub(crate) fn drag_payload(&self, step: &DragStep) -> Option<EventPayload> {
        let described = self.describe_window(step.player, step.window)?;
        let mut placed: Vec<(i32, ItemSpec)> = step
            .drag
            .placed
            .iter()
            .map(|(slot, stack)| (i32::try_from(*slot).unwrap_or(i32::MAX), spec_of(stack)))
            .collect();
        placed.sort_by_key(|(slot, _)| *slot);
        let raw_slots: Vec<i32> = placed.iter().map(|(slot, _)| *slot).collect();
        Some(EventPayload::InventoryDrag(InventoryDrag {
            cursor: step.drag.remaining.as_ref().map(spec_of),
            inventory: described.top.clone(),
            inventory_slots: raw_slots
                .iter()
                .map(|raw| described.convert_slot(*raw))
                .collect(),
            new_items: placed,
            old_cursor: spec_of(&step.drag.cursor),
            raw_slots,
            result: EventResult::Default,
            drag_type: if step.drag.single {
                DragType::Single
            } else {
                DragType::Even
            },
            view: described.view.clone(),
            viewers: described.viewers.clone(),
            who_clicked: described.view.player,
        }))
    }

    pub(crate) fn finish_drag(&mut self, step: DragStep, writes: Writes) {
        let id = step.player;
        let window = step.window;
        if self.window_of(id) != Some(window) || !self.window_valid(id, window) {
            return;
        }
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.vitals.is_dead())
        {
            return;
        }
        let holds = self.drag_holds(id, window, &step.drag);
        let denied = writes.result == Some(EventResult::Deny) || !holds;
        let cursor = if denied {
            None
        } else {
            Some(
                writes
                    .cursor
                    .and_then(written)
                    .unwrap_or_else(|| step.drag.remaining.clone()),
            )
        };
        let grid = self.inputs_before(id, window);
        self.end_drag(id, window, &step.drag, cursor);
        self.apply_window_click(id, window, &step.click, false, true);
        self.inputs_changed(id, window, grid);
    }

    fn drag_holds(&self, id: PlayerId, window: Window, drag: &Drag) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        match window {
            Window::Inventory => player.menu.drag_holds(&player.inventory, drag),
            Window::Container(_) => player.open_menu.as_ref().is_some_and(|open| {
                open.menu
                    .drag_holds(&self.open_items(open), &player.inventory, drag)
            }),
            Window::Plugin(_) => player.plugin_menu.as_ref().is_some_and(|open| {
                self.plugin_menu_items(id)
                    .is_some_and(|items| open.menu.drag_holds(items, &player.inventory, drag))
            }),
        }
    }

    fn end_drag(&mut self, id: PlayerId, window: Window, drag: &Drag, cursor: Option<Slot>) {
        match window {
            Window::Inventory => {
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                let before = Self::equipment_of(player);
                let effects = player.menu.finish_drag(&mut player.inventory, drag, cursor);
                let effects = effects
                    .into_iter()
                    .filter(|effect| {
                        !matches!(effect, rspigot_domain::menu::MenuEffect::FullResync)
                    })
                    .collect();
                self.apply_menu_effects(id, effects);
                self.broadcast_equipment_changes(id, before);
            }
            Window::Container(_) => {
                let Some(mut items) = self.open_menu_items(id) else {
                    return;
                };
                let before = items.clone();
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                let Some(open) = player.open_menu.as_mut() else {
                    return;
                };
                open.menu
                    .finish_drag(&mut items, &mut player.inventory, drag, cursor);
                self.store_open_items(id, &before, items);
            }
            Window::Plugin(_) => {
                self.with_plugin_menu(id, |menu, items, inventory| {
                    menu.finish_drag(items, inventory, drag, cursor)
                });
                self.sync_plugin_viewers_of(id);
            }
        }
    }

    pub(crate) fn crafting_inputs(&self, id: PlayerId, window: Window) -> Option<Vec<Slot>> {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => Some(InventoryMenu::crafting_grid(&player.inventory).to_vec()),
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                let range = match open.menu.kind() {
                    MenuKind::Crafting => CRAFT_GRID,
                    MenuKind::Smithing => 0..=smithing::INPUTS - 1,
                    MenuKind::Anvil => anvil::INPUT..=anvil::ADDITION,
                    MenuKind::Grindstone => grindstone::FIRST..=grindstone::SECOND,
                    _ => return None,
                };
                self.open_items(open).get(range).map(<[Slot]>::to_vec)
            }
            Window::Plugin(_) => None,
        }
    }

    pub(crate) fn result_of(&self, id: PlayerId, window: Window) -> Slot {
        let player = self.players.get(&id)?;
        match window {
            Window::Inventory => player.inventory.get(RESULT_SLOT).cloned(),
            Window::Container(_) => {
                let open = player.open_menu.as_ref()?;
                let slot = match open.menu.kind() {
                    MenuKind::Crafting => 0,
                    MenuKind::Smithing => smithing::RESULT,
                    MenuKind::Anvil => anvil::RESULT,
                    MenuKind::Grindstone => grindstone::RESULT,
                    _ => return None,
                };
                self.open_items(open).get(slot).cloned().flatten()
            }
            Window::Plugin(_) => None,
        }
    }

    pub(crate) fn inputs_changed(&mut self, id: PlayerId, window: Window, before: Inputs) {
        let after = self.crafting_inputs(id, window);
        let changed = match (&before.grid, &after) {
            (Some(old), Some(new)) => {
                old.len() != new.len()
                    || old
                        .iter()
                        .zip(new)
                        .any(|(old, new)| !stacks_match(old, new))
            }
            _ => false,
        };
        let carried = match after {
            Some(after) if changed => self.fire_prepare(id, window, after, None, before.held),
            _ => false,
        };
        if before.held && !carried {
            self.release_result(id, window);
        }
    }
}

const CREATIVE_SLOTS: std::ops::RangeInclusive<i16> = 1..=45;
const CREATIVE_STORAGE_END: i16 = 36;
const CREATIVE_ARMOR: std::ops::RangeInclusive<i16> = 5..=8;

#[derive(Debug)]
pub(crate) struct CreativePacket {
    pub(crate) player: PlayerId,
    pub(crate) slot: i16,
    pub(crate) stack: Slot,
}

#[derive(Debug)]
pub(crate) struct CreativeStep {
    pub(crate) player: PlayerId,
    pub(crate) slot: i16,
    pub(crate) stack: Slot,
    pub(crate) valid: bool,
    pub(crate) current: Slot,
}

fn creative_slot_type(slot: i16) -> InventoryTypeSlotType {
    if slot < 0 {
        InventoryTypeSlotType::Outside
    } else if slot >= CREATIVE_STORAGE_END {
        InventoryTypeSlotType::Quickbar
    } else if CREATIVE_ARMOR.contains(&slot) {
        InventoryTypeSlotType::Armor
    } else {
        InventoryTypeSlotType::Container
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_creative_slot(&mut self, id: PlayerId, slot: i16, stack: Slot) {
        self.fire_deferred(
            id,
            Continuation::CreativeSlot(Box::new(CreativePacket {
                player: id,
                slot,
                stack,
            })),
        );
    }

    pub(crate) fn creative_at_head(&mut self, packet: CreativePacket) -> Option<Continuation> {
        let CreativePacket {
            player: id,
            slot,
            stack,
        } = packet;
        let player = self.players.get(&id)?;
        if player.game_mode != GameMode::Creative {
            return None;
        }
        let drop = slot < 0;
        let placed = CREATIVE_SLOTS.contains(&slot);
        let valid = stack
            .as_ref()
            .is_none_or(|stack| stack.count.get() <= stack.item.max_stack_size());
        let current = menu_index(slot)
            .filter(|_| placed)
            .and_then(|index| player.inventory.get(index))
            .cloned();
        if drop || (placed && !stacks_match(&current, &stack)) {
            return Some(Continuation::Creative(Box::new(CreativeStep {
                player: id,
                slot,
                stack,
                valid,
                current,
            })));
        }
        if placed && valid {
            self.place_creative(id, slot, stack);
        }
        None
    }

    fn place_creative(&mut self, id: PlayerId, slot: i16, stack: Slot) {
        let Some(index) = menu_index(slot) else {
            return;
        };
        let grid = self.inputs_before(id, Window::Inventory);
        self.set_slot_by_player(id, index, stack);
        self.inputs_changed(id, Window::Inventory, grid);
    }

    pub(crate) fn creative_payload(&self, step: &CreativeStep) -> Option<EventPayload> {
        let described = self.describe_window(step.player, Window::Inventory)?;
        let raw = if step.slot < 0 {
            i32::from(OUTSIDE)
        } else {
            i32::from(step.slot)
        };
        Some(EventPayload::InventoryCreative(
            crate::plugins::events::InventoryCreative {
                action: InventoryAction::PlaceAll,
                click: ClickKind::Creative,
                clicked_inventory: described.clicked_inventory(raw),
                current_item: step.current.as_ref().map(spec_of),
                cursor: step.stack.as_ref().map(spec_of),
                hotbar_button: -1,
                inventory: described.top.clone(),
                left_click: left_click(ClickKind::Creative),
                raw_slot: raw,
                result: EventResult::Default,
                right_click: false,
                shift_click: false,
                slot: described.convert_slot(raw),
                slot_type: creative_slot_type(step.slot),
                view: described.view.clone(),
                viewers: described.viewers.clone(),
                who_clicked: described.view.player,
            },
        ))
    }

    pub(crate) fn finish_creative(&mut self, step: CreativeStep, writes: Writes) {
        let id = step.player;
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let placed = CREATIVE_SLOTS.contains(&step.slot);
        let current = menu_index(step.slot)
            .filter(|_| placed)
            .and_then(|index| player.inventory.get(index))
            .cloned();
        let creative = player.game_mode == GameMode::Creative;
        if !creative || (writes.writes() && !stacks_match(&current, &step.current)) {
            self.send_inventory(id);
            return;
        }
        if let Some(item) = writes.current_item.and_then(written)
            && placed
        {
            self.place_creative(id, step.slot, item);
        }
        let stack = writes.cursor.and_then(written).unwrap_or(step.stack);
        let valid = match writes.result.unwrap_or(EventResult::Default) {
            EventResult::Deny => {
                if step.slot >= 0 {
                    self.send_inventory(id);
                }
                return;
            }
            EventResult::Allow => true,
            EventResult::Default => step.valid,
        };
        if !valid {
            return;
        }
        if placed {
            self.place_creative(id, step.slot, stack);
        } else if step.slot < 0 {
            self.drop_creative(id, stack);
        }
    }
}
