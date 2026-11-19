use rspigot_plugin::{
    BlockPos, BlockType, DyeColor, Effect, EffectType, Entity, EntityType, Error, Explosion,
    Material, Particle, ParticleBurst, Player, Position, Sound, SoundCue, Vector, World,
};

use super::catalog::{self, WoolColor};
use super::debris::{DEBRIS_PARTICLES, blocked_by_player};
use super::impact::{
    Action, BlockKind, Cell, EffectKind, ParticleKind, SoundKind, Vec3, outward_velocity,
};
use super::select::{self, Candidate};
use crate::domain::{SheepKind, TeamId};

#[derive(Debug, Clone)]
pub struct Combatant {
    pub player: Player,
    pub team: TeamId,
    pub position: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub at: Cell,
    pub block: BlockType,
    pub delay_ticks: u32,
}

#[derive(Debug, Default)]
pub struct Applied {
    pub spawned: Vec<(SheepKind, Entity)>,
    pub landings: Vec<Landing>,
}

pub async fn apply(
    world: &World,
    actions: &[Action],
    roster: &[Combatant],
) -> Result<Applied, Error> {
    let mut applied = Applied::default();
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
                for target in members(roster, center, radius, team) {
                    target.player.effects.add(Effect::new(
                        effect_type(effect),
                        duration_ticks,
                        amplifier,
                    ));
                }
            }
            Action::Velocity {
                team,
                center,
                radius,
                velocity,
            } => {
                for target in members(roster, center, radius, team) {
                    target.player.movement.set_velocity(vector(velocity));
                }
            }
            Action::Push {
                team,
                center,
                radius,
                strength,
                lift,
            } => {
                for target in members(roster, center, radius, team) {
                    let velocity = outward_velocity(center, target.position, strength, lift);
                    target.player.movement.set_velocity(vector(velocity));
                }
            }
            Action::Lightning { at } => {
                world.spawn(EntityType::LightningBolt, position(at)).await?;
            }
            Action::SpawnSheep { kind, at, velocity } => {
                let entity = world.spawn(EntityType::Sheep, position(at)).await?;
                paint(&entity, kind);
                entity.set_velocity(vector(velocity));
                applied.spawned.push((kind, entity));
            }
            Action::Scatter {
                from,
                to,
                delay_ticks,
            } => {
                if let Some(block) = lift(world, from).await {
                    applied.landings.push(Landing {
                        at: to,
                        block,
                        delay_ticks,
                    });
                }
            }
        }
    }
    Ok(applied)
}

async fn lift(world: &World, from: Cell) -> Option<BlockType> {
    let block = world.block(block_pos(from));
    let kind = match block.get_state().await {
        Ok(state) => state.kind,
        Err(error) => {
            tracing::warn!("could not read a debris block: {error}");
            return None;
        }
    };
    if is_empty(kind) {
        return None;
    }
    block.set(BlockType::Air);
    world.particle.spawn(
        ParticleBurst::new(Particle::Block(kind), cell_center(from)).count(DEBRIS_PARTICLES),
    );
    Some(kind)
}

pub async fn settle(world: &World, landing: Cell, block: BlockType, players: &[Vec3]) {
    if blocked_by_player(landing, players) {
        return;
    }
    let target = world.block(block_pos(landing));
    match target.get_state().await {
        Ok(state) if is_empty(state.kind) => {
            target.set(block);
            world.particle.spawn(
                ParticleBurst::new(Particle::Block(block), cell_center(landing))
                    .count(DEBRIS_PARTICLES),
            );
        }
        Ok(_) => {}
        Err(error) => tracing::warn!("could not land a debris block: {error}"),
    }
}

const fn is_empty(kind: BlockType) -> bool {
    matches!(
        kind,
        BlockType::Air | BlockType::CaveAir | BlockType::VoidAir
    )
}

pub fn paint(entity: &Entity, kind: SheepKind) {
    if let Some(sheep) = entity.as_sheep() {
        sheep.set_color(dye_color(catalog::wool_color(kind)));
    }
}

pub fn nearest_enemy(
    roster: &[Combatant],
    from: Vec3,
    range: f64,
    thrower_team: TeamId,
) -> Option<Vec3> {
    select::nearest_enemy(&candidates(roster), from, range, thrower_team)
}

