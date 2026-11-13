use rspigot_domain::EntityId;
use rspigot_domain::entity::physics::{has_tag, on_pos_legacy};
use rspigot_domain::entity::species::strider;
use rspigot_domain::entity::{Entity, EntityState, living};
use rspigot_domain::random::RandomSource;
use rspigot_entities::EntityKind as Kind;

use crate::plugins::events::StriderTemperatureChange;
use crate::plugins::steps::ShiverStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const WARM_BLOCKS: &str = "minecraft:strider_warm_blocks";
const HAPPY_SOUND: &str = "minecraft:entity.strider.happy";
const RETREAT_SOUND: &str = "minecraft:entity.strider.retreat";

impl<L: ClientLink> Server<L> {
    pub(crate) fn strider_pre_tick(&mut self, id: EntityId) -> bool {
        self.strider_sounds(id);
        let Some(entity) = self.entities.store.get(id) else {
            return true;
        };
        if living::is_no_ai(&entity.base) {
            return true;
        }
        let base = &entity.base;
        let warm = self
            .world
            .chunks
            .block(base.position.block())
            .is_some_and(|state| has_tag(state, WARM_BLOCKS))
            || self
                .world
                .chunks
                .block(on_pos_legacy(base, &self.world.chunks))
                .is_some_and(|state| has_tag(state, WARM_BLOCKS))
            || base.fluid.lava_height > 0.0;
        let on_warm_strider = self.entities.rides.vehicle_of(id).is_some_and(|vehicle| {
            self.entities.store.get(vehicle).is_some_and(|carrier| {
                carrier.base.kind == Kind::Strider && !strider::is_suffocating(&carrier.base)
            })
        });
        let shivering = strider::should_shiver(warm, on_warm_strider);
        if shivering != strider::is_suffocating(base) {
            let lane = self.lane_for(None, id);
            self.fire(
                lane,
                Continuation::Shiver(ShiverStep {
                    strider: id,
                    shivering,
                }),
                move |server| {
                    let entity = server.entity_view(id)?;
                    Some(EventPayload::StriderTemperatureChange(
                        StriderTemperatureChange {
                            entity_type: entity.kind.clone(),
                            entity,
                            shivering,
                        },
                    ))
                },
            );
        }
        true
    }

    pub(crate) fn finish_shiver(&mut self, step: ShiverStep) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(step.strider)
        {
            strider::set_suffocating(base, &mut state.attributes, step.shivering);
        }
    }

    fn strider_sounds(&mut self, id: EntityId) {
        let Some((tempted, panicking)) = self
            .entities
            .mobs
            .entries
            .get(&id)
            .map(|entry| (entry.ai.is_tempted(), entry.ai.is_panicking()))
        else {
            return;
        };
        let mut random = self.mob_random(id);
        let sound = if tempted && random.next_int(strider::HAPPY_ODDS) == 0 {
            Some(HAPPY_SOUND)
        } else if !tempted && panicking && random.next_int(strider::RETREAT_ODDS) == 0 {
            Some(RETREAT_SOUND)
        } else {
            None
        };
        self.store_mob_random(id, random);
        if let Some(sound) = sound {
            self.entity_sound(id, sound, 1.0, 1.0);
        }
    }

    pub(crate) fn strider_post_tick(&mut self, id: EntityId) {
        let above = self.entities.store.get(id).is_some_and(|entity| {
            let pos = entity.base.position.block().offset(0, 1, 0);
            self.world
                .chunks
                .block(pos)
                .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::LAVA))
        });
        if let Some(entity) = self.entities.store.get_mut(id) {
            strider::floats(&mut entity.base, above);
        }
    }
}
