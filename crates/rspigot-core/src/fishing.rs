use rand::Rng;
use rspigot_domain::entity::hook::{self, HookPhase, HookState};
use rspigot_domain::entity::physics::{self, MoveTraits};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, arrow, raycast, thrown};
use rspigot_domain::item::ItemStack;
use rspigot_domain::particle::Particle;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3, mth};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::loot::{self, LootContext, LootEntity};
use crate::plugins::events::{
    CreatureSpawnReason, EntityRemoveCause, EquipmentSlot, PlayerFish, PlayerFishPatch,
    PlayerFishState,
};
use crate::plugins::steps::{Birth, CastStep, ReelStage, ReelStep, Spawned};
use crate::plugins::{Continuation, EventKind, EventPayload, view_of_entity};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::stats::custom;

const FISHING_TABLE: &str = "minecraft:gameplay/fishing";
const LUCK_OF_THE_SEA: &str = "minecraft:luck_of_the_sea";
const LURE: &str = "minecraft:lure";
const LURE_TICKS_PER_LEVEL: i32 = 100;
const THROW_SOUND: &str = "minecraft:entity.fishing_bobber.throw";
const RETRIEVE_SOUND: &str = "minecraft:entity.fishing_bobber.retrieve";
const SPLASH_SOUND: &str = "minecraft:entity.fishing_bobber.splash";
const THROW_VOLUME: f32 = 0.5;
const SPLASH_VOLUME: f32 = 0.25;
const PITCH_BASE: f32 = 0.4;
const PITCH_SPREAD: f32 = 0.4;
const PITCH_FLOOR: f32 = 0.8;
const ITEM_PICKUP_DELAY: u16 = 10;
const ITEM_PULL: f64 = 0.1;
const ITEM_LIFT: f64 = 0.08;
const PULL_STATUS: i8 = 31;
const EXP_SPREAD: i32 = 6;
const XP_OFFSET: f64 = 0.5;
const PARTICLE_RANGE: f64 = 64.0;
const OPEN_WATER_RADIUS: i32 = 2;
const DAMAGE_ITEM: i32 = 3;
const DAMAGE_ENTITY: i32 = 5;
const DAMAGE_GROUND: i32 = 2;
const DAMAGE_FISH: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Invalid,
    AboveWater,
    InsideWater,
}

fn slot_of(hand: Hand) -> EquipmentSlot {
    match hand {
        Hand::Main => EquipmentSlot::Hand,
        Hand::Off => EquipmentSlot::OffHand,
    }
}

fn level_of(rod: &ItemStack, enchantment: &str) -> i32 {
    rod.enchantments()
        .iter()
        .find(|(name, _)| name == enchantment)
        .map_or(0, |(_, level)| i32::from(*level))
}

enum HookOf {
    Active(EntityId),
    Pending,
    Absent,
}

impl<L: ClientLink> Server<L> {
    fn hook_of(&self, player: PlayerId) -> HookOf {
        let Some(hook) = self.players.get(&player).and_then(|p| p.fishing) else {
            return HookOf::Absent;
        };
        match self.entities.store.get(hook) {
            Some(entity) if matches!(entity.state, EntityState::Hook(_)) => HookOf::Active(hook),
            _ if self.spawning_now(hook) => HookOf::Pending,
            _ => HookOf::Absent,
        }
    }

    pub(crate) fn forget_hook(&mut self, player: PlayerId) {
        if let Some(player) = self.players.get_mut(&player) {
            player.fishing = None;
        }
    }

    pub(crate) fn use_fishing_rod(&mut self, id: PlayerId, hand: Hand, rotation: Rotation) -> bool {
        let Some(rod) = self
            .held(id, hand)
            .filter(|stack| stack.item == ItemKind::FishingRod.id())
        else {
            return false;
        };
        match self.hook_of(id) {
            HookOf::Pending => {}
            HookOf::Active(hook) => self.reel_in(id, hand, hook, &rod),
            HookOf::Absent => self.cast_hook(id, hand, &rod, rotation),
        }
        true
    }

