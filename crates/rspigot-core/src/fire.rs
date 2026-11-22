use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::fire::{self, InsideEffect};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::health::DamageSource;
use crate::plugins::{AfterHurt, BurnFollow, Combuster, Holder, Struck};
use crate::ports::ClientLink;
use crate::server::Server;

const EXTINGUISH_SOUND: &str = "minecraft:entity.generic.extinguish_fire";
const BURN_SOUND: &str = "minecraft:entity.generic.burn";
const PLAYER_FIRE_TICKS: (i32, i32) = (1, 3);

impl<L: ClientLink> Server<L> {
    pub(crate) fn apply_fire_inside(&mut self, id: EntityId) {
        let chunks = &self.world.chunks;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let was_on_fire = fire::is_on_fire(&entity.base);
        let before = entity.base.remaining_fire_ticks;
        let effects = fire::inside_effects(&mut entity.base, chunks);
        for effect in effects {
            let alive = self.entities.store.get(id).is_some_and(|e| {
                !e.base.is_removed()
                    && e.living().is_none_or(|l| {
                        !rspigot_domain::entity::living::is_dead_or_dying(&e.base, l)
                    })
            });
            if !alive {
                break;
            }
            self.apply_inside_effect(id, effect);
        }
        self.apply_wall_and_floor(id);
        let in_rain = self
            .entities
            .store
            .get(id)
            .is_some_and(|e| self.is_in_rain(&e.base));
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if in_rain {
            fire::clear_fire(&mut entity.base);
        }
        let on_fire = fire::is_on_fire(&entity.base);
        let increased = entity.base.remaining_fire_ticks > before;
        if !on_fire && !increased {
            entity.base.remaining_fire_ticks = 0;
        }
        if was_on_fire && !on_fire {
            self.extinguish_sound(id);
        }
    }

    fn apply_wall_and_floor(&mut self, id: EntityId) {
        let chunks = &self.world.chunks;
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_removed() || entity.living().is_none() {
            return;
        }
        let in_wall = fire::is_in_wall(&entity.base, chunks);
        let hot_floor = fire::is_hot_floor(&entity.base, chunks);
        if in_wall {
            self.hurt_entity(
                id,
                &DamageSource::new(DamageType::InWall),
                fire::IN_WALL_DAMAGE,
            );
        }
        if hot_floor {
            self.hurt_entity(
                id,
                &DamageSource::new(DamageType::HotFloor),
                fire::HOT_FLOOR_DAMAGE,
            );
        }
    }

