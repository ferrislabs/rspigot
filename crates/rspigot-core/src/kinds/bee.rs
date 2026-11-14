use rspigot_ai::{
    BeeAction, BeeFacts, CROP_GROWTH_EVENT, CROP_GROWTH_PARTICLES, MobView, TargetReason,
};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::species::{Species, bee};
use rspigot_domain::entity::{Entity, EntityState, living};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_items::ItemKind;

use crate::commands::Target;
use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::plugins::events::EntityPotionEffectCause;
use crate::plugins::steps::{Grief, MobAttackFollow, Struck};
use crate::ports::ClientLink;
use crate::server::Server;

const POISON: &str = "minecraft:poison";
const WITHER: &str = "minecraft:wither";
const BEE_FOOD: &str = "minecraft:bee_food";
const WITHER_ROSE_TICKS: u32 = 40;
const EYEBLOSSOM_TICKS: u32 = 25;
const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
const ROLL_RANGE_SQR: f64 = bee::ROLL_DISTANCE_SQR;

fn bee_state(entity: &Entity) -> Option<&bee::BeeState> {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| &mob.species) {
            Some(Species::Bee(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn bee_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(state) = bee_state(entity) else {
            return;
        };
        view.species.bee = BeeFacts {
            hive_pos: state.hive_pos,
            flower_pos: state.flower_pos,
            has_nectar: bee::has_nectar(&entity.base),
            has_stung: bee::has_stung(&entity.base),
            ticks_without_nectar: state.ticks_without_nectar,
            stay_out_countdown: state.stay_out_countdown,
            crops_grown: state.crops_grown,
            locate_hive_cooldown: state.locate_hive_cooldown,
            locate_flower_cooldown: state.locate_flower_cooldown,
        };
    }

    pub(crate) fn bee_action(&mut self, id: EntityId, action: BeeAction) {
        match action {
            BeeAction::Alert { mob, target } => {
                if let Some(view) = self.entity_view(mob) {
                    self.request_target(view, target, TargetReason::TargetAttackedEntity);
                }
            }
            BeeAction::EnterHive(pos) => self.bee_enters_hive(id, pos),
            BeeAction::GrowCrop { pos, to } => {
                let Some(view) = self.entity_view(id) else {
                    return;
                };
                self.change_block(view, pos, to, Grief::BeeGrow { bee: id, pos, to }, false);
            }
        }
    }

    pub(crate) fn finish_bee_grow(&mut self, id: EntityId, pos: BlockPos, to: BlockStateId) {
        self.level_event(CROP_GROWTH_EVENT, pos, CROP_GROWTH_PARTICLES);
        self.set_block(pos, to);
        if let Some((_, state)) = self.bee_parts(id) {
            state.crops_grown += 1;
        }
    }

    pub(crate) fn bee_ai_step(&mut self, id: EntityId) {
        let Some((angry, target_close, tick_count)) = self.bee_rolling_inputs(id) else {
            return;
        };
        let hive_valid = self.bee_hive_valid(id);
        let Some((base, state)) = self.bee_parts(id) else {
            return;
        };
        state.tick_cooldowns();
        let rolling = angry && !bee::has_stung(base) && target_close;
        bee::set_rolling(base, rolling);
        if tick_count % bee::HIVE_VALIDATION_INTERVAL == 0 && !hive_valid {
            state.hive_pos = None;
        }
    }

    fn bee_rolling_inputs(&self, id: EntityId) -> Option<(bool, bool, i32)> {
        let entity = self.entities.store.get(id)?;
        let game_time = self.world.level.game_time;
        let angry = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.neutral)
            .is_some_and(|anger| anger.is_angry(game_time));
        let close = self.mob_target(id).is_some_and(|target| {
            self.entities.store.get(target).map_or_else(
                || {
                    self.players
                        .values()
                        .find(|player| player.entity_id == target)
                        .is_some_and(|player| {
                            player.position.distance_squared(entity.base.position) < ROLL_RANGE_SQR
                        })
                },
                |found| found.base.position.distance_squared(entity.base.position) < ROLL_RANGE_SQR,
            )
        });
        Some((angry, close, entity.base.tick_count))
    }

    fn bee_hive_valid(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(hive) = bee_state(entity).and_then(|state| state.hive_pos) else {
            return false;
        };
        let here = entity.base.position.block();
        let dx = i64::from(hive.x - here.x);
        let dy = i64::from(hive.y - here.y);
        let dz = i64::from(hive.z - here.z);
        let limit = i64::from(bee::MAX_DISTANCE_TO_TARGET);
        dx * dx + dy * dy + dz * dz < limit * limit && self.hive_at(hive).is_some()
    }

    pub(crate) fn bee_custom_ai_step(&mut self, id: EntityId) {
        let in_water = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| entity.base.fluid.in_water);
        let mut random = self.mob_random(id);
        let Some((base, state)) = self.bee_parts(id) else {
            return;
        };
        let step = bee::server_step(state, base, in_water, &mut random);
        self.store_mob_random(id, random);
        if step.drown {
            self.hurt_mob(id, &DamageSource::new(DamageType::Drown), 1.0);
        }
        if step.die_of_sting {
            let health = self
                .entities
                .store
                .get(id)
                .map_or(0.0, |entity| living::health(&entity.base));
            self.hurt_mob(id, &DamageSource::new(DamageType::Generic), health);
        }
        self.update_persistent_anger(id, false);
    }

    pub(crate) fn bee_after_hurt(&mut self, id: EntityId) {
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.stop_pollinating();
        }
    }

    pub(crate) fn bee_hurt_target(&mut self, attacker: EntityId, target: Target) -> bool {
        let Some(source_entity) = self.mob_source(attacker) else {
            return false;
        };
        let Some(damage) = self
            .entities
            .store
            .get_mut(attacker)
            .and_then(Entity::living_mut)
            .map(|state| state.attribute(ATTACK_DAMAGE) as i32 as f32)
        else {
            return false;
        };
        let source = DamageSource::by(DamageType::Sting, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = MobAttackFollow {
            attacker,
            target,
            knockback: 0.0,
            yaw: 0.0,
            golem: false,
            hoglin: false,
            bee: true,
        };
        self.strike(
            None,
            victim,
            &source,
            damage,
            crate::plugins::steps::AfterHurt::MobAttack(follow),
        )
    }

    pub(crate) fn bee_finish_attack(&mut self, attacker: EntityId, target: Target, hurt: bool) {
        if !hurt {
            return;
        }
        let seconds = bee::sting_poison_seconds(self.world.difficulty);
        if seconds > 0 {
            let victim = match target {
                Target::Player(player) => self.players.get(&player).map(|p| p.entity_id),
                Target::Entity(entity) => self
                    .entities
                    .store
                    .get(entity)
                    .filter(|found| found.living().is_some())
                    .map(|_| entity),
            };
            if let (Some(victim), Some(poison)) = (victim, EffectId::from_name(POISON)) {
                let effect = EffectInstance::new(
                    poison,
                    EffectDuration::Ticks((seconds * 20) as u32),
                    0,
                    true,
                );
                self.add_effect_to(victim, effect, EntityPotionEffectCause::Attack);
            }
        }
        if let Some((base, _)) = self.bee_parts(attacker) {
            bee::set_has_stung(base, true);
        }
        self.stop_being_angry(attacker);
        self.entity_sound(attacker, bee::STING_SOUND, 1.0, 1.0);
    }

    pub(crate) fn bee_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let item = held?.item;
        if !item_tag(item, BEE_FOOD) {
            return None;
        }
        let (name, ticks) = match item.kind() {
            Some(ItemKind::WitherRose) => (WITHER, WITHER_ROSE_TICKS),
            Some(ItemKind::OpenEyeblossom) => (POISON, EYEBLOSSOM_TICKS),
            _ => return None,
        };
        let effect = EffectId::from_name(name)?;
        self.consume_held(player, hand, 1);
        self.add_effect_to(
            target,
            EffectInstance::new(effect, EffectDuration::Ticks(ticks), 0, true),
            EntityPotionEffectCause::Unknown,
        );
        Some(Interaction::Predicted)
    }
}
