use std::num::NonZeroU8;

use rand::Rng;
use rspigot_ai::{MobView, TargetReason, TargetRequest, WitherAction, WitherFacts};
use rspigot_domain::boss_event::{BossColor, BossOverlay, ServerBossEvent};
use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::species::{Species, wither};
use rspigot_domain::entity::{Entity, EntityState, hurting, living};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use uuid::Uuid;

use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::mobs::ServerRandom;
use crate::plugins::events::{EntityRegainHealthRegainReason, ExplosionPrime, ExplosionPrimePatch};
use crate::plugins::steps::{Grief, PrimeKind, PrimeStep, Seeker};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const WITHER_IMMUNE: &str = "minecraft:wither_immune";
const WITHER_IMMUNE_TO: &str = "minecraft:wither_immune_to";
const ENTITY_TAGS: &str = "minecraft:entity_type";
const WITHER_FRIENDS: &str = "minecraft:wither_friends";

fn state_of(entity: &mut Entity) -> Option<&mut wither::WitherState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Wither(state)) => Some(state),
        _ => None,
    }
}

fn is_friend(kind: Kind) -> bool {
    rspigot_data::tags::tag(ENTITY_TAGS, WITHER_FRIENDS)
        .is_some_and(|ids| ids.contains(&u32::from(kind.id().0)))
}

