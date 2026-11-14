use rspigot_ai::{MobView, clip_misses, default_attack_reach, inflate};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species::{Species, cube};
use rspigot_domain::entity::{Aabb, Entity, EntityBase, EntityState, RemovalReason, fire, living};
use rspigot_domain::player::GameMode;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::commands::Target;
use crate::health::DamageSource;
use crate::mobs::ChunkView;
use crate::plugins::events::{
    CreatureSpawnReason, CubeMobSplit, EntityRemoveCause, EntityTransform, EntityTransformReason,
    EventPayload,
};
use crate::plugins::steps::{AfterHurt, MobAttackFollow, Struck};
use crate::plugins::{Birth, Continuation, ScatterStep, SplitStep, view_of_entity};
use crate::ports::ClientLink;
use crate::server::Server;

const PLAYER_WIDTH: f32 = 0.6;
const TOUCH_INFLATE_X: f64 = 1.0;
const TOUCH_INFLATE_Y: f64 = 0.5;
const SQUISH_PARTICLE_DENSITY: f32 = 16.0;
const SQUISH_PITCH_DIVISOR: f32 = 0.8;
const FULL_TURN: f32 = 360.0;

fn cube_state_mut(entity: &mut Entity) -> Option<&mut cube::CubeState> {
    match &mut entity.state {
        EntityState::Living(living) => match living.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Cube(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn cube_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.cube.can_deal_damage =
            cube::can_deal_damage(&entity.base, !living::is_no_ai(&entity.base));
    }

    pub(crate) fn cube_after_act(&mut self, id: EntityId) {
        let sound = self
            .entities
            .mobs
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.ai.take_cube_sound());
        if let Some(sound) = sound {
            self.entity_sound(id, sound.sound, sound.volume, sound.pitch);
        }
    }

    pub(crate) fn cube_post_tick(&mut self, id: EntityId) {
        self.cube_contact(id);
        self.cube_landing(id);
    }

    fn cube_landing(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let on_ground = entity.base.on_ground;
        let kind = entity.base.kind;
        let width = entity.base.dimensions.width;
        let tiny = cube::is_tiny(&entity.base);
        let volume = cube::sound_volume(&entity.base);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(state) = cube_state_mut(entity) else {
            return;
        };
        let landed = on_ground && !state.was_on_ground;
        state.was_on_ground = on_ground;
        if !landed {
            return;
        }
        let mut random = self.mob_random(id);
        let limit = width * 2.0 * SQUISH_PARTICLE_DENSITY;
        let mut particle = 0;
        while (particle as f32) < limit {
            random.next_float();
            random.next_float();
            particle += 1;
        }
        let pitch =
            ((random.next_float() - random.next_float()) * 0.2 + 1.0) / SQUISH_PITCH_DIVISOR;
        self.store_mob_random(id, random);
        self.entity_sound(id, cube::squish_sound(kind, tiny), volume, pitch);
    }

    fn cube_alive(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_some_and(|entity| {
            !entity.base.is_removed()
                && entity
                    .living()
                    .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        })
    }

    fn cube_contact(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !self.cube_alive(id)
            || !cube::can_deal_damage(&entity.base, !living::is_no_ai(&entity.base))
        {
            return;
        }
        let area = entity.base.bounding_box();
        let players: Vec<_> = self
            .players
            .values()
            .filter(|player| {
                !player.vitals.removed
                    && !player.vitals.dead
                    && player.vitals.health > 0.0
                    && player.game_mode != GameMode::Spectator
                    && inflate(
                        &Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32),
                        TOUCH_INFLATE_X,
                        TOUCH_INFLATE_Y,
                        TOUCH_INFLATE_X,
                    )
                    .intersects(&area)
            })
            .map(|player| player.id)
            .collect();
        for player in players {
            self.cube_deal_damage(id, Target::Player(player));
        }
        let golems: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.kind == Kind::IronGolem
                    && other.base.id != id
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| other.base.id)
            .collect();
        for golem in golems {
            if self.cube_alive(id) {
                self.cube_deal_damage(id, Target::Entity(golem));
            }
        }
    }

    fn cube_deal_damage(&mut self, id: EntityId, target: Target) {
        if !self.cube_alive(id) {
            return;
        }
        let (target_box, target_eye) = match target {
            Target::Player(player) => {
                let Some(player) = self.players.get(&player) else {
                    return;
                };
                (
                    Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32),
                    player.eye_position(),
                )
            }
            Target::Entity(entity) => {
                let Some(entity) = self.entities.store.get(entity) else {
                    return;
                };
                if !entity
                    .living()
                    .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
                {
                    return;
                }
                (
                    entity.base.bounding_box(),
                    Vec3::new(
                        entity.base.position.x,
                        entity.base.eye_y(),
                        entity.base.position.z,
                    ),
                )
            }
        };
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let reach = default_attack_reach();
        let attack_box = inflate(&entity.base.bounding_box(), reach, 0.0, reach);
        if !attack_box.intersects(&target_box) {
            return;
        }
        let eye = Vec3::new(
            entity.base.position.x,
            entity.base.eye_y(),
            entity.base.position.z,
        );
        let view = ChunkView {
            chunks: &self.world.chunks,
            height: self.world.dimension.height,
        };
        if !clip_misses(&view, eye, target_eye) {
            return;
        }
        let Some(source_entity) = self.mob_source(id) else {
            return;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let damage = cube::attack_damage(base, state);
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
            puffer: None,
            cube: true,
        };
        self.strike(None, victim, &source, damage, AfterHurt::MobAttack(follow));
    }

    pub(crate) fn cube_finish_attack(&mut self, attacker: EntityId, hurt: bool) {
        if !hurt {
            return;
        }
        let mut random = self.mob_random(attacker);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.store_mob_random(attacker, random);
        self.entity_sound(attacker, cube::ATTACK_SOUND, 1.0, pitch);
    }

    pub(crate) fn splitting(&self, id: EntityId) -> bool {
        self.decisions.anywhere(|then| match then {
            Continuation::Split(step) => step.id == id,
            Continuation::Scatter(step) => step.id == id,
            _ => false,
        })
    }

    pub(crate) fn cube_splits(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if !cube::is_cube(entity.base.kind) || cube::size(&entity.base) < 2 {
            return false;
        }
        if self.splitting(id) {
            return true;
        }
        let mut random = self.mob_random(id);
        let count = cube::split_count(&mut random);
        self.store_mob_random(id, random);
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::Split(SplitStep { id, count }),
            move |server| {
                let entity = server.entity_view(id)?;
                Some(EventPayload::CubeMobSplit(CubeMobSplit {
                    count,
                    entity_type: entity.kind.clone(),
                    entity,
                }))
            },
        );
        true
    }

    fn remove_dead_cube(&mut self, id: EntityId) {
        self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
    }

    pub(crate) fn finish_cube_split(
        &mut self,
        step: SplitStep,
        cancelled: bool,
        patch: crate::plugins::events::CubeMobSplitPatch,
    ) {
        let SplitStep { id, count } = step;
        if self.entities.store.get(id).is_none() {
            return;
        }
        let count = patch.count.unwrap_or(count);
        if cancelled || count <= 0 {
            self.remove_dead_cube(id);
            return;
        }
        let children = self.cube_children(id, count);
        if children.is_empty() {
            self.remove_dead_cube(id);
            return;
        }
        let views: Vec<_> = children.iter().map(view_of_entity).collect();
        let lane = self.lane_for(None, id);
        let step = ScatterStep { id, children };
        self.fire(lane, Continuation::Scatter(Box::new(step)), move |server| {
            let entity = server.entity_view(id)?;
            let first = views.first()?.clone();
            Some(EventPayload::EntityTransform(EntityTransform {
                entity_type: entity.kind.clone(),
                entity,
                transform_reason: EntityTransformReason::Split,
                transformed_entities: views.clone(),
                transformed_entity: first,
            }))
        });
    }

    pub(crate) fn finish_cube_scatter(&mut self, step: ScatterStep, cancelled: bool) {
        let ScatterStep { id, children } = step;
        if cancelled {
            self.remove_dead_cube(id);
            return;
        }
        for child in children {
            self.spawn_entity(child, Birth::of(CreatureSpawnReason::SlimeSplit));
        }
        self.remove_dead_cube(id);
    }

    fn cube_children(&mut self, id: EntityId, count: i32) -> Vec<Entity> {
        let Some(parent) = self.entities.store.get(id) else {
            return Vec::new();
        };
        let Some(parent_state) = parent.living().cloned() else {
            return Vec::new();
        };
        let parent_base = parent.base.clone();
        let kind = parent_base.kind;
        let width = parent_base.dimensions.width;
        let half = cube::size(&parent_base) / 2;
        let mut random = self.mob_random(id);
        let mut children = Vec::new();
        for index in 0..count {
            let (dx, dz) = cube::split_offset(width, index);
            let mut base = self.new_entity_base(kind);
            let mut state = living::LivingState::new(&mut base, parent_base.yaw);
            copy_common(&parent_base, &parent_state, &mut base, &mut state);
            cube::set_size(&mut base, &mut state, half, true);
            let yaw = random.next_float() * FULL_TURN;
            base.snap_to(
                Vec3::new(
                    parent_base.position.x + f64::from(dx),
                    parent_base.position.y + cube::SPLIT_LIFT,
                    parent_base.position.z + f64::from(dz),
                ),
                yaw,
                0.0,
            );
            children.push(Entity::new(base, EntityState::Living(Box::new(state))));
        }
        self.store_mob_random(id, random);
        children
    }
}

fn copy_common(
    from: &EntityBase,
    from_state: &living::LivingState,
    to: &mut EntityBase,
    to_state: &mut living::LivingState,
) {
    to_state.absorption = from_state.absorption;
    for effect in from_state.effects.iter() {
        let _ = to_state.effects.apply(*effect);
    }
    living::set_left_handed(to, living::is_left_handed(from));
    living::set_no_ai(to, living::is_no_ai(from));
    if let (Some(new_mob), Some(old_mob)) = (&mut to_state.mob, &from_state.mob) {
        new_mob.persistence_required = old_mob.persistence_required;
    }
    to.set_custom_name(from.custom_name().cloned());
    to.set_custom_name_visible(from.custom_name_visible());
    fire::set_on_fire_flag(to, fire::is_on_fire(from));
    to.invulnerable = from.invulnerable;
    to.invulnerable_time = from.invulnerable_time;
    to.set_no_gravity(from.is_no_gravity());
    to.portal_cooldown = from.portal_cooldown;
    to.set_silent(from.is_silent());
    to.tags = from.tags.clone();
    to.custom_data = from.custom_data.clone();
}
