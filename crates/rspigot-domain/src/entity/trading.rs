use std::num::NonZeroU8;

use rspigot_nbt::{Compound, Tag};

use crate::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use crate::ids::ItemId;
use crate::item::ItemStack;
use rspigot_items::ItemKind;

pub const MAX_STACK: i32 = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct ItemCost {
    pub item: ItemId,
    pub count: i32,
    pub components: Compound,
}

impl ItemCost {
    pub fn new(item: ItemId, count: i32) -> Self {
        Self {
            item,
            count,
            components: Compound::new(),
        }
    }

    pub fn max_stack_size(&self) -> i32 {
        i32::from(self.item.max_stack_size())
    }

    pub fn test(&self, stack: &ItemStack) -> bool {
        stack.item == self.item
            && self
                .components
                .iter()
                .all(|(key, value)| stack.components.get(key) == Some(value))
    }

    pub fn item_stack(&self) -> Option<ItemStack> {
        let count = u8::try_from(self.count.clamp(1, 99)).ok()?;
        let mut stack = ItemStack::new(self.item, NonZeroU8::new(count)?);
        stack.components = self.components.clone();
        Some(stack)
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert(
            "id",
            self.item.kind().map_or("minecraft:air", ItemKind::name),
        );
        out.insert("count", self.count);
        if !self.components.is_empty() {
            out.insert("components", self.components.clone());
        }
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let kind = ItemKind::from_name(nbt.get_str("id")?)?;
        if kind == ItemKind::Air {
            return None;
        }
        let count = nbt.get_i32("count").unwrap_or(1);
        if count < 1 {
            return None;
        }
        let mut cost = Self::new(kind.id(), count);
        if let Some(components) = nbt.get_compound("components") {
            cost.components = components.clone();
        }
        Some(cost)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MerchantOffer {
    pub cost_a: ItemCost,
    pub cost_b: Option<ItemCost>,
    pub result: ItemStack,
    pub uses: i32,
    pub max_uses: i32,
    pub reward_exp: bool,
    pub special_price_diff: i32,
    pub demand: i32,
    pub price_multiplier: f32,
    pub xp: i32,
}

impl MerchantOffer {
    pub fn new(
        cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> Self {
        Self {
            cost_a,
            cost_b,
            result,
            uses: 0,
            max_uses,
            reward_exp: true,
            special_price_diff: 0,
            demand: 0,
            price_multiplier,
            xp,
        }
    }

    pub fn modified_cost_count(&self, cost: &ItemCost) -> i32 {
        let count = cost.count;
        let demand_bonus = ((count * self.demand) as f32 * self.price_multiplier).floor() as i32;
        let demand_bonus = demand_bonus.max(0);
        (count + demand_bonus + self.special_price_diff).clamp(1, cost.max_stack_size().max(1))
    }

    pub fn cost_a_stack(&self) -> Option<ItemStack> {
        let count = self.modified_cost_count(&self.cost_a);
        let mut stack = self.cost_a.item_stack()?;
        stack.count = NonZeroU8::new(u8::try_from(count).ok()?)?;
        Some(stack)
    }

    pub fn cost_b_stack(&self) -> Option<ItemStack> {
        self.cost_b.as_ref().and_then(ItemCost::item_stack)
    }

    pub fn update_demand(&mut self) {
        self.demand = self.demand + self.uses - (self.max_uses - self.uses);
    }

    pub fn increase_uses(&mut self) {
        self.uses += 1;
    }

    pub fn reset_uses(&mut self) {
        self.uses = 0;
    }

    pub fn is_out_of_stock(&self) -> bool {
        self.max_uses <= self.uses
    }

    pub fn set_to_out_of_stock(&mut self) {
        self.uses = self.max_uses;
    }

    pub fn needs_restock(&self) -> bool {
        0 < self.uses
    }

    pub fn reset_special_price_diff(&mut self) {
        self.special_price_diff = 0;
    }

    pub fn add_to_special_price_diff(&mut self, add: i32) {
        self.special_price_diff += add;
    }

    pub fn satisfied_by(&self, buy_a: Option<&ItemStack>, buy_b: Option<&ItemStack>) -> bool {
        let Some(buy_a) = buy_a else {
            return false;
        };
        if !self.cost_a.test(buy_a)
            || i32::from(buy_a.count.get()) < self.modified_cost_count(&self.cost_a)
        {
            return false;
        }
        match &self.cost_b {
            None => buy_b.is_none(),
            Some(cost) => buy_b.is_some_and(|stack| {
                cost.test(stack) && i32::from(stack.count.get()) >= cost.count
            }),
        }
    }

    pub fn take(&self, buy_a: &mut Option<ItemStack>, buy_b: &mut Option<ItemStack>) -> bool {
        if !self.satisfied_by(buy_a.as_ref(), buy_b.as_ref()) {
            return false;
        }
        shrink(buy_a, self.modified_cost_count(&self.cost_a));
        if let Some(cost) = &self.cost_b {
            shrink(buy_b, cost.count);
        }
        true
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert("buy", self.cost_a.to_nbt());
        if let Some(cost) = &self.cost_b {
            out.insert("buyB", cost.to_nbt());
        }
        out.insert("sell", item_stack_to_nbt(&self.result));
        out.insert("uses", self.uses);
        out.insert("maxUses", self.max_uses);
        out.insert("rewardExp", self.reward_exp);
        out.insert("specialPrice", self.special_price_diff);
        out.insert("demand", self.demand);
        out.insert("priceMultiplier", self.price_multiplier);
        out.insert("xp", self.xp);
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let cost_a = ItemCost::from_nbt(nbt.get_compound("buy")?)?;
        let cost_b = nbt.get_compound("buyB").and_then(ItemCost::from_nbt);
        let result = item_stack_from_nbt(nbt.get_compound("sell")?)?;
        Some(Self {
            cost_a,
            cost_b,
            result,
            uses: nbt.get_i32("uses").unwrap_or(0),
            max_uses: nbt.get_i32("maxUses").unwrap_or(4),
            reward_exp: nbt.get_bool("rewardExp").unwrap_or(true),
            special_price_diff: nbt.get_i32("specialPrice").unwrap_or(0),
            demand: nbt.get_i32("demand").unwrap_or(0),
            price_multiplier: nbt.get_f64("priceMultiplier").unwrap_or(0.0) as f32,
            xp: nbt.get_i32("xp").unwrap_or(1),
        })
    }
}

fn shrink(slot: &mut Option<ItemStack>, amount: i32) {
    let Some(stack) = slot else {
        return;
    };
    let left = i32::from(stack.count.get()) - amount;
    *slot = u8::try_from(left)
        .ok()
        .and_then(NonZeroU8::new)
        .map(|count| ItemStack {
            count,
            ..stack.clone()
        });
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MerchantOffers {
    offers: Vec<MerchantOffer>,
}

impl MerchantOffers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.offers.len()
    }

    pub fn push(&mut self, offer: MerchantOffer) {
        self.offers.push(offer);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, MerchantOffer> {
        self.offers.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, MerchantOffer> {
        self.offers.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&MerchantOffer> {
        self.offers.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut MerchantOffer> {
        self.offers.get_mut(index)
    }

    pub fn recipe_for(
        &self,
        buy_a: Option<&ItemStack>,
        buy_b: Option<&ItemStack>,
        index: usize,
    ) -> Option<&MerchantOffer> {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        self.recipe_index_for(buy_a, buy_b, index)
            .map(|found| &self.offers[found])
    }

    pub fn recipe_index_for(
        &self,
        buy_a: Option<&ItemStack>,
        buy_b: Option<&ItemStack>,
        index: i32,
    ) -> Option<usize> {
        if let Ok(index) = usize::try_from(index)
            && index > 0
            && index < self.offers.len()
        {
            return self.offers[index]
                .satisfied_by(buy_a, buy_b)
                .then_some(index);
        }
        self.offers
            .iter()
            .position(|offer| offer.satisfied_by(buy_a, buy_b))
    }

    pub fn to_nbt(&self) -> Compound {
        Compound::new().with(
            "Recipes",
            Tag::List(
                self.offers
                    .iter()
                    .map(|offer| Tag::Compound(offer.to_nbt()))
                    .collect(),
            ),
        )
    }

    pub fn from_nbt(nbt: &Compound) -> Self {
        Self {
            offers: nbt
                .get_list("Recipes")
                .map(|recipes| {
                    recipes
                        .iter()
                        .filter_map(|tag| match tag {
                            Tag::Compound(offer) => MerchantOffer::from_nbt(offer),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl<'a> IntoIterator for &'a MerchantOffers {
    type Item = &'a MerchantOffer;
    type IntoIter = std::slice::Iter<'a, MerchantOffer>;

    fn into_iter(self) -> Self::IntoIter {
        self.offers.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(kind: ItemKind, count: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
    }

    fn offer() -> MerchantOffer {
        MerchantOffer::new(
            ItemCost::new(ItemKind::Emerald.id(), 4),
            None,
            stack(ItemKind::IronBoots, 1),
            12,
            1,
            0.2,
        )
    }

    #[test]
    fn demand_and_special_prices_scale_the_cost_within_the_stack_size() {
        let mut offer = offer();
        assert_eq!(offer.modified_cost_count(&offer.cost_a.clone()), 4);
        offer.demand = 10;
        assert_eq!(offer.modified_cost_count(&offer.cost_a.clone()), 12);
        offer.special_price_diff = -20;
        assert_eq!(offer.modified_cost_count(&offer.cost_a.clone()), 1);
        offer.special_price_diff = 500;
        assert_eq!(offer.modified_cost_count(&offer.cost_a.clone()), 64);
    }

    #[test]
    fn demand_follows_the_uses_against_the_maximum() {
        let mut offer = offer();
        offer.uses = 9;
        offer.update_demand();
        assert_eq!(offer.demand, 9 - 3);
        offer.reset_uses();
        offer.update_demand();
        assert_eq!(offer.demand, 6 - 12);
        assert!(!offer.needs_restock());
    }

    #[test]
    fn an_offer_is_satisfied_by_enough_matching_items() {
        let offer = offer();
        assert!(!offer.satisfied_by(None, None));
        assert!(!offer.satisfied_by(Some(&stack(ItemKind::Emerald, 3)), None));
        assert!(offer.satisfied_by(Some(&stack(ItemKind::Emerald, 4)), None));
        assert!(!offer.satisfied_by(Some(&stack(ItemKind::Diamond, 4)), None));
        assert!(!offer.satisfied_by(
            Some(&stack(ItemKind::Emerald, 4)),
            Some(&stack(ItemKind::Emerald, 1))
        ));
    }

    #[test]
    fn taking_shrinks_both_payment_stacks() {
        let mut offer = offer();
        offer.cost_b = Some(ItemCost::new(ItemKind::Book.id(), 2));
        let mut a = Some(stack(ItemKind::Emerald, 6));
        let mut b = Some(stack(ItemKind::Book, 2));
        assert!(offer.take(&mut a, &mut b));
        assert_eq!(a.map(|s| s.count.get()), Some(2));
        assert!(b.is_none());
        let mut short = Some(stack(ItemKind::Emerald, 1));
        let mut none = None;
        assert!(!offer.take(&mut short, &mut none));
    }

    #[test]
    fn stock_helpers_follow_the_use_counter() {
        let mut offer = offer();
        offer.increase_uses();
        assert!(offer.needs_restock());
        offer.set_to_out_of_stock();
        assert!(offer.is_out_of_stock());
        offer.reset_uses();
        assert!(!offer.is_out_of_stock());
    }

    #[test]
    fn offers_round_trip_through_the_vanilla_codec_layout() {
        let mut offer = offer();
        offer.cost_b = Some(ItemCost::new(ItemKind::Book.id(), 1));
        offer.uses = 2;
        offer.demand = 3;
        offer.special_price_diff = -1;
        offer.reward_exp = false;
        let mut offers = MerchantOffers::new();
        offers.push(offer.clone());
        let nbt = offers.to_nbt();
        let recipe = match nbt.get_list("Recipes").unwrap().first().unwrap() {
            Tag::Compound(recipe) => recipe,
            _ => panic!("compound expected"),
        };
        assert_eq!(
            recipe.get_compound("buy").unwrap().get_i32("count"),
            Some(4)
        );
        assert_eq!(
            recipe.get_compound("sell").unwrap().get_str("id"),
            Some("minecraft:iron_boots")
        );
        assert_eq!(recipe.get_i32("maxUses"), Some(12));
        assert_eq!(recipe.get_i32("specialPrice"), Some(-1));
        assert_eq!(MerchantOffers::from_nbt(&nbt).get(0), Some(&offer));
    }

    #[test]
    fn missing_fields_take_their_codec_defaults() {
        let nbt = Compound::new().with(
            "Recipes",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with(
                        "buy",
                        Compound::new()
                            .with("id", "minecraft:emerald")
                            .with("count", 1),
                    )
                    .with(
                        "sell",
                        Compound::new()
                            .with("id", "minecraft:bread")
                            .with("count", 3),
                    ),
            )]),
        );
        let offers = MerchantOffers::from_nbt(&nbt);
        let offer = offers.get(0).unwrap();
        assert_eq!(offer.max_uses, 4);
        assert_eq!(offer.xp, 1);
        assert!(offer.reward_exp);
        assert_eq!(offer.price_multiplier, 0.0);
    }

    #[test]
    fn the_recipe_lookup_prefers_the_selected_index() {
        let mut offers = MerchantOffers::new();
        offers.push(offer());
        offers.push(MerchantOffer::new(
            ItemCost::new(ItemKind::Emerald.id(), 1),
            None,
            stack(ItemKind::Bread, 1),
            12,
            1,
            0.05,
        ));
        let one = stack(ItemKind::Emerald, 1);
        assert!(offers.recipe_for(Some(&one), None, 1).is_some());
        assert!(offers.recipe_for(Some(&one), None, 0).is_some());
        assert!(
            offers.recipe_for(Some(&one), None, 0).unwrap().result.item == ItemKind::Bread.id()
        );
    }
}