fn can_destroy(state: BlockStateId) -> bool {
    !state.is_air() && !has_tag(state, WITHER_IMMUNE)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn wither_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.wither = WitherFacts {
            invulnerable_ticks: wither::invulnerable_ticks(&entity.base),
            targets: [0, 1, 2].map(|head| wither::alternative_target(&entity.base, head)),
        };
    }

    pub(crate) fn wither_registered(&mut self, id: EntityId) {
        let Some(name) = self.entity_name(id) else {
            return;
        };
        let uuid = Uuid::from_u128(self.rng.random());
        let mut bar = ServerBossEvent::new(uuid, name, BossColor::Purple, BossOverlay::Progress);
        bar.set_darken_screen(true);
        let fresh = self.entities.store.get(id).is_some_and(|entity| {
            wither::invulnerable_ticks(&entity.base) == wither::SPAWN_INVULNERABLE_TICKS
        });
        if fresh {
            bar.set_progress(0.0);
        }
        self.add_boss_bar(id, bar);
    }

    fn health_of(&mut self, id: EntityId) -> Option<(f32, f32)> {
        let entity = self.entities.store.get_mut(id)?;
        let living = entity.living_mut()?;
        let max = living.max_health() as f32;
        Some((living::health(&entity.base), max))
    }

    pub(crate) fn wither_ai_step(&mut self, id: EntityId) {
        let Some((health, max)) = self.health_of(id) else {
            return;
        };
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let target = match wither::alternative_target(&entity.base, 0) {
            head if head > 0 => self.entity_eye(EntityId(head)).map(|(at, _)| at),
            _ => None,
        };
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let steering = wither::steer(
            entity.base.velocity,
            entity.base.position,
            target,
            wither::is_powered(health, max),
        );
        entity.base.velocity = steering.velocity;
        if let Some(yaw) = steering.yaw {
            entity.base.set_yaw(yaw);
        }
    }

    pub(crate) fn wither_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let invulnerable = wither::invulnerable_ticks(&entity.base);
        if invulnerable < 1 {
            self.wither_awake_step(id);
        } else {
            self.wither_growing_step(id, invulnerable - 1);
        }
    }

    fn wither_awake_step(&mut self, id: EntityId) {
        let target = self.mob_target(id).map_or(0, |target| target.0);
        let mut break_now = false;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        wither::set_alternative_target(&mut entity.base, 0, target);
        let tick_count = entity.base.tick_count;
        if let Some(state) = state_of(entity)
            && state.destroy_blocks_tick > 0
        {
            state.destroy_blocks_tick -= 1;
            break_now = state.destroy_blocks_tick == 0;
        }
        if break_now && self.config.rules.mob_griefing {
            self.wither_break_blocks(id);
        }
        if tick_count % wither::REGEN_INTERVAL == 0 {
            self.heal_entity(
                id,
                wither::REGEN_AMOUNT,
                EntityRegainHealthRegainReason::Regen,
            );
        }
        if let Some((health, max)) = self.health_of(id) {
            self.update_boss_bar(id, |bar| bar.set_progress(health / max));
        }
    }

    fn wither_break_blocks(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let width = entity.base.dimensions.width;
        let height = entity.base.dimensions.height;
        let origin = entity.base.position.block();
        if let Some(state) = state_of(entity) {
            state.breaking_blocks = true;
            state.broke_blocks = false;
        }
        let reach = (width / 2.0 + 1.0).floor() as i32;
        let tall = height.floor() as i32;
        for y in origin.y..=origin.y + tall {
            for z in origin.z - reach..=origin.z + reach {
                for x in origin.x - reach..=origin.x + reach {
                    let pos = BlockPos::new(x, y, z);
                    let Some(state) = self.world.chunks.block(pos) else {
                        continue;
                    };
                    if !can_destroy(state) {
                        continue;
                    }
                    let Some(view) = self.entity_view(id) else {
                        return;
                    };
                    let change = Grief::WitherBreak { wither: id, pos };
                    self.change_block(view, pos, BlockStateId::AIR, change, false);
                }
            }
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let at = entity.base.position.block();
        let broke = state_of(entity).is_some_and(|state| {
            state.breaking_blocks = false;
            std::mem::take(&mut state.broke_blocks)
        });
        if broke {
            self.level_event(wither::BREAK_EVENT, at, 0);
        }
    }

    pub(crate) fn finish_wither_break(&mut self, id: EntityId, pos: BlockPos) {
        if !self.living_entity_alive(id) {
            return;
        }
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| can_destroy(*state))
        else {
            return;
        };
        let _ = state;
        self.destroy_block_dropping(pos, Some(id));
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let at = entity.base.position.block();
        let announce = state_of(entity).is_some_and(|state| {
            if state.breaking_blocks {
                state.broke_blocks = true;
                false
            } else {
                true
            }
        });
        if announce {
            self.level_event(wither::BREAK_EVENT, at, 0);
        }
    }

    fn wither_growing_step(&mut self, id: EntityId, remaining: i32) {
        self.update_boss_bar(id, |bar| {
            bar.set_progress(1.0 - remaining as f32 / wither::SPAWN_INVULNERABLE_TICKS as f32)
        });
        if remaining < 1 {
            self.wither_spawn_explosion(id);
            self.wither_spawn_sound(id);
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        wither::set_invulnerable_ticks(&mut entity.base, remaining);
        if entity.base.tick_count % wither::SPAWN_REGEN_INTERVAL == 0 {
            self.heal_entity(
                id,
                wither::SPAWN_REGEN_AMOUNT,
                EntityRegainHealthRegainReason::WitherSpawn,
            );
        }
    }

    fn wither_spawn_explosion(&mut self, id: EntityId) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        let radius = wither::EXPLOSION_RADIUS;
        let step = PrimeStep {
            source: id,
            radius,
            fire: false,
            kind: PrimeKind::WitherSpawn,
        };
        self.fire(lane, Continuation::Prime(step), move |_| {
            Some(EventPayload::ExplosionPrime(ExplosionPrime {
                entity_type: view.kind.clone(),
                entity: view,
                fire: false,
                radius,
            }))
        });
    }

    pub(crate) fn finish_wither_prime(
        &mut self,
        step: PrimeStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        if cancelled || !self.living_entity_alive(step.source) {
            return;
        }
        let Some(entity) = self.entities.store.get(step.source) else {
            return;
        };
        let center = Vec3::new(
            entity.base.position.x,
            entity.base.eye_y(),
            entity.base.position.z,
        );
        let Some(source) = self.mob_source(step.source) else {
            return;
        };
        let damage = DamageSource::by(DamageType::PlayerExplosion, source);
        let interaction = self.mob_block_interaction();
        let radius = patch.radius.unwrap_or(step.radius);
        let fire = patch.fire.unwrap_or(step.fire);
        self.explode(
            Some(step.source),
            &damage,
            center,
            radius,
            interaction,
            fire,
            None,
        );
    }

    fn wither_spawn_sound(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_silent() {
            return;
        }
        let at = entity.base.position;
        self.global_level_event_near(at, wither::SPAWN_EVENT);
    }

    pub(crate) fn global_level_event_near(&mut self, at: Vec3, event: i32) {
        let range = f64::from(i32::from(self.config.view_distance) * 16);
        let events: Vec<(rspigot_domain::player::PlayerId, BlockPos)> = self
            .players
            .values()
            .map(|player| {
                let dx = at.x - player.position.x;
                let dz = at.z - player.position.z;
                let distance = dx * dx + dz * dz;
                if distance <= range * range {
                    return (player.id, at.block());
                }
                let length = distance.sqrt();
                let x = player.position.x + dx / length * range;
                let z = player.position.z + dz / length * range;
                (
                    player.id,
                    BlockPos::new(
                        (x + 0.5).floor() as i32,
                        (at.y + 0.5).floor() as i32,
                        (z + 0.5).floor() as i32,
                    ),
                )
            })
            .collect();
        for (player, pos) in events {
            self.push(
                crate::server::Audience::One(player),
                ClientEvent::GlobalLevelEvent {
                    event,
                    pos,
                    data: 0,
                },
            );
        }
    }

    pub(crate) fn wither_action(&mut self, id: EntityId, action: WitherAction) {
        match action {
            WitherAction::Shoot {
                head,
                target,
                dangerous,
            } => self.wither_shoot(id, usize::from(head), target, dangerous),
            WitherAction::ShootAt { head, target } => {
                self.wither_shoot_at(id, usize::from(head), target);
            }
            WitherAction::Aim { head, target } => self.wither_aim_head(id, head, target),
            WitherAction::Clear { head } => self.set_wither_head_target(id, head, None),
        }
    }

    fn wither_aim_head(&mut self, id: EntityId, head: u8, target: EntityId) {
        let Some(mob) = self.entity_view(id) else {
            return;
        };
        let from = self
            .entities
            .store
            .get(id)
            .map(|entity| wither::alternative_target(&entity.base, usize::from(head)))
            .filter(|current| *current > 0)
            .map(EntityId);
        let request = TargetRequest {
            from,
            to: Some(target),
            reason: TargetReason::ClosestEntity,
            tempt: false,
        };
        self.target_event(mob, request, Seeker::WitherHead(head), true);
    }

    pub(crate) fn set_wither_head_target(
        &mut self,
        id: EntityId,
        head: u8,
        target: Option<EntityId>,
    ) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            wither::set_alternative_target(
                &mut entity.base,
                usize::from(head),
                target.map_or(0, |target| target.0),
            );
        }
    }

    pub(crate) fn wither_shoot_at(&mut self, id: EntityId, head: usize, target: EntityId) {
        let Some((at, eye_y)) = self.entity_eye(target) else {
            return;
        };
        let eye_height = (eye_y - at.y) as f32;
        let dangerous = head == 0 && {
            let mut random = ServerRandom(&mut self.rng);
            random.next_float() < wither::DANGEROUS_CHANCE
        };
        let aim = Vec3::new(at.x, wither::shot_target_y(at.y, eye_height), at.z);
        self.wither_shoot(id, head, aim, dangerous);
    }

    fn wither_shoot(&mut self, id: EntityId, head: usize, target: Vec3, dangerous: bool) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let silent = entity.base.is_silent();
        let position = entity.base.position;
        let body_yaw = match entity.living() {
            Some(living) => living.body_yaw,
            None => entity.base.yaw,
        };
        let scale = match entity.living_mut() {
            Some(living) => living.attribute("minecraft:scale") as f32,
            None => 1.0,
        };
        if !silent {
            self.level_event(wither::SHOOT_EVENT, position.block(), 0);
        }
        let from = Vec3::new(
            wither::head_x(position, body_yaw, scale, head),
            wither::head_y(position, scale, head),
            wither::head_z(position, body_yaw, scale, head),
        );
        let direction = (target - from).normalize();
        self.launch_hurting_flagged(Kind::WitherSkull, id, direction, from, 1, dangerous);
    }

    pub(crate) fn wither_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        if source.kind.in_tag(WITHER_IMMUNE_TO)
            || source
                .causing
                .as_ref()
                .is_some_and(|causing| causing.kind == Kind::Wither)
        {
            return None;
        }
        let (health, max) = self.health_of(id)?;
        let entity = self.entities.store.get_mut(id)?;
        if wither::invulnerable_ticks(&entity.base) > 0
            && !source.kind.is(DamageTag::BypassesInvulnerability)
        {
            return None;
        }
        if wither::is_powered(health, max)
            && source.direct.as_ref().is_some_and(|direct| {
                direct.kind.extends("AbstractArrow") || hurting::is_wind_charge(direct.kind)
            })
        {
            return None;
        }
        if source
            .causing
            .as_ref()
            .is_some_and(|causing| is_friend(causing.kind))
        {
            return None;
        }
        if let Some(state) = state_of(entity)
            && state.destroy_blocks_tick < 1
        {
            state.destroy_blocks_tick = wither::DESTROY_BLOCKS_TICKS;
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.wither_hurt();
        }
        Some(amount)
    }

    pub(crate) fn wither_death_drops(&mut self, id: EntityId) -> Vec<ItemStack> {
        let _ = id;
        vec![ItemStack::new(ItemKind::NetherStar.id(), NonZeroU8::MIN)]
    }

    pub(crate) fn wither_item_dropped(&mut self, item: EntityId) {
        if let Some(Entity {
            state: EntityState::Item(state),
            ..
        }) = self.entities.store.get_mut(item)
        {
            state.age = wither::NETHER_STAR_AGE;
        }
    }

    pub(crate) fn wither_refuses_effects(&self, id: EntityId) -> bool {
        self.kind_of(id) == Some(Kind::Wither)
    }
}
