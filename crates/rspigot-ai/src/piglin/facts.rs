use rspigot_items::ItemId;

pub const PIGLIN_INVENTORY_SIZE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PiglinFacts {
    pub cannot_hunt: bool,
    pub dancing: bool,
    pub converting: bool,
    pub inventory: [Option<(ItemId, u8)>; PIGLIN_INVENTORY_SIZE],
    pub feet: Option<ItemId>,
    pub legs: Option<ItemId>,
    pub chest: Option<ItemId>,
    pub head: Option<ItemId>,
}

impl PiglinFacts {
    pub fn can_add_to_inventory(&self, item: ItemId) -> bool {
        let max = item.max_stack_size();
        self.inventory
            .iter()
            .any(|slot| slot.is_none_or(|(held, count)| held == item && count < max))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoglinFacts {
    pub converting: bool,
}
