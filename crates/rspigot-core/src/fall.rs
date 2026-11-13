use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::attribute::{FALL_DAMAGE_MULTIPLIER, SAFE_FALL_DISTANCE};
use rspigot_domain::damage::{self, DamageType};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};

use crate::health::{DamageSource, attributes};
use crate::ports::ClientLink;
use crate::server::Server;

const SUPPORT_OFFSET: f64 = 0.2;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const SOFT_LANDING: f32 = 0.2;
const BED_REDUCTION: f64 = 0.5;
const STALAGMITE_BONUS: f64 = 2.5;
const STALAGMITE_MULTIPLIER: f32 = 2.0;
const BEDS_TAG: &str = "minecraft:beds";
const BIG_FALL: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallImpact {
    pub distance: f64,
    pub multiplier: f32,
    pub kind: DamageType,
}

impl FallImpact {
    pub(crate) fn source(&self, below: (BlockPos, BlockStateId)) -> DamageSource {
        DamageSource::from_block(
            self.kind,
            (self.kind == DamageType::Stalagmite).then_some(below),
        )
    }
}

fn is_bed(kind: BlockKind) -> bool {
    rspigot_data::tags::tag("minecraft:block", BEDS_TAG)
        .is_some_and(|beds| beds.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn fall_impact(
    below: BlockStateId,
    distance: f64,
    suppress_bounce: bool,
) -> Option<FallImpact> {
    let impact = |distance: f64, multiplier: f32, kind: DamageType| {
        Some(FallImpact {
            distance,
            multiplier,
            kind,
        })
    };
    match below.kind() {
        Some(BlockKind::HayBlock | BlockKind::HoneyBlock) => {
            impact(distance, SOFT_LANDING, DamageType::Fall)
        }
        Some(BlockKind::SlimeBlock) if suppress_bounce => None,
        Some(BlockKind::SlimeBlock) => impact(distance, 0.0, DamageType::Fall),
        Some(BlockKind::PowderSnow) => None,
        Some(BlockKind::PointedDripstone)
            if below.is_speleothem_toward(Face::Up)
                && below.property("thickness") == Some("tip") =>
        {
            impact(
                distance + STALAGMITE_BONUS,
                STALAGMITE_MULTIPLIER,
                DamageType::Stalagmite,
            )
        }
        Some(kind) if kind == BlockKind::ShelfMushroom || is_bed(kind) => {
            impact(distance * (1.0 - BED_REDUCTION), 1.0, DamageType::Fall)
        }
        _ => impact(distance, 1.0, DamageType::Fall),
    }
}

impl<L: ClientLink> Server<L> {
    fn supporting_block(&self, position: Vec3) -> (BlockPos, BlockStateId) {
        let y = (position.y - SUPPORT_OFFSET).floor() as i32;
        let center = BlockPos::new(position.x.floor() as i32, y, position.z.floor() as i32);
        let state = |pos: BlockPos| self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR);
        let solid = |pos: BlockPos| state(pos).flags().contains(StateFlags::HAS_COLLISION);
        if solid(center) {
            return (center, state(center));
        }
        let min_x = (position.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (position.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_z = (position.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (position.z + PLAYER_HALF_WIDTH).floor() as i32;
        (min_x..=max_x)
            .flat_map(|x| (min_z..=max_z).map(move |z| BlockPos::new(x, y, z)))
            .filter(|&pos| solid(pos))
            .min_by(|a, b| {
                let distance = |pos: BlockPos| {
                    pos.center()
                        .distance_squared(Vec3::new(position.x, f64::from(y), position.z))
                };
                distance(*a).total_cmp(&distance(*b))
            })
            .map_or((center, state(center)), |pos| (pos, state(pos)))
    }

    pub(crate) fn check_fall(&mut self, id: PlayerId, dy: f64, on_ground: bool) {
        let in_water = self.feet_in_water(id);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.vitals.is_dead() {
            return;
        }
        let vitals = &mut player.vitals;
        if in_water {
            vitals.fall_distance = 0.0;
        } else if dy < 0.0 {
            vitals.fall_distance -= f64::from(dy as f32);
        }
        if on_ground {
            let distance = vitals.fall_distance;
            let sneaking = player.sneaking;
            let position = player.position;
            let entity = player.entity_id;
            let (pos, below) = self.supporting_block(position);
            if below.kind() == Some(BlockKind::TurtleEgg) {
                if !sneaking {
                    self.egg_stepped_on(entity, true, pos);
                }
                if distance > 0.0 {
                    self.egg_fallen_on(entity, true, pos);
                }
            }
            if distance > 0.0
                && let Some(impact) = fall_impact(below, distance, sneaking)
            {
                self.cause_player_fall_damage(id, impact, (pos, below));
            }
            if let Some(player) = self.players.get_mut(&id) {
                player.vitals.fall_distance = 0.0;
            }
        }
        if dy > 0.0
            && let Some(player) = self.players.get_mut(&id)
        {
            player.vitals.fall_distance = 0.0;
        }
    }

    fn cause_player_fall_damage(
        &mut self,
        id: PlayerId,
        impact: FallImpact,
        below: (BlockPos, BlockStateId),
    ) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.abilities().may_fly {
            return;
        }
        let stats = attributes(player);
        let amount = damage::fall_damage(
            impact.distance,
            impact.multiplier,
            stats.get(SAFE_FALL_DISTANCE),
            stats.get(FALL_DAMAGE_MULTIPLIER),
        );
        if amount > 0 {
            if let Some(sounds) = rspigot_entities::EntityKind::Player.sounds() {
                let sound = if amount > BIG_FALL {
                    sounds.big_fall
                } else {
                    sounds.small_fall
                };
                let position = player.position;
                self.play_sound(Some(id), position, sound, sounds.source, 1.0, 1.0);
            }
            self.hurt(id, &impact.source(below), amount as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
        kind.state_with(properties).unwrap_or_default()
    }

    #[test]
    fn plain_blocks_take_the_whole_fall() {
        let stone = BlockKind::Stone.default_state();
        assert_eq!(
            fall_impact(stone, 20.0, false),
            Some(FallImpact {
                distance: 20.0,
                multiplier: 1.0,
                kind: DamageType::Fall
            })
        );
    }

    #[test]
    fn hay_and_honey_cut_damage_to_a_fifth() {
        for kind in [BlockKind::HayBlock, BlockKind::HoneyBlock] {
            let impact = fall_impact(kind.default_state(), 20.0, false).unwrap();
            assert_eq!(impact.multiplier, 0.2);
            assert_eq!(
                damage::fall_damage(impact.distance, impact.multiplier, 3.0, 1.0),
                3
            );
        }
    }

    #[test]
    fn slime_cancels_and_sneaking_on_it_skips_damage_entirely() {
        let slime = BlockKind::SlimeBlock.default_state();
        assert_eq!(
            fall_impact(slime, 30.0, false).map(|i| i.multiplier),
            Some(0.0)
        );
        assert_eq!(fall_impact(slime, 30.0, true), None);
        assert_eq!(
            fall_impact(BlockKind::PowderSnow.default_state(), 30.0, false),
            None
        );
    }

    #[test]
    fn beds_halve_the_distance() {
        let bed = BlockKind::RedBed.default_state();
        let impact = fall_impact(bed, 10.0, false).unwrap();
        assert_eq!(impact.distance, 5.0);
        assert_eq!(
            damage::fall_damage(impact.distance, impact.multiplier, 3.0, 1.0),
            2
        );
        let shelf = fall_impact(BlockKind::ShelfMushroom.default_state(), 10.0, false).unwrap();
        assert_eq!(shelf.distance, 5.0);
    }

    #[test]
    fn stalagmite_tips_add_distance_and_double_damage() {
        let tip = state(
            BlockKind::PointedDripstone,
            &[
                ("thickness", "tip"),
                ("vertical_direction", "up"),
                ("waterlogged", "false"),
            ],
        );
        let impact = fall_impact(tip, 4.0, false).unwrap();
        assert_eq!(impact.kind, DamageType::Stalagmite);
        assert_eq!(
            damage::fall_damage(impact.distance, impact.multiplier, 3.0, 1.0),
            7
        );
        let base = state(
            BlockKind::PointedDripstone,
            &[
                ("thickness", "base"),
                ("vertical_direction", "up"),
                ("waterlogged", "false"),
            ],
        );
        assert_eq!(
            fall_impact(base, 4.0, false).map(|i| i.kind),
            Some(DamageType::Fall)
        );
    }
}