fn candidates(roster: &[Combatant]) -> Vec<Candidate> {
    roster
        .iter()
        .map(|combatant| Candidate {
            team: combatant.team,
            position: combatant.position,
        })
        .collect()
}

fn members(
    roster: &[Combatant],
    center: Vec3,
    radius: f64,
    team: TeamId,
) -> impl Iterator<Item = &Combatant> {
    select::within(&candidates(roster), center, radius, team)
        .into_iter()
        .map(|index| &roster[index])
}

pub const fn wool_material(color: WoolColor) -> Material {
    match color {
        WoolColor::Red => Material::RedWool,
        WoolColor::White => Material::WhiteWool,
        WoolColor::Magenta => Material::MagentaWool,
        WoolColor::Blue => Material::BlueWool,
        WoolColor::Orange => Material::OrangeWool,
        WoolColor::LightBlue => Material::LightBlueWool,
        WoolColor::Green => Material::GreenWool,
        WoolColor::Yellow => Material::YellowWool,
        WoolColor::Pink => Material::PinkWool,
        WoolColor::Brown => Material::BrownWool,
        WoolColor::Black => Material::BlackWool,
        WoolColor::Gray => Material::GrayWool,
    }
}

pub const fn dye_color(color: WoolColor) -> DyeColor {
    match color {
        WoolColor::Red => DyeColor::Red,
        WoolColor::White => DyeColor::White,
        WoolColor::Magenta => DyeColor::Magenta,
        WoolColor::Blue => DyeColor::Blue,
        WoolColor::Orange => DyeColor::Orange,
        WoolColor::LightBlue => DyeColor::LightBlue,
        WoolColor::Green => DyeColor::Green,
        WoolColor::Yellow => DyeColor::Yellow,
        WoolColor::Pink => DyeColor::Pink,
        WoolColor::Brown => DyeColor::Brown,
        WoolColor::Black => DyeColor::Black,
        WoolColor::Gray => DyeColor::Gray,
    }
}

pub const fn sheep_wool(kind: SheepKind) -> Material {
    wool_material(catalog::wool_color(kind))
}

const fn position(v: Vec3) -> Position {
    Position {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

const fn vector(v: Vec3) -> Vector {
    Vector {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

fn cell_center(cell: Cell) -> Position {
    Position {
        x: f64::from(cell.x) + 0.5,
        y: f64::from(cell.y) + 0.5,
        z: f64::from(cell.z) + 0.5,
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
        SoundKind::Ice => Sound::BlockGlassBreak,
        SoundKind::Thunder => Sound::EntityLightningBoltThunder,
        SoundKind::Heal => Sound::EntityPlayerLevelup,
        SoundKind::Teleport => Sound::EntityEndermanTeleport,
        SoundKind::Darkness => Sound::EntityGhastShoot,
        SoundKind::Split => Sound::EntitySheepAmbient,
        SoundKind::Warp => Sound::EntityEnderDragonShoot,
        SoundKind::Intergalactic => Sound::EntityWitherSpawn,
        SoundKind::Hiss => Sound::EntityCreeperPrimed,
    }
}

const fn effect_type(kind: EffectKind) -> EffectType {
    match kind {
        EffectKind::Slowness => EffectType::Slowness,
        EffectKind::Regeneration => EffectType::Regeneration,
        EffectKind::Blindness => EffectType::Blindness,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_kind_has_a_distinct_item_colour_and_a_distinct_entity_colour() {
        let items: HashSet<_> = SheepKind::ALL.into_iter().map(sheep_wool).collect();
        let entities: HashSet<_> = SheepKind::ALL
            .into_iter()
            .map(|kind| dye_color(catalog::wool_color(kind)))
            .collect();
        assert_eq!(items.len(), SheepKind::ALL.len());
        assert_eq!(entities.len(), SheepKind::ALL.len());
    }

    #[test]
    fn the_item_and_the_entity_share_one_colour_source() {
        for kind in SheepKind::ALL {
            let color = catalog::wool_color(kind);
            assert_eq!(
                format!("{:?}", wool_material(color)),
                format!("{:?}Wool", dye_color(color)),
                "{kind:?}"
            );
            assert_eq!(sheep_wool(kind), wool_material(color));
        }
    }

    #[test]
    fn debris_stops_at_air_and_keeps_solid_blocks() {
        assert!(is_empty(BlockType::Air));
        assert!(!is_empty(BlockType::QuartzBlock));
    }
}
