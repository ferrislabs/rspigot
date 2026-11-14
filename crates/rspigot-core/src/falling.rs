use std::collections::BTreeMap;
use std::num::NonZeroU8;

use rand::Rng;
use rspigot_blocks::{
    BlockKind, Face, Gravity, SCAFFOLDING_MAX_DISTANCE, StateFlags, scaffolding_settle,
    speleothem_thickness,
};
use rspigot_domain::composter;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::falling::{self, FallingState};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rustc_hash::FxHashSet;

use crate::health::{DamageSource, SourceEntity};
use crate::loot::{self, LootContext};
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::steps::{Grief, Landing};
use crate::plugins::{Continuation, EntityView, view_of_entity};
use crate::ports::ClientLink;
use crate::server::Server;

const FALLING_DELAY: u64 = 2;
const STALACTITE_DELAY: u64 = 2;
const STALAGMITE_DELAY: u64 = 1;
const SCAFFOLDING_DELAY: u64 = 1;
const MAX_FALL_TICKS: i32 = 600;
const OUT_OF_WORLD_TICKS: i32 = 100;
const BLOCK_HALF_WIDTH: f64 = 0.49;
const BLOCK_HEIGHT: f64 = 0.98;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const MIN_STALACTITE_SIZE: i32 = 6;
const MAX_FALL_DAMAGE: i32 = 40;

#[derive(Debug, Default)]
pub struct ScheduledFalls {
    due: BTreeMap<u64, Vec<(BlockPos, BlockKind)>>,
    pending: FxHashSet<(BlockPos, BlockKind)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallHurt {
    pub per_distance: f32,
    pub max: i32,
}

impl FallHurt {
    pub const ANVIL: Self = Self {
        per_distance: 2.0,
        max: MAX_FALL_DAMAGE,
    };

    pub fn stalactite(size: i32) -> Self {
        Self {
            per_distance: size.max(MIN_STALACTITE_SIZE) as f32,
            max: MAX_FALL_DAMAGE,
        }
    }

    pub fn damage(self, fall_distance: f64) -> f32 {
        let blocks = fall_blocks(fall_distance);
        if blocks < 0 {
            return 0.0;
        }
        ((blocks as f32 * self.per_distance).floor() as i32).min(self.max) as f32
    }
}

pub fn anvil_degrade_chance(fall_distance: f64) -> f32 {
    if FallHurt::ANVIL.damage(fall_distance) > 0.0 {
        0.05 + fall_blocks(fall_distance) as f32 * 0.05
    } else {
        0.0
    }
}

fn fall_blocks(fall_distance: f64) -> i32 {
    (fall_distance - 1.0).ceil() as i32
}

fn overlaps(block: Vec3, player: Vec3, height: f64) -> bool {
    let reach = BLOCK_HALF_WIDTH + PLAYER_HALF_WIDTH;
    (block.x - player.x).abs() < reach
        && (block.z - player.z).abs() < reach
        && block.y < player.y + height
        && block.y + BLOCK_HEIGHT > player.y
}

fn is_free(state: BlockStateId) -> bool {
    state.is_air()
        || state.flags().contains(StateFlags::REPLACEABLE)
        || matches!(
            state.kind(),
            Some(BlockKind::Water | BlockKind::Lava | BlockKind::Fire | BlockKind::SoulFire)
        )
}

fn sturdy(state: BlockStateId) -> bool {
    let flags = state.flags();
    flags.contains(StateFlags::SOLID_RENDER) || flags.contains(StateFlags::FULL_COLLISION_CUBE)
}

fn has_water(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::WATER)
}

fn water_source(state: BlockStateId) -> bool {
    has_water(state)
        && (state.kind() != Some(BlockKind::Water) || state.property("level") == Some("0"))
}

fn gravity(state: BlockStateId) -> Gravity {
    state.kind().map_or(Gravity::None, BlockKind::gravity)
}

fn left_behind(state: BlockStateId) -> BlockStateId {
    if state.property("waterlogged") == Some("true") {
        BlockKind::Water.default_state()
    } else {
        BlockStateId::AIR
    }
}

