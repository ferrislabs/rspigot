use rand::Rng;
use rspigot_ai::{MobView, TurtleAction, TurtleFacts};
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::species::{Species, turtle};
use rspigot_domain::entity::{Entity, EntityState, ageable};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::game_events::GameEventContext;
use crate::mob_actions::new_mob;
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::Grief;
use crate::ports::ClientLink;
use crate::server::Server;

const EGG_HATCH_CHANCE_DIMENSION: &str = "minecraft:overworld";
const RANDOM_TICKS_PER_SECTION: usize = 3;
const SECTION_SIZE: i32 = 16;
const LAY_PITCH_BASE: f32 = 0.9;
const LAY_PITCH_SPREAD: f32 = 0.2;
const LAY_VOLUME: f32 = 0.3;
const CRACK_VOLUME: f32 = 0.7;
const MAX_EGGS: i32 = 4;
const EGG_STEP_ODDS: i32 = 100;
const EGG_FALL_ODDS: i32 = 3;
const HATCH_SPREAD_X: f64 = 0.3;
const HATCH_SPREAD_STEP: f64 = 0.2;
const HATCH_SPREAD_Z: f64 = 0.3;
const PLANT_GROWTH_EVENT: i32 = 2012;
const PLANT_GROWTH_DATA: i32 = 15;
const BLOCK_BREAK_EVENT: i32 = 2001;
const EGG_LAND_STEP: f64 = 0.2;

pub(crate) fn egg_count(state: BlockStateId) -> i32 {
    state
        .property("eggs")
        .and_then(|eggs| eggs.parse().ok())
        .unwrap_or(1)
}

pub(crate) fn hatch_stage(state: BlockStateId) -> i32 {
    state
        .property("hatch")
        .and_then(|hatch| hatch.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn egg_state(hatch: i32, eggs: i32) -> BlockStateId {
    BlockKind::TurtleEgg
        .state_with(&[("hatch", &hatch.to_string()), ("eggs", &eggs.to_string())])
        .unwrap_or_else(|| BlockKind::TurtleEgg.default_state())
}

fn turtle_state(entity: &Entity) -> Option<turtle::TurtleState> {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Turtle(state)) => Some(state),
            _ => None,
        },
        _ => None,
    }
}

fn turtle_facts_of(entity: &Entity) -> TurtleFacts {
    let state = turtle_state(entity).unwrap_or_default();
    TurtleFacts {
        home_pos: state.home_pos,
        has_egg: turtle::has_egg(&entity.base),
        laying_egg: turtle::is_laying_egg(&entity.base),
        lay_egg_counter: state.lay_egg_counter,
        going_home: state.going_home,
        travel_pos: state.travel_pos,
    }
}

