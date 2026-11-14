use rspigot_domain::anvil;
use rspigot_domain::container_menu::MenuKind;
use rspigot_domain::item::{ItemStack, Slot, same_item, stacks_match};
use rspigot_domain::menu::{ClickInput, RESULT_SLOT};
use rspigot_domain::player::PlayerId;
use rspigot_domain::smithing::{self, Outcome};

use crate::events::ContainerClick;
use crate::menu::Move;
use crate::plugins::clicks::written;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    PrepareAnvil, PrepareAnvilPatch, PrepareItemCraft, PrepareItemCraftPatch, PrepareSmithing,
    PrepareSmithingPatch,
};
use crate::plugins::views::Window;
use crate::plugins::{Access, EventKind, EventPatch, EventPayload, ItemSpec, spec_of};
use crate::ports::ClientLink;
use crate::server::Server;

const CRAFT_RESULT: usize = 0;

#[derive(Debug)]
pub(crate) struct ResultLoop {
    pub(crate) slot: usize,
    pub(crate) moved: ItemStack,
    pub(crate) click: ContainerClick,
}

#[derive(Debug)]
pub(crate) struct PrepareStep {
    pub(crate) player: PlayerId,
    pub(crate) window: Window,
    pub(crate) inputs: Vec<Slot>,
    pub(crate) vanilla: Slot,
    pub(crate) then: Option<ResultLoop>,
    pub(crate) held: bool,
}

pub(crate) struct Inputs {
    pub(crate) grid: Option<Vec<Slot>>,
    pub(crate) held: bool,
}

