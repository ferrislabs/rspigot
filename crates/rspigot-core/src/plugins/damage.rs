use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::floored;
use rspigot_domain::entity::{EntityState, RemovalReason};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId};

use crate::health::DamageSource;
use crate::plugins::block::snapshot_of;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    EntityDamage, EntityDamageByBlock, EntityDamageByEntity, EntityDamageCause, EntityRemoveCause,
};
use crate::plugins::steps::{AfterHurt, HurtStep, Outcome, Struck};
use crate::plugins::{EventKind, EventPatch, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Default)]
pub(crate) struct Impacts {
    last: Option<bool>,
    pub(crate) knocked: Option<Vec<(PlayerId, rspigot_domain::Vec3)>>,
}

enum Blame {
    Block(Option<(BlockPos, BlockStateId)>),
    Entity,
    Nobody,
}

fn blame_of(source: &DamageSource) -> (EntityDamageCause, Blame) {
    let kind = source.kind;
    let damager = source.direct.is_some() || source.causing.is_some();
    match kind {
        DamageType::Explosion | DamageType::PlayerExplosion if damager => {
            (EntityDamageCause::EntityExplosion, Blame::Entity)
        }
        DamageType::Explosion | DamageType::PlayerExplosion => {
            (EntityDamageCause::BlockExplosion, Blame::Block(None))
        }
        DamageType::OutOfWorld => (EntityDamageCause::Void, Blame::Block(None)),
        DamageType::Lava => (EntityDamageCause::Lava, Blame::Block(source.direct_block)),
        _ if source.direct_block.is_some() => (
            match kind {
                DamageType::InFire => EntityDamageCause::Fire,
                DamageType::Magic => EntityDamageCause::Magic,
                _ => EntityDamageCause::Contact,
            },
            Blame::Block(source.direct_block),
        ),
        _ if damager => (
            match kind {
                DamageType::Arrow | DamageType::Thrown => EntityDamageCause::Projectile,
                DamageType::FallingAnvil
                | DamageType::FallingBlock
                | DamageType::FallingStalactite => EntityDamageCause::FallingBlock,
                DamageType::Fall => EntityDamageCause::Fall,
                DamageType::Magic => EntityDamageCause::Magic,
                _ => EntityDamageCause::EntityAttack,
            },
            Blame::Entity,
        ),
        _ => (
            match kind {
                DamageType::Fall => EntityDamageCause::Fall,
                DamageType::InFire => EntityDamageCause::Fire,
                DamageType::Starve => EntityDamageCause::Starvation,
                DamageType::Drown => EntityDamageCause::Drowning,
                DamageType::Wither => EntityDamageCause::Wither,
                DamageType::OnFire => EntityDamageCause::FireTick,
                DamageType::Magic => EntityDamageCause::Magic,
                DamageType::GenericKill => EntityDamageCause::Kill,
                _ => EntityDamageCause::Custom,
            },
            Blame::Nobody,
        ),
    }
}

fn damage_kind(source: &DamageSource) -> EventKind {
    match blame_of(source).1 {
        Blame::Block(_) => EventKind::EntityDamageByBlock,
        Blame::Entity => EventKind::EntityDamageByEntity,
        Blame::Nobody => EventKind::EntityDamage,
    }
}

