use rspigot_ai::{FrogAction, MemoryModuleType, MemoryValue, MobAi};
use rspigot_domain::EntityId;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::frog::{self, FrogState};
use rspigot_domain::entity::{Entity, RemovalReason, living};
use rspigot_domain::random::RandomSource;

use crate::commands::Target;
use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

const BREED_EXPERIENCE: i32 = 7;

fn frog_state(entity: &mut Entity) -> Option<&mut FrogState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Frog(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn frog_registered(&mut self, id: EntityId, ai: &mut MobAi) {
        let pending = self
            .entities
            .store
            .get_mut(id)
            .and_then(frog_state)
            .and_then(|state| state.pending_long_jump.take());
        if let Some(long_jump) = pending {
            ai.init_frog_memories(long_jump);
        }
    }

    pub(crate) fn frog_action(&mut self, id: EntityId, action: FrogAction) {
        match action {
            FrogAction::SetTongueTarget(target) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    frog::set_tongue_target(&mut entity.base, target.0 as u32);
                }
            }
            FrogAction::EraseTongueTarget => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    frog::erase_tongue_target(&mut entity.base);
                }
            }
            FrogAction::Pull { target, velocity } => {
                if let Some(entity) = self.entities.store.get_mut(target) {
                    entity.base.velocity = velocity;
                    entity.base.needs_sync = true;
                }
            }
            FrogAction::Eat(target) => self.frog_eat(id, target),
        }
    }

    fn frog_eat(&mut self, id: EntityId, target: EntityId) {
        let alive = |server: &Self| {
            server.entities.store.get(target).is_some_and(|entity| {
                !entity.base.is_removed()
                    && entity
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
            })
        };
        if !alive(self) {
            return;
        }
        self.do_hurt_target(id, Target::Entity(target));
        if !alive(self) {
            self.remove_entity(
                target,
                RemovalReason::Killed,
                Some(EntityRemoveCause::Death),
            );
        }
    }

    pub(crate) fn frog_breed(&mut self, parent: EntityId, partner: EntityId) {
        let Some(position) = self
            .entities
            .store
            .get(parent)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let mut random = self.mob_random(parent);
        let experience = random.next_int(BREED_EXPERIENCE) + 1;
        self.store_mob_random(parent, random);
        self.finish_childless_breeding(parent, partner, position, experience);
        if let Some(brain) = self
            .entities
            .mobs
            .entries
            .get_mut(&parent)
            .and_then(|entry| entry.ai.behavior_brain_mut())
        {
            brain
                .memories_mut()
                .set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
        }
    }
}
