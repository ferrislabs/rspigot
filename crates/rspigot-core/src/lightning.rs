use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, lightning};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::EntityKind as Kind;

use crate::explosion::fire_state_on;
use crate::health::DamageSource;
use crate::plugins::events::{
    BlockIgnite, BlockIgniteCause, CreeperPower, CreeperPowerCause, EntityRemoveCause,
};
use crate::plugins::steps::{EmpowerStep, Follow, Holder, ScorchStep};
use crate::plugins::{Combuster, Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_lightning(&mut self, id: EntityId) {
        let gap = rand::Rng::random_range(&mut self.rng, 0..lightning::FLASH_GAP);
        let Some(Entity {
            base,
            state: EntityState::Lightning(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        let visual = state.visual_only;
        let step = state.tick(gap);
        if !visual {
            if step.strike && matches!(self.world.difficulty, Difficulty::Normal | Difficulty::Hard)
            {
                self.spawn_lightning_fire(id, lightning::FIRE_SPREAD);
            }
            if let Some(extra) = step.fire {
                self.spawn_lightning_fire(id, extra);
            }
        }
        if step.strike {
            self.lightning_strikes_rod(id);
        }
        if step.hit {
            self.lightning_hit_area(id);
        }
        if step.discard {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }

    fn lightning_strikes_rod(&mut self, bolt: EntityId) {
        let Some(position) = self
            .entities
            .store
            .get(bolt)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let strike = rspigot_domain::Vec3::new(position.x, position.y - 1.0e-6, position.z).block();
        self.lightning_rod_struck(strike);
    }

    fn spawn_lightning_fire(&mut self, bolt: EntityId, extra: i32) {
        let Some(position) = self
            .entities
            .store
            .get(bolt)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let origin = position.block();
        self.scorch(bolt, origin);
        for _ in 0..extra {
            let dx = rand::Rng::random_range(&mut self.rng, 0..3) - 1;
            let dy = rand::Rng::random_range(&mut self.rng, 0..3) - 1;
            let dz = rand::Rng::random_range(&mut self.rng, 0..3) - 1;
            self.scorch(bolt, origin.offset(dx, dy, dz));
        }
    }

    fn scorch(&mut self, bolt: EntityId, pos: BlockPos) {
        let free = self
            .world
            .chunks
            .block(pos)
            .is_some_and(BlockStateId::is_air);
        let below = self
            .world
            .chunks
            .block(pos.offset(0, -1, 0))
            .unwrap_or_default();
        if !free || below.collision_shape().is_empty() {
            return;
        }
        let state = fire_state_on(below);
        let Some(view) = self.entity_view(bolt) else {
            return;
        };
        let lane = self.lane_for(None, bolt);
        self.fire(
            lane,
            Continuation::Scorch(ScorchStep { pos, state }),
            move |_| {
                Some(EventPayload::BlockIgnite(BlockIgnite {
                    block: pos,
                    cause: BlockIgniteCause::Lightning,
                    igniting_block: None,
                    igniting_entity: Some(view),
                    player: None,
                }))
            },
        );
    }

    pub(crate) fn finish_scorch(&mut self, step: ScorchStep, cancelled: bool) {
        let free = self
            .world
            .chunks
            .block(step.pos)
            .is_some_and(BlockStateId::is_air);
        if !cancelled && free {
            self.set_block(step.pos, step.state);
        }
    }

    fn lightning_hit_area(&mut self, bolt: EntityId) {
        let Some(position) = self
            .entities
            .store
            .get(bolt)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let radius = lightning::HIT_RADIUS;
        let area = Aabb::new(
            position.x - radius,
            position.y - radius,
            position.z - radius,
            position.x + radius,
            position.y + lightning::HIT_HEIGHT + radius,
            position.z + radius,
        );
        for victim in self.living_in(&area) {
            self.thunder_hit(bolt, victim);
        }
    }

    fn thunder_hit(&mut self, bolt: EntityId, victim: EntityId) {
        let Some(kind) = self.victim_kind(victim) else {
            return;
        };
        if kind == Kind::Pig && self.world.difficulty != Difficulty::Peaceful {
            self.convert_with(victim, Kind::ZombifiedPiglin, None, Follow::Zap { bolt });
            return;
        }
        self.lightning_burn(bolt, victim, kind);
        if kind == Kind::Creeper {
            self.ask_empower(bolt, victim);
        }
    }

    fn victim_kind(&self, victim: EntityId) -> Option<Kind> {
        if self.player_of_entity(victim).is_some() {
            return Some(Kind::Player);
        }
        self.kind_of(victim)
    }

    fn lightning_burn(&mut self, bolt: EntityId, victim: EntityId, kind: Kind) {
        if self.fire_ticks_of(victim) > 0 {
            let ticks = self.fire_ticks_of(victim);
            self.restore_fire_ticks(victim, ticks + 1);
        } else if let Some(source) = self.entity_view(bolt) {
            let holder = self
                .player_of_entity(victim)
                .map_or(Holder::Mob(victim), Holder::Player);
            self.combust(holder, lightning::IGNITE_SECONDS, Combuster::Entity(source));
        }
        if kind.fire_immune() {
            return;
        }
        let Some(direct) = self.mob_source(bolt) else {
            return;
        };
        let source = DamageSource {
            kind: DamageType::LightningBolt,
            causing: None,
            direct: Some(direct),
            position: None,
            knockback: None,
            direct_block: None,
        };
        self.hurt_entity(victim, &source, lightning::DAMAGE);
    }

    fn ask_empower(&mut self, bolt: EntityId, creeper: EntityId) {
        let lane = self.lane_for(None, creeper);
        self.fire(
            lane,
            Continuation::Empower(EmpowerStep { creeper }),
            move |server| {
                let entity = server.entity_view(creeper)?;
                Some(EventPayload::CreeperPower(CreeperPower {
                    cause: CreeperPowerCause::Lightning,
                    entity_type: entity.kind.clone(),
                    entity,
                    lightning: server.entity_view(bolt),
                }))
            },
        );
    }

    pub(crate) fn finish_empower(&mut self, step: EmpowerStep, cancelled: bool) {
        if cancelled || !self.living_entity_alive(step.creeper) {
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(step.creeper) {
            species::set_powered(&mut entity.base, true);
        }
    }
}
