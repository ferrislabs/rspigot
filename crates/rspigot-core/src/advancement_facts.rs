use rspigot_data::variants::VariantFamily;
use rspigot_domain::entity::reward::damage_value;
use rspigot_domain::entity::species;
use rspigot_domain::entity::{Entity, EntityBase};
use rspigot_domain::item::ItemStack;
use rspigot_domain::jukebox;
use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemId};

use rspigot_domain::BlockPos;
use rspigot_domain::player::PlayerId;

use crate::advancement_conditions::StackFacts;
use crate::advancements::AdvancementEvent;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) fn variant_component(base: &EntityBase) -> Option<(&'static str, &'static str)> {
    let family = match base.kind {
        EntityKind::Cat => VariantFamily::Cat,
        EntityKind::Wolf => VariantFamily::Wolf,
        EntityKind::Frog => {
            return Some(("minecraft:frog/variant", species::frog::variant_name(base)));
        }
        _ => return None,
    };
    let key = match family {
        VariantFamily::Cat => "minecraft:cat/variant",
        _ => "minecraft:wolf/variant",
    };
    Some((
        key,
        species::variant_name(family, species::variant(base, family)),
    ))
}

pub(crate) fn body_stack(entity: &Entity) -> Option<StackFacts> {
    let stack = entity.living()?.equipment(EquipmentSlot::Body)?;
    Some(StackFacts {
        item: stack.item.0,
        damage: Some(damage_value(stack)),
        playable: false,
    })
}

pub(crate) fn is_playable(item: u16) -> bool {
    let stack = ItemStack::new(ItemId(item), std::num::NonZeroU8::MIN);
    jukebox::is_playable(&stack)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn item_used_on_block(
        &mut self,
        id: PlayerId,
        stack: Option<&ItemStack>,
        pos: BlockPos,
    ) {
        let mut event = AdvancementEvent::named("minecraft:item_used_on_block");
        event.item = stack.map(|stack| stack.item.0);
        event.at = Some(pos);
        self.trigger_event(id, event);
    }
}
