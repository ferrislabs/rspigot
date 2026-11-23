use rspigot_domain::Vec3;
use rspigot_domain::entity::species::bogged;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, item::ItemStack};
use rspigot_entities::SoundSource;
use std::num::NonZeroU8;

use crate::loot::{self, LootContext};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn shear_bogged(&mut self, target: EntityId) {
        let Some(entity) = self.entities.store.get(target) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        self.play_entity_sound(target, bogged::SHEAR_SOUND, SoundSource::Players, 1.0, 1.0);
        let drops = rspigot_data::loot::table(bogged::SHEAR_TABLE).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        let height = self
            .entities
            .store
            .get(target)
            .map_or(0.0, |entity| f64::from(entity.base.dimensions.height));
        for stack in drops {
            for _ in 0..stack.count.get() {
                let single = ItemStack {
                    count: NonZeroU8::MIN,
                    ..stack.clone()
                };
                let mut random = self.mob_random(target);
                let push = Vec3::new(
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                    f64::from(random.next_float() * 0.05),
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                );
                self.store_mob_random(target, random);
                self.drop_at_location(target, height, single, push);
            }
        }
        if let Some(entity) = self.entities.store.get_mut(target) {
            bogged::set_sheared(&mut entity.base, true);
        }
    }
}
