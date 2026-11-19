use rspigot_plugin::{
    BlockPos, BlockType, Effect, EffectType, Entity, EntityType, Error, Explosion, Material,
    Particle, ParticleBurst, Position, Sound, SoundCue, Vector, World,
};

use super::catalog::{self, WoolColor};
use super::impact::{Action, BlockKind, Cell, EffectKind, ParticleKind, SoundKind, Vec3};
use crate::domain::{SheepKind, TeamId};

pub async fn apply(
    world: &World,
    thrower: &Entity,
    actions: &[Action],
    team_of: impl Fn(&Entity) -> Option<TeamId>,
) -> Result<Vec<(SheepKind, Entity)>, Error> {
    let mut spawned = Vec::new();
    for action in actions {
        match *action {
            Action::Explode {
                at,
                power,
                breaks_blocks,
            } => world
                .explosion
                .create(Explosion::new(position(at), power, breaks_blocks)),
            Action::SetBlock { at, block } => world.block(block_pos(at)).set(block_type(block)),
            Action::Particle { kind, at, count } => world
                .particle
                .spawn(ParticleBurst::new(particle(kind), position(at)).count(count)),
            Action::Sound { kind, at } => world
                .sound
                .play(SoundCue::new(sound(kind)).at(position(at))),
            Action::Effect {
                team,
                center,
                radius,
                effect,
                duration_ticks,
                amplifier,
            } => {
                for target in members(world, center, radius, team, &team_of).await? {
                    target.add_effect(Effect::new(effect_type(effect), duration_ticks, amplifier));
                }
            }
            Action::Velocity {
                team,
                center,
                radius,
                velocity,
            } => {
                for target in members(world, center, radius, team, &team_of).await? {
                    target.set_velocity(vector(velocity));
                }
            }
            Action::Lightning { at } => {
                world.spawn(EntityType::LightningBolt, position(at)).await?;
            }
            Action::SpawnSheep { kind, at, velocity } => {
                let entity = world.spawn(EntityType::Sheep, position(at)).await?;
                entity.set_velocity(vector(velocity));
                spawned.push((kind, entity));
            }
            Action::TeleportThrower { to } => thrower.teleport(position(to)),
        }
    }
    Ok(spawned)
}

pub async fn nearest_enemy(
    world: &World,
    from: Vec3,
    range: f64,
    thrower_team: TeamId,
    team_of: impl Fn(&Entity) -> Option<TeamId>,
) -> Result<Option<Vec3>, Error> {
    let found = members(world, from, range, thrower_team.other(), &team_of).await?;
    Ok(found
        .iter()
        .map(|entity| vec3(entity.position))
        .min_by(|a, b| distance_squared(from, *a).total_cmp(&distance_squared(from, *b))))
}

pub const fn wool_material(color: WoolColor) -> Material {
    match color {
        WoolColor::Red => Material::RedWool,
        WoolColor::Orange => Material::OrangeWool,
        WoolColor::LightBlue => Material::LightBlueWool,
        WoolColor::Green => Material::GreenWool,
        WoolColor::Yellow => Material::YellowWool,
        WoolColor::Pink => Material::PinkWool,
        WoolColor::Purple => Material::PurpleWool,
        WoolColor::Brown => Material::BrownWool,
        WoolColor::Black => Material::BlackWool,
        WoolColor::Gray => Material::GrayWool,
    }
}

pub const fn sheep_wool(kind: SheepKind) -> Material {
    wool_material(catalog::wool_color(kind))
}

async fn members(
    world: &World,
    center: Vec3,
    radius: f64,
    team: TeamId,
    team_of: &impl Fn(&Entity) -> Option<TeamId>,
) -> Result<Vec<Entity>, Error> {
    let nearby = world
        .entities
        .of(EntityType::Player)
        .near(position(center), radius)
        .get_entities()
        .await?;
    Ok(nearby
        .into_iter()
        .filter(|entity| team_of(entity) == Some(team))
        .collect())
}

fn distance_squared(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

const fn position(v: Vec3) -> Position {
    Position {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

const fn vec3(p: Position) -> Vec3 {
    Vec3 {
        x: p.x,
        y: p.y,
        z: p.z,
    }
}

const fn vector(v: Vec3) -> Vector {
    Vector {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

const fn block_pos(c: Cell) -> BlockPos {
    BlockPos {
        x: c.x,
        y: c.y,
        z: c.z,
    }
}

const fn block_type(block: BlockKind) -> BlockType {
    match block {
        BlockKind::Fire => BlockType::Fire,
        BlockKind::Ice => BlockType::Ice,
    }
}

const fn particle(kind: ParticleKind) -> Particle {
    match kind {
        ParticleKind::Explosion => Particle::Explosion,
        ParticleKind::Flame => Particle::Flame,
        ParticleKind::Snowflake => Particle::Snowflake,
        ParticleKind::Crit => Particle::Crit,
        ParticleKind::Heart => Particle::Heart,
        ParticleKind::Portal => Particle::Portal,
        ParticleKind::LargeSmoke => Particle::LargeSmoke,
    }
}

const fn sound(kind: SoundKind) -> Sound {
    match kind {
        SoundKind::Explode | SoundKind::Quake => Sound::EntityGenericExplode,
        SoundKind::Fire => Sound::BlockFireAmbient,
        SoundKind::Ice => Sound::BlockGlassBreak,
        SoundKind::Thunder => Sound::EntityLightningBoltThunder,
        SoundKind::Heal => Sound::EntityPlayerLevelup,
        SoundKind::Teleport => Sound::EntityEndermanTeleport,
        SoundKind::Darkness => Sound::EntityGhastShoot,
        SoundKind::Split => Sound::EntitySheepAmbient,
    }
}

const fn effect_type(kind: EffectKind) -> EffectType {
    match kind {
        EffectKind::Slowness => EffectType::Slowness,
        EffectKind::Regeneration => EffectType::Regeneration,
        EffectKind::Blindness => EffectType::Blindness,
    }
}