    fn extinguish_sound(&mut self, id: EntityId) {
        let pitch = 1.6 + (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.4;
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        let source = self.entity_source(id);
        self.play_sound(None, position, EXTINGUISH_SOUND, source, 0.7, pitch);
    }

    fn apply_inside_effect(&mut self, id: EntityId, effect: InsideEffect) {
        match effect {
            InsideEffect::Freeze => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    entity.base.in_powder_snow = true;
                }
            }
            InsideEffect::ClearFreeze => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    entity.base.set_ticks_frozen(0);
                }
            }
            InsideEffect::FireIgnite => {
                let ignite = self.entities.store.get_mut(id).and_then(|e| {
                    fire::fire_ignite(&mut e.base, false, || 0).then(|| e.base.position.block())
                });
                if let Some(block) = ignite {
                    self.combust(
                        Holder::Mob(id),
                        fire::FIRE_SECONDS,
                        Combuster::Block(Some(block)),
                    );
                }
            }
            InsideEffect::InFire(damage) => {
                self.hurt_inside(id, &DamageSource::new(DamageType::InFire), damage);
            }
            InsideEffect::LavaIgnite(lava) => {
                let Some(entity) = self.entities.store.get(id) else {
                    return;
                };
                if entity.base.kind.fire_immune() || self.item_resists_fire(id) {
                    return;
                }
                if entity.living().is_some() && entity.base.remaining_fire_ticks <= 0 {
                    self.combust(
                        Holder::Mob(id),
                        fire::LAVA_SECONDS,
                        Combuster::Block(Some(lava)),
                    );
                } else {
                    self.ignite_entity(id, fire::LAVA_SECONDS);
                }
            }
            InsideEffect::LavaHurt(pos) => self.lava_hurt(id, pos),
            InsideEffect::MeltPowderSnow(pos) => {
                let melt = self
                    .entities
                    .store
                    .get(id)
                    .is_some_and(|e| fire::is_on_fire(&e.base))
                    && self.config.rules.mob_griefing;
                if melt {
                    self.destroy_block_silently(pos);
                }
            }
            InsideEffect::Cauldron(pos) => self.cauldron_extinguishes(id, pos),
            InsideEffect::Extinguish => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    fire::clear_fire(&mut entity.base);
                }
            }
            InsideEffect::Hurt(kind, amount) => {
                self.hurt_inside(id, &DamageSource::new(kind), amount);
            }
        }
    }

    fn lava_source(&self, pos: BlockPos) -> DamageSource {
        let state = self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR);
        DamageSource::from_block(DamageType::Lava, Some((pos, state)))
    }

    fn hurt_inside(&mut self, id: EntityId, source: &DamageSource, amount: f32) -> bool {
        if self.is_item_entity(id) {
            self.hurt_item(id, source, amount)
        } else {
            self.hurt_entity(id, source, amount)
        }
    }

    fn item_resists_fire(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_some_and(|entity| {
            rspigot_domain::entity::item::stack(&entity.base)
                .is_some_and(|stack| crate::items::stack_resists(stack, DamageType::InFire))
        })
    }

    fn lava_hurt(&mut self, id: EntityId, pos: BlockPos) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.kind.fire_immune() {
            return;
        }
        if self.is_item_entity(id) {
            let source = self.lava_source(pos);
            self.hurt_item(id, &source, fire::LAVA_DAMAGE);
            return;
        }
        let silent = entity.base.is_silent();
        let position = entity.base.position;
        let then = if silent {
            AfterHurt::Nothing
        } else {
            AfterHurt::Burn(BurnFollow {
                position,
                player: false,
                entity: id,
            })
        };
        let victim = self.struck(id);
        let source = self.lava_source(pos);
        self.strike(None, victim, &source, fire::LAVA_DAMAGE, then);
    }

    pub(crate) fn burn_sound(&mut self, follow: BurnFollow) {
        let pitch = 2.0 + self.rng.random::<f32>() * 0.4;
        let source = if follow.player {
            rspigot_entities::SoundSource::Players
        } else {
            self.entity_source(follow.entity)
        };
        self.play_sound(None, follow.position, BURN_SOUND, source, 0.4, pitch);
    }

    pub(crate) fn ignite_player(&mut self, id: PlayerId, seconds: f32) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let ticks = fire::seconds_to_ticks(seconds);
        if player.vitals.fire_ticks < ticks {
            player.vitals.fire_ticks = ticks;
            if player.abilities().invulnerable {
                player.vitals.fire_ticks = player.vitals.fire_ticks.min(1);
            }
        }
        self.broadcast_metadata(id);
    }

    pub(crate) fn tick_player_fire(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() || player.game_mode == GameMode::Spectator {
            return;
        }
        let was_burning = player.vitals.fire_ticks > 0;
        let mut base = EntityBase::new(player.entity_id, player.profile.id.0, Kind::Player);
        base.position = player.position;
        base.old_position = player.position - player.known_movement;
        base.on_ground = player.on_ground;
        let sneaking = player.sneaking;
        base.remaining_fire_ticks = player.vitals.fire_ticks;
        base.dimensions.height = player.height() as f32;
        rspigot_domain::entity::fluid::update(&mut base, &self.world.chunks);
        let burn = fire::base_tick(&mut base).burn;
        let before = base.remaining_fire_ticks;
        let effects = fire::inside_effects(&mut base, &self.world.chunks);
        if let Some(player) = self.players.get_mut(&id) {
            player.vitals.fire_ticks = base.remaining_fire_ticks;
        }
        if burn {
            self.hurt(
                id,
                &DamageSource::new(DamageType::OnFire),
                fire::ON_FIRE_DAMAGE,
            );
        }
        for effect in effects {
            if self.players.get(&id).is_none_or(|p| p.vitals.is_dead()) {
                break;
            }
            self.apply_player_effect(id, effect);
        }
        if fire::is_in_wall(&base, &self.world.chunks)
            && self.players.get(&id).is_some_and(|p| !p.vitals.is_dead())
        {
            self.hurt(
                id,
                &DamageSource::new(DamageType::InWall),
                fire::IN_WALL_DAMAGE,
            );
        }
        if !sneaking
            && fire::is_hot_floor(&base, &self.world.chunks)
            && self.players.get(&id).is_some_and(|p| !p.vitals.is_dead())
        {
            self.hurt(
                id,
                &DamageSource::new(DamageType::HotFloor),
                fire::HOT_FLOOR_DAMAGE,
            );
        }
        let in_rain = self.is_in_rain(&base);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if in_rain || base.fluid.in_water {
            player.vitals.fire_ticks = player.vitals.fire_ticks.min(0);
        }
        let on_fire = player.vitals.fire_ticks > 0;
        if !on_fire && player.vitals.fire_ticks <= before {
            player.vitals.fire_ticks = -fire::PLAYER_FIRE_IMMUNE_TICKS;
        }
        if was_burning != on_fire {
            if was_burning {
                let position = player.position;
                let pitch = 1.6 + (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.4;
                self.play_sound(
                    None,
                    position,
                    EXTINGUISH_SOUND,
                    rspigot_entities::SoundSource::Players,
                    0.7,
                    pitch,
                );
            }
            self.broadcast_metadata(id);
        }
    }

    fn apply_player_effect(&mut self, id: PlayerId, effect: InsideEffect) {
        match effect {
            InsideEffect::FireIgnite => {
                let extra = self
                    .rng
                    .random_range(PLAYER_FIRE_TICKS.0..PLAYER_FIRE_TICKS.1);
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                if player.vitals.fire_ticks < 0 {
                    player.vitals.fire_ticks += 1;
                } else {
                    player.vitals.fire_ticks += extra;
                }
                if player.vitals.fire_ticks >= 0 {
                    let block = player.position.block();
                    self.combust(
                        Holder::Player(id),
                        fire::FIRE_SECONDS,
                        Combuster::Block(Some(block)),
                    );
                }
            }
            InsideEffect::InFire(damage) => {
                self.hurt(id, &DamageSource::new(DamageType::InFire), damage);
            }
            InsideEffect::LavaIgnite(lava) => {
                if self
                    .players
                    .get(&id)
                    .is_some_and(|player| player.vitals.fire_ticks <= 0)
                {
                    self.combust(
                        Holder::Player(id),
                        fire::LAVA_SECONDS,
                        Combuster::Block(Some(lava)),
                    );
                } else {
                    self.ignite_player(id, fire::LAVA_SECONDS);
                }
            }
            InsideEffect::LavaHurt(pos) => {
                let Some((position, entity)) =
                    self.players.get(&id).map(|p| (p.position, p.entity_id))
                else {
                    return;
                };
                let follow = BurnFollow {
                    position,
                    player: true,
                    entity,
                };
                let source = self.lava_source(pos);
                self.strike(
                    None,
                    Struck::Player(id),
                    &source,
                    fire::LAVA_DAMAGE,
                    AfterHurt::Burn(follow),
                );
            }
            InsideEffect::MeltPowderSnow(pos) => {
                if self
                    .players
                    .get(&id)
                    .is_some_and(|p| p.vitals.fire_ticks > 0)
                {
                    self.destroy_block_silently(pos);
                }
            }
            InsideEffect::Extinguish => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.vitals.fire_ticks = player.vitals.fire_ticks.min(0);
                }
            }
            InsideEffect::Cauldron(pos) => {
                if let Some(entity) = self.players.get(&id).map(|player| player.entity_id) {
                    self.cauldron_extinguishes(entity, pos);
                }
            }
            InsideEffect::Hurt(kind, amount) => {
                self.hurt(id, &DamageSource::new(kind), amount);
            }
            InsideEffect::Freeze | InsideEffect::ClearFreeze => {}
        }
    }

    pub fn entity_fire_ticks(&self, id: EntityId) -> Option<i32> {
        self.entities
            .store
            .get(id)
            .map(|e| e.base.remaining_fire_ticks)
    }

    pub fn player_fire_ticks(&self, id: PlayerId) -> Option<i32> {
        self.players.get(&id).map(|p| p.vitals.fire_ticks)
    }

    pub(crate) fn entity_position(&self, id: EntityId) -> Option<Vec3> {
        self.entities.store.get(id).map(|e| e.base.position)
    }
}
