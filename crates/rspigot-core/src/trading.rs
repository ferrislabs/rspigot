use rspigot_data::trading as data;
use rspigot_domain::container_menu::{ContainerMenu, MenuKind};
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::villager::{self, MerchantState};
use rspigot_domain::entity::trade_gen::{self, TradeEnv};
use rspigot_domain::entity::trading::{ItemCost, MerchantOffer, MerchantOffers};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::text::Component;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::containers::{Backing, Lid, OpenMenu};
use crate::events::{ClientEvent, EntityStatus};
use crate::interact::{Interaction, box_distance_squared};
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    EntityPotionEffectCause, EventResult, TradeSelect, VillagerAcquireTrade, VillagerReplenishTrade,
};
use crate::plugins::steps::{AcquireStep, ReplenishStep, SelectTradeStep};
use crate::plugins::views::Window;
use crate::plugins::{
    Continuation, EventPatch, EventPayload, Lane, MerchantRef, Trade, spec_of, stack_of,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const TRADER_TITLE: &str = "entity.minecraft.wandering_trader";
const TRADE_RANGE_BUFFER: f64 = 4.0;
const ENTITY_REACH: f64 = 3.0;
const CREATIVE_REACH_BONUS: f64 = 2.0;
const REGENERATION_TICKS: u32 = 200;
const ORB_LIFT: f64 = 0.5;
const HAPPY_EVENT: i8 = 14;
const ORB_BASE: i32 = 3;
const ORB_SPREAD: i32 = 4;
const LEVEL_UP_ORB_BONUS: i32 = 5;
const VOICE_WINDOW: i32 = 20;
const MOB_AMBIENT_INTERVAL: i32 = 80;
const VILLAGER_YES: &str = "minecraft:entity.villager.yes";
const VILLAGER_NO: &str = "minecraft:entity.villager.no";
const TRADER_YES: &str = "minecraft:entity.wandering_trader.yes";
const TRADER_NO: &str = "minecraft:entity.wandering_trader.no";
const VILLAGER_TYPES: &str = "minecraft:villager_type";
const HERO_OF_THE_VILLAGE: &str = "minecraft:hero_of_the_village";
const TRADER_COMMON: &str = "minecraft:wandering_trader/common";
const TRADER_UNCOMMON: &str = "minecraft:wandering_trader/uncommon";
const TRADER_BUYING: &str = "minecraft:wandering_trader/buying";

fn merchant_of(entity: &Entity) -> Option<&MerchantState> {
    entity.living()?.mob.as_ref()?.merchant.as_deref()
}

fn merchant_mut(entity: &mut Entity) -> Option<&mut MerchantState> {
    entity.living_mut()?.mob.as_mut()?.merchant.as_deref_mut()
}

fn is_merchant(kind: Kind) -> bool {
    matches!(kind, Kind::Villager | Kind::WanderingTrader)
}

fn trade_of(offer: &MerchantOffer) -> Trade {
    let mut ingredients = Vec::new();
    ingredients.extend(offer.cost_a.item_stack().as_ref().map(spec_of));
    ingredients.extend(offer.cost_b_stack().as_ref().map(spec_of));
    Trade {
        result: spec_of(&offer.result),
        ingredients,
        uses: offer.uses,
        max_uses: offer.max_uses,
        experience_reward: offer.reward_exp,
        villager_experience: offer.xp,
        price_multiplier: offer.price_multiplier,
        demand: offer.demand,
        special_price: offer.special_price_diff,
    }
}

fn cost_of(stack: &ItemStack) -> ItemCost {
    let mut cost = ItemCost::new(stack.item, i32::from(stack.count.get()));
    cost.components = stack.components.clone();
    cost
}

fn offer_of(trade: &Trade) -> Option<MerchantOffer> {
    let result = stack_of(&trade.result).ok().flatten()?;
    let mut costs = trade
        .ingredients
        .iter()
        .filter_map(|spec| stack_of(spec).ok().flatten());
    let first = cost_of(&costs.next()?);
    let second = costs.next().map(|stack| cost_of(&stack));
    let mut offer = MerchantOffer::new(
        first,
        second,
        result,
        trade.max_uses,
        trade.villager_experience,
        trade.price_multiplier,
    );
    offer.uses = trade.uses;
    offer.reward_exp = trade.experience_reward;
    offer.demand = trade.demand;
    offer.special_price_diff = trade.special_price;
    Some(offer)
}

fn title_of(entity: &Entity) -> &'static str {
    if entity.base.kind == Kind::WanderingTrader {
        TRADER_TITLE
    } else {
        villager::profession(&entity.base).title_key()
    }
}

