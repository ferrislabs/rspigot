use rspigot_ai::{MobView, SquidFacts};
use rspigot_domain::entity::species::{Species, squid};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::particle::{Particle, ParticleData};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;

use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const INK: &str = "minecraft:squid_ink";
const GLOW_INK: &str = "minecraft:glow_squid_ink";
const SQUIRT: &str = "minecraft:entity.squid.squirt";
const GLOW_SQUIRT: &str = "minecraft:entity.glow_squid.squirt";
const INK_SPEED: f64 = 0.1;
const INK_SPREAD: f32 = 0.6;
const INK_ADULT_SCALE: f32 = 0.3;
const INK_BABY_SCALE: f32 = 0.1;
const INK_RANGE: f32 = 2.0;
const DEG: f32 = std::f32::consts::PI / 180.0;

fn squid_state(entity: &Entity) -> Option<squid::SquidState> {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Squid(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

fn rotate(vec: Vec3, pitch: f32, yaw: f32) -> Vec3 {
    let (sin_x, cos_x) = (
        f64::from(mth::sin(f64::from(pitch * DEG))),
        f64::from(mth::cos(f64::from(pitch * DEG))),
    );
    let tilted = Vec3::new(
        vec.x,
        vec.y * cos_x + vec.z * sin_x,
        vec.z * cos_x - vec.y * sin_x,
    );
    let turn = -yaw * DEG;
    let (sin_y, cos_y) = (
        f64::from(mth::sin(f64::from(turn))),
        f64::from(mth::cos(f64::from(turn))),
    );
    Vec3::new(
        tilted.x * cos_y + tilted.z * sin_y,
        tilted.y,
        tilted.z * cos_y - tilted.x * sin_y,
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn squid_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(state) = squid_state(entity) else {
            return;
        };
        view.species.squid = SquidFacts {
            movement: state.movement,
        };
    }

    pub(crate) fn squid_post_tick(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(Entity {
            base,
            state: rspigot_domain::entity::EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let Some(mut state) = living.mob.as_ref().and_then(|mob| match mob.species {
            Species::Squid(state) => Some(state),
            _ => None,
        }) else {
            return;
        };
        let wrapped = squid::ai_step(base, living, &mut state, &mut random);
        let glow = base.kind == Kind::GlowSquid;
        if glow {
            let dark = squid::dark_ticks(base);
            if dark > 0 {
                squid::set_dark_ticks(base, dark - 1);
            }
        }
        base.needs_sync = true;
        if let Some(mob) = living.mob.as_mut() {
            mob.species = Species::Squid(state);
        }
        self.store_mob_random(id, random);
        if wrapped {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::Event(squid::TENTACLE_EVENT),
                },
            );
        }
    }

    pub(crate) fn squid_after_hurt(&mut self, id: EntityId, _source: &DamageSource) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let glow = entity.base.kind == Kind::GlowSquid;
        let attacked = entity
            .living()
            .is_some_and(|living| living.damage.last_hurt_by_mob.is_some());
        if glow {
            self.glow_squid_darken(id);
        }
        if attacked {
            self.squid_spawn_ink(id);
        }
    }

    fn glow_squid_darken(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            squid::set_dark_ticks(&mut entity.base, squid::DARK_TICKS_ON_HURT);
        }
    }

    fn squid_spawn_ink(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let (position, glow, baby) = (
            entity.base.position,
            entity.base.kind == Kind::GlowSquid,
            entity.is_baby(),
        );
        let (pitch, yaw) = squid_state(entity).map_or((0.0, 0.0), |state| {
            (state.x_body_rot_old, state.y_body_rot_old)
        });
        let sound = if glow { GLOW_SQUIRT } else { SQUIRT };
        self.play_mob_voice(id, move |_| Some(sound));
        let origin = rotate(Vec3::new(0.0, 1.0, 0.0), pitch, yaw) + position;
        let mut random = self.mob_random(id);
        let scale = if baby {
            INK_BABY_SCALE
        } else {
            INK_ADULT_SCALE
        };
        for _ in 0..squid::INK_PARTICLES {
            let x = f64::from(random.next_float() * INK_SPREAD - 0.3);
            let z = f64::from(random.next_float() * INK_SPREAD - 0.3);
            let direction = rotate(Vec3::new(x, -1.0, z), pitch, yaw);
            let velocity = direction.scale(f64::from(scale + random.next_float() * INK_RANGE));
            self.push(
                Audience::Trackers(id),
                ClientEvent::Particle(Particle {
                    kind: if glow { GLOW_INK } else { INK },
                    data: ParticleData::None,
                    position: Vec3::new(origin.x, origin.y + 0.5, origin.z),
                    count: 0,
                    spread: velocity,
                    speed: INK_SPEED,
                }),
            );
        }
        self.store_mob_random(id, random);
    }
}
