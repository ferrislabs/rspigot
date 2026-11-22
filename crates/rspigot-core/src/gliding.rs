use rspigot_domain::entity::reward::{DAMAGE_COMPONENT, max_damage};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::stack_components;

use crate::events::Hand;
use crate::plugins::events::EntityToggleGlide;
use crate::plugins::steps::GlideStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const CHEST_SLOT: usize = 6;
const LEVITATION: &str = "minecraft:levitation";
const WEAR_PERIOD: i32 = 10;

fn is_glider(stack: &ItemStack) -> bool {
    stack_components::is_glider(stack)
}

fn next_damage_breaks(stack: &ItemStack) -> bool {
    let damage = stack.components.get_i32(DAMAGE_COMPONENT).unwrap_or(0);
    max_damage(stack).is_some_and(|max| damage + 1 >= max)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn can_glide(&self, id: PlayerId) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        !player.on_ground
            && !self.entities.rides.is_passenger(player.entity_id)
            && !player.effects.has(LEVITATION)
            && player
                .inventory
                .get(CHEST_SLOT)
                .is_some_and(|stack| is_glider(stack) && !next_damage_breaks(stack))
    }

    pub(crate) fn start_fall_flying(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let startable = !player.fall_flying
            && self.can_glide(id)
            && !self.in_water(player.position)
            && !player.vitals.is_dead();
        if startable {
            self.ask_glide(id, true);
        } else if player.fall_flying {
            self.set_fall_flying(id, false);
        }
    }

    fn ask_glide(&mut self, id: PlayerId, gliding: bool) {
        self.fire(
            id,
            Continuation::Glide(GlideStep {
                player: id,
                gliding,
            }),
            move |server| {
                let entity = server.player_entity_view(id)?;
                Some(EventPayload::EntityToggleGlide(EntityToggleGlide {
                    entity_type: entity.kind.clone(),
                    entity,
                    gliding,
                }))
            },
        );
    }

    fn player_entity_view(&self, id: PlayerId) -> Option<crate::plugins::EntityView> {
        let entity = self.players.get(&id)?.entity_id;
        self.subject_view(entity)
    }

    pub(crate) fn finish_glide(&mut self, step: GlideStep, cancelled: bool) {
        let GlideStep { player, gliding } = step;
        let Some(current) = self.players.get(&player).map(|p| p.fall_flying) else {
            return;
        };
        if cancelled {
            self.resync_flags(player);
            return;
        }
        if current != gliding {
            self.set_fall_flying(player, gliding);
        }
    }

    fn set_fall_flying(&mut self, id: PlayerId, flying: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.fall_flying == flying {
            return;
        }
        player.fall_flying = flying;
        player.fall_fly_ticks = 0;
        self.resync_flags(id);
    }

    fn resync_flags(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let entries = Self::metadata(player);
        let event = crate::events::ClientEvent::EntityMetadata { entity, entries };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }

    pub(crate) fn tick_fall_flying(&mut self) {
        let flying: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.fall_flying)
            .map(|player| player.id)
            .collect();
        for id in flying {
            self.tick_glider(id);
        }
    }

    fn tick_glider(&mut self, id: PlayerId) {
        if !self.can_glide(id) {
            self.ask_glide(id, false);
            return;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.fall_fly_ticks += 1;
        let ticks = player.fall_fly_ticks;
        if ticks % WEAR_PERIOD == 0 && (ticks / WEAR_PERIOD) % 2 == 0 {
            self.wear_slot(id, Hand::Main, CHEST_SLOT, 1);
        }
    }
}
