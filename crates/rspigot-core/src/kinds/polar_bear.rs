use rspigot_ai::{MobView, PolarBearAction};
use rspigot_domain::EntityId;
use rspigot_domain::entity::species::{Species, polar_bear};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::mobs::DAMAGE_SOURCE_MEMORY;
use crate::ports::ClientLink;
use crate::server::Server;

const PANIC_CAUSES: &str = "minecraft:panic_causes";
const PANIC_ENVIRONMENTAL_CAUSES: &str = "minecraft:panic_environmental_causes";
const WARNING_SOUND: &str = "minecraft:entity.polar_bear.warning";

impl<L: ClientLink> Server<L> {
    pub(crate) fn polar_bear_view(&self, entity: &Entity, view: &mut MobView) {
        let tag = if view.baby {
            PANIC_CAUSES
        } else {
            PANIC_ENVIRONMENTAL_CAUSES
        };
        let game_time = self.world.level.game_time;
        view.panic = entity
            .living()
            .and_then(|state| state.damage.last_source)
            .is_some_and(|(kind, at)| game_time - at <= DAMAGE_SOURCE_MEMORY && kind.in_tag(tag));
    }

    pub(crate) fn polar_bear_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn polar_bear_post_tick(&mut self, id: EntityId) {
        if let Some(bear) = self.polar_bear_state(id) {
            bear.tick();
        }
        self.update_persistent_anger(id, true);
    }

    pub(crate) fn polar_bear_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn polar_bear_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn polar_bear_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn polar_bear_after_hurt(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
    }

    pub(crate) fn polar_bear_action(&mut self, id: EntityId, action: PolarBearAction) {
        match action {
            PolarBearAction::SetStanding(standing) => {
                if let Some(entity) = self.entities.store.get_mut(id)
                    && matches!(species_of(entity), Some(Species::PolarBear(_)))
                {
                    polar_bear::set_standing(&mut entity.base, standing);
                }
            }
            PolarBearAction::Warn => {
                let warned = self
                    .polar_bear_state(id)
                    .is_some_and(|bear| bear.start_warning());
                if warned {
                    self.play_mob_voice(id, |_| Some(WARNING_SOUND));
                }
            }
        }
    }

    fn polar_bear_state(&mut self, id: EntityId) -> Option<&mut polar_bear::PolarBearState> {
        let EntityState::Living(living) = &mut self.entities.store.get_mut(id)?.state else {
            return None;
        };
        match living.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::PolarBear(bear)) => Some(bear),
            _ => None,
        }
    }
}

fn species_of(entity: &Entity) -> Option<Species> {
    entity
        .living()
        .and_then(|living| living.mob.as_ref())
        .map(|mob| mob.species)
}
