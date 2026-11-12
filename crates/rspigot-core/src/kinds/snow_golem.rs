use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::snow_golem::{
    self, MELT_DAMAGE, SHOOT_PITCH_BASE, SHOOT_POWER, SHOOT_UNCERTAINTY, SNOW_TRAIL_OFFSET,
    SNOW_TRAIL_SPOTS, TARGET_DISTANCE_SCALE, TARGET_EYE_OFFSET,
};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3, mth};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::loot::{self, LootContext};
use crate::plugins::steps::FormStep;
use crate::ports::ClientLink;
use crate::server::Server;

const SHOOT_SOUND: &str = "minecraft:entity.snow_golem.shoot";
const SHEAR_SOUND: &str = "minecraft:entity.snow_golem.shear";
const SHEAR_TABLE: &str = "minecraft:shearing/snow_golem";
const CANNOT_SUPPORT_SNOW: &str = "minecraft:cannot_support_snow_layer";
const SUPPORT_OVERRIDE_SNOW: &str = "minecraft:support_override_snow_layer";
const FULL_LAYERS: &str = "8";
const FACE_EPSILON: f64 = 1.0e-7;

fn in_block_tag(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn top_face_is_full(state: BlockStateId) -> bool {
    let boxes: Vec<_> = state
        .collision_shape()
        .iter()
        .filter(|aabb| (aabb.max_y - 1.0).abs() < FACE_EPSILON)
        .collect();
    if boxes.is_empty() {
        return false;
    }
    let mut xs = vec![0.0, 1.0];
    let mut zs = vec![0.0, 1.0];
    for aabb in &boxes {
        xs.extend([aabb.min_x, aabb.max_x]);
        zs.extend([aabb.min_z, aabb.max_z]);
    }
    xs.retain(|x| (0.0..=1.0).contains(x));
    zs.retain(|z| (0.0..=1.0).contains(z));
    xs.sort_by(f64::total_cmp);
    zs.sort_by(f64::total_cmp);
    xs.windows(2).all(|x| {
        zs.windows(2).all(|z| {
            if x[1] - x[0] < FACE_EPSILON || z[1] - z[0] < FACE_EPSILON {
                return true;
            }
            let (cx, cz) = ((x[0] + x[1]) / 2.0, (z[0] + z[1]) / 2.0);
            boxes.iter().any(|aabb| {
                aabb.min_x <= cx && cx <= aabb.max_x && aabb.min_z <= cz && cz <= aabb.max_z
            })
        })
    })
}

pub(crate) fn snow_layer_survives(below: BlockStateId) -> bool {
    if in_block_tag(below, CANNOT_SUPPORT_SNOW) {
        return false;
    }
    if in_block_tag(below, SUPPORT_OVERRIDE_SNOW) {
        return true;
    }
    top_face_is_full(below)
        || (below.kind() == Some(BlockKind::Snow)
            && below
                .properties()
                .any(|(name, value)| name == "layers" && value == FULL_LAYERS))
}

pub(crate) fn trail_spot(position: Vec3, index: i32) -> BlockPos {
    let offset = |step: i32| f64::from((step * 2 - 1) as f32 * SNOW_TRAIL_OFFSET);
    BlockPos::new(
        mth::floor(position.x + offset(index % 2)),
        mth::floor(position.y),
        mth::floor(position.z + offset((index / 2) % 2)),
    )
}

pub(crate) fn readiness_for_shearing(entity: &Entity) -> bool {
    if entity.base.kind == Kind::SnowGolem {
        return snow_golem::has_pumpkin(&entity.base);
    }
    !rspigot_domain::entity::species::is_sheared(&entity.base) && !entity.is_baby()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn snow_golem_throw(&mut self, shooter: EntityId, target: EntityId) {
        let Some((position, eye_y)) = self.entity_eye(target) else {
            return;
        };
        let Some(golem) = self.entities.store.get(shooter) else {
            return;
        };
        let origin = golem.base.position;
        let dx = position.x - origin.x;
        let dy = eye_y - TARGET_EYE_OFFSET;
        let dz = position.z - origin.z;
        let lift = (dx * dx + dz * dz).sqrt() * TARGET_DISTANCE_SCALE;
        self.launch_thrown(
            Kind::Snowball,
            shooter,
            move |at: Vec3| Vec3::new(dx, dy + lift - at.y, dz),
            SHOOT_POWER,
            SHOOT_UNCERTAINTY,
        );
        let mut random = self.mob_random(shooter);
        let pitch = SHOOT_PITCH_BASE / (random.next_float() * 0.4 + 0.8);
        self.store_mob_random(shooter, random);
        self.entity_sound(shooter, SHOOT_SOUND, 1.0, pitch);
    }

    pub(crate) fn entity_eye(&self, id: EntityId) -> Option<(Vec3, f64)> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some((player.position, player.position.y + player.eye_height()));
        }
        let entity = self.entities.store.get(id)?;
        Some((entity.base.position, entity.base.eye_y()))
    }

    pub(crate) fn snow_golem_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let biome = self.biome_at(position.block());
        let dimension = self.world.dimension.dimension_type.to_string();
        if snow_golem::melts_in(biome, &dimension) {
            self.hurt_entity(id, &DamageSource::new(DamageType::OnFire), MELT_DAMAGE);
        }
        if self.config.rules.mob_griefing {
            let Some(entity) = self.entities.store.get(id) else {
                return;
            };
            let position = entity.base.position;
            for index in 0..SNOW_TRAIL_SPOTS {
                let pos = trail_spot(position, index);
                self.form_snow_trail(id, pos);
            }
        }
        self.water_sensitive_damage(id);
    }

    fn form_snow_trail(&mut self, id: EntityId, pos: BlockPos) {
        let snow = BlockKind::Snow.default_state();
        if !self.snow_fits(pos) {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let view = crate::plugins::view_of_entity(entity);
        let step = FormStep {
            entity: id,
            pos,
            state: snow,
        };
        self.form_block(view, step);
    }

    fn snow_fits(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .block(pos)
            .is_some_and(BlockStateId::is_air)
            && self
                .world
                .chunks
                .block(pos.offset(0, -1, 0))
                .is_some_and(snow_layer_survives)
    }

    pub(crate) fn finish_snow_trail(&mut self, step: FormStep) {
        let alive = self.entities.store.get(step.entity).is_some();
        if alive && self.snow_fits(step.pos) {
            self.set_block(step.pos, step.state);
        }
    }

    pub(crate) fn snow_golem_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if held?.item != ItemKind::Shears.id() {
            return None;
        }
        let ready = self
            .entities
            .store
            .get(target)
            .is_some_and(readiness_for_shearing);
        ready.then(|| self.ask_shear(player, target, hand))
    }

    pub(crate) fn shear_snow_golem(&mut self, player: PlayerId, target: EntityId, hand: Hand) {
        let Some(entity) = self.entities.store.get(target) else {
            return;
        };
        let position = entity.base.position;
        let eye_height = f64::from(entity.base.eye_height());
        let facts = crate::mobs::loot_facts_of(entity);
        self.play_sound(None, position, SHEAR_SOUND, SoundSource::Players, 1.0, 1.0);
        if let Some(entity) = self.entities.store.get_mut(target) {
            snow_golem::set_pumpkin(&mut entity.base, false);
        }
        let drops = rspigot_data::loot::table(SHEAR_TABLE).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        for stack in drops {
            for _ in 0..stack.count.get() {
                let single = ItemStack {
                    count: NonZeroU8::MIN,
                    ..stack.clone()
                };
                self.drop_at_location(target, eye_height, single, Vec3::ZERO);
            }
        }
        self.hurt_held(player, hand, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind) -> BlockStateId {
        kind.default_state()
    }

    #[test]
    fn the_trail_covers_the_four_corners_a_quarter_block_around_the_feet() {
        let at = Vec3::new(4.0, 64.7, 9.0);
        let spots: Vec<_> = (0..SNOW_TRAIL_SPOTS).map(|i| trail_spot(at, i)).collect();
        assert_eq!(
            spots,
            vec![
                BlockPos::new(3, 64, 8),
                BlockPos::new(4, 64, 8),
                BlockPos::new(3, 64, 9),
                BlockPos::new(4, 64, 9),
            ]
        );
        let centre = Vec3::new(4.5, 64.0, 9.5);
        assert!((0..SNOW_TRAIL_SPOTS).all(|i| trail_spot(centre, i) == BlockPos::new(4, 64, 9)));
    }

    #[test]
    fn snow_survives_on_full_tops_and_the_override_tag_but_not_on_ice() {
        assert!(snow_layer_survives(state(BlockKind::Stone)));
        assert!(snow_layer_survives(state(BlockKind::GrassBlock)));
        assert!(snow_layer_survives(state(BlockKind::SoulSand)));
        assert!(snow_layer_survives(state(BlockKind::Mud)));
        assert!(snow_layer_survives(state(BlockKind::HoneyBlock)));
        assert!(!snow_layer_survives(state(BlockKind::Ice)));
        assert!(!snow_layer_survives(state(BlockKind::PackedIce)));
        assert!(!snow_layer_survives(state(BlockKind::Barrier)));
        assert!(!snow_layer_survives(state(BlockKind::Air)));
        assert!(!snow_layer_survives(state(BlockKind::Farmland)));
    }

    #[test]
    fn snow_stacks_only_on_a_full_layer_of_snow() {
        let layers = |count: &str| {
            BlockKind::Snow
                .state_with(&[("layers", count)])
                .expect("a snow state")
        };
        assert!(snow_layer_survives(layers("8")));
        assert!(!snow_layer_survives(layers("7")));
        assert!(!snow_layer_survives(layers("1")));
    }

    #[test]
    fn a_top_slab_has_a_full_top_face_and_a_bottom_slab_does_not() {
        let slab = |kind: &str| {
            BlockKind::StoneSlab
                .state_with(&[("type", kind), ("waterlogged", "false")])
                .expect("a slab state")
        };
        assert!(snow_layer_survives(slab("top")));
        assert!(snow_layer_survives(slab("double")));
        assert!(!snow_layer_survives(slab("bottom")));
    }
}
