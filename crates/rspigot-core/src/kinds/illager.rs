use rspigot_ai::MobView;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::living::{self, MOVEMENT_SPEED};
use rspigot_domain::entity::species::{Species, ravager, vindicator};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::{ClientEvent, EntityStatus};
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::plugins::steps::Grief;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const LEAVES_TAG: &str = "minecraft:leaves";

fn ravager_of(entity: &mut Entity) -> Option<&mut ravager::RavagerState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Ravager(state)) => Some(state),
        _ => None,
    }
}

fn vindicator_of(entity: &mut Entity) -> Option<&mut vindicator::VindicatorState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Vindicator(state)) => Some(state),
        _ => None,
    }
}

pub(crate) fn celebrate_sound(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Witch => Some(rspigot_domain::entity::species::witch::CELEBRATE_SOUND),
        Kind::Pillager => Some(rspigot_domain::entity::species::pillager::CELEBRATE_SOUND),
        Kind::Vindicator => Some(vindicator::CELEBRATE_SOUND),
        Kind::Ravager => Some(ravager::CELEBRATE_SOUND),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn vindicator_view(&self, entity: &Entity, view: &mut MobView) {
        self.witch_view(entity, view);
        if let Some(Species::Vindicator(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            view.species.raider.johnny = state.johnny;
        }
    }

    pub(crate) fn ravager_view(&self, entity: &Entity, view: &mut MobView) {
        self.witch_view(entity, view);
        if let Some(Species::Ravager(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            view.species.raider.blind = !state.can_see();
        }
    }

    pub(crate) fn vindicator_ai_step(&mut self, id: EntityId) {
        self.raider_ai_step(id);
        if let Some(entity) = self.entities.store.get_mut(id) {
            let name = entity.base.custom_name().cloned();
            if let Some(state) = vindicator_of(entity) {
                state.name_makes_johnny(name.as_ref());
            }
        }
    }

    pub(crate) fn vindicator_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if living::is_no_ai(&entity.base) {
            return;
        }
        let raided = self.raid_at(entity.base.position.block()).is_some();
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().profile.can_open_doors = raided;
        }
    }

    pub(crate) fn ravager_is_immobile(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|living| living.mob.as_ref())
            .is_some_and(|mob| match &mob.species {
                Species::Ravager(state) => state.blocks_actions(),
                _ => false,
            })
    }

    pub(crate) fn ravager_attack(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id)
            && let Some(state) = ravager_of(entity)
        {
            state.attack_tick = ravager::ATTACK_TICKS;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(ravager::ATTACK_EVENT),
            },
        );
        self.entity_sound(id, ravager::ATTACK_SOUND, 1.0, 1.0);
    }

    pub(crate) fn ravager_ai_step(&mut self, id: EntityId) {
        self.raider_ai_step(id);
        let target = self.mob_target(id).is_some();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let horizontal_collision = entity.base.horizontal_collision;
        let on_ground = entity.base.on_ground;
        let area = entity.base.bounding_box().inflate(ravager::LEAF_REACH);
        let Some(living) = entity.living_mut() else {
            return;
        };
        let Some(Species::Ravager(mut state)) = living.mob.as_ref().map(|mob| mob.species) else {
            return;
        };
        let immobile = state.blocks_actions();
        let current = living
            .attributes
            .instance(MOVEMENT_SPEED)
            .map_or(0.0, |instance| instance.base);
        let speed = ravager::blended_speed(current, target, immobile);
        living.attributes.set_base(MOVEMENT_SPEED, speed);
        let [roar, stunned_end] = ravager::count_down(&mut state);
        if let Some(mob) = living.mob.as_mut() {
            mob.species = Species::Ravager(state);
        }
        if horizontal_collision && self.config.rules.mob_griefing {
            let mut broke = false;
            let low = BlockPos::new(
                area.min_x.floor() as i32,
                area.min_y.floor() as i32,
                area.min_z.floor() as i32,
            );
            let high = BlockPos::new(
                area.max_x.floor() as i32,
                area.max_y.floor() as i32,
                area.max_z.floor() as i32,
            );
            for x in low.x..=high.x {
                for y in low.y..=high.y {
                    for z in low.z..=high.z {
                        let pos = BlockPos::new(x, y, z);
                        let leaves = self.world.chunks.block(pos).is_some_and(|state| {
                            rspigot_domain::entity::physics::has_tag(state, LEAVES_TAG)
                        });
                        if !leaves {
                            continue;
                        }
                        let Some(view) = self.entity_view(id) else {
                            return;
                        };
                        let change = Grief::RavagerBreak { ravager: id, pos };
                        self.change_block(view, pos, BlockStateId::AIR, change, false);
                        broke = true;
                    }
                }
            }
            if !broke && on_ground {
                self.mob_jump_from_ground(id);
            }
        }
        if roar == ravager::Tick::Roar {
            self.ravager_roar(id);
        }
        if stunned_end == ravager::Tick::StunnedEnd {
            self.entity_sound(id, ravager::ROAR_SOUND, 1.0, 1.0);
        }
    }

    pub(crate) fn finish_ravager_break(&mut self, id: EntityId, pos: BlockPos) {
        if !self.living_entity_alive(id) {
            return;
        }
        let leaves = self
            .world
            .chunks
            .block(pos)
            .is_some_and(|state| rspigot_domain::entity::physics::has_tag(state, LEAVES_TAG));
        if leaves {
            self.destroy_block_dropping(pos, Some(id));
        }
    }

    fn mob_jump_from_ground(&mut self, id: EntityId) {
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().jump.jump();
        }
    }

    fn ravager_roar(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let area = entity.base.bounding_box().inflate(ravager::ROAR_RANGE);
        let origin = entity.base.position;
        let griefing = self.config.rules.mob_griefing;
        let Some(source) = self.mob_source(id) else {
            return;
        };
        let mut victims: Vec<(EntityId, bool, bool)> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter(|player| {
                rspigot_domain::entity::Aabb::of_size(player.position, 0.6, player.height() as f32)
                    .intersects(&area)
            })
            .map(|player| (player.entity_id, true, false))
            .collect();
        victims.extend(
            self.entities
                .store
                .iter()
                .filter(|other| other.base.id != id && !other.base.is_removed())
                .filter(|other| other.base.kind != Kind::Ravager)
                .filter(|other| griefing || other.base.kind != Kind::ArmorStand)
                .filter(|other| other.base.bounding_box().intersects(&area))
                .filter_map(|other| {
                    let living = other.living()?;
                    (!living::is_dead_or_dying(&other.base, living)).then_some((
                        other.base.id,
                        false,
                        other.base.kind.extends("AbstractIllager"),
                    ))
                }),
        );
        for (victim, player, illager) in victims {
            if !illager {
                let source = DamageSource::by(DamageType::MobAttack, source.clone());
                self.hurt_entity(victim, &source, ravager::ROAR_DAMAGE);
            }
            if !player && let Some(other) = self.entities.store.get_mut(victim) {
                let push = ravager::strong_knockback(origin, other.base.position);
                other.base.velocity = Vec3::new(
                    other.base.velocity.x + push.x,
                    other.base.velocity.y + push.y,
                    other.base.velocity.z + push.z,
                );
                other.base.sync_velocity = true;
            }
        }
        self.game_event(
            GameEvent::EntityAction,
            origin,
            GameEventContext::of(Some(id)),
        );
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(ravager::ROAR_EVENT),
            },
        );
    }

    pub(crate) fn raider_celebrate_sound(&mut self, id: EntityId) {
        let Some(kind) = self.kind_of(id) else {
            return;
        };
        let Some(sound) = celebrate_sound(kind) else {
            return;
        };
        let rule = kind.sounds().map(|sounds| sounds.pitch);
        let pitch = match rule {
            Some(rule) => self.voice_pitch(rule, false),
            None => 1.0,
        };
        self.entity_sound(id, sound, 1.0, pitch);
    }
}
