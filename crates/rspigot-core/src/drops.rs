use rspigot_domain::entity::item::THROWN_PICKUP_DELAY;
use rspigot_domain::entity::{Entity, EntityState, ItemState};
use rspigot_domain::item::{HOTBAR_START, ItemStack, Slot, split};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use uuid::Uuid;

use crate::plugins::Continuation;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::{DropSource, DropStep, Launch, SelectedDrop};
use crate::ports::ClientLink;
use crate::server::Server;

const DROPS_PER_TICK: u32 = 20;
const CREATIVE_DROP_STEP: u32 = 20;
const CREATIVE_DROP_THRESHOLD: u32 = 1480;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DropLimits {
    creative: u32,
    last_tick: u64,
    this_tick: u32,
}

impl DropLimits {
    pub fn tick(&mut self) {
        self.creative = self.creative.saturating_sub(1);
    }

    fn allow_creative(&mut self) -> bool {
        if self.creative >= CREATIVE_DROP_THRESHOLD {
            return false;
        }
        self.creative += CREATIVE_DROP_STEP;
        true
    }

    fn allow_single(&mut self, tick: u64) -> bool {
        if self.last_tick != tick {
            self.this_tick = 0;
            self.last_tick = tick;
            return true;
        }
        self.this_tick += 1;
        self.this_tick < DROPS_PER_TICK
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_drop_action(&mut self, id: PlayerId, all: bool) {
        let tick = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.vitals.is_dead()
            || !player.vitals.client_loaded()
            || player.game_mode == GameMode::Spectator
        {
            return;
        }
        if !all && !player.drops.allow_single(tick) {
            tracing::warn!("{} dropped their items too quickly!", player.profile.name);
            self.disconnect_player(
                id,
                Component::text("You dropped your items too quickly (Hacking?)"),
            );
            return;
        }
        self.drop_selected(id, all);
    }

    fn drop_selected(&mut self, id: PlayerId, all: bool) {
        self.fire_deferred(
            id,
            Continuation::DropSelected(SelectedDrop { player: id, all }),
        );
    }

    pub(crate) fn drop_selected_at_head(&mut self, step: SelectedDrop) -> Option<DropStep> {
        let player = self.players.get(&step.player)?;
        let slot = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        let mut held: Slot = player.inventory.get(slot).cloned();
        let amount = if step.all { u32::MAX } else { 1 };
        let stack = split(&mut held, amount)?;
        self.drop_step(
            step.player,
            stack,
            DropSource::Selected { slot },
            true,
            true,
        )
    }

    pub(crate) fn drop_creative(&mut self, id: PlayerId, stack: Slot) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !player.drops.allow_creative() {
            tracing::warn!(
                "Player {} was dropping items too fast in creative mode, ignoring.",
                player.profile.name
            );
            return;
        }
        if let Some(stack) = stack {
            self.ask_drop(id, stack, DropSource::Creative, true, true);
        }
    }

    pub(crate) fn drop_from_player(
        &mut self,
        id: PlayerId,
        stack: ItemStack,
        thrower: bool,
        predicted: bool,
    ) {
        self.ask_drop(id, stack, DropSource::Released, thrower, predicted);
    }

    pub(crate) fn throw_as(
        &mut self,
        launch: Launch,
        stack: ItemStack,
        thrower: Option<Uuid>,
        reserved: Option<(rspigot_domain::EntityId, Uuid)>,
    ) {
        let Launch {
            position,
            velocity,
            yaw,
        } = launch;
        let mut base = match reserved {
            Some((id, uuid)) => rspigot_domain::entity::EntityBase::new(
                id,
                uuid,
                rspigot_entities::EntityKind::Item,
            ),
            None => self.new_entity_base(rspigot_entities::EntityKind::Item),
        };
        base.position = position;
        base.old_position = position;
        base.set_yaw(yaw);
        base.velocity = velocity;
        let mut state = ItemState::new(&mut base, stack, THROWN_PICKUP_DELAY);
        state.thrower = thrower;
        self.add_entity(
            Entity::new(base, EntityState::Item(state)),
            CreatureSpawnReason::Default,
        );
    }
}
