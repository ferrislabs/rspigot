use rand::Rng;
use rspigot_data::enchantments;
use rspigot_domain::BlockPos;
use rspigot_domain::container_menu::{EnchantPlan, MenuKind};
use rspigot_domain::enchanting;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::item::Slot;
use rspigot_domain::menu::Clicker;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_entities::SoundSource;

use crate::plugins::events::{EnchantItem, EnchantItemPatch, PrepareItemEnchant};
use crate::plugins::views::Window;
use crate::plugins::{Access, Continuation, EnchantOffer, EventKind, EventPayload, Lane, spec_of};
use crate::ports::ClientLink;
use crate::server::Server;

const POWER_PROVIDER: &str = "minecraft:enchantment_power_provider";
const POWER_TRANSMITTER: &str = "minecraft:enchantment_power_transmitter";
const TABLE_USE: &str = "minecraft:block.enchantment_table.use";

#[derive(Debug)]
pub(crate) struct PrepareEnchantStep {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
}

#[derive(Debug)]
pub(crate) struct EnchantStep {
    pub(crate) player: PlayerId,
    pub(crate) window: i32,
    pub(crate) plan: EnchantPlan,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn bookshelf_count(&self, pos: BlockPos) -> i32 {
        let chunks = &self.world.chunks;
        enchanting::bookshelf_offsets()
            .filter(|(x, y, z)| {
                let shelf = chunks.block(pos.offset(*x, *y, *z));
                let between = chunks.block(pos.offset(x / 2, *y, z / 2));
                shelf.is_some_and(|state| has_tag(state, POWER_PROVIDER))
                    && between.is_some_and(|state| has_tag(state, POWER_TRANSMITTER))
            })
            .count() as i32
    }

    fn enchanting_window(&self, id: PlayerId) -> Option<(i32, BlockPos)> {
        let open = self.players.get(&id)?.open_menu.as_ref()?;
        (open.menu.kind() == MenuKind::Enchantment).then_some((open.window, open.backing.pos))
    }

    pub(crate) fn prepare_enchanting_menu(&mut self, id: PlayerId) {
        let Some((_, pos)) = self.enchanting_window(id) else {
            return;
        };
        let shelves = self.bookshelf_count(pos);
        if let Some(open) = self
            .players
            .get_mut(&id)
            .and_then(|player| player.open_menu.as_mut())
        {
            open.menu.set_bookshelves(shelves);
        }
    }

    pub(crate) fn enchantment_seed_of(&mut self, id: PlayerId) -> i32 {
        let fresh = self.rng.random::<i32>() | 1;
        let Some(player) = self.players.get_mut(&id) else {
            return fresh;
        };
        if player.vitals.experience.seed == 0 {
            player.vitals.experience.seed = fresh;
        }
        player.vitals.experience.seed
    }

    pub(crate) fn after_enchant_slots(&mut self, id: PlayerId) {
        let Some((window, _)) = self.enchanting_window(id) else {
            return;
        };
        let Some(open) = self
            .players
            .get_mut(&id)
            .and_then(|player| player.open_menu.as_mut())
        else {
            return;
        };
        if !open.menu.offers_recomputed() {
            return;
        }
        if !self.listened(EventKind::PrepareItemEnchant) {
            return;
        }
        if self
            .plugins
            .listens(EventKind::PrepareItemEnchant, Access::Decide)
            && let Some(open) = self
                .players
                .get_mut(&id)
                .and_then(|player| player.open_menu.as_mut())
        {
            open.menu.hold_offers();
        }
        self.fire(
            Lane::Player(id),
            Continuation::PrepareEnchant(PrepareEnchantStep { player: id, window }),
            move |server| {
                let open = server.players.get(&id)?.open_menu.as_ref()?;
                let item = server.open_items(open).first().cloned().flatten()?;
                if !enchanting::is_enchantable(&item) {
                    return None;
                }
                let described = server.describe_window(id, Window::Container(window))?;
                let offers = open.menu.offers();
                let offers = (0..3)
                    .filter_map(|slot| {
                        let def =
                            enchantments::all().get(usize::try_from(offers.clues[slot]).ok()?)?;
                        Some(EnchantOffer {
                            enchantment: def.name.to_owned(),
                            level: offers.levels[slot],
                            cost: offers.costs[slot],
                        })
                    })
                    .collect();
                Some(EventPayload::PrepareItemEnchant(PrepareItemEnchant {
                    enchant_block: open.backing.pos,
                    enchanter: server.snapshot_of(id)?,
                    enchantment_bonus: open.menu.bookshelves(),
                    inventory: described.top,
                    item: spec_of(&item),
                    offers,
                    view: described.view,
                    viewers: described.viewers,
                }))
            },
        );
    }

