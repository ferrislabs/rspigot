use rspigot_nbt::{Compound, Tag};

use super::villager::{DATA_UNHAPPY_COUNTER, MerchantState};
use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::trading::MerchantOffers;

pub const DEFAULT_DESPAWN_DELAY: i32 = 48_000;
pub const KEYS: &[&str] = &["DespawnDelay", "wander_target", "Offers", "Inventory"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WanderingTraderState {
    pub despawn_delay: i32,
    pub wander_target: Option<BlockPos>,
}

impl Default for WanderingTraderState {
    fn default() -> Self {
        Self {
            despawn_delay: DEFAULT_DESPAWN_DELAY,
            wander_target: None,
        }
    }
}

impl WanderingTraderState {
    pub fn tick_despawn(&mut self, trading: bool) -> bool {
        if self.despawn_delay > 0 && !trading {
            self.despawn_delay -= 1;
            return self.despawn_delay == 0;
        }
        false
    }
}

pub fn define(base: &mut EntityBase) -> WanderingTraderState {
    base.data.define(DATA_UNHAPPY_COUNTER, SyncedValue::Int(0));
    WanderingTraderState::default()
}

fn block_pos_tag(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x, pos.y, pos.z])
}

fn read_block_pos(tag: &Tag) -> Option<BlockPos> {
    match tag {
        Tag::IntArray(values) if values.len() == 3 => {
            Some(BlockPos::new(values[0], values[1], values[2]))
        }
        _ => None,
    }
}

pub fn read(nbt: &Compound, state: &mut WanderingTraderState, merchant: &mut MerchantState) {
    state.despawn_delay = nbt.get_i32("DespawnDelay").unwrap_or(0);
    state.wander_target = nbt.get("wander_target").and_then(read_block_pos);
    merchant.offers = nbt.get_compound("Offers").map(MerchantOffers::from_nbt);
    merchant.load_inventory(nbt.get("Inventory"));
}

pub fn write(state: &WanderingTraderState, merchant: &MerchantState, out: &mut Compound) {
    out.insert("DespawnDelay", state.despawn_delay);
    if let Some(target) = state.wander_target {
        out.insert("wander_target", block_pos_tag(target));
    }
    if let Some(offers) = &merchant.offers {
        out.insert("Offers", offers.to_nbt());
    }
    out.insert("Inventory", merchant.inventory_nbt());
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    #[test]
    fn a_trader_despawns_after_its_delay_unless_it_is_trading() {
        let mut state = WanderingTraderState {
            despawn_delay: 2,
            wander_target: None,
        };
        assert!(!state.tick_despawn(true));
        assert_eq!(state.despawn_delay, 2);
        assert!(!state.tick_despawn(false));
        assert!(state.tick_despawn(false));
        assert!(!state.tick_despawn(false));
        assert_eq!(state.despawn_delay, 0);
    }

    #[test]
    fn the_new_trader_waits_forty_eight_thousand_ticks() {
        let mut base =
            EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::WanderingTrader);
        let state = define(&mut base);
        assert_eq!(state.despawn_delay, 48_000);
        assert_eq!(base.data.get(18), Some(&SyncedValue::Int(0)));
    }

    #[test]
    fn the_despawn_delay_and_the_wander_target_survive_a_round_trip() {
        let nbt = Compound::new()
            .with("DespawnDelay", 1234)
            .with("wander_target", Tag::IntArray(vec![10, 64, -3]));
        let mut state = WanderingTraderState::default();
        let mut merchant = MerchantState::new();
        read(&nbt, &mut state, &mut merchant);
        assert_eq!(state.despawn_delay, 1234);
        assert_eq!(state.wander_target, Some(BlockPos::new(10, 64, -3)));
        let mut out = Compound::new();
        write(&state, &merchant, &mut out);
        assert_eq!(out.get_i32("DespawnDelay"), Some(1234));
        assert_eq!(
            out.get("wander_target"),
            Some(&Tag::IntArray(vec![10, 64, -3]))
        );
        assert!(!out.contains_key("Offers"));
    }
}
