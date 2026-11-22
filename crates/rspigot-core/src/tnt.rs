use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::{Entity, EntityState, tnt};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use uuid::Uuid;

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::plugins::events::{
    CreatureSpawnReason, ExplosionPrime, ExplosionPrimePatch, TNTPrime, TNTPrimeCause,
};
use crate::plugins::steps::{Birth, PrimeKind, PrimeStep};
use crate::plugins::{Continuation, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::game_event::GameEvent;

const PRIMED_SOUND: &str = "minecraft:entity.tnt.primed";
const EXPLODED_FUSE_BASE: i32 = 10;
const EXPLODED_FUSE_SPREAD: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Lighter {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) flint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TntPrimeStep {
    pub(crate) pos: BlockPos,
    pub(crate) cause: TNTPrimeCause,
    pub(crate) by_block: Option<BlockPos>,
    pub(crate) by_entity: Option<EntityId>,
    pub(crate) owner: Option<Uuid>,
    pub(crate) remove: bool,
    pub(crate) fuse: i32,
    pub(crate) lighter: Option<Lighter>,
}

impl TntPrimeStep {
    pub(crate) fn of(pos: BlockPos, cause: TNTPrimeCause) -> Self {
        Self {
            pos,
            cause,
            by_block: None,
            by_entity: None,
            owner: None,
            remove: true,
            fuse: tnt::DEFAULT_FUSE,
            lighter: None,
        }
    }

    pub(crate) fn by_block(self, block: BlockPos) -> Self {
        Self {
            by_block: Some(block),
            ..self
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tnt_neighbor_changed(&mut self, pos: BlockPos, source: BlockPos) {
        if self.has_neighbor_signal(pos) {
            self.prime_tnt(TntPrimeStep::of(pos, TNTPrimeCause::Redstone).by_block(source));
        }
    }

    pub(crate) fn prime_tnt(&mut self, step: TntPrimeStep) {
        if !self.config.rules.tnt_explodes {
            return;
        }
        let TntPrimeStep {
            pos,
            cause,
            by_block,
            by_entity,
            ..
        } = step;
        self.fire(
            Lane::Block(pos),
            Continuation::TntPrime(Box::new(step)),
            move |server| {
                Some(EventPayload::TNTPrime(TNTPrime {
                    block: pos,
                    cause,
                    priming_block: by_block,
                    priming_entity: by_entity.and_then(|id| server.subject_view(id)),
                }))
            },
        );
    }

    pub(crate) fn finish_tnt_prime(&mut self, step: TntPrimeStep, cancelled: bool) {
        if cancelled {
            return;
        }
        if step.remove {
            if self.block_or_air(step.pos).kind() != Some(BlockKind::Tnt) {
                return;
            }
            self.set_block(step.pos, BlockStateId::AIR);
        }
        let center = Vec3::new(
            f64::from(step.pos.x) + 0.5,
            f64::from(step.pos.y),
            f64::from(step.pos.z) + 0.5,
        );
        self.spawn_primed_tnt(center, step.fuse, step.owner);
        self.play_sound(None, center, PRIMED_SOUND, SoundSource::Blocks, 1.0, 1.0);
        self.block_game_event(
            GameEvent::PrimeFuse,
            step.pos,
            GameEventContext::of(step.by_entity),
        );
        if let Some(lighter) = step.lighter {
            if lighter.flint {
                self.hurt_held(lighter.player, lighter.hand, 1);
            } else {
                self.consume_held(lighter.player, lighter.hand, 1);
            }
            self.refresh_hands(lighter.player);
        }
    }

    pub(crate) fn tnt_will_destroy(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
        creative: bool,
    ) {
        if creative
            || state.kind() != Some(BlockKind::Tnt)
            || state.property("unstable") != Some("true")
        {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let (entity, owner) = (player.entity_id, player.profile.id.0);
        self.prime_tnt(TntPrimeStep {
            remove: false,
            by_entity: Some(entity),
            owner: Some(owner),
            ..TntPrimeStep::of(pos, TNTPrimeCause::BlockBreak)
        });
    }

    pub(crate) fn arrow_primes_tnt(&mut self, arrow: EntityId, pos: BlockPos, state: BlockStateId) {
        if state.kind() != Some(BlockKind::Tnt) {
            return;
        }
        let owner = match self.entities.store.get(arrow).map(|entity| &entity.state) {
            Some(EntityState::Arrow(state)) => state.owner,
            _ => None,
        };
        self.prime_tnt(TntPrimeStep {
            by_entity: Some(arrow),
            owner,
            ..TntPrimeStep::of(pos, TNTPrimeCause::Projectile)
        });
    }

    pub(crate) fn prime_exploded_tnt(&mut self, pos: BlockPos, igniter: Option<EntityId>) {
        let owner = igniter.and_then(|id| {
            self.players
                .values()
                .find(|player| player.entity_id == id)
                .map(|player| player.profile.id.0)
        });
        let fuse = self.rng.random_range(0..EXPLODED_FUSE_SPREAD) + EXPLODED_FUSE_BASE;
        self.prime_tnt(TntPrimeStep {
            remove: false,
            fuse,
            owner,
            by_entity: igniter,
            ..TntPrimeStep::of(pos, TNTPrimeCause::Explosion)
        });
    }

    pub(crate) fn spawn_primed_tnt(
        &mut self,
        position: Vec3,
        fuse: i32,
        owner: Option<Uuid>,
    ) -> Option<EntityId> {
        let angle = self.rng.random::<f64>() * std::f64::consts::TAU;
        let mut base = self.new_entity_base(Kind::Tnt);
        base.position = position;
        base.old_position = position;
        base.velocity = tnt::ignite_velocity(angle);
        let mut state = tnt::define(&mut base);
        state.owner = owner;
        tnt::set_fuse(&mut base, fuse);
        let id = base.id;
        self.spawn_entity(
            Entity::new(base, EntityState::Tnt(state)),
            Birth::of(CreatureSpawnReason::Default),
        );
        self.entities.store.get(id).map(|_| id)
    }

    pub(crate) fn use_tnt(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(held) = held else {
            return false;
        };
        let flint = held.item == ItemKind::FlintAndSteel.id();
        if clicked.kind() != Some(BlockKind::Tnt)
            || !(flint || held.item == ItemKind::FireCharge.id())
        {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let (entity, owner) = (player.entity_id, player.profile.id.0);
        self.prime_tnt(TntPrimeStep {
            by_entity: Some(entity),
            owner: Some(owner),
            lighter: Some(Lighter {
                player: id,
                hand,
                flint,
            }),
            ..TntPrimeStep::of(pos, TNTPrimeCause::Player)
        });
        true
    }

    pub(crate) fn tick_tnt(&mut self, id: EntityId) {
        let limit = self.config.max_tnt_per_tick;
        if limit > 0 {
            let (stamp, ticked) = &mut self.tnt_ticked;
            if *stamp != self.tick {
                (*stamp, *ticked) = (self.tick, 0);
            }
            *ticked += 1;
            if *ticked > limit {
                return;
            }
        }
        let chunks = &self.world.chunks;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if !matches!(entity.state, EntityState::Tnt(_)) || entity.base.is_removed() {
            return;
        }
        if tnt::tick(&mut entity.base, chunks) {
            self.explode_tnt(id);
        }
    }

    fn explode_tnt(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let EntityState::Tnt(state) = &entity.state else {
            return;
        };
        let radius = state.power;
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        let step = PrimeStep {
            source: id,
            radius,
            fire: false,
            kind: PrimeKind::Tnt,
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

    pub(crate) fn finish_tnt_explosion(
        &mut self,
        step: PrimeStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        let id = step.source;
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_removed() {
            return;
        }
        let center = tnt::explosion_center(&entity.base);
        let owner = match &entity.state {
            EntityState::Tnt(state) => state.owner,
            _ => None,
        };
        if cancelled {
            self.remove_entity(id, rspigot_domain::entity::RemovalReason::Discarded, None);
            return;
        }
        let direct = self.mob_source(id);
        let causing = owner.and_then(|uuid| {
            let player = self
                .players
                .values()
                .find(|player| player.profile.id.0 == uuid)?
                .id;
            self.source_of(player)
        });
        let damage = match (causing, direct) {
            (Some(causing), Some(direct)) => DamageSource {
                kind: DamageType::Explosion,
                causing: Some(causing),
                direct: Some(direct),
                position: None,
                knockback: None,
                direct_block: None,
            },
            (None, Some(direct)) => DamageSource::by(DamageType::Explosion, direct),
            _ => DamageSource::new(DamageType::Explosion),
        };
        let interaction = self.tnt_block_interaction();
        self.explode(
            Some(id),
            &damage,
            center,
            patch.radius.unwrap_or(step.radius),
            interaction,
            patch.fire.unwrap_or(step.fire),
            Some(id),
        );
    }
}