    pub(crate) fn finish_prepare_enchant(&mut self, step: PrepareEnchantStep, cancelled: bool) {
        let Some((window, _)) = self.enchanting_window(step.player) else {
            return;
        };
        if window != step.window {
            return;
        }
        if let Some(open) = self
            .players
            .get_mut(&step.player)
            .and_then(|player| player.open_menu.as_mut())
        {
            if cancelled {
                open.menu.clear_offers();
            } else {
                open.menu.release_offers();
            }
        }
        self.sync_menu_data();
    }

    pub(crate) fn enchant_button(&mut self, id: PlayerId, window: i32, button: i32) -> bool {
        let Some((open_window, _)) = self.enchanting_window(id) else {
            return false;
        };
        if open_window != window {
            return true;
        }
        let Some(plan) = self.current_enchant_plan(id, button) else {
            return true;
        };
        let step = EnchantStep {
            player: id,
            window,
            plan: plan.clone(),
        };
        self.fire(
            Lane::Player(id),
            Continuation::Enchant(Box::new(step)),
            move |server| {
                let open = server.players.get(&id)?.open_menu.as_ref()?;
                let item = server.open_items(open).first().cloned().flatten()?;
                let described = server.describe_window(id, Window::Container(window))?;
                let hint = enchantments::all().get(usize::try_from(plan.clue.0).ok()?)?;
                Some(EventPayload::EnchantItem(EnchantItem {
                    enchant_block: open.backing.pos,
                    enchanter: server.snapshot_of(id)?,
                    enchantment_hint: hint.name.to_owned(),
                    enchants_to_add: plan
                        .picks
                        .iter()
                        .map(|(name, level)| (name.clone(), i32::from(*level)))
                        .collect(),
                    exp_level_cost: plan.cost,
                    inventory: described.top,
                    item: spec_of(&item),
                    level_hint: plan.clue.1,
                    view: described.view,
                    viewers: described.viewers,
                    which_button: button,
                }))
            },
        );
        true
    }

    fn current_enchant_plan(&self, id: PlayerId, button: i32) -> Option<EnchantPlan> {
        let player = self.players.get(&id)?;
        if player.game_mode == GameMode::Spectator {
            return None;
        }
        let open = player.open_menu.as_ref()?;
        let clicker = Clicker {
            infinite_materials: player.game_mode == GameMode::Creative,
            experience_level: player.vitals.experience.level,
        };
        open.menu
            .enchant_plan(&self.open_items(open), button, clicker)
    }

    pub(crate) fn finish_enchant(
        &mut self,
        step: EnchantStep,
        cancelled: bool,
        patch: EnchantItemPatch,
    ) {
        let EnchantStep {
            player: id,
            window,
            plan,
        } = step;
        if cancelled {
            return;
        }
        let Some((open_window, pos)) = self.enchanting_window(id) else {
            return;
        };
        let button = i32::try_from(plan.button).unwrap_or(-1);
        if open_window != window || self.current_enchant_plan(id, button).as_ref() != Some(&plan) {
            return;
        }
        let level_cost = patch.exp_level_cost.unwrap_or(plan.cost);
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let infinite = player.game_mode == GameMode::Creative;
        let level = player.vitals.experience.level;
        if level < level_cost && !infinite {
            return;
        }
        let clicker = Clicker {
            infinite_materials: infinite,
            experience_level: level,
        };
        let new_seed = self.rng.random::<i32>();
        let Some(open) = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
        else {
            return;
        };
        let before: Vec<Slot> = self.open_items(open);
        let mut items = before.clone();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let tick_count = player.vitals.tick_count as i32;
        if let Some(open) = player.open_menu.as_mut() {
            let picks = plan.picks.clone();
            open.menu
                .enchant(&mut items, &plan, &picks, new_seed, clicker);
        }
        player
            .vitals
            .experience
            .give_levels(-i32::try_from(plan.lapis).unwrap_or(0), tick_count);
        player.vitals.experience.seed = new_seed;
        self.store_open_items(id, &before, items);
        let pitch = self.rng.random::<f32>() * 0.1 + 0.9;
        self.play_sound(
            None,
            pos.center(),
            TABLE_USE,
            SoundSource::Blocks,
            1.0,
            pitch,
        );
        self.sync_open_menu(id, false);
        self.sync_menu_data();
        self.sync_inventory(id);
        self.after_enchant_slots(id);
    }
}