fn waterlogged(state: BlockStateId, water: bool) -> BlockStateId {
    state
        .with_property("waterlogged", if water { "true" } else { "false" })
        .unwrap_or(state)
}

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

impl<L: ClientLink> Server<L> {
    fn block_or_air(&self, pos: BlockPos) -> BlockStateId {
        self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR)
    }

    pub(crate) fn update_neighbour_shapes(&mut self, pos: BlockPos) {
        for face in Face::ALL {
            self.update_shape(step(pos, face), face.opposite());
        }
    }

    pub(crate) fn placement_state(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        pitch: f32,
        sneaking: bool,
    ) -> Option<BlockStateId> {
        let Some(kind) = state.kind() else {
            return Some(state);
        };
        let replaced = self.block_or_air(pos);
        if kind == BlockKind::DriedGhast {
            return Some(waterlogged(state, water_source(replaced)));
        }
        match kind.gravity() {
            Gravity::ConcretePowder => Some(match kind.concrete() {
                Some(concrete) if has_water(replaced) || self.touches_liquid(pos) => {
                    concrete.default_state()
                }
                _ => state,
            }),
            Gravity::Scaffolding => Some(waterlogged(
                self.settle_scaffolding(pos, state),
                water_source(replaced),
            )),
            Gravity::Speleothem => {
                let looking = if pitch < 0.0 { Face::Up } else { Face::Down };
                let tip = [looking.opposite(), looking]
                    .into_iter()
                    .find(|&tip| self.speleothem_survives(pos, kind, tip))?;
                let thickness = self.speleothem_thickness_at(pos, kind, tip, !sneaking);
                let direction = if tip == Face::Up { "up" } else { "down" };
                let placed = kind
                    .state_with(&[("vertical_direction", direction), ("thickness", thickness)])?;
                Some(waterlogged(placed, water_source(replaced)))
            }
            _ => Some(state),
        }
    }

    pub(crate) fn schedule(&mut self, pos: BlockPos, kind: BlockKind, delay: u64) {
        if self.falls.pending.insert((pos, kind)) {
            let due = self.current_tick() + delay;
            self.falls.due.entry(due).or_default().push((pos, kind));
        }
    }

    pub(crate) fn on_place(&mut self, pos: BlockPos) {
        self.fluid_changed(pos);
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind() else {
            return;
        };
        if kind == BlockKind::Composter {
            if composter::needs_ready_tick(state) {
                self.schedule(pos, kind, composter::READY_DELAY_TICKS);
            }
            return;
        }
        if kind == BlockKind::SnifferEgg {
            if let Some(delay) = self.sniffer_egg_placed(pos, state) {
                self.schedule(pos, kind, delay);
            }
            return;
        }
        if kind == BlockKind::TurtleEgg {
            self.turtle_egg_placed(pos);
            return;
        }
        if kind == BlockKind::Frogspawn {
            let delay = self.frogspawn_placed();
            self.schedule(pos, kind, delay);
            return;
        }
        match kind.gravity() {
            Gravity::Falling | Gravity::ConcretePowder | Gravity::Anvil => {
                self.schedule(pos, kind, FALLING_DELAY)
            }
            Gravity::Scaffolding => self.schedule(pos, kind, SCAFFOLDING_DELAY),
            Gravity::Speleothem | Gravity::None => {}
        }
    }

    fn update_shape(&mut self, pos: BlockPos, direction: Face) {
        self.fluid_changed(pos);
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind() else {
            return;
        };
        if kind == BlockKind::Frogspawn {
            self.frogspawn_neighbour_changed(pos);
            return;
        }
        match kind.gravity() {
            Gravity::Falling | Gravity::Anvil => self.schedule(pos, kind, FALLING_DELAY),
            Gravity::ConcretePowder => match kind.concrete() {
                Some(concrete) if self.touches_liquid(pos) => {
                    self.set_block(pos, concrete.default_state());
                }
                _ => self.schedule(pos, kind, FALLING_DELAY),
            },
            Gravity::Speleothem => self.update_speleothem(pos, state, kind, direction),
            Gravity::Scaffolding => self.schedule(pos, kind, SCAFFOLDING_DELAY),
            Gravity::None => {}
        }
    }

    fn touches_liquid(&self, pos: BlockPos) -> bool {
        let inside = has_water(self.block_or_air(pos));
        Face::ALL.into_iter().any(|face| {
            let neighbour = self.block_or_air(step(pos, face));
            (face != Face::Down || inside) && has_water(neighbour) && !sturdy(neighbour)
        })
    }

    fn update_speleothem(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        kind: BlockKind,
        direction: Face,
    ) {
        let Some(tip) = state.tip_direction() else {
            return;
        };
        if direction.is_horizontal()
            || (tip == Face::Down && self.falls.pending.contains(&(pos, kind)))
        {
            return;
        }
        if direction == tip.opposite() && !self.speleothem_survives(pos, kind, tip) {
            let delay = if tip == Face::Down {
                STALACTITE_DELAY
            } else {
                STALAGMITE_DELAY
            };
            self.schedule(pos, kind, delay);
            return;
        }
        let merge = state.property("thickness") == Some("tip_merge");
        let thickness = self.speleothem_thickness_at(pos, kind, tip, merge);
        if let Some(next) = state.with_property("thickness", thickness)
            && next != state
        {
            self.set_block(pos, next);
        }
    }

    fn speleothem_survives(&self, pos: BlockPos, kind: BlockKind, tip: Face) -> bool {
        let support = self.block_or_air(step(pos, tip.opposite()));
        sturdy(support) || (support.is_speleothem_toward(tip) && support.kind() == Some(kind))
    }

    fn speleothem_thickness_at(
        &self,
        pos: BlockPos,
        kind: BlockKind,
        tip: Face,
        merge: bool,
    ) -> &'static str {
        let toward = self.block_or_air(step(pos, tip));
        let away = self.block_or_air(step(pos, tip.opposite()));
        speleothem_thickness(kind, tip, toward, away, merge)
    }

    fn settle_scaffolding(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let below = self.block_or_air(step(pos, Face::Down));
        let sides = Face::HORIZONTAL.map(|face| self.block_or_air(step(pos, face)));
        scaffolding_settle(state, below, sturdy(below), sides)
    }

    fn survives(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(kind) = state.kind() else {
            return true;
        };
        match (kind.gravity(), state.tip_direction()) {
            (Gravity::Speleothem, Some(tip)) => self.speleothem_survives(pos, kind, tip),
            (Gravity::Scaffolding, _) => {
                self.settle_scaffolding(pos, state).scaffolding_distance()
                    < Some(SCAFFOLDING_MAX_DISTANCE)
            }
            _ => true,
        }
    }

    pub(crate) fn tick_falling_blocks(&mut self) {
        let now = self.current_tick();
        let due: Vec<(BlockPos, BlockKind)> = {
            let later = self.falls.due.split_off(&(now + 1));
            let ready = std::mem::replace(&mut self.falls.due, later);
            ready.into_values().flatten().collect()
        };
        for (pos, kind) in due {
            self.falls.pending.remove(&(pos, kind));
            let state = self.block_or_air(pos);
            if state.kind() == Some(kind) {
                self.block_tick(pos, state, kind);
            }
        }
    }

    fn block_tick(&mut self, pos: BlockPos, state: BlockStateId, kind: BlockKind) {
        if kind == BlockKind::Composter {
            self.composter_ready(pos, state);
            return;
        }
        if kind == BlockKind::SnifferEgg {
            self.sniffer_egg_tick(pos, state);
            return;
        }
        if kind == BlockKind::Frogspawn {
            self.frogspawn_tick(pos);
            return;
        }
        if kind == BlockKind::DriedGhast {
            self.dried_ghast_tick(pos);
            return;
        }
        match kind.gravity() {
            Gravity::Falling | Gravity::ConcretePowder => self.try_fall(pos, state, None),
            Gravity::Anvil => self.try_fall(pos, state, Some(FallHurt::ANVIL)),
            Gravity::Speleothem => {
                let stalagmite = state.tip_direction() == Some(Face::Up);
                if stalagmite && !self.survives(pos, state) {
                    self.destroy_block(pos, state);
                } else {
                    self.spawn_falling_stalactite(pos, state);
                }
            }
            Gravity::Scaffolding => {
                let settled = self.settle_scaffolding(pos, state);
                let limit = Some(SCAFFOLDING_MAX_DISTANCE);
                if settled.scaffolding_distance() == limit {
                    if state.scaffolding_distance() == limit {
                        self.fall(pos, settled, None);
                    } else {
                        self.destroy_block(pos, state);
                    }
                } else if settled != state {
                    self.set_block(pos, settled);
                }
            }
            Gravity::None => {}
        }
    }

    fn try_fall(&mut self, pos: BlockPos, state: BlockStateId, hurt: Option<FallHurt>) {
        let below = self.world.chunks.block(step(pos, Face::Down));
        if pos.y > self.world.dimension.height.min_y && below.is_some_and(is_free) {
            self.fall(pos, state, hurt);
        }
    }

    fn spawn_falling_stalactite(&mut self, top: BlockPos, state: BlockStateId) {
        let mut pos = top;
        let mut current = state;
        while current.is_speleothem_toward(Face::Down) {
            if current.is_speleothem_tip(true) {
                self.fall(pos, current, Some(FallHurt::stalactite(1 + top.y - pos.y)));
                return;
            }
            self.fall(pos, current, None);
            pos = step(pos, Face::Down);
            current = self.block_or_air(pos);
        }
    }

    fn destroy_block(&mut self, pos: BlockPos, state: BlockStateId) {
        let context = LootContext::block(state, None, None);
        for stack in loot::block_drops(&context, &mut self.rng) {
            self.spawn_item(pos, stack);
        }
        self.set_block(pos, left_behind(state));
    }

    fn fall(&mut self, pos: BlockPos, state: BlockStateId, hurt: Option<FallHurt>) {
        let mut base = self.new_entity_base(Kind::FallingBlock);
        let position = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        base.position = position;
        base.old_position = position;
        let mut falling = FallingState::new(&mut base, waterlogged(state, false), position.block());
        if let Some(hurt) = hurt {
            falling.hurt_entities = true;
            falling.fall_hurt_amount = hurt.per_distance;
            falling.fall_hurt_max = hurt.max;
        }
        let entity = Entity::new(base, EntityState::Falling(falling));
        let view = view_of_entity(&entity);
        let step = Grief::Fall {
            pos,
            state,
            entity: Box::new(entity),
        };
        self.change_block(view, pos, left_behind(state), step, false);
    }

    pub(crate) fn finish_fall(&mut self, pos: BlockPos, state: BlockStateId, entity: Entity) {
        if self.block_or_air(pos) != state {
            return;
        }
        self.set_block(pos, left_behind(state));
        self.add_entity(entity, CreatureSpawnReason::Default);
    }

    fn water_on_path(&self, from: BlockPos, to: BlockPos) -> Option<BlockPos> {
        (to.y..=from.y)
            .rev()
            .map(|y| BlockPos::new(from.x, y, from.z))
            .find(|&pos| {
                let state = self.block_or_air(pos);
                water_source(state) || state.flags().contains(StateFlags::HAS_COLLISION)
            })
            .filter(|&pos| has_water(self.block_or_air(pos)))
    }

    pub(crate) fn tick_falling_entity(&mut self, id: EntityId) {
        if self.landing(id) {
            return;
        }
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Falling(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if state.state.is_air() {
            self.remove_entity(id, RemovalReason::Discarded, None);
            return;
        }
        let original = state.state;
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        let from = base.position;
        let landing = falling::fall_step(base, state, chunks);
        let time = state.time;
        let on_ground = base.on_ground;
        let speed = base.velocity.length_sqr();
        let mut pos = base.position.block();
        self.frogspawn_crushed(pos);
        if let Some(landing) = landing
            && let Some(impact) = crate::fall::fall_impact(landing.state, landing.distance, false)
        {
            self.cause_fall_damage(id, impact.distance);
        }
        let concrete = gravity(original) == Gravity::ConcretePowder;
        let mut in_water = concrete && has_water(self.block_or_air(pos));
        if concrete
            && speed > 1.0
            && let Some(hit) = self.water_on_path(from.block(), pos)
        {
            pos = hit;
            in_water = true;
        }
        if on_ground || in_water {
            if let Some(entity) = self.entities.store.get_mut(id) {
                falling::absorb_landing(&mut entity.base);
            }
            self.land(id, original, pos, in_water);
            return;
        }
        let height = self.world.dimension.height;
        let outside = pos.y <= height.min_y || pos.y > height.max_y();
        if time > MAX_FALL_TICKS || (time > OUT_OF_WORLD_TICKS && outside) {
            self.drop_falling(id, original, pos);
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            falling::apply_drag(&mut entity.base);
        }
    }

    fn cause_fall_damage(&mut self, id: EntityId, distance: f64) {
        let Some(Entity {
            base,
            state: EntityState::Falling(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        if !state.hurt_entities {
            return;
        }
        let hurt = FallHurt {
            per_distance: state.fall_hurt_amount,
            max: state.fall_hurt_max,
        };
        let block = state.state;
        let position = base.position;
        if fall_blocks(distance) < 0 {
            return;
        }
        let damage = hurt.damage(distance);
        let kind = match gravity(block) {
            Gravity::Anvil => DamageType::FallingAnvil,
            Gravity::Speleothem => DamageType::FallingStalactite,
            _ => DamageType::FallingBlock,
        };
        let source = DamageSource::by(
            kind,
            SourceEntity {
                entity: id,
                player: None,
                kind: rspigot_entities::EntityKind::FallingBlock,
                creative: false,
                name: Component::translatable("entity.minecraft.falling_block", Vec::new()),
                position,
            },
        );
        let victims: Vec<PlayerId> = self
            .players
            .values()
            .filter(|p| {
                !matches!(p.game_mode, GameMode::Creative | GameMode::Spectator)
                    && !p.vitals.is_dead()
                    && overlaps(position, p.position, p.height())
            })
            .map(|p| p.id)
            .collect();
        for victim in victims {
            self.hurt(victim, &source, damage);
        }
        let falling_box = self.entities.store.get(id).map(|e| e.base.bounding_box());
        let mobs: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|e| e.living().is_some() && e.base.id != id)
            .filter(|e| falling_box.is_some_and(|b| b.intersects(&e.base.bounding_box())))
            .map(|e| e.base.id)
            .collect();
        for mob in mobs {
            self.hurt_mob(mob, &source, damage);
        }
        if gravity(block) != Gravity::Anvil {
            return;
        }
        let chance = anvil_degrade_chance(distance);
        if chance > 0.0
            && self.rng.random::<f32>() < chance
            && let Some(Entity {
                state: EntityState::Falling(state),
                ..
            }) = self.entities.store.get_mut(id)
        {
            match state.state.damaged_anvil() {
                Some(damaged) => state.state = damaged,
                None => state.cancel_drop = true,
            }
        }
    }

    fn land(&mut self, id: EntityId, original: BlockStateId, pos: BlockPos, in_water: bool) {
        let Some(Entity {
            state: EntityState::Falling(falling),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let cancel_drop = falling.cancel_drop;
        let drop_item = falling.drop_item;
        let block = falling.state;
        let dropper = self.entity_view(id);
        if cancel_drop {
            self.remove_falling(id, EntityRemoveCause::Despawn);
            return;
        }
        let replaced = self.block_or_air(pos);
        let replaceable = replaced.is_air() || replaced.flags().contains(StateFlags::REPLACEABLE);
        let concrete = gravity(original) == Gravity::ConcretePowder;
        let free_below = self
            .world
            .chunks
            .block(step(pos, Face::Down))
            .is_some_and(is_free)
            && !(concrete && in_water);
        if replaceable && !free_below && self.survives(pos, block) {
            let state = if block.property("waterlogged").is_some() && water_source(replaced) {
                waterlogged(block, true)
            } else {
                block
            };
            if let Some(dropper) = dropper {
                let step = Grief::Land(Box::new(Landing {
                    falling: id,
                    pos,
                    original,
                    replaced,
                    state,
                    drop_item,
                }));
                self.change_block(dropper, pos, state, step, false);
            }
            return;
        }
        self.remove_falling(id, EntityRemoveCause::Drop);
        if drop_item {
            self.drop_item(dropper, original, pos);
        }
    }

    fn landing(&self, id: EntityId) -> bool {
        self.decisions.anywhere(|then| {
            matches!(then, Continuation::ChangeBlock(step)
                if matches!(&step.change, Grief::Land(landing) if landing.falling == id))
        })
    }

    pub(crate) fn finish_land(&mut self, landing: Landing, cancelled: bool) {
        let Landing {
            falling: id,
            pos,
            original,
            replaced,
            state,
            drop_item,
        } = landing;
        if self.entities.store.get(id).is_none() {
            return;
        }
        let dropper = self.entity_view(id);
        self.remove_falling(id, EntityRemoveCause::Despawn);
        if cancelled {
            return;
        }
        if self.block_or_air(pos) != replaced {
            if drop_item {
                self.drop_item(dropper, original, pos);
            }
            return;
        }
        if self.set_block(pos, state) {
            self.on_land(pos, original, replaced);
        } else if drop_item {
            self.drop_item(dropper, original, pos);
        }
    }

    fn on_land(&mut self, pos: BlockPos, original: BlockStateId, replaced: BlockStateId) {
        if let Some(concrete) = original.kind().and_then(BlockKind::concrete)
            && (has_water(replaced) || self.touches_liquid(pos))
        {
            self.set_block(pos, concrete.default_state());
        }
    }

    fn drop_falling(&mut self, id: EntityId, original: BlockStateId, pos: BlockPos) {
        let drop_item = matches!(
            self.entities.store.get(id).map(|e| &e.state),
            Some(EntityState::Falling(state)) if state.drop_item
        );
        let dropper = self.entity_view(id);
        self.remove_falling(id, EntityRemoveCause::Drop);
        if drop_item {
            self.drop_item(dropper, original, pos);
        }
    }

    fn drop_item(&mut self, dropper: Option<EntityView>, original: BlockStateId, pos: BlockPos) {
        let Some(item) = original.kind().and_then(BlockKind::item) else {
            return;
        };
        let stack = ItemStack::new(ItemId(item), NonZeroU8::MIN);
        let launch = self.block_drop_launch(pos);
        let item = crate::items::launched(launch, stack);
        match dropper {
            Some(dropper) => self.drop_from(dropper, item),
            None => self.add_entity(item, CreatureSpawnReason::Default),
        }
    }

    fn remove_falling(&mut self, id: EntityId, cause: EntityRemoveCause) {
        self.remove_entity(id, RemovalReason::Discarded, Some(cause));
    }

    pub fn falling_blocks(&self) -> usize {
        self.entities
            .store
            .count_where(|e| matches!(e.state, EntityState::Falling(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anvil_damage_grows_two_per_block_after_the_first_and_caps_at_forty() {
        assert_eq!(FallHurt::ANVIL.damage(0.0), 0.0);
        assert_eq!(FallHurt::ANVIL.damage(1.0), 0.0);
        assert_eq!(FallHurt::ANVIL.damage(1.5), 2.0);
        assert_eq!(FallHurt::ANVIL.damage(3.0), 4.0);
        assert_eq!(FallHurt::ANVIL.damage(21.0), 40.0);
        assert_eq!(FallHurt::ANVIL.damage(80.0), 40.0);
    }

    #[test]
    fn stalactite_damage_scales_with_column_size_of_at_least_six() {
        assert_eq!(FallHurt::stalactite(1).damage(3.0), 12.0);
        assert_eq!(FallHurt::stalactite(4).damage(3.0), 12.0);
        assert_eq!(FallHurt::stalactite(8).damage(3.0), 16.0);
        assert_eq!(FallHurt::stalactite(8).damage(10.0), 40.0);
        assert_eq!(FallHurt::stalactite(8).damage(0.5), 0.0);
    }

    #[test]
    fn anvil_degrade_chance_follows_fall_distance() {
        assert_eq!(anvil_degrade_chance(0.5), 0.0);
        assert_eq!(anvil_degrade_chance(1.0), 0.0);
        assert!((anvil_degrade_chance(2.0) - 0.1).abs() < 1e-6);
        assert!((anvil_degrade_chance(5.5) - 0.3).abs() < 1e-6);
        assert!(anvil_degrade_chance(20.0) >= 1.0);
    }
}
