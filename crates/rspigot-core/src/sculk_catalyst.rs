use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::reward;
use rspigot_domain::player::PlayerId;
use rspigot_domain::sculk_spread::{self as spread, Cursor};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_nbt::{Compound, Tag};

use crate::advancements::AdvancementEvent;
use crate::game_events::TimerKind;
use crate::mobs::ServerRandom;
use crate::plugins::events::SculkBloom;
use crate::plugins::gaps::{BloomStep, GapStep};
use crate::plugins::{EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;

const CURSORS: &str = "cursors";
const BLOOM: &str = "bloom";
const SHRIEKER_ODDS: i32 = 11;
const NEIGHBOURS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

fn is_replaceable(state: BlockStateId) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", spread::REPLACEABLE_TAG)
            .is_some_and(|tag| tag.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn centre(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

fn pos_of(cursor: &Cursor) -> BlockPos {
    BlockPos::new(cursor.x, cursor.y, cursor.z)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn sculk_catalyst_death(&mut self, victim: EntityId, killer: Option<PlayerId>) {
        let Some(entity) = self.entities.store.get(victim) else {
            return;
        };
        if entity.living().is_none() || !reward::should_drop_experience(entity) {
            return;
        }
        let at = entity.base.position;
        let Some(catalyst) = self.catalyst_listening(at) else {
            return;
        };
        let Some(entity) = self.entities.store.get(victim) else {
            return;
        };
        let experience = reward::base_experience_reward(entity, &mut ServerRandom(&mut self.rng));
        if experience <= 0 {
            return;
        }
        let step = BloomStep {
            catalyst,
            at,
            experience,
            killer,
            victim,
        };
        self.fire_gap(Lane::Block(catalyst), GapStep::Bloom(step), move |_| {
            Some(EventPayload::SculkBloom(SculkBloom {
                block: catalyst,
                charge: experience,
            }))
        });
    }

    pub(crate) fn finish_catalyst_death(&mut self, step: BloomStep) {
        let BloomStep {
            catalyst,
            at,
            experience,
            killer,
            victim,
        } = step;
        if experience <= 0 {
            return;
        }
        let start = at.block();
        let mut cursors = self.catalyst_cursors(catalyst);
        for charge in spread::split_charge(experience) {
            cursors.push(Cursor {
                x: start.x,
                y: start.y,
                z: start.z,
                charge,
                update_delay: 1,
            });
        }
        cursors.truncate(spread::MAX_CURSORS);
        self.store_catalyst_cursors(catalyst, &cursors);
        self.schedule_sculk(catalyst, 1, TimerKind::Catalyst);
        if let Some(player) = killer {
            let from = self.players.get(&player).map_or(at, |p| p.position);
            let mut event = AdvancementEvent::named("minecraft:kill_mob_near_sculk_catalyst");
            event.entities = self.slot_near("entity", victim, from).into_iter().collect();
            self.trigger_event(player, event);
        }
        self.catalyst_bloom(catalyst);
    }

    fn catalyst_listening(&self, at: Vec3) -> Option<BlockPos> {
        let origin = at.block();
        let range = spread::CATALYST_RANGE;
        let limit = f64::from(range * range);
        let mut best: Option<(f64, BlockPos)> = None;
        for dx in -range..=range {
            for dy in -range..=range {
                for dz in -range..=range {
                    let pos = origin.offset(dx, dy, dz);
                    let distance = centre(pos).distance_squared(at);
                    if distance > limit || best.is_some_and(|(nearest, _)| nearest <= distance) {
                        continue;
                    }
                    if self
                        .world
                        .chunks
                        .block(pos)
                        .is_some_and(|state| state.kind() == Some(BlockKind::SculkCatalyst))
                    {
                        best = Some((distance, pos));
                    }
                }
            }
        }
        best.map(|(_, pos)| pos)
    }

    fn catalyst_bloom(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if let Some(bloomed) = state.with_property(BLOOM, "true") {
            self.set_block(pos, bloomed);
        }
        let pitch = 0.6 + self.rng.random::<f32>() * 0.4;
        self.play_sound(
            None,
            centre(pos),
            spread::BLOOM_SOUND,
            SoundSource::Blocks,
            2.0,
            pitch,
        );
        self.schedule_sculk(pos, spread::BLOOM_TICKS, TimerKind::CatalystBloom);
    }

    pub(crate) fn catalyst_bloom_end(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.kind() == Some(BlockKind::SculkCatalyst)
            && let Some(calm) = state.with_property(BLOOM, "false")
        {
            self.set_block(pos, calm);
        }
    }

    fn catalyst_cursors(&self, pos: BlockPos) -> Vec<Cursor> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| match entity.data.get(CURSORS) {
                Some(Tag::IntArray(data)) => Some(spread::unpack(data)),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn store_catalyst_cursors(&mut self, pos: BlockPos, cursors: &[Cursor]) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut entity = chunk
            .block_entity(pos)
            .cloned()
            .unwrap_or_else(|| BlockEntity {
                pos,
                kind: Identifier::minecraft("sculk_catalyst"),
                data: Compound::new(),
            });
        entity
            .data
            .insert(CURSORS, Tag::IntArray(spread::pack(cursors)));
        chunk.set_block_entity(entity);
    }

    pub(crate) fn catalyst_tick(&mut self, pos: BlockPos) {
        if self
            .world
            .chunks
            .block(pos)
            .is_none_or(|state| state.kind() != Some(BlockKind::SculkCatalyst))
        {
            return;
        }
        let cursors = self.catalyst_cursors(pos);
        let mut next: Vec<Cursor> = Vec::new();
        for cursor in cursors {
            let moved = self.update_cursor(pos, cursor);
            if moved.charge <= 0 {
                continue;
            }
            match next
                .iter_mut()
                .find(|other| (other.x, other.y, other.z) == (moved.x, moved.y, moved.z))
            {
                Some(other) => spread::merge(other, &moved),
                None => next.push(moved),
            }
        }
        next.truncate(spread::MAX_CURSORS);
        self.store_catalyst_cursors(pos, &next);
        if !next.is_empty() {
            self.schedule_sculk(pos, 1, TimerKind::Catalyst);
        }
    }

    fn update_cursor(&mut self, catalyst: BlockPos, cursor: Cursor) -> Cursor {
        if cursor.update_delay > 0 {
            return Cursor {
                update_delay: cursor.update_delay - 1,
                ..cursor
            };
        }
        let pos = pos_of(&cursor);
        if self.world.chunks.block(pos).is_none() {
            return cursor;
        }
        let order = self.shuffled_neighbours();
        for (dx, dy, dz) in order {
            let target = pos.offset(dx, dy, dz);
            if self.world.chunks.block(target).is_some_and(is_replaceable) {
                self.set_block(target, BlockKind::Sculk.default_state());
                self.play_sound(
                    None,
                    centre(target),
                    spread::SPREAD_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                return Cursor {
                    x: target.x,
                    y: target.y,
                    z: target.z,
                    charge: cursor.charge - 1,
                    update_delay: 0,
                };
            }
        }
        let on_sculk = self
            .world
            .chunks
            .block(pos)
            .is_some_and(|state| state.kind() == Some(BlockKind::Sculk));
        let mut charge = cursor.charge;
        if on_sculk {
            charge = self.sculk_growth(catalyst, pos, charge);
        } else if self.rng.random_range(0..spread::CHARGE_DECAY_RATE) == 0 {
            charge = spread::halved(charge);
        }
        let order = self.shuffled_neighbours();
        let step = order
            .into_iter()
            .map(|(dx, dy, dz)| pos.offset(dx, dy, dz))
            .find(|target| {
                self.world
                    .chunks
                    .block(*target)
                    .is_some_and(|state| state.kind() == Some(BlockKind::Sculk))
            });
        let at = step.unwrap_or(pos);
        Cursor {
            x: at.x,
            y: at.y,
            z: at.z,
            charge,
            update_delay: 0,
        }
    }

    fn sculk_growth(&mut self, catalyst: BlockPos, pos: BlockPos, charge: i32) -> i32 {
        if charge == 0 || self.rng.random_range(0..spread::CHARGE_DECAY_RATE) != 0 {
            return charge;
        }
        let distance = centre(pos).distance_squared(centre(catalyst));
        let near = distance < f64::from(spread::NO_GROWTH_RADIUS * spread::NO_GROWTH_RADIUS);
        let above = pos.offset(0, 1, 0);
        let free = self
            .world
            .chunks
            .block(above)
            .is_some_and(BlockStateId::is_air);
        if !near && free {
            if self.rng.random_range(0..spread::GROWTH_SPAWN_COST) < charge {
                self.grow_on(above);
            }
            return (charge - spread::GROWTH_SPAWN_COST).max(0);
        }
        if self.rng.random_range(0..spread::ADDITIONAL_DECAY_RATE) != 0 {
            charge
        } else if near {
            charge - 1
        } else {
            charge - spread::decay_penalty(distance, charge)
        }
    }

    fn grow_on(&mut self, pos: BlockPos) {
        let shrieker = self.rng.random_range(0..SHRIEKER_ODDS) == 0;
        let (kind, name) = if shrieker {
            (BlockKind::SculkShrieker, "sculk_shrieker")
        } else {
            (BlockKind::SculkSensor, "sculk_sensor")
        };
        let state = if shrieker {
            kind.state_with(&[("can_summon", "true")])
                .unwrap_or_else(|| kind.default_state())
        } else {
            kind.default_state()
        };
        self.set_block(pos, state);
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(name),
                data: Compound::new(),
            });
        }
    }

    fn shuffled_neighbours(&mut self) -> [(i32, i32, i32); 6] {
        let mut order = NEIGHBOURS;
        for index in (1..order.len()).rev() {
            let pick = self.rng.random_range(0..=index);
            order.swap(index, pick);
        }
        order
    }
}
