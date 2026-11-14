use rspigot_ai::{BrainMessage, MobAi, MobBody};
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::mobs::sky_darken;
use crate::ports::ClientLink;
use crate::server::Server;

const BRAIN: &str = "Brain";

impl<L: ClientLink> Server<L> {
    pub(crate) fn brain_message(&mut self, to: EntityId, message: BrainMessage) {
        let Some(view) = self.mob_view(to) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&to) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            entry.ai.deliver(&view, &world, &mut body, message);
        }
        let actions = entry.ai.take_actions();
        self.entities.mobs.entries.insert(to, entry);
        for action in actions {
            self.apply_mob_action(to, action);
        }
    }

    pub(crate) fn stop_mob_brain(&mut self, id: EntityId) {
        let has_brain = self
            .entities
            .mobs
            .entries
            .get(&id)
            .is_some_and(|entry| entry.ai.behavior_brain().is_some());
        if !has_brain {
            return;
        }
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            entry.ai.stop_behavior_brain(&view, &world, &mut body);
        }
        let actions = entry.ai.take_actions();
        self.entities.mobs.entries.insert(id, entry);
        for action in actions {
            self.apply_mob_action(id, action);
        }
    }

    pub(crate) fn brain_ambient_voice(&mut self, id: EntityId) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let idle = entry
            .ai
            .behavior_brain()
            .is_some_and(|brain| brain.state().is_active(rspigot_ai::Activity::Idle));
        let sound = {
            let world = self.mob_world(sky_darken(&self.world.level));
            let mut body = MobBody::of(&view);
            entry.ai.ambient_sound(&view, &world, &mut body)
        };
        self.entities.mobs.entries.insert(id, entry);
        let silent_unless_idle = matches!(
            view.kind,
            rspigot_entities::EntityKind::Piglin | rspigot_entities::EntityKind::PiglinBrute
        );
        if silent_unless_idle && !idle {
            return;
        }
        self.play_mob_voice(id, move |_| sound);
    }

    pub(crate) fn mob_brain_nbt(&self, id: EntityId) -> Option<Compound> {
        self.entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.save_behavior_brain())
    }

    pub(crate) fn entity_nbt(&self, entity: &Entity, world: Option<Uuid>) -> Option<Compound> {
        let mut saved = rspigot_domain::entity::save::save_entity(entity, world)?;
        if let Some(brain) = self.mob_brain_nbt(entity.base.id) {
            saved.insert(BRAIN, brain);
        }
        Some(saved)
    }

    pub(crate) fn load_mob_brain(entity: &Entity, ai: &mut MobAi) {
        if let Some(brain) = entity.extra.get_compound(BRAIN) {
            ai.load_behavior_brain(brain);
        }
    }
}