pub(crate) fn settled_result(patch: Option<EventPatch>) -> Option<Option<ItemSpec>> {
    match patch {
        Some(EventPatch::PrepareItemCraft(PrepareItemCraftPatch { result })) => result,
        Some(EventPatch::PrepareSmithing(PrepareSmithingPatch { result })) => result,
        Some(EventPatch::PrepareAnvil(PrepareAnvilPatch { result })) => result,
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    fn prepare_kind(&self, id: PlayerId, window: Window, inputs: &[Slot]) -> Option<EventKind> {
        let kind = match window {
            Window::Inventory => return Some(EventKind::PrepareItemCraft),
            Window::Container(_) => self.players.get(&id)?.open_menu.as_ref()?.menu.kind(),
            Window::Plugin(_) => return None,
        };
        match kind {
            MenuKind::Crafting => Some(EventKind::PrepareItemCraft),
            MenuKind::Anvil => Some(EventKind::PrepareAnvil),
            MenuKind::Smithing => {
                let made = smithing::craft(
                    inputs.get(smithing::TEMPLATE)?,
                    inputs.get(smithing::BASE)?,
                    inputs.get(smithing::ADDITION)?,
                );
                (made != Outcome::NoRecipe).then_some(EventKind::PrepareSmithing)
            }
            _ => None,
        }
    }

    fn result_slot(&self, id: PlayerId, window: Window) -> Option<usize> {
        match window {
            Window::Inventory => Some(RESULT_SLOT),
            Window::Container(_) => match self.players.get(&id)?.open_menu.as_ref()?.menu.kind() {
                MenuKind::Crafting => Some(CRAFT_RESULT),
                MenuKind::Smithing => Some(smithing::RESULT),
                MenuKind::Anvil => Some(anvil::RESULT),
                _ => None,
            },
            Window::Plugin(_) => None,
        }
    }

    pub(crate) fn prepare_decided(&self, id: PlayerId, window: Window) -> bool {
        let Some(inputs) = self.crafting_inputs(id, window) else {
            return false;
        };
        let menu = match window {
            Window::Container(_) => self
                .players
                .get(&id)
                .and_then(|player| player.open_menu.as_ref())
                .map(|open| open.menu.kind()),
            _ => None,
        };
        let kind = match menu {
            Some(MenuKind::Smithing) => EventKind::PrepareSmithing,
            Some(MenuKind::Anvil) => EventKind::PrepareAnvil,
            _ => EventKind::PrepareItemCraft,
        };
        !inputs.is_empty() && self.plugins.listens(kind, Access::Decide)
    }

    pub(crate) fn inputs_before(&mut self, id: PlayerId, window: Window) -> Inputs {
        let grid = self.crafting_inputs(id, window);
        let held = grid.is_some() && self.prepare_decided(id, window);
        if held {
            self.hold_result(id, window);
        }
        Inputs { grid, held }
    }

    fn hold_result(&mut self, id: PlayerId, window: Window) {
        let Some(slot) = self.result_slot(id, window) else {
            return;
        };
        let items = match window {
            Window::Container(_) => self.open_menu_items(id),
            _ => None,
        };
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        match window {
            Window::Inventory => player.menu.hold_result(&player.inventory),
            Window::Container(_) => {
                if let (Some(open), Some(items)) = (player.open_menu.as_mut(), items) {
                    open.menu.hold(slot, &items, &player.inventory);
                }
            }
            Window::Plugin(_) => {}
        }
    }

    pub(crate) fn release_result(&mut self, id: PlayerId, window: Window) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        match window {
            Window::Inventory => player.menu.release_result(),
            Window::Container(shown) => {
                if let Some(open) = player
                    .open_menu
                    .as_mut()
                    .filter(|open| open.window == shown)
                {
                    open.menu.release();
                }
            }
            Window::Plugin(_) => {}
        }
    }

    pub(crate) fn fire_prepare(
        &mut self,
        id: PlayerId,
        window: Window,
        inputs: Vec<Slot>,
        then: Option<ResultLoop>,
        held: bool,
    ) -> bool {
        let kind = self
            .prepare_kind(id, window, &inputs)
            .filter(|kind| self.listened(*kind));
        let Some(kind) = kind else {
            if held {
                self.release_result(id, window);
            }
            if let Some(next) = then {
                self.continue_result(id, window, next);
            }
            return held;
        };
        let vanilla = self.result_of(id, window);
        let recipe = (kind == EventKind::PrepareItemCraft)
            .then(|| self.crafting_recipe(id, window))
            .flatten();
        let result = vanilla.as_ref().map(spec_of);
        let step = PrepareStep {
            player: id,
            window,
            inputs,
            vanilla,
            then,
            held,
        };
        self.fire(
            Lane::Player(id),
            Continuation::Prepare(kind, Box::new(step)),
            move |server| {
                let described = server.describe_window(id, window)?;
                Some(match kind {
                    EventKind::PrepareSmithing => EventPayload::PrepareSmithing(PrepareSmithing {
                        inventory: described.top,
                        result,
                        view: described.view,
                        viewers: described.viewers,
                    }),
                    EventKind::PrepareAnvil => EventPayload::PrepareAnvil(PrepareAnvil {
                        inventory: described.top,
                        result,
                        view: described.view,
                        viewers: described.viewers,
                    }),
                    _ => EventPayload::PrepareItemCraft(PrepareItemCraft {
                        inventory: described.top,
                        recipe: recipe.map(|recipe| recipe.id.to_owned()),
                        repair: recipe.is_some_and(|recipe| recipe.repair),
                        view: described.view,
                        viewers: described.viewers,
                        result,
                    }),
                })
            },
        );
        held
    }

    pub(crate) fn finish_prepare(&mut self, step: PrepareStep, result: Option<Option<ItemSpec>>) {
        let PrepareStep {
            player: id,
            window,
            inputs,
            vanilla,
            then,
            held,
        } = step;
        if held {
            self.release_result(id, window);
        }
        if self.window_of(id) != Some(window) {
            return;
        }
        let current = self.crafting_inputs(id, window);
        let same = current.as_ref().is_some_and(|current| {
            current.len() == inputs.len()
                && current
                    .iter()
                    .zip(&inputs)
                    .all(|(now, then)| stacks_match(now, then))
        });
        if !same {
            return;
        }
        let settled = result.and_then(written).unwrap_or(vanilla);
        self.put_result(id, window, settled);
        if let Some(next) = then {
            self.continue_result(id, window, next);
        }
    }

    fn put_result(&mut self, id: PlayerId, window: Window, stack: Slot) {
        let Some(slot) = self.result_slot(id, window) else {
            return;
        };
        match window {
            Window::Inventory => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.inventory.set(slot, stack);
                }
                self.sync_inventory(id);
            }
            Window::Container(_) => {
                if let Some(entry) = self
                    .players
                    .get_mut(&id)
                    .and_then(|player| player.open_menu.as_mut())
                    .and_then(|open| {
                        open.local.resize(open.menu.kind().container_size(), None);
                        open.local.get_mut(slot)
                    })
                {
                    *entry = stack;
                }
                self.sync_open_menu(id, false);
            }
            Window::Plugin(_) => {}
        }
    }

    pub(crate) fn shift_result(
        &mut self,
        id: PlayerId,
        window: Window,
        click: &ContainerClick,
        full: bool,
    ) -> bool {
        let Some(slot) = self.result_slot(id, window) else {
            return false;
        };
        let on_result = click.input == ClickInput::QuickMove
            && usize::try_from(click.slot).is_ok_and(|clicked| clicked == slot);
        if !on_result || !self.prepare_decided(id, window) {
            return false;
        }
        let grid = self.inputs_before(id, window);
        let moved = self.apply_window_move(id, window, click, Some(Move::Once(slot)), full);
        self.after_result_move(id, window, grid, slot, moved, click);
        true
    }

    fn continue_result(&mut self, id: PlayerId, window: Window, next: ResultLoop) {
        let emptied = self
            .crafting_inputs(id, window)
            .is_none_or(|inputs| inputs.iter().all(Option::is_none));
        if emptied
            || self.window_of(id) != Some(window)
            || !same_item(&self.result_of(id, window), &Some(next.moved.clone()))
        {
            return;
        }
        let grid = self.inputs_before(id, window);
        let click = ContainerClick {
            changed: Vec::new(),
            ..next.click
        };
        let moved = self.apply_window_move(id, window, &click, Some(Move::Once(next.slot)), false);
        self.after_result_move(id, window, grid, next.slot, moved, &click);
    }

    fn after_result_move(
        &mut self,
        id: PlayerId,
        window: Window,
        grid: Inputs,
        slot: usize,
        moved: Slot,
        click: &ContainerClick,
    ) {
        let then = moved.map(|moved| ResultLoop {
            slot,
            moved,
            click: click.clone(),
        });
        let after = self.crafting_inputs(id, window);
        let changed = match (&grid.grid, &after) {
            (Some(before), Some(after)) => {
                before.len() != after.len()
                    || before
                        .iter()
                        .zip(after)
                        .any(|(old, new)| !stacks_match(old, new))
            }
            _ => false,
        };
        match after {
            Some(after) if changed => {
                self.fire_prepare(id, window, after, then, grid.held);
            }
            _ => {
                if grid.held {
                    self.release_result(id, window);
                }
                if let Some(next) = then {
                    self.continue_result(id, window, next);
                }
            }
        }
    }
}