fn sampler_seed(game_time: i64) -> i64 {
    let mut z = (game_time as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as i64
}

fn block_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

fn is_sand(state: BlockStateId) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", turtle::SAND_TAG)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn turtle_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.turtle = turtle_facts_of(entity);
    }

    fn on_sand(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .block(pos.offset(0, -1, 0))
            .is_some_and(is_sand)
    }

    pub(crate) fn turtle_action(&mut self, id: EntityId, action: TurtleAction) {
        match action {
            TurtleAction::LayEgg { target } => self.turtle_lays_egg(id, target),
        }
    }

    fn turtle_lays_egg(&mut self, id: EntityId, target: BlockPos) {
        let Some(at) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position.block())
        else {
            return;
        };
        let mut random = self.mob_random(id);
        let announced = random.next_int(MAX_EGGS) + 1;
        let placed = random.next_int(MAX_EGGS) + 1;
        self.store_mob_random(id, random);
        if let Some(Entity {
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable.fall_in_love(turtle::LOVE_AFTER_LAYING, None, None);
        }
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let refused = false;
        self.change_block(
            view,
            target,
            egg_state(0, announced),
            Grief::TurtleEgg {
                turtle: id,
                at,
                pos: target,
                state: egg_state(0, placed),
            },
            refused,
        );
    }

    pub(crate) fn finish_turtle_egg(
        &mut self,
        id: EntityId,
        at: BlockPos,
        pos: BlockPos,
        state: BlockStateId,
    ) {
        let pitch = LAY_PITCH_BASE + self.rng.random::<f32>() * LAY_PITCH_SPREAD;
        self.play_sound(
            None,
            block_center(at),
            turtle::LAY_EGG_SOUND,
            SoundSource::Blocks,
            LAY_VOLUME,
            pitch,
        );
        self.set_block(pos, state);
        self.game_event(
            GameEvent::BlockPlace,
            block_center(pos),
            GameEventContext::entity_in(Some(id), state),
        );
    }

    pub(crate) fn turtle_egg_placed(&mut self, pos: BlockPos) {
        if self.on_sand(pos) {
            self.level_event(PLANT_GROWTH_EVENT, pos, PLANT_GROWTH_DATA);
        }
    }

    pub(crate) fn turtle_breed(&mut self, parent: EntityId, partner: EntityId) {
        let Some(position) = self
            .entities
            .store
            .get(parent)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        if let Some(entity) = self.entities.store.get_mut(parent) {
            turtle::set_has_egg(&mut entity.base, true);
        }
        for id in [parent, partner] {
            if let Some(Entity {
                base,
                state: EntityState::Living(living),
                ..
            }) = self.entities.store.get_mut(id)
                && let Some(state) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                ageable::set_age(base, state, turtle::EGG_AGE_AFTER_BREEDING);
                state.reset_love();
            }
        }
        let mut random = self.mob_random(parent);
        let experience = random.next_int(turtle::EGG_MAX_EXPERIENCE) + 1;
        self.store_mob_random(parent, random);
        if self.config.rules.mob_drops {
            self.spawn_orb(position, experience);
        }
    }

    pub(crate) fn turtle_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = turtle_state(entity) else {
            return;
        };
        let alive = entity.living().is_some_and(|living| {
            !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
        });
        if !alive
            || !turtle::is_laying_egg(&entity.base)
            || state.lay_egg_counter <= 0
            || state.lay_egg_counter % turtle::LAY_PERIOD != 0
        {
            return;
        }
        let pos = entity.base.position.block();
        if !self.on_sand(pos) {
            return;
        }
        let below = self
            .world
            .chunks
            .block(pos.offset(0, -1, 0))
            .unwrap_or(BlockStateId::AIR);
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(below.0));
        self.game_event(
            GameEvent::EntityAction,
            block_center(pos),
            GameEventContext::of(Some(id)),
        );
    }

    pub(crate) fn turtle_grew_up(&mut self, id: EntityId) {
        if self.config.rules.mob_drops {
            self.drop_gift_loot(id, turtle::GROW_LOOT);
        }
    }

    fn egg_hatch_chance(&self) -> f32 {
        if self.world.dimension.key.as_str() != EGG_HATCH_CHANCE_DIMENSION {
            return 0.0;
        }
        turtle::hatch_chance(self.world.level.day_time)
    }

    pub(crate) fn tick_turtle_eggs(&mut self) {
        let chunks = self.random_tick_chunks();
        if chunks.is_empty() {
            return;
        }
        let height = self.world.dimension.height;
        let sections = (height.min_y..height.min_y + height.height as i32)
            .step_by(SECTION_SIZE as usize)
            .collect::<Vec<i32>>();
        let mut sampler = LegacyRandom::new(sampler_seed(self.world.level.game_time));
        let mut eggs = Vec::new();
        for chunk in chunks {
            let Some(column) = self.world.chunks.get(chunk) else {
                continue;
            };
            for base_y in &sections {
                for _ in 0..RANDOM_TICKS_PER_SECTION {
                    let x = chunk.x * SECTION_SIZE + sampler.next_int(SECTION_SIZE);
                    let y = base_y + sampler.next_int(SECTION_SIZE);
                    let z = chunk.z * SECTION_SIZE + sampler.next_int(SECTION_SIZE);
                    let pos = BlockPos::new(x, y, z);
                    if column.block(pos).and_then(BlockStateId::kind) == Some(BlockKind::TurtleEgg)
                    {
                        eggs.push(pos);
                    }
                }
            }
        }
        for pos in eggs {
            self.turtle_egg_random_tick(pos);
        }
    }

    fn turtle_egg_random_tick(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.kind() != Some(BlockKind::TurtleEgg) {
            return;
        }
        let chance = self.egg_hatch_chance();
        if !(chance > 0.0 && self.rng.random::<f32>() < chance) || !self.on_sand(pos) {
            return;
        }
        let stage = hatch_stage(state);
        if stage < 2 {
            let grown = egg_state(stage + 1, egg_count(state));
            self.grow_block(crate::plugins::Lane::World, pos, grown);
            return;
        }
        self.turtle_egg_hatches(pos, state);
    }

    pub(crate) fn finish_turtle_egg_crack(&mut self, pos: BlockPos) {
        let pitch = LAY_PITCH_BASE + self.rng.random::<f32>() * LAY_PITCH_SPREAD;
        self.play_sound(
            None,
            block_center(pos),
            turtle::EGG_CRACK_SOUND,
            SoundSource::Blocks,
            CRACK_VOLUME,
            pitch,
        );
        let state = self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR);
        self.game_event(
            GameEvent::BlockChange,
            block_center(pos),
            GameEventContext::block(state),
        );
    }

    fn turtle_egg_hatches(&mut self, pos: BlockPos, state: BlockStateId) {
        let pitch = LAY_PITCH_BASE + self.rng.random::<f32>() * LAY_PITCH_SPREAD;
        self.play_sound(
            None,
            block_center(pos),
            turtle::EGG_HATCH_SOUND,
            SoundSource::Blocks,
            CRACK_VOLUME,
            pitch,
        );
        self.set_block(pos, BlockStateId::AIR);
        self.game_event(
            GameEvent::BlockDestroy,
            block_center(pos),
            GameEventContext::block(state),
        );
        for index in 0..egg_count(state) {
            self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
            let mut child = new_mob(self.new_entity_base(Kind::Turtle));
            if let EntityState::Living(living) = &mut child.state {
                if let Some(state) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
                    ageable::set_age(&mut child.base, state, ageable::BABY_START_AGE);
                }
                if let Some(Species::Turtle(turtle_state)) =
                    living.mob.as_mut().map(|mob| &mut mob.species)
                {
                    turtle_state.home_pos = pos;
                }
            }
            let at = Vec3::new(
                f64::from(pos.x) + HATCH_SPREAD_X + f64::from(index) * HATCH_SPREAD_STEP,
                f64::from(pos.y),
                f64::from(pos.z) + HATCH_SPREAD_Z,
            );
            child.base.snap_to(at, 0.0, 0.0);
            let own = LegacyRandom::new(self.rng.random());
            self.spawn_entity(child, Birth::of(CreatureSpawnReason::Egg).with_random(own));
        }
    }

    fn can_destroy_egg(&self, entity: EntityId, player: bool) -> bool {
        let Some(other) = self.entities.store.get(entity) else {
            return player;
        };
        match other.base.kind {
            Kind::Turtle | Kind::Bat => false,
            kind if kind == Kind::Player || player => true,
            _ => other.living().is_some() && self.config.rules.mob_griefing,
        }
    }

    pub(crate) fn egg_trampled(
        &mut self,
        entity: EntityId,
        player: bool,
        pos: BlockPos,
        odds: i32,
    ) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.kind() != Some(BlockKind::TurtleEgg) || !self.can_destroy_egg(entity, player) {
            return;
        }
        if self.rng.random_range(0..odds) != 0 {
            return;
        }
        self.decrease_eggs(pos, state);
    }

    pub(crate) fn egg_stepped_on(&mut self, entity: EntityId, player: bool, pos: BlockPos) {
        self.egg_trampled(entity, player, pos, EGG_STEP_ODDS);
    }

    pub(crate) fn egg_fallen_on(&mut self, entity: EntityId, player: bool, pos: BlockPos) {
        let zombie = self
            .entities
            .store
            .get(entity)
            .is_some_and(|other| rspigot_domain::entity::species::is_zombie(other.base.kind));
        if !zombie {
            self.egg_trampled(entity, player, pos, EGG_FALL_ODDS);
        }
    }

    pub(crate) fn decrease_eggs(&mut self, pos: BlockPos, state: BlockStateId) {
        let pitch = LAY_PITCH_BASE + self.rng.random::<f32>() * LAY_PITCH_SPREAD;
        self.play_sound(
            None,
            block_center(pos),
            turtle::EGG_BREAK_SOUND,
            SoundSource::Blocks,
            CRACK_VOLUME,
            pitch,
        );
        let eggs = egg_count(state);
        if eggs < 2 {
            self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
            self.set_block(pos, BlockStateId::AIR);
            return;
        }
        self.set_block(pos, egg_state(hatch_stage(state), eggs - 1));
        self.game_event(
            GameEvent::BlockDestroy,
            block_center(pos),
            GameEventContext::block(state),
        );
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
    }

    pub(crate) fn mob_steps_on_egg(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !entity.base.on_ground {
            return;
        }
        let position = entity.base.position;
        let below = Vec3::new(position.x, position.y - EGG_LAND_STEP, position.z).block();
        if self.world.chunks.block(below).and_then(BlockStateId::kind) == Some(BlockKind::TurtleEgg)
        {
            self.egg_stepped_on(id, false, below);
        }
    }

    pub(crate) fn random_tick_chunks(&self) -> Vec<ChunkPos> {
        self.ticking_chunks_near_players()
    }
}
