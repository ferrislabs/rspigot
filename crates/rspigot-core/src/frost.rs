use rspigot_blocks::BlockKind;
use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::{self, EQUIPMENT_SLOTS};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

use crate::health::{CHEST_SLOT, DamageSource, FEET_SLOT, HEAD_SLOT, LEGS_SLOT};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const TICKS_REQUIRED_TO_FREEZE: i32 = 140;
const THAW_PER_TICK: i32 = 2;
const HURT_PERIOD: u32 = 40;
const FREEZE_DAMAGE: f32 = 1.0;
const EXTRA_DAMAGE_FACTOR: f32 = 5.0;

const fn freeze_immune_kind(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Stray | Kind::PolarBear | Kind::SnowGolem | Kind::Wither
    )
}

const fn freeze_hurts_extra(kind: Kind) -> bool {
    matches!(kind, Kind::Strider | Kind::Blaze | Kind::MagmaCube)
}

fn freeze_immune_wearable(item: ItemKind) -> bool {
    matches!(
        item,
        ItemKind::LeatherBoots
            | ItemKind::LeatherLeggings
            | ItemKind::LeatherChestplate
            | ItemKind::LeatherHelmet
            | ItemKind::LeatherHorseArmor
    )
}

fn next_frozen(ticks: i32, freezing: bool) -> i32 {
    if freezing {
        (ticks + 1).min(TICKS_REQUIRED_TO_FREEZE)
    } else {
        (ticks - THAW_PER_TICK).max(0)
    }
}

fn mob_wears_immune(entity: &Entity) -> bool {
    let Some(state) = entity.living() else {
        return false;
    };
    EQUIPMENT_SLOTS
        .iter()
        .filter(|slot| {
            matches!(
                slot,
                rspigot_items::EquipmentSlot::Feet
                    | rspigot_items::EquipmentSlot::Legs
                    | rspigot_items::EquipmentSlot::Chest
                    | rspigot_items::EquipmentSlot::Head
            )
        })
        .filter_map(|slot| state.equipment[living::slot_index(*slot)].as_ref())
        .any(|stack| stack.item.kind().is_some_and(freeze_immune_wearable))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_freezing(&mut self) {
        for id in self.entities.store.ids() {
            self.tick_mob_freezing(id);
        }
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.tick_player_freezing(id);
        }
    }

    fn tick_mob_freezing(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !matches!(entity.state, EntityState::Living(_))
            || !entity.is_simulated()
            || !self.entity_ticking(entity.base.chunk())
            || !self.living_entity_alive(id)
        {
            return;
        }
        let kind = entity.base.kind;
        let can_freeze = !freeze_immune_kind(kind) && !mob_wears_immune(entity);
        let freezing = entity.base.in_powder_snow && can_freeze;
        let tick_count = entity.base.tick_count;
        let ticks = next_frozen(entity.base.ticks_frozen(), freezing);
        if let Some(entity) = self.entities.store.get_mut(id)
            && entity.base.ticks_frozen() != ticks
        {
            entity.base.set_ticks_frozen(ticks);
        }
        if (tick_count as u32).is_multiple_of(HURT_PERIOD)
            && ticks >= TICKS_REQUIRED_TO_FREEZE
            && can_freeze
        {
            let amount = if freeze_hurts_extra(kind) {
                FREEZE_DAMAGE * EXTRA_DAMAGE_FACTOR
            } else {
                FREEZE_DAMAGE
            };
            self.hurt_entity(id, &DamageSource::new(DamageType::Freeze), amount);
        }
    }

    fn tick_player_freezing(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() || player.dim != self.world.dimension.key {
            return;
        }
        let spectator = player.game_mode == GameMode::Spectator;
        let wears_immune = [HEAD_SLOT, CHEST_SLOT, LEGS_SLOT, FEET_SLOT]
            .into_iter()
            .filter_map(|slot| player.inventory.get(slot))
            .any(|stack| stack.item.kind().is_some_and(freeze_immune_wearable));
        let can_freeze = !spectator && !wears_immune;
        let in_snow =
            self.block_or_air(player.position.block()).kind() == Some(BlockKind::PowderSnow);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let ticks = next_frozen(player.vitals.ticks_frozen, in_snow && can_freeze);
        player.vitals.ticks_frozen = ticks;
        let tick_count = player.vitals.tick_count;
        if tick_count.is_multiple_of(HURT_PERIOD) && ticks >= TICKS_REQUIRED_TO_FREEZE && can_freeze
        {
            self.hurt(id, &DamageSource::new(DamageType::Freeze), FREEZE_DAMAGE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frost_builds_one_tick_at_a_time_up_to_the_cap() {
        assert_eq!(next_frozen(0, true), 1);
        assert_eq!(
            next_frozen(TICKS_REQUIRED_TO_FREEZE, true),
            TICKS_REQUIRED_TO_FREEZE
        );
    }

    #[test]
    fn frost_thaws_two_ticks_at_a_time_down_to_zero() {
        assert_eq!(next_frozen(10, false), 8);
        assert_eq!(next_frozen(1, false), 0);
    }
}
