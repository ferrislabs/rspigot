use rand::Rng;
use rspigot_domain::entity::reward::{damage_value, max_damage, set_damage_value};
use rspigot_domain::item::{HOTBAR_START, OFFHAND_SLOT};
use rspigot_domain::player::PlayerId;

use crate::enchant_effects;
use crate::plugins::events::{EquipmentSlot, PlayerItemMend, PlayerItemMendPatch};
use crate::plugins::steps::OrbStep;
use crate::plugins::{Continuation, EventKind, EventPayload, spec_of};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug)]
pub(crate) struct MendStep {
    pub(crate) orb: OrbStep,
    pub(crate) slot: usize,
    pub(crate) repair: i32,
    pub(crate) modified: i32,
}

impl<L: ClientLink> Server<L> {
    fn mendable_slots(&self, id: PlayerId) -> Vec<(usize, EquipmentSlot)> {
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
        };
        let held = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        [
            (held, EquipmentSlot::Hand),
            (OFFHAND_SLOT, EquipmentSlot::OffHand),
            (8, EquipmentSlot::Feet),
            (7, EquipmentSlot::Legs),
            (6, EquipmentSlot::Chest),
            (5, EquipmentSlot::Head),
        ]
        .into_iter()
        .filter(|(slot, _)| {
            player.inventory.get(*slot).is_some_and(|stack| {
                max_damage(stack).is_some()
                    && damage_value(stack) > 0
                    && enchant_effects::repairs_with_xp(stack)
            })
        })
        .collect()
    }

    pub(crate) fn mend_with_orb(&mut self, orb: OrbStep) {
        let candidates = self.mendable_slots(orb.player);
        if candidates.is_empty() || orb.amount <= 0 {
            self.ask_orb_experience(orb);
            return;
        }
        let (slot, equipment) = candidates[self.rng.random_range(0..candidates.len())];
        let Some(stack) = self
            .players
            .get(&orb.player)
            .and_then(|player| player.inventory.get(slot))
            .cloned()
        else {
            self.ask_orb_experience(orb);
            return;
        };
        let modified = enchant_effects::modify_repair_from_xp(&stack, orb.amount, &mut self.rng);
        if modified <= 0 {
            self.ask_orb_experience(orb);
            return;
        }
        let repair = modified.min(damage_value(&stack));
        let step = MendStep {
            orb,
            slot,
            repair,
            modified,
        };
        if !self.listened(EventKind::PlayerItemMend) {
            self.apply_mend(step, false, PlayerItemMendPatch::default());
            return;
        }
        let player = step.orb.player;
        let collector = step.orb.orb;
        let amount = step.repair;
        self.fire(player, Continuation::Mend(Box::new(step)), move |server| {
            Some(EventPayload::PlayerItemMend(PlayerItemMend {
                experience_orb: server.entity_view(collector)?,
                item: spec_of(&stack),
                player: server.snapshot_of(player)?,
                repair_amount: amount,
                slot: equipment,
            }))
        });
    }

    pub(crate) fn apply_mend(
        &mut self,
        step: MendStep,
        cancelled: bool,
        patch: PlayerItemMendPatch,
    ) {
        let MendStep {
            orb,
            slot,
            repair,
            modified,
            ..
        } = step;
        if cancelled {
            self.ask_orb_experience(orb);
            return;
        }
        let repair = patch.repair_amount.unwrap_or(repair);
        let Some(player) = self.players.get_mut(&orb.player) else {
            return;
        };
        if let Some(mut stack) = player.inventory.get(slot).cloned() {
            let damage = damage_value(&stack);
            set_damage_value(&mut stack, damage - repair);
            player.inventory.set(slot, Some(stack));
        }
        let id = orb.player;
        self.sync_inventory(id);
        if repair <= 0 {
            self.ask_orb_experience(OrbStep { amount: 0, ..orb });
            return;
        }
        let remaining = orb.amount - repair * orb.amount / modified;
        if remaining > 0 {
            self.mend_with_orb(OrbStep {
                amount: remaining,
                ..orb
            });
        } else {
            self.ask_orb_experience(OrbStep { amount: 0, ..orb });
        }
    }
}
