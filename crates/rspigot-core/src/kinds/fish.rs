use rspigot_ai::{FishFacts, MobView, WaterAction, inflate, is_scary_kind_to_pufferfish};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::species::{Species, fish};
use rspigot_domain::entity::{Entity, EntityState, living};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::commands::Target;
use crate::events::{ClientEvent, GameEvent};
use crate::health::DamageSource;
use crate::mobs::ServerRandom;
use crate::plugins::events::EntityPotionEffectCause;
use crate::plugins::steps::{AfterHurt, MobAttackFollow, Struck};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::random::RandomSource;

const POISON: &str = "minecraft:poison";
const BLOW_UP: &str = "minecraft:entity.puffer_fish.blow_up";
const BLOW_OUT: &str = "minecraft:entity.puffer_fish.blow_out";
const STING: &str = "minecraft:entity.puffer_fish.sting";

fn fish_state(entity: &Entity) -> Option<fish::FishState> {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Fish(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

fn fish_state_mut(entity: &mut Entity) -> Option<&mut fish::FishState> {
    match &mut entity.state {
        EntityState::Living(living) => match living.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Fish(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

fn flop_sound(kind: Kind) -> &'static str {
    match kind {
        Kind::Cod => "minecraft:entity.cod.flop",
        Kind::Salmon => "minecraft:entity.salmon.flop",
        Kind::Pufferfish => "minecraft:entity.puffer_fish.flop",
        _ => "minecraft:entity.tropical_fish.flop",
    }
}

pub(crate) fn fish_facts_of(entity: &Entity) -> (Option<EntityId>, i32) {
    fish_state(entity).map_or((None, 0), |state| (state.leader, state.school_size))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn fish_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(state) = fish_state(entity) else {
            return;
        };
        view.species.fish = FishFacts {
            leader: state.leader,
            school_size: state.school_size,
            inflate_counter: state.inflate_counter,
            deflate_timer: state.deflate_timer,
        };
    }

    pub(crate) fn fish_pre_tick(&mut self, id: EntityId) -> bool {
        if self.kind_of(id) != Some(Kind::Pufferfish) {
            return true;
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return true;
        };
        let alive = entity.living().is_some_and(|state| {
            living::effective_ai(&entity.base, state)
                && !living::is_dead_or_dying(&entity.base, state)
        });
        if !alive || entity.base.is_removed() {
            return true;
        }
        let Some(mut state) = fish_state(entity) else {
            return true;
        };
        let mut puff = fish::puff_state(&entity.base);
        let mut sound = None;
        if state.inflate_counter > 0 {
            if puff == 0 {
                sound = Some(BLOW_UP);
                puff = 1;
            } else if state.inflate_counter > fish::INFLATE_FULL_AFTER && puff == 1 {
                sound = Some(BLOW_UP);
                puff = 2;
            }
            state.inflate_counter += 1;
        } else if puff != 0 {
            if state.deflate_timer > fish::DEFLATE_HALF_AFTER && puff == 2 {
                sound = Some(BLOW_OUT);
                puff = 1;
            } else if state.deflate_timer > fish::DEFLATE_FULL_AFTER && puff == 1 {
                sound = Some(BLOW_OUT);
                puff = 0;
            }
            state.deflate_timer += 1;
        }
        let changed = puff != fish::puff_state(&entity.base);
        if changed {
            fish::set_puff_state(&mut entity.base, puff);
        }
        if let Some(slot) = fish_state_mut(entity) {
            *slot = state;
        }
        if let Some(sound) = sound {
            self.play_mob_voice(id, move |_| Some(sound));
        }
        true
    }

    pub(crate) fn fish_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let base = &mut entity.base;
        if base.fluid.in_water || !base.on_ground || !base.vertical_collision {
            return;
        }
        let kind = base.kind;
        let mut random = ServerRandom(&mut self.rng);
        let x = f64::from((random.next_float() * 2.0 - 1.0) * fish::FLOP_HORIZONTAL_SPREAD);
        let z = f64::from((random.next_float() * 2.0 - 1.0) * fish::FLOP_HORIZONTAL_SPREAD);
        base.velocity = base.velocity + Vec3::new(x, fish::FLOP_VERTICAL_PUSH, z);
        base.on_ground = false;
        base.needs_sync = true;
        let sound = flop_sound(kind);
        self.play_mob_voice(id, move |_| Some(sound));
    }

    pub(crate) fn fish_post_tick(&mut self, id: EntityId) {
        let Some(kind) = self.kind_of(id) else {
            return;
        };
        if fish::is_schooling(kind) {
            self.fish_regroup(id);
        }
        if kind == Kind::Pufferfish {
            self.pufferfish_touch(id);
            self.pufferfish_touch_players(id);
        }
    }

    fn fish_regroup(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = fish_state(entity) else {
            return;
        };
        if state.school_size <= 1 {
            return;
        }
        if ServerRandom(&mut self.rng).next_int(fish::SCHOOL_REGROUP_ODDS) != 1 {
            return;
        }
        let kind = entity.base.kind;
        let range = fish::SCHOOL_SEARCH_RANGE;
        let area = inflate(&entity.base.bounding_box(), range, range, range);
        let neighbours = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.kind == kind
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .count();
        if neighbours < 2
            && let Some(slot) = self.entities.store.get_mut(id).and_then(fish_state_mut)
        {
            slot.school_size = 1;
        }
    }

    fn pufferfish_touch(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let puff = fish::puff_state(&entity.base);
        if puff <= 0 || living::is_dead_or_dying(&entity.base, living) {
            return;
        }
        let reach = fish::PUFFER_CONTACT_INFLATE;
        let area = inflate(&entity.base.bounding_box(), reach, reach, reach);
        let victims: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.id != id
                    && other.base.kind.extends("Mob")
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
                    && other
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&other.base, state))
                    && is_scary_kind_to_pufferfish(other.base.kind)
            })
            .map(|other| other.base.id)
            .collect();
        for victim in victims {
            self.pufferfish_sting(id, Target::Entity(victim), puff);
        }
    }

    fn pufferfish_touch_players(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let puff = fish::puff_state(&entity.base);
        if puff <= 0 {
            return;
        }
        let area = entity.base.bounding_box();
        let touched: Vec<_> = self
            .players
            .values()
            .filter(|player| {
                !player.vitals.removed
                    && !player.vitals.dead
                    && player.vitals.health > 0.0
                    && player.game_mode != rspigot_domain::player::GameMode::Spectator
                    && inflate(
                        &rspigot_domain::entity::Aabb::of_size(
                            player.position,
                            0.6,
                            player.height() as f32,
                        ),
                        1.0,
                        0.5,
                        1.0,
                    )
                    .intersects(&area)
            })
            .map(|player| player.id)
            .collect();
        for player in touched {
            self.pufferfish_sting(id, Target::Player(player), puff);
        }
    }

    fn pufferfish_sting(&mut self, id: EntityId, target: Target, puff: i32) {
        let Some(source_entity) = self.mob_source(id) else {
            return;
        };
        let source = DamageSource::by(DamageType::MobAttack, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = MobAttackFollow {
            attacker: id,
            target,
            knockback: 0.0,
            yaw: 0.0,
            golem: false,
            hoglin: false,
            bee: false,
            puffer: Some(puff),
            cube: false,
        };
        self.strike(
            None,
            victim,
            &source,
            (puff + 1) as f32,
            AfterHurt::MobAttack(follow),
        );
    }

    pub(crate) fn pufferfish_finish_sting(
        &mut self,
        attacker: EntityId,
        target: Target,
        puff: i32,
    ) {
        let victim = match target {
            Target::Player(player) => {
                let Some(found) = self.players.get(&player) else {
                    return;
                };
                let entity = found.entity_id;
                if !self
                    .entities
                    .store
                    .get(attacker)
                    .is_some_and(|e| e.base.is_silent())
                {
                    self.push(
                        Audience::One(player),
                        ClientEvent::GameEvent(GameEvent::PufferFishSting),
                    );
                }
                entity
            }
            Target::Entity(entity) => entity,
        };
        if let Some(poison) = EffectId::from_name(POISON) {
            let effect = EffectInstance::new(
                poison,
                EffectDuration::Ticks((puff * fish::STING_POISON_TICKS) as u32),
                0,
                true,
            );
            self.add_effect_to(victim, effect, EntityPotionEffectCause::Attack);
        }
        if matches!(target, Target::Entity(_)) {
            self.play_mob_voice(attacker, |_| Some(STING));
        }
    }

    pub(crate) fn fish_joined_school(&mut self, leader: EntityId) {
        if let Some(slot) = self.entities.store.get_mut(leader).and_then(fish_state_mut) {
            slot.school_size += 1;
        }
    }

    pub(crate) fn water_action(&mut self, id: EntityId, action: WaterAction) {
        match action {
            WaterAction::Recruit { leader, follower } => {
                if let Some(slot) = self
                    .entities
                    .store
                    .get_mut(follower)
                    .and_then(fish_state_mut)
                {
                    slot.leader = Some(leader);
                }
                if let Some(slot) = self.entities.store.get_mut(leader).and_then(fish_state_mut) {
                    slot.school_size += 1;
                }
            }
            WaterAction::StopFollowing => {
                let leader = self
                    .entities
                    .store
                    .get_mut(id)
                    .and_then(fish_state_mut)
                    .and_then(|slot| slot.leader.take());
                if let Some(leader) = leader
                    && let Some(slot) = self.entities.store.get_mut(leader).and_then(fish_state_mut)
                {
                    slot.school_size -= 1;
                }
            }
            WaterAction::BreathAssist => self.breath_assist(id),
            WaterAction::DropHeldItem => self.dolphin_drop_held_item(id),
            WaterAction::GiveDolphinsGrace(player) => self.dolphins_grace(id, player),
        }
    }
}