pub(crate) fn settled_damage(patch: Option<EventPatch>) -> Option<f64> {
    match patch {
        Some(EventPatch::EntityDamage(patch)) => patch.damage,
        Some(EventPatch::EntityDamageByEntity(patch)) => patch.damage,
        Some(EventPatch::EntityDamageByBlock(patch)) => patch.damage,
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn struck_entity(&self, victim: Struck) -> Option<EntityId> {
        match victim {
            Struck::Player(id) => self.players.get(&id).map(|player| player.entity_id),
            Struck::Mob(id) | Struck::Item(id) => Some(id),
        }
    }

    pub(crate) fn struck(&self, id: EntityId) -> Struck {
        self.player_of_entity(id)
            .map_or(Struck::Mob(id), Struck::Player)
    }

    pub(crate) fn strike(
        &mut self,
        cause: Option<PlayerId>,
        victim: Struck,
        source: &DamageSource,
        amount: f32,
        then: AfterHurt,
    ) -> bool {
        let asked = match victim {
            Struck::Player(id) => self.ask_hurt_player(id, source, amount),
            Struck::Mob(id) => self.ask_hurt_mob(id, source, amount),
            Struck::Item(id) => self.ask_hurt_item(id, amount),
        };
        let (Some(amount), Some(subject)) = (asked, self.struck_entity(victim)) else {
            self.after_hurt(then, Outcome::Missed);
            return false;
        };
        let kind = damage_kind(source);
        if !self.listened(kind) {
            return self.land_hurt(victim, source, amount, then, false);
        }
        self.impacts.last = None;
        let lane = self.lane_for(cause, subject);
        let step = HurtStep {
            kind,
            victim,
            source: source.clone(),
            amount,
            then,
        };
        self.fire(lane, Continuation::Hurt(Box::new(step)), move |server| {
            server.hurt_payload(victim, source, amount)
        });
        self.impacts.last.take().unwrap_or(true)
    }

    pub(crate) fn strike_behind(
        &mut self,
        lane: Lane,
        victim: Struck,
        source: DamageSource,
        amount: f32,
        then: AfterHurt,
    ) {
        let step = HurtStep {
            kind: damage_kind(&source),
            victim,
            source,
            amount,
            then,
        };
        self.fire_deferred(lane, Continuation::Strike(Box::new(step)));
    }

    pub(crate) fn strike_at_head(&mut self, step: HurtStep) {
        let HurtStep {
            victim,
            source,
            amount,
            then,
            ..
        } = step;
        self.strike(None, victim, &source, amount, then);
    }

    fn hurt_payload(
        &self,
        victim: Struck,
        source: &DamageSource,
        amount: f32,
    ) -> Option<EventPayload> {
        let entity = self.subject_view(self.struck_entity(victim)?)?;
        let entity_type = entity.kind.clone();
        let final_damage = f64::from(match victim {
            Struck::Player(id) => self.player_final_damage(id, source.kind, amount),
            Struck::Mob(id) => self.mob_final_damage(id, source.kind, amount),
            Struck::Item(_) => amount,
        });
        let damage = f64::from(amount);
        let damage_source = self.damage_view(source);
        let (cause, blame) = blame_of(source);
        Some(match blame {
            Blame::Block(block) => EventPayload::EntityDamageByBlock(EntityDamageByBlock {
                cause,
                damage,
                damage_source,
                damager: block.map(|(pos, _)| pos),
                damager_block_state: block.map(|(pos, state)| snapshot_of(pos, state)),
                entity,
                entity_type,
                final_damage,
            }),
            Blame::Entity => EventPayload::EntityDamageByEntity(EntityDamageByEntity {
                cause,
                damage,
                damager: damage_source
                    .direct
                    .clone()
                    .or_else(|| damage_source.causing.clone())?,
                damage_source,
                entity,
                entity_type,
                final_damage,
            }),
            Blame::Nobody => EventPayload::EntityDamage(EntityDamage {
                cause,
                damage,
                damage_source,
                entity,
                entity_type,
                final_damage,
            }),
        })
    }

    pub(super) fn finish_hurt(&mut self, step: HurtStep, cancelled: bool, damage: Option<f64>) {
        let HurtStep {
            victim,
            source,
            amount,
            then,
            ..
        } = step;
        let amount = damage.map_or(amount, |damage| floored(damage as f32));
        let hurt = self.land_hurt(victim, &source, amount, then, cancelled);
        self.impacts.last = Some(hurt);
    }

    fn land_hurt(
        &mut self,
        victim: Struck,
        source: &DamageSource,
        amount: f32,
        then: AfterHurt,
        cancelled: bool,
    ) -> bool {
        let outcome = if cancelled {
            Outcome::Cancelled
        } else if self.abandoned(&then) {
            return false;
        } else {
            let hurt = match victim {
                Struck::Player(id) => self.apply_hurt_player(id, source, amount),
                Struck::Mob(id) => self.apply_hurt_mob(id, source, amount),
                Struck::Item(id) => self.apply_hurt_item(id, amount),
            };
            if hurt { Outcome::Hurt } else { Outcome::Missed }
        };
        if let Struck::Mob(id) = victim {
            self.kind_hurt_settled(id, source);
        }
        self.after_hurt(then, outcome);
        outcome.hurt()
    }

    fn abandoned(&self, then: &AfterHurt) -> bool {
        let attacker = match then {
            AfterHurt::Attack(follow) => follow.attacker,
            AfterHurt::Sweep(follow) => follow.attacker,
            AfterHurt::MobAttack(follow) => return !self.living_entity_alive(follow.attacker),
            _ => return false,
        };
        self.players
            .get(&attacker)
            .is_none_or(|player| player.vitals.is_dead())
    }

    fn after_hurt(&mut self, then: AfterHurt, outcome: Outcome) {
        match then {
            AfterHurt::Nothing => {}
            AfterHurt::Attack(follow) => self.finish_attack(*follow, outcome.hurt()),
            AfterHurt::Sweep(follow) => self.finish_sweep(follow, outcome.hurt()),
            AfterHurt::MobAttack(follow) => self.finish_mob_attack(follow, outcome.hurt()),
            AfterHurt::Arrow(follow) => self.finish_arrow_hit(follow, outcome.hurt()),
            AfterHurt::Thrown(follow) => self.finish_thrown_hit(follow),
            AfterHurt::Burn(follow) => {
                if outcome.hurt() {
                    self.burn_sound(follow);
                }
            }
            AfterHurt::Blast(follow) => {
                if outcome != Outcome::Cancelled {
                    self.blast_knockback(follow);
                }
            }
        }
    }

    fn ask_hurt_item(&self, id: EntityId, amount: f32) -> Option<f32> {
        let entity = self.entities.store.get(id)?;
        (matches!(entity.state, EntityState::Item(_))
            && !entity.base.is_removed()
            && self.config.rules.mob_griefing)
            .then_some(floored(amount))
    }

    fn apply_hurt_item(&mut self, id: EntityId, amount: f32) -> bool {
        let Some(EntityState::Item(state)) = self.entities.store.get_mut(id).map(|e| &mut e.state)
        else {
            return false;
        };
        state.health -= amount.ceil() as i32;
        if state.health <= 0 {
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
        }
        true
    }
}
