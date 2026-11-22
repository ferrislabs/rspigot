use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::villager::MerchantState;
use rspigot_domain::entity::trading::{ItemCost, MerchantOffer, MerchantOffers};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_entities::EntityKind as Kind;

use crate::plugins::items::{spec_of, stack_of};
use crate::plugins::{ActionError, ActionErrorKind, Trade};
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_TRADES: usize = 64;
const MAX_INGREDIENTS: usize = 2;

fn merchant_mut(entity: &mut Entity) -> Option<&mut MerchantState> {
    entity.living_mut()?.mob.as_mut()?.merchant.as_deref_mut()
}

fn is_merchant(kind: Kind) -> bool {
    matches!(kind, Kind::Villager | Kind::WanderingTrader)
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn cost_of(stack: &ItemStack) -> ItemCost {
    let mut cost = ItemCost::new(stack.item, i32::from(stack.count.get()));
    cost.components = stack.components.clone();
    cost
}

fn offer_of(trade: &Trade) -> Result<MerchantOffer, ActionError> {
    if trade.ingredients.is_empty() || trade.ingredients.len() > MAX_INGREDIENTS {
        return Err(invalid("a trade takes one or two ingredients"));
    }
    if trade.max_uses < 1 || trade.uses < 0 || trade.villager_experience < 0 {
        return Err(invalid(
            "a trade needs max uses of 1 or more and no negative counts",
        ));
    }
    if !trade.price_multiplier.is_finite() || trade.price_multiplier < 0.0 {
        return Err(invalid("a trade needs a price multiplier of 0 or more"));
    }
    let result = stack_of(&trade.result)
        .map_err(invalid)?
        .ok_or_else(|| invalid("a trade needs a result"))?;
    let mut costs = Vec::new();
    for ingredient in &trade.ingredients {
        costs.push(cost_of(
            &stack_of(ingredient)
                .map_err(invalid)?
                .ok_or_else(|| invalid("an ingredient cannot be air"))?,
        ));
    }
    let mut costs = costs.into_iter();
    let first = costs
        .next()
        .ok_or_else(|| invalid("a trade needs an ingredient"))?;
    let mut offer = MerchantOffer::new(
        first,
        costs.next(),
        result,
        trade.max_uses,
        trade.villager_experience,
        trade.price_multiplier,
    );
    offer.uses = trade.uses;
    offer.reward_exp = trade.experience_reward;
    offer.demand = trade.demand;
    offer.special_price_diff = trade.special_price;
    Ok(offer)
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

impl<L: ClientLink> Server<L> {
    fn merchant_kind(&self, id: EntityId) -> Result<(), ActionError> {
        let entity =
            self.entities.store.get(id).ok_or_else(|| {
                ActionError::new(ActionErrorKind::TargetGone, "the entity is gone")
            })?;
        if is_merchant(entity.base.kind) {
            Ok(())
        } else {
            Err(invalid(format!(
                "{} is not a merchant",
                entity.base.kind.name()
            )))
        }
    }

    pub(crate) fn set_merchant_trades(
        &mut self,
        id: EntityId,
        trades: &[Trade],
    ) -> Result<(), ActionError> {
        self.merchant_kind(id)?;
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
        let merchant = self
            .entities
            .store
            .get_mut(id)
            .and_then(merchant_mut)
            .ok_or_else(|| invalid("the entity has no trades to set"))?;
        merchant.offers = Some(offers);
        if self.trading_player_of(id).is_some() {
            self.send_trading_offers(id);
        }
        Ok(())
    }

    pub(crate) fn merchant_trades(&mut self, id: EntityId) -> Result<Vec<Trade>, ActionError> {
        self.merchant_kind(id)?;
        let offers = self
            .merchant_offers_of(id)
            .ok_or_else(|| invalid("the entity has no trades"))?;
        Ok(offers.iter().map(trade_of).collect())
    }

    pub(crate) fn open_merchant(
        &mut self,
        player: PlayerId,
        id: EntityId,
    ) -> Result<(), ActionError> {
        self.merchant_kind(id)?;
        if !self.players.contains_key(&player) {
            return Err(ActionError::player_gone());
        }
        let offers = self
            .merchant_offers_of(id)
            .ok_or_else(|| invalid("the entity has no trades"))?;
        if offers.is_empty() {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                "the merchant has no trades",
            ));
        }
        if self.trading_player_of(id).is_some() {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                "the merchant is already trading",
            ));
        }
        self.begin_trading(player, id);
        Ok(())
    }
}