    fn cast_hook(&mut self, id: PlayerId, hand: Hand, rod: &ItemStack, rotation: Rotation) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let (owner, eye) = (player.profile.id.0, player.eye_position());
        let luck = level_of(rod, LUCK_OF_THE_SEA);
        let lure = level_of(rod, LURE) * LURE_TICKS_PER_LEVEL;
        let mut base = self.new_entity_base(Kind::FishingBobber);
        let seed = self.rng.random::<i64>();
        let mut state = HookState::new(&mut base, seed, luck, lure);
        state.owner = Some(owner);
        hook::launch(&mut base, &mut state, eye, rotation.yaw, rotation.pitch);
        let entity = Entity::new(base, EntityState::Hook(Box::new(state)));
        if let Some(player) = self.players.get_mut(&id) {
            player.fishing = Some(entity.base.id);
        }
        let view = view_of_entity(&entity);
        let step = CastStep {
            player: id,
            hand,
            hook: entity,
        };
        self.fire(id, Continuation::Cast(Box::new(step)), move |server| {
            Some(EventPayload::PlayerFish(PlayerFish {
                caught: None,
                exp_to_drop: 0,
                hand: Some(slot_of(hand)),
                hook: view,
                player: server.snapshot_of(id)?,
                state: PlayerFishState::Fishing,
            }))
        });
    }

    pub(crate) fn finish_cast(&mut self, step: CastStep, cancelled: bool) {
        let CastStep { player, hand, hook } = step;
        if cancelled {
            self.forget_hook(player);
            return;
        }
        let Some(position) = self.players.get(&player).map(|p| p.position) else {
            return;
        };
        let pitch = PITCH_BASE / (self.rng.random::<f32>() * PITCH_SPREAD + PITCH_FLOOR);
        self.play_sound(
            Some(player),
            position,
            THROW_SOUND,
            SoundSource::Neutral,
            THROW_VOLUME,
            pitch,
        );
        self.spawn_entity(
            hook,
            Birth::of(CreatureSpawnReason::Default)
                .by(player)
                .then(Spawned::Hook { player }),
        );
        self.player_swing(player, hand, false);
    }

    fn fish_payload(
        &self,
        player: PlayerId,
        hook: EntityId,
        caught: Option<rspigot_domain::EntityId>,
        item: Option<&Entity>,
        exp: i32,
        state: PlayerFishState,
    ) -> Option<EventPayload> {
        let caught = match (item, caught) {
            (Some(item), _) => Some(view_of_entity(item)),
            (None, Some(id)) => self.subject_view(id),
            (None, None) => None,
        };
        Some(EventPayload::PlayerFish(PlayerFish {
            caught,
            exp_to_drop: exp,
            hand: None,
            hook: self.entity_view(hook)?,
            player: self.snapshot_of(player)?,
            state,
        }))
    }

    fn notice_fish(&mut self, player: PlayerId, hook: EntityId, state: PlayerFishState) {
        self.fire(
            player,
            Continuation::Notice(EventKind::PlayerFish),
            move |server| server.fish_payload(player, hook, None, None, 0, state),
        );
    }

    fn hook_owner(&self, hook: EntityId) -> Option<PlayerId> {
        let EntityState::Hook(state) = &self.entities.store.get(hook)?.state else {
            return None;
        };
        let owner = state.owner?;
        self.players
            .values()
            .find(|player| player.profile.id.0 == owner)
            .map(|player| player.id)
    }

    fn hook_should_stop(&mut self, hook: EntityId, player: PlayerId) -> bool {
        let Some(owner) = self.players.get(&player) else {
            return true;
        };
        let holds_rod = [Hand::Main, Hand::Off].into_iter().any(|hand| {
            self.held(player, hand)
                .is_some_and(|stack| stack.item == ItemKind::FishingRod.id())
        });
        let near = self.entities.store.get(hook).is_some_and(|entity| {
            entity.base.position.distance_squared(owner.position) <= hook::MAX_DISTANCE_SQR
        });
        let active = !owner.vitals.is_dead() && owner.game_mode != GameMode::Spectator;
        if active && holds_rod && near {
            return false;
        }
        self.discard_hook(hook);
        true
    }

    fn discard_hook(&mut self, hook: EntityId) {
        let owner = self.hook_owner(hook);
        self.remove_entity(
            hook,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
        if let Some(owner) = owner {
            self.forget_hook(owner);
        }
    }

    fn hook_waits(&self, hook: EntityId) -> bool {
        self.decisions.anywhere(|then| match then {
            Continuation::Bite(waiting) => *waiting == hook,
            Continuation::Reel(step) => step.hook == hook,
            Continuation::ProjectileHit { arrow, .. } => *arrow == hook,
            _ => false,
        })
    }

    pub(crate) fn tick_hook(&mut self, id: EntityId) {
        if self.hook_waits(id) {
            return;
        }
        let Some(player) = self.hook_owner(id) else {
            self.discard_hook(id);
            return;
        };
        if self.hook_should_stop(id, player) {
            return;
        }
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        if base.on_ground {
            state.life += 1;
            if state.life > hook::MAX_LIFE {
                self.discard_hook(id);
                return;
            }
        } else {
            state.life = 0;
        }
        let pos = base.position.block();
        let height = physics::water_height(&self.world.chunks, pos);
        let in_water = height.is_some_and(|h| h > 0.0);
        let fluid_height = height.map_or(0.0, |h| h as f32);
        match state.phase {
            HookPhase::Flying => {
                if state.hooked.is_some() {
                    base.velocity = Vec3::ZERO;
                    state.phase = HookPhase::HookedInEntity;
                    return;
                }
                if in_water {
                    let (x, y, z) = hook::WATER_ENTRY;
                    base.velocity = base.velocity.multiply(x, y, z);
                    state.phase = HookPhase::Bobbing;
                    return;
                }
                self.hook_strike(id);
            }
            HookPhase::HookedInEntity => {
                let hooked = state.hooked;
                let follow = hooked
                    .filter(|target| {
                        self.entities
                            .store
                            .get(*target)
                            .is_none_or(|entity| !entity.base.is_removed())
                    })
                    .and_then(|target| self.entity_box(target));
                let Some(Entity {
                    base,
                    state: EntityState::Hook(state),
                    ..
                }) = self.entities.store.get_mut(id)
                else {
                    return;
                };
                match follow {
                    Some((position, box_height, _)) => {
                        base.position = Vec3::new(
                            position.x,
                            position.y + box_height * hook::HOOKED_HEIGHT,
                            position.z,
                        );
                        base.needs_sync = true;
                    }
                    None => {
                        hook::set_hooked(base, state, None);
                        state.phase = HookPhase::Flying;
                    }
                }
                return;
            }
            HookPhase::Bobbing => {
                self.hook_bob(id, pos, in_water, fluid_height);
            }
        }
        self.hook_move(id);
    }

    fn hook_bob(&mut self, id: EntityId, pos: BlockPos, in_water: bool, fluid_height: f32) {
        let checks_water = matches!(
            self.entities.store.get(id),
            Some(Entity { state: EntityState::Hook(state), .. })
                if (state.nibble >= 1 || state.time_until_hooked >= 1)
                    && state.open_water
                    && state.out_of_water_time <= 9
        );
        let open = !checks_water || self.open_water_at(pos);
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let velocity = base.velocity;
        let mut offset =
            (base.position.y + velocity.y - f64::from(pos.y)) - f64::from(fluid_height);
        if offset.abs() < 0.01 {
            offset += offset.signum() * 0.1;
        }
        let rise = state.random.next_float();
        base.velocity = Vec3::new(
            velocity.x * hook::BOBBING_DRAG,
            velocity.y - offset * f64::from(rise) * 0.2,
            velocity.z * hook::BOBBING_DRAG,
        );
        if state.nibble < 1 && state.time_until_hooked < 1 {
            state.open_water = true;
        } else {
            state.open_water = state.open_water && state.out_of_water_time <= 9 && open;
        }
        if !in_water {
            state.out_of_water_time = (state.out_of_water_time + 1).min(hook::MAX_OUT_OF_WATER);
            return;
        }
        state.out_of_water_time = (state.out_of_water_time - 1).max(0);
        if hook::is_biting(base) {
            let dip =
                f64::from(state.random.next_float()) * -0.1 * f64::from(state.random.next_float());
            base.velocity = base.velocity + Vec3::new(0.0, dip, 0.0);
        }
        self.catching_fish(id, pos);
    }

    fn hook_move(&mut self, id: EntityId) {
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let in_water_block = physics::water_height(chunks, base.position.block()).is_some();
        if !in_water_block && !base.on_ground && state.hooked.is_none() {
            base.velocity = base.velocity + Vec3::new(0.0, -hook::GRAVITY, 0.0);
        }
        let movement = base.velocity;
        physics::move_entity(base, chunks, movement, &MoveTraits::plain(hook::GRAVITY));
        physics::apply_inside_blocks(base, chunks, false);
        thrown::update_rotation(base);
        if state.phase == HookPhase::Flying && (base.on_ground || base.horizontal_collision) {
            base.velocity = Vec3::ZERO;
        }
        base.velocity = base.velocity.scale(hook::DRAG);
        base.needs_sync = true;
    }

    fn hook_strike(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let from = base.position;
        let to = from + base.velocity;
        let margin = f64::from(arrow::hit_margin(base.tick_count));
        let area = arrow::search_box(base);
        let owner = self.owner_entity(state.owner);
        let left_owner = state.left_owner
            || owner
                .and_then(|owner| self.entity_box(owner))
                .is_none_or(|(_, _, bounds)| !bounds.intersects(&area));
        let block = raycast::clip_blocks(&self.world.chunks, from, to);
        let end = block.map_or(to, |hit| hit.location);
        let mut candidates = self.hit_candidates(id, owner, left_owner, &[], &area);
        candidates.extend(
            self.entities
                .store
                .iter()
                .filter(|e| {
                    matches!(e.state, EntityState::Item(_))
                        && !e.base.is_removed()
                        && e.base.bounding_box().intersects(&area)
                })
                .map(|e| (e.base.id, e.base.bounding_box(), e.base.position)),
        );
        let nearest = candidates
            .into_iter()
            .filter_map(|(target, bounds, _)| {
                raycast::clip_box(&bounds.inflate(margin), from, end).map(|at| (target, at))
            })
            .min_by(|(_, one), (_, other)| {
                from.distance_squared(*one)
                    .total_cmp(&from.distance_squared(*other))
            });
        if let Some(Entity {
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            state.left_owner = left_owner;
        }
        match (nearest, block) {
            (Some((target, _)), _) => self.ask_projectile_hit(id, target),
            (None, Some(hit)) => {
                self.notice_block_hit(id, hit);
                if let Some(Entity { base, .. }) = self.entities.store.get_mut(id) {
                    let distance = hit.location.distance_squared(base.position).sqrt();
                    base.velocity = base.velocity.normalize().scale(distance);
                }
            }
            (None, None) => {}
        }
    }

    pub(crate) fn hook_hit_entity(&mut self, id: EntityId, target: EntityId) {
        if let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            hook::set_hooked(base, state, Some(target));
        }
    }

    fn open_water_at(&self, pos: BlockPos) -> bool {
        let mut previous = Column::Invalid;
        for dy in -1..=2 {
            let low = pos.offset(-OPEN_WATER_RADIUS, dy, -OPEN_WATER_RADIUS);
            let high = pos.offset(OPEN_WATER_RADIUS, dy, OPEN_WATER_RADIUS);
            let column = self.column_type(low, high);
            match column {
                Column::AboveWater if previous == Column::Invalid => return false,
                Column::InsideWater if previous == Column::AboveWater => return false,
                Column::Invalid => return false,
                _ => {}
            }
            previous = column;
        }
        true
    }

    fn column_type(&self, from: BlockPos, to: BlockPos) -> Column {
        let mut found: Option<Column> = None;
        for x in from.x..=to.x {
            for y in from.y..=to.y {
                for z in from.z..=to.z {
                    let here = self.block_column(BlockPos::new(x, y, z));
                    found = Some(match (found, here) {
                        (None, here) => here,
                        (Some(Column::Invalid), _) | (_, Column::Invalid) => Column::Invalid,
                        (Some(one), other) if one == other => one,
                        _ => Column::Invalid,
                    });
                    if found == Some(Column::Invalid) {
                        return Column::Invalid;
                    }
                }
            }
        }
        found.unwrap_or(Column::Invalid)
    }

    fn block_column(&self, pos: BlockPos) -> Column {
        let Some(state) = self.world.chunks.block(pos) else {
            return Column::AboveWater;
        };
        if state.is_air() || state.kind() == Some(rspigot_blocks::BlockKind::LilyPad) {
            return Column::AboveWater;
        }
        let source_water = state.kind() == Some(rspigot_blocks::BlockKind::Water)
            && state
                .property("level")
                .and_then(|level| level.parse::<i32>().ok())
                == Some(0);
        if source_water {
            Column::InsideWater
        } else {
            Column::AboveWater
        }
    }

    fn catching_fish(&mut self, id: EntityId, pos: BlockPos) {
        let above = pos.offset(0, 1, 0);
        let rain = self.raining_at(above);
        let covered = !self.chunk_view().can_see_sky_at(above);
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let speed = hook::wait_modifier(rain, covered, &mut state.random);
        if state.nibble >= 1 {
            state.nibble -= 1;
            if state.nibble < 1 {
                state.time_until_lured = 0;
                state.time_until_hooked = 0;
                hook::set_biting(base, false);
                if let Some(player) = self.hook_owner(id) {
                    self.notice_fish(player, id, PlayerFishState::FailedAttempt);
                }
            }
            return;
        }
        if state.time_until_hooked >= 1 {
            state.time_until_hooked -= speed;
            if state.time_until_hooked < 1 {
                if let Some(player) = self.hook_owner(id) {
                    self.fire(player, Continuation::Bite(id), move |server| {
                        server.fish_payload(player, id, None, None, 0, PlayerFishState::Bite)
                    });
                }
                return;
            }
            state.fish_angle += state.random.triangle(0.0, 9.188) as f32;
            return;
        }
        if state.time_until_lured >= 1 {
            state.time_until_lured -= speed;
            let chance = hook::bite_chance(state.time_until_lured);
            if state.random.next_float() < chance {
                self.lure_particles(id);
            }
            let Some(Entity {
                state: EntityState::Hook(state),
                ..
            }) = self.entities.store.get_mut(id)
            else {
                return;
            };
            if state.time_until_lured < 1 {
                state.fish_angle = hook::next_float_between(&mut state.random, 0.0, 360.0);
                state.time_until_hooked = state
                    .random
                    .next_int_between(hook::MIN_LURE, hook::MAX_LURE);
            }
            return;
        }
        let wait = state
            .random
            .next_int_between(hook::MIN_WAIT, hook::MAX_WAIT);
        state.time_until_lured = wait - state.lure_speed;
    }

    fn lure_particles(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let angle = hook::next_float_between(&mut state.random, 0.0, 360.0) * mth::DEG_TO_RAD;
        let distance = hook::next_float_between(&mut state.random, 25.0, 60.0);
        let at = Vec3::new(
            base.position.x + f64::from(mth::sin(f64::from(angle)) * distance) * 0.1,
            base.position.y.floor() + 1.0,
            base.position.z + f64::from(mth::cos(f64::from(angle)) * distance) * 0.1,
        );
        let below = at.block().offset(0, -1, 0);
        let water = self
            .world
            .chunks
            .block(below)
            .is_some_and(|block| block.kind() == Some(rspigot_blocks::BlockKind::Water));
        if water {
            let Some(Entity {
                state: EntityState::Hook(state),
                ..
            }) = self.entities.store.get_mut(id)
            else {
                return;
            };
            let count = 2 + state.random.next_int(2);
            self.hook_particle("minecraft:splash", at, count, Vec3::new(0.1, 0.0, 0.1), 0.0);
        }
    }

    fn hook_particle(
        &mut self,
        kind: &'static str,
        position: Vec3,
        count: i32,
        spread: Vec3,
        speed: f64,
    ) {
        self.push(
            Audience::Nearby {
                position,
                range: PARTICLE_RANGE,
                except: None,
            },
            ClientEvent::Particle(Particle {
                count,
                spread,
                speed,
                ..Particle::single(kind, position)
            }),
        );
    }

    pub(crate) fn finish_bite(&mut self, id: EntityId, cancelled: bool) {
        if cancelled {
            return;
        }
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let position = base.position;
        let width = f64::from(base.dimensions.width);
        let pitch = (state.random.next_float() - state.random.next_float()) * 0.4 + 1.0;
        state.nibble = state
            .random
            .next_int_between(hook::MIN_NIBBLE, hook::MAX_NIBBLE);
        hook::set_biting(base, true);
        self.play_sound(
            None,
            position,
            SPLASH_SOUND,
            SoundSource::Neutral,
            SPLASH_VOLUME,
            pitch,
        );
        let count = (width * 20.0 + 1.0).round() as i32;
        let spread = Vec3::new(width, 0.0, width);
        let at = Vec3::new(position.x, position.y + 0.5, position.z);
        self.hook_particle("minecraft:bubble", at, count, spread, 0.2);
        self.hook_particle("minecraft:fishing", at, count, spread, 0.2);
    }

    fn reel_in(&mut self, id: PlayerId, hand: Hand, hook_id: EntityId, rod: &ItemStack) {
        if self.hook_should_stop(hook_id, id) {
            self.reel_done(id, hand, None, 0);
            return;
        }
        let Some(Entity {
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get(hook_id)
        else {
            self.reel_done(id, hand, None, 0);
            return;
        };
        if let Some(caught) = state.hooked {
            self.ask_reel(id, hand, hook_id, ReelStage::Entity { caught });
        } else if state.nibble > 0 {
            let loot = self.fishing_loot(id, hook_id, rod);
            self.next_fish(id, hand, hook_id, loot);
        } else {
            self.reel_ground(id, hand, hook_id, 0);
        }
    }

    fn fishing_loot(
        &mut self,
        player: PlayerId,
        hook_id: EntityId,
        rod: &ItemStack,
    ) -> Vec<ItemStack> {
        let Some(table) = rspigot_data::loot::table(FISHING_TABLE) else {
            return Vec::new();
        };
        let Some(Entity {
            base,
            state: EntityState::Hook(state),
            ..
        }) = self.entities.store.get(hook_id)
        else {
            return Vec::new();
        };
        let this = LootEntity {
            open_water: Some(state.open_water),
            ..LootEntity::of(Kind::FishingBobber)
        };
        let biome = rspigot_data::registries::biome_name(self.biome_at(base.position.block()).0);
        let luck = (state.luck + self.player_luck(player)) as f32;
        let context = LootContext {
            tool: Some(rod),
            this: Some(&this),
            luck,
            biome,
            ..LootContext::default()
        };
        loot::table_drops(table, &context, &mut self.rng)
    }

    fn player_luck(&self, _player: PlayerId) -> i32 {
        0
    }

    fn next_fish(&mut self, id: PlayerId, hand: Hand, hook_id: EntityId, loot: Vec<ItemStack>) {
        let mut loot = loot.into_iter();
        let Some(first) = loot.next() else {
            self.reel_ground(id, hand, hook_id, DAMAGE_FISH);
            return;
        };
        let Some(position) = self.entities.store.get(hook_id).map(|e| e.base.position) else {
            return;
        };
        let item = self.item_entity(position, Vec3::ZERO, ITEM_PICKUP_DELAY, first);
        let exp = self.rng.random_range(1..=EXP_SPREAD);
        let stage = ReelStage::Fish {
            item: Box::new(item),
            exp,
            rest: loot.collect(),
        };
        self.ask_reel(id, hand, hook_id, stage);
    }

    fn ask_reel(&mut self, id: PlayerId, hand: Hand, hook_id: EntityId, stage: ReelStage) {
        let state = match &stage {
            ReelStage::Fish { .. } => PlayerFishState::CaughtFish,
            ReelStage::Entity { .. } => PlayerFishState::CaughtEntity,
            ReelStage::Ground => PlayerFishState::InGround,
            ReelStage::Reel => PlayerFishState::ReelIn,
        };
        let (view, exp, caught) = match &stage {
            ReelStage::Fish { item, exp, .. } => (Some(view_of_entity(item)), *exp, None),
            ReelStage::Entity { caught } => (None, 0, Some(*caught)),
            _ => (None, 0, None),
        };
        let step = ReelStep {
            player: id,
            hand,
            hook: hook_id,
            stage,
        };
        self.fire(id, Continuation::Reel(Box::new(step)), move |server| {
            let mut payload = server.fish_payload(id, hook_id, caught, None, exp, state)?;
            if let (EventPayload::PlayerFish(event), Some(view)) = (&mut payload, view) {
                event.caught = Some(view);
            }
            Some(payload)
        });
    }

    pub(crate) fn finish_reel(&mut self, step: ReelStep, cancelled: bool, patch: PlayerFishPatch) {
        let ReelStep {
            player,
            hand,
            hook,
            stage,
        } = step;
        if !self.players.contains_key(&player) {
            return;
        }
        if cancelled {
            self.reel_done(player, hand, None, 0);
            return;
        }
        match stage {
            ReelStage::Fish { item, exp, rest } => {
                self.land_fish(player, hook, *item, patch.exp_to_drop.unwrap_or(exp));
                self.next_fish(player, hand, hook, rest);
            }
            ReelStage::Entity { caught } => {
                self.pull_entity(hook, caught);
                self.push(
                    Audience::Trackers(hook),
                    ClientEvent::EntityStatus {
                        entity: hook,
                        status: EntityStatus::Event(PULL_STATUS),
                    },
                );
                let damage = if self
                    .entities
                    .store
                    .get(caught)
                    .is_some_and(|e| matches!(e.state, EntityState::Item(_)))
                {
                    DAMAGE_ITEM
                } else {
                    DAMAGE_ENTITY
                };
                self.reel_ground(player, hand, hook, damage);
            }
            ReelStage::Ground => self.reel_end(player, hand, hook, DAMAGE_GROUND),
            ReelStage::Reel => self.reel_done(player, hand, Some(hook), 0),
        }
    }

    fn reel_ground(&mut self, id: PlayerId, hand: Hand, hook_id: EntityId, damage: i32) {
        let grounded = self
            .entities
            .store
            .get(hook_id)
            .is_some_and(|entity| entity.base.on_ground);
        if grounded {
            self.ask_reel(id, hand, hook_id, ReelStage::Ground);
        } else {
            self.reel_end(id, hand, hook_id, damage);
        }
    }

    fn reel_end(&mut self, id: PlayerId, hand: Hand, hook_id: EntityId, damage: i32) {
        if damage == 0 {
            self.ask_reel(id, hand, hook_id, ReelStage::Reel);
        } else {
            self.reel_done(id, hand, Some(hook_id), damage);
        }
    }

    fn reel_done(&mut self, id: PlayerId, hand: Hand, hook: Option<EntityId>, damage: i32) {
        if let Some(hook) = hook {
            self.discard_hook(hook);
        }
        if damage > 0 {
            self.wear(id, hand, damage);
        }
        if let Some(position) = self.players.get(&id).map(|p| p.position) {
            let pitch = PITCH_BASE / (self.rng.random::<f32>() * PITCH_SPREAD + PITCH_FLOOR);
            self.play_sound(
                Some(id),
                position,
                RETRIEVE_SOUND,
                SoundSource::Neutral,
                1.0,
                pitch,
            );
        }
        self.player_swing(id, hand, false);
    }

    fn land_fish(&mut self, player: PlayerId, hook: EntityId, mut item: Entity, exp: i32) {
        let Some(position) = self.players.get(&player).map(|p| p.position) else {
            return;
        };
        let Some(origin) = self.entities.store.get(hook).map(|e| e.base.position) else {
            return;
        };
        let (dx, dy, dz) = (
            position.x - origin.x,
            position.y - origin.y,
            position.z - origin.z,
        );
        let distance = (dx * dx + dy * dy + dz * dz).sqrt().sqrt();
        item.base.velocity = Vec3::new(
            dx * ITEM_PULL,
            dy * ITEM_PULL + distance * ITEM_LIFT,
            dz * ITEM_PULL,
        );
        let fish = rspigot_domain::entity::item::stack(&item.base)
            .is_some_and(|stack| crate::interact::item_tag(stack.item, "minecraft:fishes"));
        self.add_entity(item, CreatureSpawnReason::Default);
        if exp > 0 {
            let at = Vec3::new(position.x, position.y + XP_OFFSET, position.z + XP_OFFSET);
            self.spawn_orb(at, exp);
        }
        if fish {
            self.award_custom(player, custom::FISH_CAUGHT, 1);
        }
    }

    fn pull_entity(&mut self, hook: EntityId, target: EntityId) {
        let Some(owner) = self.hook_owner(hook).and_then(|p| self.players.get(&p)) else {
            return;
        };
        let Some(origin) = self.entities.store.get(hook).map(|e| e.base.position) else {
            return;
        };
        let delta = (owner.position - origin).scale(hook::PULL);
        if let Some(player) = self
            .players
            .values_mut()
            .find(|player| player.entity_id == target)
        {
            player.vitals.velocity = player.vitals.velocity + delta;
            player.vitals.sync_velocity = true;
        } else if let Some(entity) = self.entities.store.get_mut(target) {
            entity.base.velocity = entity.base.velocity + delta;
            entity.base.needs_sync = true;
        }
    }
}