impl<L: ClientLink> Server<L> {
    fn villager_type_name(&self, id: EntityId) -> Option<&'static str> {
        let entity = self.entities.store.get(id)?;
        if entity.base.kind != Kind::Villager {
            return None;
        }
        let (kind, _, _) = villager::villager_data(&entity.base);
        rspigot_data::registries::static_entries(VILLAGER_TYPES)?
            .get(kind as usize)
            .copied()
    }

    pub(crate) fn merchant_offers_of(&mut self, id: EntityId) -> Option<MerchantOffers> {
        self.ensure_offers(id);
        self.entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.clone())
    }

    pub(crate) fn ensure_offers(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if !is_merchant(entity.base.kind) {
            return;
        }
        let Some(merchant) = merchant_mut(entity) else {
            return;
        };
        if merchant.offers.is_some() {
            return;
        }
        merchant.offers = Some(MerchantOffers::new());
        self.update_trades(id);
    }

    pub(crate) fn update_trades(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Villager) => {
                let Some(entity) = self.entities.store.get(id) else {
                    return;
                };
                let (_, profession, level) = villager::villager_data(&entity.base);
                if let Some(set) = profession.trade_set(level) {
                    self.add_offers_from_trade_set(id, &set);
                    self.refresh_trading_prices(id);
                }
            }
            Some(Kind::WanderingTrader) => {
                for set in [TRADER_BUYING, TRADER_UNCOMMON, TRADER_COMMON] {
                    self.add_offers_from_trade_set(id, set);
                }
            }
            _ => {}
        }
    }

    fn add_offers_from_trade_set(&mut self, id: EntityId, set_name: &str) {
        let Some(set) = data::trade_set(set_name) else {
            return;
        };
        let variant = self.villager_type_name(id);
        let env = TradeEnv { variant };
        let generated = trade_gen::offers_from_set(set, &env, &mut ServerRandom(&mut self.rng));
        for offer in generated {
            if set.allow_duplicates {
                self.push_offer(id, offer);
            } else {
                self.ask_acquire(id, offer);
            }
        }
    }

    fn push_offer(&mut self, id: EntityId, offer: MerchantOffer) {
        if let Some(offers) = self
            .entities
            .store
            .get_mut(id)
            .and_then(merchant_mut)
            .and_then(|merchant| merchant.offers.as_mut())
        {
            offers.push(offer);
        }
    }

    fn ask_acquire(&mut self, id: EntityId, offer: MerchantOffer) {
        let Some(view) = self.entity_view(id) else {
            self.push_offer(id, offer);
            return;
        };
        let recipe = trade_of(&offer);
        let lane = self.lane_for(None, id);
        let step = AcquireStep {
            merchant: id,
            offer,
        };
        self.fire(lane, Continuation::Acquire(Box::new(step)), move |_| {
            Some(EventPayload::VillagerAcquireTrade(VillagerAcquireTrade {
                entity_type: view.kind.clone(),
                entity: view,
                recipe,
            }))
        });
    }

    pub(crate) fn finish_acquire(
        &mut self,
        step: AcquireStep,
        cancelled: bool,
        patch: Option<EventPatch>,
    ) {
        let AcquireStep { merchant, offer } = step;
        if cancelled || self.entities.store.get(merchant).is_none() {
            return;
        }
        let replaced = match patch {
            Some(EventPatch::VillagerAcquireTrade(patch)) => {
                patch.recipe.as_ref().and_then(offer_of)
            }
            _ => None,
        };
        self.push_offer(merchant, replaced.unwrap_or(offer));
    }

    pub(crate) fn ask_replenish(&mut self, id: EntityId, index: usize, uses: i32) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let Some(recipe) = self
            .entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.as_ref())
            .and_then(|offers| offers.get(index))
            .map(trade_of)
        else {
            return;
        };
        let lane = self.lane_for(None, id);
        let step = ReplenishStep {
            merchant: id,
            index,
            uses,
        };
        self.fire(lane, Continuation::Replenish(step), move |_| {
            Some(EventPayload::VillagerReplenishTrade(
                VillagerReplenishTrade {
                    entity_type: view.kind.clone(),
                    entity: view,
                    recipe,
                },
            ))
        });
    }

    pub(crate) fn finish_replenish(&mut self, step: ReplenishStep, cancelled: bool) {
        if !cancelled {
            return;
        }
        let ReplenishStep {
            merchant,
            index,
            uses,
        } = step;
        if let Some(offer) = self
            .entities
            .store
            .get_mut(merchant)
            .and_then(merchant_mut)
            .and_then(|merchant| merchant.offers.as_mut())
            .and_then(|offers| offers.get_mut(index))
        {
            offer.uses = uses;
        }
    }

    pub(crate) fn replenish_all(&mut self, id: EntityId, before: &[i32]) {
        for (index, uses) in before.iter().enumerate() {
            self.ask_replenish(id, index, *uses);
        }
    }

    pub(crate) fn offer_uses(&self, id: EntityId) -> Vec<i32> {
        self.entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.as_ref())
            .map(|offers| offers.iter().map(|offer| offer.uses).collect())
            .unwrap_or_default()
    }

    pub(crate) fn trading_player_of(&self, id: EntityId) -> Option<EntityId> {
        self.entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.trading_player)
    }

    pub(crate) fn villager_special_prices(&mut self, id: EntityId, player: EntityId) {
        let Some(uuid) = self
            .players
            .values()
            .find(|candidate| candidate.entity_id == player)
            .map(|candidate| candidate.profile.id.0)
        else {
            return;
        };
        let hero = self
            .players
            .values()
            .find(|candidate| candidate.entity_id == player)
            .and_then(|candidate| {
                EffectId::from_name(HERO_OF_THE_VILLAGE)
                    .and_then(|effect| candidate.effects.get(effect))
                    .map(|effect| i32::from(effect.amplifier))
            });
        if let Some(merchant) = self.entities.store.get_mut(id).and_then(merchant_mut) {
            let reputation = merchant.gossips.reputation(uuid);
            if let Some(offers) = merchant.offers.as_mut() {
                villager::update_special_prices(offers, reputation, hero);
            }
        }
        self.send_trading_offers(id);
    }

    pub(crate) fn refresh_trading_prices(&mut self, id: EntityId) {
        if let Some(player) = self.trading_player_of(id) {
            self.villager_special_prices(id, player);
        }
    }

    pub(crate) fn reset_special_prices_of(&mut self, id: EntityId) {
        if let Some(offers) = self
            .entities
            .store
            .get_mut(id)
            .and_then(merchant_mut)
            .and_then(|merchant| merchant.offers.as_mut())
        {
            villager::reset_special_prices(offers);
        }
    }

    fn offers_packet(&self, id: EntityId, window: i32) -> Option<ClientEvent> {
        let entity = self.entities.store.get(id)?;
        let offers = merchant_of(entity)?.offers.clone()?;
        let villager = entity.base.kind == Kind::Villager;
        let (level, xp) = if villager {
            let (_, _, level) = villager::villager_data(&entity.base);
            let xp = entity
                .living()
                .and_then(|living| living.mob.as_ref())
                .and_then(|mob| match &mob.species {
                    Species::Villager(state) => Some(state.xp),
                    _ => None,
                })
                .unwrap_or(0);
            (level, xp)
        } else {
            (1, 0)
        };
        Some(ClientEvent::MerchantOffers {
            window,
            offers: offers.iter().cloned().collect(),
            level,
            xp,
            show_progress: villager,
            can_restock: villager,
        })
    }

    pub(crate) fn send_trading_offers(&mut self, id: EntityId) {
        let Some(player_entity) = self.trading_player_of(id) else {
            return;
        };
        let Some(player_id) = self.player_of_entity(player_entity) else {
            return;
        };
        let offers = self
            .entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.clone())
            .filter(|offers| !offers.is_empty());
        let Some(offers) = offers else {
            return;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let Some(open) = player
            .open_menu
            .as_mut()
            .filter(|open| open.trader == Some(id))
        else {
            return;
        };
        let mut items = open.local.clone();
        open.menu
            .set_offers(&mut items, &mut player.inventory, offers);
        open.local = items;
        let window = open.window;
        if let Some(packet) = self.offers_packet(id, window)
            && let Some(player) = self.players.get(&player_id)
        {
            player.link.send(packet);
        }
        self.sync_open_menu(player_id, false);
    }

    pub(crate) fn merchant_stop_trading(&mut self, id: EntityId) {
        let had = self.trading_player_of(id).is_some();
        if let Some(merchant) = self.entities.store.get_mut(id).and_then(merchant_mut) {
            merchant.trading_player = None;
        }
        if had && self.kind_of(id) == Some(Kind::Villager) {
            self.reset_special_prices_of(id);
        }
    }

    pub(crate) fn trader_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: crate::events::Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if held.is_some_and(|stack| stack.item == rspigot_items::ItemKind::VillagerSpawnEgg.id()) {
            return None;
        }
        let entity = self.entities.store.get(target)?;
        let alive = entity.living().is_some_and(|living| !living.dead);
        let trading = merchant_of(entity).is_some_and(|merchant| merchant.trading_player.is_some());
        if !alive || trading || entity.is_baby() {
            return None;
        }
        let _ = hand;
        let offers = self.merchant_offers_of(target)?;
        if offers.is_empty() {
            return Some(Interaction::Consume);
        }
        self.begin_trading(player, target);
        Some(Interaction::ServerOnly)
    }

    pub(crate) fn begin_trading(&mut self, player: PlayerId, id: EntityId) {
        let Some(player_entity) = self.player_entity_id(player) else {
            return;
        };
        if self.kind_of(id) == Some(Kind::Villager) {
            self.villager_special_prices(id, player_entity);
        }
        if let Some(merchant) = self.entities.store.get_mut(id).and_then(merchant_mut) {
            merchant.trading_player = Some(player_entity);
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let title = title_of(entity);
        let position = entity.base.position.block();
        self.close_open_menu(player, true);
        let inventory = self.next_inventory_id();
        let open = OpenMenu {
            window: 0,
            inventory,
            title,
            menu: ContainerMenu::new(MenuKind::Merchant),
            backing: Backing {
                pos: position,
                second: None,
                entity: None,
                ender: false,
                block: rspigot_blocks::BlockKind::Air,
                lid: Lid::None,
            },
            state_id: 0,
            local: vec![None; MenuKind::Merchant.container_size()],
            data: Vec::new(),
            trader: Some(id),
        };
        self.ask_open(player, position, open);
    }

    pub(crate) fn trader_open_in_reach(&self, player: PlayerId, open: &OpenMenu) -> bool {
        let Some(id) = open.trader else {
            return false;
        };
        let Some(state) = self.players.get(&player) else {
            return false;
        };
        let Some(player_entity) = self.player_entity_id(player) else {
            return false;
        };
        state.open_menu.is_none()
            && state.plugin_menu.is_none()
            && !state.vitals.is_dead()
            && self.trader_valid(id, player, player_entity)
    }

    fn trader_valid(&self, id: EntityId, player: PlayerId, player_entity: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(state) = self.players.get(&player) else {
            return false;
        };
        let alive = !entity.base.is_removed()
            && entity.living().is_some_and(|living| {
                !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
            });
        let reach = ENTITY_REACH
            + if state.game_mode == rspigot_domain::player::GameMode::Creative {
                CREATIVE_REACH_BONUS
            } else {
                0.0
            }
            + TRADE_RANGE_BUFFER;
        alive
            && merchant_of(entity)
                .is_some_and(|merchant| merchant.trading_player == Some(player_entity))
            && box_distance_squared(&entity.base.bounding_box(), state.eye_position())
                < reach * reach
    }

    pub(crate) fn trader_menu_valid(&self, player: PlayerId, id: EntityId) -> bool {
        let Some(player_entity) = self.player_entity_id(player) else {
            return false;
        };
        self.trader_valid(id, player, player_entity)
    }

    pub(crate) fn show_trader_menu(&mut self, player: PlayerId, mut open: OpenMenu) {
        let Some(id) = open.trader else {
            return;
        };
        let Some(offers) = self
            .entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.clone())
        else {
            return;
        };
        let Some(state) = self.players.get_mut(&player) else {
            return;
        };
        let window = crate::containers::next_window(state);
        open.window = window;
        let mut items = open.local.clone();
        open.menu
            .set_offers(&mut items, &mut state.inventory, offers);
        open.local = items.clone();
        state.link.send(ClientEvent::OpenScreen {
            window,
            menu: MenuKind::Merchant.protocol_id(),
            title: Component::translatable(open.title, Vec::new()),
        });
        crate::containers::sync_menu_view(
            &state.link,
            window,
            &mut open.state_id,
            &mut open.menu,
            &items,
            &state.inventory,
            true,
        );
        state.open_menu = Some(open);
        if let Some(packet) = self.offers_packet(id, window)
            && let Some(state) = self.players.get(&player)
        {
            state.link.send(packet);
        }
    }

    pub(crate) fn cancel_trading_open(&mut self, open: &OpenMenu) {
        if let Some(id) = open.trader {
            self.merchant_stop_trading(id);
        }
    }

    pub(crate) fn select_trade(&mut self, player: PlayerId, index: i32) {
        let Some(open) = self
            .players
            .get(&player)
            .and_then(|state| state.open_menu.as_ref())
        else {
            return;
        };
        let (Some(trader), window) = (open.trader, open.window) else {
            return;
        };
        if !self.trader_menu_valid(player, trader) {
            return;
        }
        let Some(view) = self.entity_view(trader) else {
            self.apply_trade_selection(player, index);
            return;
        };
        let lane = Lane::Player(player);
        let step = SelectTradeStep {
            player,
            window,
            index,
        };
        self.fire(lane, Continuation::SelectTrade(step), move |server| {
            let described = server.describe_window(player, Window::Container(window))?;
            Some(EventPayload::TradeSelect(TradeSelect {
                index,
                inventory: described.top,
                merchant: MerchantRef::Entity(view),
                result: EventResult::Default,
                who_clicked: described.view.player.clone(),
                view: described.view,
                viewers: described.viewers,
            }))
        });
    }

    pub(crate) fn finish_select_trade(&mut self, step: SelectTradeStep, cancelled: bool) {
        let SelectTradeStep {
            player,
            window,
            index,
        } = step;
        let current = self
            .players
            .get(&player)
            .and_then(|state| state.open_menu.as_ref())
            .is_some_and(|open| open.window == window && open.trader.is_some());
        if !current {
            return;
        }
        if cancelled {
            self.sync_open_menu(player, true);
            self.sync_inventory(player);
            return;
        }
        self.apply_trade_selection(player, index);
    }

    fn apply_trade_selection(&mut self, player: PlayerId, index: i32) {
        let Some(state) = self.players.get_mut(&player) else {
            return;
        };
        let Some(open) = state.open_menu.as_mut() else {
            return;
        };
        let backing = open.backing;
        let mut items = open.local.clone();
        let effects = open
            .menu
            .select_trade(&mut items, &mut state.inventory, index);
        open.local = items;
        self.container_effects(player, backing, effects);
        self.sync_open_menu(player, false);
        self.sync_inventory(player);
    }

    pub(crate) fn trade_taken(&mut self, player: PlayerId, offer: usize) {
        let Some(trader) = self
            .players
            .get(&player)
            .and_then(|state| state.open_menu.as_ref())
            .and_then(|open| open.trader)
        else {
            return;
        };
        self.notify_trade(trader, offer);
        self.send_trading_offers(trader);
    }

    fn notify_trade(&mut self, id: EntityId, index: usize) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(offer) = merchant_mut(entity)
            .and_then(|merchant| merchant.offers.as_mut())
            .and_then(|offers| offers.get_mut(index))
        else {
            return;
        };
        offer.increase_uses();
        let offer = offer.clone();
        self.reset_ambient_sound(id);
        self.reward_trade_xp(id, &offer);
    }

    fn reward_trade_xp(&mut self, id: EntityId, offer: &MerchantOffer) {
        if self.kind_of(id) == Some(Kind::WanderingTrader) {
            if offer.reward_exp {
                let amount = self.mob_orb_roll(id);
                self.spawn_trade_orb(id, amount);
            }
            return;
        }
        let mut random = self.mob_random(id);
        let mut orb = random.next_int(ORB_SPREAD) + ORB_BASE;
        self.store_mob_random(id, random);
        let mut level_up = false;
        if let Some(entity) = self.entities.store.get_mut(id) {
            let (_, _, level) = villager::villager_data(&entity.base);
            let trading = merchant_of(entity).and_then(|merchant| merchant.trading_player);
            if let Some(Species::Villager(state)) = entity
                .living_mut()
                .and_then(|living| living.mob.as_mut())
                .map(|mob| &mut mob.species)
            {
                state.xp += offer.xp;
                level_up = state.should_increase_level(level);
            }
            if let Some(merchant) = merchant_mut(entity) {
                merchant.last_traded_player = trading;
            }
        }
        if level_up {
            self.increase_merchant_career(id);
            if let Some(regeneration) = EffectId::from_name("minecraft:regeneration") {
                self.add_effect_to(
                    id,
                    EffectInstance::new(
                        regeneration,
                        EffectDuration::Ticks(REGENERATION_TICKS),
                        0,
                        true,
                    ),
                    EntityPotionEffectCause::VillagerTrade,
                );
            }
            orb += LEVEL_UP_ORB_BONUS;
        }
        if offer.reward_exp {
            self.spawn_trade_orb(id, orb);
        }
    }

    fn mob_orb_roll(&mut self, id: EntityId) -> i32 {
        let mut random = self.mob_random(id);
        let amount = random.next_int(ORB_SPREAD) + ORB_BASE;
        self.store_mob_random(id, random);
        amount
    }

    fn spawn_trade_orb(&mut self, id: EntityId, amount: i32) {
        let Some(position) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        self.spawn_orb(
            Vec3::new(position.x, position.y + ORB_LIFT, position.z),
            amount,
        );
    }

    pub(crate) fn increase_merchant_career(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let (kind, profession, level) = villager::villager_data(&entity.base);
        villager::set_villager_data(&mut entity.base, kind, profession, level + 1);
        self.update_trades(id);
    }

    pub(crate) fn merchant_happy_tick(&mut self, id: EntityId) {
        let Some(traded) = self
            .entities
            .store
            .get_mut(id)
            .and_then(merchant_mut)
            .and_then(|merchant| merchant.last_traded_player.take())
        else {
            return;
        };
        self.villager_reputation_event(
            id,
            rspigot_domain::entity::gossip::ReputationEvent::Trade,
            traded,
        );
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(HAPPY_EVENT),
            },
        );
    }

    pub(crate) fn merchant_offers_empty_after_generation(&mut self, id: EntityId) -> bool {
        self.ensure_offers(id);
        self.merchant_offers_empty(id)
    }

    pub(crate) fn merchant_offers_empty(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .and_then(merchant_of)
            .and_then(|merchant| merchant.offers.as_ref())
            .is_none_or(MerchantOffers::is_empty)
    }

    pub(crate) fn trade_updated_voice(&mut self, player: PlayerId, valid: bool) {
        let Some(trader) = self
            .players
            .get(&player)
            .and_then(|state| state.open_menu.as_ref())
            .and_then(|open| open.trader)
        else {
            return;
        };
        let Some(kind) = self.kind_of(trader) else {
            return;
        };
        let Some(entry) = self.entities.mobs.entries.get_mut(&trader) else {
            return;
        };
        if entry.ambient_sound_time <= VOICE_WINDOW - MOB_AMBIENT_INTERVAL {
            return;
        }
        entry.ambient_sound_time = -MOB_AMBIENT_INTERVAL;
        let sound = match (kind, valid) {
            (Kind::WanderingTrader, true) => TRADER_YES,
            (Kind::WanderingTrader, false) => TRADER_NO,
            (_, true) => VILLAGER_YES,
            (_, false) => VILLAGER_NO,
        };
        self.play_mob_voice(trader, move |_| Some(sound));
    }
}
