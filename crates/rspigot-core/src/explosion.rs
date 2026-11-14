use rand::Rng;
use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species::{self, Species};
use rspigot_domain::entity::{Aabb, EntityState, RemovalReason, item, living};
use rspigot_domain::explosion::{self, MIN_RADIUS, PACKET_RANGE_SQR};
use rspigot_domain::item::{ItemStack, same_item_same_components};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::loot::{self, LootContext, LootEntity};
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    EntityExplode, EntityExplodePatch, EntityRemoveCause, ExplosionPrime, ExplosionPrimePatch,
    ExplosionResult,
};
use crate::plugins::steps::{ExplodeStep, PrimeKind, PrimeStep};
use crate::plugins::{AfterHurt, BlastFollow, Holder, Struck};
use crate::plugins::{Continuation, EventPayload, SetDiff};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::game_event::GameEvent;

const MAX_MERGED_STACK: u32 = 16;
const EXPLOSION_KNOCKBACK_RESISTANCE: &str = "minecraft:explosion_knockback_resistance";
const POP_SPREAD: f64 = 0.25;
const SOUL_FIRE_BASE: &str = "minecraft:soul_fire_base_blocks";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockInteraction {
    Keep,
    Destroy,
    DestroyWithDecay,
}

pub(crate) struct HitEntity {
    pub(crate) id: EntityId,
    pub(crate) player: Option<PlayerId>,
    pub(crate) position: Vec3,
    pub(crate) eye: Vec3,
    pub(crate) bounds: Aabb,
    pub(crate) living: bool,
    pub(crate) part: Option<rspigot_domain::dragon::Part>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn explode_creeper(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return true;
        };
        let Some(Species::Creeper(creeper)) = entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .map(|mob| mob.species)
        else {
            return false;
        };
        let multiplier = if species::is_powered(&entity.base) {
            2.0
        } else {
            1.0
        };
        let radius = creeper.explosion_radius as f32 * multiplier;
        let Some(view) = self.entity_view(id) else {
            return false;
        };
        let lane = self.lane_for(None, id);
        let step = PrimeStep {
            source: id,
            radius,
            fire: false,
            kind: PrimeKind::Creeper,
        };
        self.fire(lane, Continuation::Prime(step), move |_| {
            Some(EventPayload::ExplosionPrime(ExplosionPrime {
                entity_type: view.kind.clone(),
                entity: view,
                fire: false,
                radius,
            }))
        });
        self.entities
            .store
            .get(id)
            .is_none_or(|entity| entity.living().is_some_and(|living| living.dead))
    }

    pub(crate) fn finish_prime(
        &mut self,
        step: PrimeStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        match step.kind {
            PrimeKind::Projectile => {
                self.finish_projectile_prime(step, cancelled, patch);
                return;
            }
            PrimeKind::WitherSpawn => {
                self.finish_wither_prime(step, cancelled, patch);
                return;
            }
            PrimeKind::Creeper => {}
        }
        let id = step.source;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let alive = !entity.base.is_removed()
            && entity
                .living()
                .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living));
        if !alive {
            return;
        }
        if cancelled {
            if let Some(Species::Creeper(creeper)) = entity
                .living_mut()
                .and_then(|living| living.mob.as_mut())
                .map(|mob| &mut mob.species)
            {
                creeper.swell = 0;
            }
            return;
        }
        let center = entity.base.position;
        if let Some(living) = entity.living_mut() {
            living.dead = true;
        }
        let Some(source) = self.mob_source(id) else {
            return;
        };
        let damage = DamageSource::by(DamageType::PlayerExplosion, source);
        let interaction = self.mob_block_interaction();
        let radius = patch.radius.unwrap_or(step.radius);
        let fire = patch.fire.unwrap_or(step.fire);
        self.explode(
            Some(id),
            &damage,
            center,
            radius,
            interaction,
            fire,
            Some(id),
        );
    }

    pub(crate) fn mob_block_interaction(&self) -> BlockInteraction {
        if !self.config.rules.mob_griefing {
            BlockInteraction::Keep
        } else if self.config.rules.mob_explosion_drop_decay {
            BlockInteraction::DestroyWithDecay
        } else {
            BlockInteraction::Destroy
        }
    }

    pub(crate) fn tnt_block_interaction(&self) -> BlockInteraction {
        if self.config.rules.mob_griefing {
            BlockInteraction::Destroy
        } else {
            BlockInteraction::Keep
        }
    }

    pub(crate) fn discard_cause(&self, id: EntityId) -> EntityRemoveCause {
        let projectile = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| matches!(entity.state, EntityState::Hurting(_)));
        if projectile {
            EntityRemoveCause::Hit
        } else {
            EntityRemoveCause::Explode
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn explode(
        &mut self,
        source: Option<EntityId>,
        damage: &DamageSource,
        center: Vec3,
        radius: f32,
        interaction: BlockInteraction,
        fire: bool,
        discard: Option<EntityId>,
    ) {
        self.explode_capped(
            source,
            damage,
            center,
            radius,
            interaction,
            fire,
            discard,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn explode_capped(
        &mut self,
        source: Option<EntityId>,
        damage: &DamageSource,
        center: Vec3,
        radius: f32,
        interaction: BlockInteraction,
        fire: bool,
        discard: Option<EntityId>,
        resistance_cap: Option<f32>,
    ) {
        self.game_event(GameEvent::Explode, center, GameEventContext::of(source));
        let radius = radius.max(0.0);
        if radius < MIN_RADIUS {
            if let Some(source) = discard {
                let cause = self.discard_cause(source);
                self.remove_entity(source, RemovalReason::Discarded, Some(cause));
            }
            return;
        }
        let height = self.world.dimension.height;
        let positions = {
            let mut random = ServerRandom(&mut self.rng);
            explosion::exploded_positions_capped(
                &self.world.chunks,
                height,
                center,
                radius,
                &mut random,
                resistance_cap,
            )
        };
        let knocked = self.hurt_by_explosion(source, damage, center, radius);
        let mut shuffled = positions;
        explosion::shuffle(&mut shuffled, &mut ServerRandom(&mut self.rng));
        let blocks: Vec<BlockPos> = shuffled
            .into_iter()
            .rev()
            .filter(|pos| {
                self.world
                    .chunks
                    .block(*pos)
                    .is_some_and(|state| !state.is_air())
            })
            .collect();
        let explosion_yield = if interaction == BlockInteraction::DestroyWithDecay {
            1.0 / radius
        } else {
            1.0
        };
        let step = ExplodeStep {
            center,
            radius,
            interaction,
            source,
            blocks,
            knocked,
            explosion_yield,
            fire,
            discard,
        };
        let blamed = source.or_else(|| damage.causing.as_ref().map(|causing| causing.entity));
        match blamed {
            Some(entity) => self.ask_entity_explosion(entity, step),
            None => self.ask_explosion(step),
        }
    }

    fn ask_entity_explosion(&mut self, entity: EntityId, step: ExplodeStep) {
        let Some(view) = self.subject_view(entity) else {
            self.settle_explosion(step, false, SetDiff::default(), None);
            return;
        };
        let block_list = step.blocks.clone();
        let explosion_result = match step.interaction {
            BlockInteraction::Keep => ExplosionResult::Keep,
            BlockInteraction::Destroy => ExplosionResult::Destroy,
            BlockInteraction::DestroyWithDecay => ExplosionResult::DestroyWithDecay,
        };
        let explosion_yield = step.explosion_yield;
        let lane = self.lane_for(None, entity);
        self.fire(
            lane,
            Continuation::EntityExplode(Box::new(step)),
            move |server| {
                let location =
                    server.location_of(view.position, rspigot_domain::Rotation::default());
                Some(EventPayload::EntityExplode(EntityExplode {
                    block_list,
                    entity_type: view.kind.clone(),
                    entity: view,
                    explosion_result,
                    location,
                    explosion_yield,
                }))
            },
        );
    }

    pub(crate) fn finish_entity_explosion(
        &mut self,
        step: ExplodeStep,
        cancelled: bool,
        patch: EntityExplodePatch,
    ) {
        self.settle_explosion(step, cancelled, patch.block_list, patch.explosion_yield);
    }

    pub(crate) fn create_fire(&mut self, blocks: &[BlockPos]) {
        for pos in blocks {
            if self.rng.random_range(0..3) != 0 {
                continue;
            }
            let here = self.world.chunks.block(*pos).unwrap_or(BlockStateId::AIR);
            let below = self
                .world
                .chunks
                .block(pos.offset(0, -1, 0))
                .unwrap_or(BlockStateId::AIR);
            if !here.is_air() || !below.flags().contains(StateFlags::SOLID_RENDER) {
                continue;
            }
            self.set_block(*pos, fire_state_on(below));
        }
    }

    pub(crate) fn blast(&mut self, step: ExplodeStep) {
        let ExplodeStep {
            center,
            radius,
            interaction,
            source,
            blocks,
            knocked,
            explosion_yield,
            ..
        } = step;
        let silent = source
            .and_then(|id| self.entities.store.get(id))
            .is_some_and(|e| e.base.is_silent());
        let large = radius >= explosion::LARGE_RADIUS && interaction != BlockInteraction::Keep;
        let viewers: Vec<PlayerId> = self
            .players
            .values()
            .filter(|p| p.position.distance_squared(center) < PACKET_RANGE_SQR)
            .map(|p| p.id)
            .collect();
        for player in viewers {
            let knockback = knocked
                .iter()
                .find(|(id, _)| *id == player)
                .map(|(_, v)| *v);
            self.push(
                Audience::One(player),
                ClientEvent::Explode {
                    center,
                    radius,
                    block_count: blocks.len() as i32,
                    knockback,
                    large,
                    play_sound: !silent,
                    gust: None,
                },
            );
        }
        if interaction != BlockInteraction::Keep {
            let decay = (explosion_yield < 1.0).then(|| 1.0 / explosion_yield);
            self.blow_up_blocks(&blocks, source, decay);
        }
    }

    pub(crate) fn explosion_targets(
        &self,
        source: Option<EntityId>,
        area: &Aabb,
    ) -> Vec<HitEntity> {
        let mut out: Vec<HitEntity> = self
            .entities
            .store
            .iter()
            .filter(|e| Some(e.base.id) != source && !e.base.is_removed())
            .filter(|e| !self.entities.dragons.contains_key(&e.base.id))
            .filter(|e| e.base.bounding_box().intersects(area))
            .filter(|e| e.is_simulated())
            .map(|e| HitEntity {
                id: e.base.id,
                player: None,
                position: e.base.position,
                eye: Vec3::new(e.base.position.x, e.base.eye_y(), e.base.position.z),
                bounds: e.base.bounding_box(),
                living: e.living().is_some(),
                part: None,
            })
            .collect();
        out.extend(self.dragon_explosion_parts(source, area));
        out.extend(
            self.players
                .values()
                .filter(|p| !p.vitals.is_dead())
                .filter_map(|p| {
                    let bounds = Aabb::of_size(p.position, 0.6, p.height() as f32);
                    bounds.intersects(area).then(|| HitEntity {
                        id: p.entity_id,
                        player: Some(p.id),
                        position: p.position,
                        eye: p.eye_position(),
                        bounds,
                        living: true,
                        part: None,
                    })
                }),
        );
        out
    }

    fn hurt_by_explosion(
        &mut self,
        source: Option<EntityId>,
        damage: &DamageSource,
        center: Vec3,
        radius: f32,
    ) -> Vec<(PlayerId, Vec3)> {
        if radius < 1.0e-5 {
            return Vec::new();
        }
        let outer = self.impacts.knocked.replace(Vec::new());
        let area = explosion::hurt_area(center, radius);
        for target in self.explosion_targets(source, &area) {
            let distance =
                target.position.distance_squared(center).sqrt() / f64::from(radius * 2.0);
            if distance > 1.0 {
                continue;
            }
            let exposure = explosion::seen_percent(&self.world.chunks, center, &target.bounds);
            let amount = explosion::entity_damage(radius, center, target.position, exposure);
            let orb = target.player.is_none()
                && self
                    .entities
                    .store
                    .get(target.id)
                    .is_some_and(|e| matches!(e.state, EntityState::Orb(_)));
            if orb {
                self.damage_orb(target.id, amount);
            }
            let resistance = self
                .entities
                .store
                .get_mut(target.id)
                .and_then(|e| e.living_mut())
                .map_or(0.0, |living| {
                    living.attribute(EXPLOSION_KNOCKBACK_RESISTANCE)
                });
            let knockback = explosion::knockback(
                radius,
                center,
                target.position,
                target.eye,
                exposure,
                resistance,
            );
            let then = knockback.map_or(AfterHurt::Nothing, |knockback| {
                AfterHurt::Blast(BlastFollow {
                    target: target.id,
                    player: target.player,
                    knockback,
                    source,
                })
            });
            if orb {
                if let AfterHurt::Blast(follow) = then {
                    self.blast_knockback(follow);
                }
                continue;
            }
            if let Some(part) = target.part {
                self.dragon_note_part(target.id, part);
            }
            let crystal = self
                .entities
                .store
                .get(target.id)
                .is_some_and(|e| e.base.kind == Kind::EndCrystal);
            let victim = match target.player {
                Some(player) => Struck::Player(player),
                None if crystal => Struck::Crystal(target.id),
                None if target.living => Struck::Mob(target.id),
                None => Struck::Item(target.id),
            };
            self.strike(None, victim, damage, amount, then);
        }
        std::mem::replace(&mut self.impacts.knocked, outer).unwrap_or_default()
    }

    pub(crate) fn blast_knockback(&mut self, follow: BlastFollow) {
        let living = follow.player.is_some()
            || self
                .entities
                .store
                .get(follow.target)
                .is_some_and(|entity| entity.living().is_some());
        if living {
            let holder = follow
                .player
                .map_or(Holder::Mob(follow.target), Holder::Player);
            self.blast_push(holder, follow.knockback, follow.source);
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(follow.target) {
            entity.base.velocity = entity.base.velocity + follow.knockback;
            entity.base.needs_sync = true;
        }
    }

    fn damage_orb(&mut self, id: EntityId, amount: f32) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let destroyed = match &mut entity.state {
            EntityState::Orb(state) => {
                state.health -= amount.ceil() as i32;
                state.health <= 0
            }
            _ => false,
        };
        if destroyed {
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
        }
    }

    pub(crate) fn blow_up_blocks(
        &mut self,
        blocks: &[BlockPos],
        source: Option<EntityId>,
        decay: Option<f32>,
    ) {
        let this = source
            .and_then(|id| self.entities.store.get(id))
            .map(|e| LootEntity::of(e.base.kind));
        let harmful = source
            .and_then(|id| self.entities.store.get(id))
            .is_some_and(|e| {
                matches!(
                    e.base.kind,
                    Kind::Tnt
                        | Kind::Creeper
                        | Kind::WitherSkull
                        | Kind::Wither
                        | Kind::TntMinecart
                )
            });
        let mut stacks: Vec<(BlockPos, ItemStack)> = Vec::new();
        for pos in blocks {
            let Some(state) = self.world.chunks.block(*pos) else {
                continue;
            };
            if state.is_air() {
                continue;
            }
            let hive = self.hive_snapshot(*pos);
            self.spawn_after_break(*pos, state, None);
            let context = LootContext {
                explosion_radius: decay,
                ..LootContext::block(state, None, this.as_ref())
            };
            let drops = loot::block_drops(&context, &mut self.rng);
            for drop in drops {
                merge_stack(&mut stacks, *pos, drop);
            }
            self.set_block(*pos, BlockStateId::AIR);
            self.hive_exploded(*pos, state, hive, harmful);
        }
        if !self.config.rules.block_drops {
            return;
        }
        for (pos, stack) in stacks {
            let x = f64::from(pos.x) + 0.5 + self.rng.random_range(-POP_SPREAD..=POP_SPREAD);
            let y =
                f64::from(pos.y) + 0.5 + self.rng.random_range(-POP_SPREAD..=POP_SPREAD) - 0.125;
            let z = f64::from(pos.z) + 0.5 + self.rng.random_range(-POP_SPREAD..=POP_SPREAD);
            let velocity = Vec3::new(
                self.rng.random::<f64>() * 0.2 - 0.1,
                0.2,
                self.rng.random::<f64>() * 0.2 - 0.1,
            );
            self.spawn_item_entity(Vec3::new(x, y, z), velocity, 10, stack);
        }
    }

    pub(crate) fn charged_creeper_drops(&mut self, killer: Option<EntityId>, victim: EntityId) {
        let Some(killer) = killer else {
            return;
        };
        let Some(entity) = self.entities.store.get(killer) else {
            return;
        };
        let powered = entity.base.kind == Kind::Creeper && species::is_powered(&entity.base);
        let dropped = matches!(
            entity.living().and_then(|l| l.mob.as_ref()).map(|m| m.species),
            Some(Species::Creeper(c)) if c.dropped_skulls
        );
        if !powered || dropped || !self.config.rules.mob_drops {
            return;
        }
        let Some(table) = rspigot_data::loot::table("minecraft:charged_creeper/root") else {
            return;
        };
        let Some(victim_entity) = self.entities.store.get(victim) else {
            return;
        };
        let position = victim_entity.base.position;
        let facts = crate::mobs::loot_facts_of(victim_entity);
        let context = LootContext {
            this: Some(&facts),
            ..LootContext::default()
        };
        let drops = loot::table_drops(table, &context, &mut self.rng);
        if drops.is_empty() {
            return;
        }
        for stack in drops {
            self.spawn_at_location(position, 0.0, stack);
        }
        if let Some(Species::Creeper(creeper)) = self
            .entities
            .store
            .get_mut(killer)
            .and_then(|e| e.living_mut())
            .and_then(|l| l.mob.as_mut())
            .map(|m| &mut m.species)
        {
            creeper.dropped_skulls = true;
        }
    }

    pub fn ground_item_count(&self) -> usize {
        self.entities
            .store
            .iter()
            .filter(|e| item::stack(&e.base).is_some())
            .count()
    }
}

pub(crate) fn fire_state_on(below: BlockStateId) -> BlockStateId {
    let soul = below.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", SOUL_FIRE_BASE)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    });
    if soul {
        BlockKind::SoulFire.default_state()
    } else {
        BlockKind::Fire.default_state()
    }
}

fn merge_stack(stacks: &mut Vec<(BlockPos, ItemStack)>, pos: BlockPos, stack: ItemStack) {
    let mut remaining = u32::from(stack.count.get());
    for (_, existing) in stacks.iter_mut() {
        if remaining == 0 {
            return;
        }
        if !same_item_same_components(existing, &stack) {
            continue;
        }
        let room = MAX_MERGED_STACK.saturating_sub(u32::from(existing.count.get()));
        let moved = room.min(remaining);
        if moved > 0 {
            existing.count =
                std::num::NonZeroU8::new((u32::from(existing.count.get()) + moved) as u8)
                    .unwrap_or(existing.count);
            remaining -= moved;
        }
    }
    if let Some(count) = std::num::NonZeroU8::new(remaining as u8) {
        stacks.push((pos, ItemStack { count, ..stack }));
    }
}
