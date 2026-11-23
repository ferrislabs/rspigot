use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::BlockPos;
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

use crate::health::DamageSource;
use crate::plugins::steps::FormStep;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn create_wither_rose(
        &mut self,
        victim: rspigot_domain::EntityId,
        source: &DamageSource,
    ) {
        let killed_by_wither = source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.kind == Kind::Wither);
        if !killed_by_wither {
            return;
        }
        let Some(position) = self.entities.store.get(victim).map(|e| e.base.position) else {
            return;
        };
        if self.config.rules.mob_griefing {
            let pos = BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            );
            let rose = BlockKind::WitherRose.default_state();
            let free = self
                .world
                .chunks
                .block(pos)
                .is_some_and(rspigot_domain::BlockStateId::is_air);
            if free
                && self.can_survive_at(rose, pos)
                && let Some(view) = self
                    .entities
                    .store
                    .get(victim)
                    .map(crate::plugins::view_of_entity)
            {
                self.form_block(
                    view,
                    FormStep {
                        entity: victim,
                        pos,
                        state: rose,
                    },
                );
                return;
            }
        }
        let stack = ItemStack::new(ItemKind::WitherRose.id(), NonZeroU8::MIN);
        self.drop_at_location(victim, 0.0, stack, rspigot_domain::Vec3::ZERO);
    }

    pub(crate) fn finish_wither_rose(&mut self, step: FormStep) {
        let free = self
            .world
            .chunks
            .block(step.pos)
            .is_some_and(rspigot_domain::BlockStateId::is_air);
        if free && self.can_survive_at(step.state, step.pos) {
            self.set_block(step.pos, step.state);
        }
    }
}
