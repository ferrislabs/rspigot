use rspigot_ai::{ArmadilloAction, ArmadilloFacts, BrainMessage, MobView};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::armadillo::{self, ArmadilloState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_items::ItemKind;

use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::player::PlayerId;

use crate::events::Hand;

const SHED_TABLE: &str = "minecraft:gameplay/armadillo_shed";
const BRUSH_TABLE: &str = "minecraft:brush/armadillo";
const AMBIENT_SOUND: &str = "minecraft:entity.armadillo.ambient";
const HURT_SOUND: &str = "minecraft:entity.armadillo.hurt";
const HURT_REDUCED_SOUND: &str = "minecraft:entity.armadillo.hurt_reduced";
const SCUTE_DROP_SOUND: &str = "minecraft:entity.armadillo.scute_drop";
const BRUSH_SOUND: &str = "minecraft:entity.armadillo.brush";
const BRUSH_DURABILITY: i32 = 16;
const PITCH_SPREAD: f32 = 0.2;

pub(crate) fn ambient_sound(entity: &Entity) -> Option<&'static str> {
    (!armadillo::is_scared(&entity.base)).then_some(AMBIENT_SOUND)
}

fn armadillo_state(entity: &mut Entity) -> Option<&mut ArmadilloState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Armadillo(state)) => Some(state),
        _ => None,
    }
}

pub(crate) fn is_scared(entity: &Entity) -> bool {
    armadillo::is_scared(&entity.base)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn armadillo_view(&self, entity: &Entity, view: &mut MobView) {
        let in_state_ticks = match entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .map(|m| m.species)
        {
            Some(Species::Armadillo(state)) => state.in_state_ticks,
            _ => 0,
        };
        view.species.armadillo = ArmadilloFacts {
            state: armadillo::state(&entity.base),
            in_state_ticks,
        };
    }

    pub(crate) fn armadillo_post_tick(&mut self, id: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(armadillo_state) {
            armadillo::tick(state);
        }
    }

    pub(crate) fn armadillo_hurt_sound(&mut self, id: EntityId) -> bool {
        let reduced = self.entities.store.get(id).is_some_and(is_scared);
        let sound = if reduced {
            HURT_REDUCED_SOUND
        } else {
            HURT_SOUND
        };
        self.play_mob_voice(id, move |_| Some(sound));
        true
    }

    pub(crate) fn armadillo_hurt_amount(&self, id: EntityId, amount: f32) -> f32 {
        let scared = self.entities.store.get(id).is_some_and(is_scared);
        armadillo::hurt_amount(scared, amount)
    }

    pub(crate) fn armadillo_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if dead
            || entity
                .living()
                .is_some_and(|state| living::is_dead_or_dying(&entity.base, state))
            || living::is_no_ai(&entity.base)
        {
            return;
        }
        let by_living = source
            .causing
            .as_ref()
            .is_some_and(|cause| cause.kind.living());
        let environmental = source.kind.in_tag("minecraft:panic_environmental_causes");
        self.brain_message(
            id,
            BrainMessage::ArmadilloHurt {
                by_living,
                environmental,
            },
        );
    }

    pub(crate) fn armadillo_action(&mut self, id: EntityId, action: ArmadilloAction) {
        match action {
            ArmadilloAction::ResetLove => {
                if let Some(ageable) = self
                    .entities
                    .store
                    .get_mut(id)
                    .and_then(Entity::living_mut)
                    .and_then(|state| state.mob.as_mut())
                    .and_then(|mob| mob.ageable.as_mut())
                {
                    ageable.reset_love();
                }
            }
        }
    }

    pub(crate) fn armadillo_custom_ai_step(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        let Some(state) = armadillo_state(entity) else {
            self.store_mob_random(id, random);
            return;
        };
        let time = state
            .scute_time
            .unwrap_or_else(|| armadillo::pick_next_scute_time(&mut random));
        state.scute_time = Some(time);
        if !alive {
            self.store_mob_random(id, random);
            return;
        }
        let remaining = time - 1;
        state.scute_time = Some(remaining);
        if remaining >= 1 || !self.config.rules.mob_drops {
            self.store_mob_random(id, random);
            return;
        }
        let next = armadillo::pick_next_scute_time(&mut random);
        let pitch = (random.next_float() - random.next_float()) * PITCH_SPREAD + 1.0;
        self.store_mob_random(id, random);
        if let Some(state) = self.entities.store.get_mut(id).and_then(armadillo_state) {
            state.scute_time = Some(next);
        }
        if self.drop_table_at(id, SHED_TABLE) {
            self.entity_sound(id, SCUTE_DROP_SOUND, 1.0, pitch);
        }
    }

    fn drop_table_at(&mut self, id: EntityId, table: &str) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let drops = rspigot_data::loot::table(table).map_or_else(Vec::new, |table| {
            let context = crate::loot::LootContext {
                this: Some(&facts),
                ..crate::loot::LootContext::default()
            };
            crate::loot::table_drops(table, &context, &mut self.rng)
        });
        let dropped = !drops.is_empty();
        for stack in drops {
            self.drop_at_location(id, 0.0, stack, Vec3::ZERO);
        }
        dropped
    }

    pub(crate) fn armadillo_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let baby = entity.is_baby();
        let scared = is_scared(entity);
        if held.is_some_and(|stack| stack.item == ItemKind::Brush.id()) && !baby {
            self.drop_table_at(target, BRUSH_TABLE);
            self.entity_sound(target, BRUSH_SOUND, 1.0, 1.0);
            self.hurt_held(player, hand, BRUSH_DURABILITY);
            return Some(Interaction::Predicted);
        }
        scared.then_some(Interaction::Pass)
    }
}
