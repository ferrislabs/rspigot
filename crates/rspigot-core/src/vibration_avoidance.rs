use rspigot_domain::entity::Entity;
use rspigot_domain::entity::item;
use rspigot_domain::{EntityId, ItemId};
use rspigot_entities::EntityKind;

use crate::advancements::AdvancementEvent;
use crate::ports::ClientLink;
use crate::server::Server;

const DAMPENING_ITEMS: &str = "minecraft:dampens_vibrations";

fn dampening_item(item: ItemId) -> bool {
    rspigot_data::tags::tag("minecraft:item", DAMPENING_ITEMS)
        .is_some_and(|items| items.binary_search(&u32::from(item.0)).is_ok())
}

pub(crate) fn entity_dampens_vibrations(entity: &Entity) -> bool {
    entity.base.kind == EntityKind::Warden
        || (entity.base.kind == EntityKind::Item
            && item::stack(&entity.base).is_some_and(|stack| dampening_item(stack.item)))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn avoid_vibration(&mut self, source: Option<EntityId>) {
        let Some(player) = source.and_then(|id| {
            self.players
                .values()
                .find(|player| player.entity_id == id)
                .map(|player| player.id)
        }) else {
            return;
        };
        self.trigger_event(player, AdvancementEvent::named("minecraft:avoid_vibration"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_items::ItemKind;

    #[test]
    fn wool_dampens_vibrations_and_stone_does_not() {
        assert!(dampening_item(ItemKind::WhiteWool.id()));
        assert!(!dampening_item(ItemKind::Stone.id()));
    }
}
