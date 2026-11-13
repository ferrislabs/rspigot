use rspigot_domain::entity::ageable;
use rspigot_domain::entity::species::{Species, tadpole};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::interact::{Interaction, item_tag};
use crate::mob_life::type_in_tag;
use crate::plugins::steps::Follow;
use crate::ports::ClientLink;
use crate::server::Server;

const CANNOT_BE_AGE_LOCKED: &str = "minecraft:cannot_be_age_locked";
const GOLDEN_DANDELION_USE: &str = "minecraft:item.golden_dandelion.use";
const GOLDEN_DANDELION_UNUSE: &str = "minecraft:item.golden_dandelion.unuse";

fn tadpole_state(
    entity: &mut Entity,
) -> Option<(
    &mut rspigot_domain::entity::EntityBase,
    &mut tadpole::TadpoleState,
)> {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return None;
    };
    match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(Species::Tadpole(state)) => Some((base, state)),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tadpole_ai_step(&mut self, id: EntityId) {
        let grown = {
            let Some(entity) = self.entities.store.get_mut(id) else {
                return;
            };
            let Some((base, state)) = tadpole_state(entity) else {
                return;
            };
            let grown = tadpole::ai_step_age(base, state);
            if state.age_lock_particle_timer > 0 {
                state.age_lock_particle_timer -= 1;
            }
            grown
        };
        if grown {
            self.tadpole_grows_up(id);
        }
    }

    fn tadpole_grows_up(&mut self, id: EntityId) {
        self.convert_with(id, Kind::Frog, None, Follow::Metamorphosis);
    }

    pub(crate) fn tadpole_metamorphosed(&mut self, old: EntityId, placed: Option<EntityId>) {
        let Some(new) = placed else {
            self.tadpole_conversion_failed(old);
            return;
        };
        if let Some(mob) = self
            .entities
            .store
            .get_mut(new)
            .and_then(Entity::living_mut)
            .and_then(|living| living.mob.as_mut())
        {
            mob.persistence_required = true;
        }
        let Some(at) = self
            .entities
            .store
            .get(old)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        self.play_sound(
            None,
            at,
            tadpole::GROW_UP_SOUND,
            SoundSource::Neutral,
            tadpole::GROW_UP_VOLUME,
            1.0,
        );
    }

    pub(crate) fn tadpole_conversion_failed(&mut self, id: EntityId) {
        if let Some((_, state)) = self.entities.store.get_mut(id).and_then(tadpole_state) {
            state.age = 0;
        }
    }

    pub(crate) fn tadpole_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let stack = held?;
        let locked = self
            .entities
            .store
            .get(target)
            .is_some_and(|entity| tadpole::is_age_locked(&entity.base));
        if item_tag(stack.item, tadpole::FOOD_TAG) && !locked {
            self.consume_held(player, hand, 1);
            let seconds = {
                let (_, state) = self
                    .entities
                    .store
                    .get_mut(target)
                    .and_then(tadpole_state)?;
                ageable::speed_up_seconds_when_feeding(tadpole::ticks_left_until_adult(state))
            };
            let grown = self
                .entities
                .store
                .get_mut(target)
                .and_then(tadpole_state)
                .is_some_and(|(_, state)| tadpole::age_up_seconds(state, seconds));
            if grown {
                self.tadpole_grows_up(target);
            }
            return Some(Interaction::Predicted);
        }
        if stack.item == ItemKind::GoldenDandelion.id() {
            let timer = self
                .entities
                .store
                .get_mut(target)
                .and_then(tadpole_state)
                .map(|(_, state)| state.age_lock_particle_timer)?;
            if timer == 0 && !type_in_tag(Kind::Tadpole, CANNOT_BE_AGE_LOCKED) {
                let (position, now_locked) = {
                    let entity = self.entities.store.get_mut(target)?;
                    let (base, state) = tadpole_state(entity)?;
                    tadpole::toggle_age_lock(base, state);
                    (base.position, tadpole::is_age_locked(base))
                };
                self.consume_held(player, hand, 1);
                let sound = if now_locked {
                    GOLDEN_DANDELION_USE
                } else {
                    GOLDEN_DANDELION_UNUSE
                };
                self.play_sound(
                    None,
                    Vec3::new(position.x, position.y + 0.5, position.z),
                    sound,
                    SoundSource::Players,
                    1.0,
                    1.0,
                );
                return Some(Interaction::Predicted);
            }
        }
        self.fish_interact(player, target, hand, held)
    }
}
