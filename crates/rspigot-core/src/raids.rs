use std::collections::BTreeMap;

use rand::Rng;
use rspigot_ai::{RaidFacts, RaiderFacts};
use rspigot_blocks::BlockKind;
use rspigot_domain::boss_event::BossMessages;
use rspigot_domain::effect::{EffectDuration, EffectInstance, EffectKind};
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::spawn::{finalize_spawn, set_item_slot};
use rspigot_domain::entity::species::{pillager, raider, vindicator};
use rspigot_domain::entity::{Entity, RemovalReason};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::raid::{
    self, JoinInfo, NewRaider, Omen, PlayerStatus, RaidEvent, RaidOutput, RaidWorld, RaiderStatus,
    Raids, Trigger,
};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::SpawnLevel;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::spawn::placement::is_spawn_position_ok;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::{EntityKind, SoundSource, SpawnHeightmap};
use rspigot_items::EquipmentSlot;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::events::ClientEvent;
use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::events::{
    CreatureSpawnReason, EntityPotionEffectCause, EntityRemoveCause, RaidFinish, RaidSpawnWave,
    RaidStop, RaidStopReason, RaidTrigger,
};
use crate::plugins::{Birth, Continuation, EntityView, EventPayload, view_of_entity};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Default)]
pub(crate) struct RaidRuntime {
    pub(crate) set: Raids,
    views: BTreeMap<EntityId, EntityView>,
    pub(crate) omen_positions: FxHashMap<PlayerId, BlockPos>,
}

pub(crate) fn facts_of(raid: &raid::Raid) -> RaidFacts {
    RaidFacts {
        center: raid.center(),
        active: raid.is_active(),
        over: raid.is_over(),
        victory: raid.is_victory(),
        loss: raid.is_loss(),
        stopped: raid.is_stopped(),
        first_wave_spawned: raid.has_first_wave_spawned(),
        between_waves: raid.is_between_waves(),
    }
}

fn stop_reason(reason: raid::RaidStopReason) -> RaidStopReason {
    match reason {
        raid::RaidStopReason::Peace => RaidStopReason::Peace,
        raid::RaidStopReason::Timeout => RaidStopReason::Timeout,
        raid::RaidStopReason::Finished => RaidStopReason::Finished,
        raid::RaidStopReason::Unspawnable => RaidStopReason::Unspawnable,
        raid::RaidStopReason::NotInVillage => RaidStopReason::NotInVillage,
        raid::RaidStopReason::Plugin => RaidStopReason::Plugin,
    }
}

pub(crate) fn is_ominous_banner(stack: &ItemStack) -> bool {
    *stack == raider::ominous_banner()
}

struct Adapter<'a, L: ClientLink> {
    server: &'a mut Server<L>,
    pending: BTreeMap<EntityId, Entity>,
}

impl<L: ClientLink> Adapter<'_, L> {
    fn status_of(&self, entity: &Entity) -> Option<RaiderStatus> {
        let living = entity.living()?;
        let state = living.mob.as_ref()?.species.raider()?;
        Some(RaiderStatus {
            removed: entity.base.is_removed(),
            same_dimension: true,
            position: entity.base.position.block(),
            tick_count: entity.base.tick_count,
            no_action_time: living.no_action_time,
            ticks_outside_raid: state.ticks_outside_raid,
            health: living::health(&entity.base),
            registered: true,
            wave: state.wave,
            leader: state.patrol_leader,
        })
    }

    fn raider_state_mut(&mut self, id: EntityId) -> Option<&mut raider::RaiderState> {
        self.server
            .entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()?
            .species
            .raider_mut()
    }

    fn place(&mut self, mut entity: Entity, info: JoinInfo) {
        let id = entity.base.id;
        let kind = entity.base.kind;
        if let Some(living) = entity.living_mut() {
            if let Some(state) = living.mob.as_mut().and_then(|mob| mob.species.raider_mut()) {
                state.raid_id = Some(info.raid);
                state.wave = info.wave;
                state.can_join_raid = true;
                state.ticks_outside_raid = 0;
                state.patrol_leader |= info.leader;
            }
            if info.leader {
                set_item_slot(living, EquipmentSlot::Head, Some(raider::ominous_banner()));
                if let Some(mob) = living.mob.as_mut() {
                    mob.drop_chances[slot_index(EquipmentSlot::Head)] =
                        raider::OMINOUS_BANNER_DROP_CHANCE;
                }
            }
        }
        let mut own = LegacyRandom::new(self.server.rng.random());
        if let Some(at) = info.place_at {
            let position = Vec3::new(
                f64::from(at.x) + 0.5,
                f64::from(at.y) + 1.0,
                f64::from(at.z) + 0.5,
            );
            entity.base.snap_to(position, 0.0, 0.0);
            let environment = self.server.spawn_environment(position);
            finalize_spawn(
                &mut entity,
                &environment,
                SpawnReason::Event,
                None,
                &mut ServerRandom(&mut self.server.rng),
                &mut own,
            );
            if let Some(living) = entity.living_mut() {
                match kind {
                    EntityKind::Vindicator => {
                        vindicator::apply_raid_buffs(
                            living,
                            info.wave,
                            info.enchant_odds,
                            &mut own,
                        );
                    }
                    EntityKind::Pillager => {
                        pillager::apply_raid_buffs(living, info.wave, info.enchant_odds, &mut own);
                    }
                    _ => {}
                }
            }
            entity.base.on_ground = true;
        }
        self.server.raids.views.insert(id, view_of_entity(&entity));
        self.server.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Raid).with_random(own),
        );
    }
}

impl<L: ClientLink> RaidWorld for Adapter<'_, L> {
    fn difficulty(&self) -> Difficulty {
        self.server.world.difficulty
    }

    fn has_chunk_at(&self, pos: BlockPos) -> bool {
        self.server.world.chunks.is_loaded(pos.chunk())
    }

    fn has_chunks_around(&self, pos: BlockPos, margin: i32) -> bool {
        let low = BlockPos::new(pos.x - margin, 0, pos.z - margin).chunk();
        let high = BlockPos::new(pos.x + margin, 0, pos.z + margin).chunk();
        (low.x..=high.x).all(|x| {
            (low.z..=high.z).all(|z| self.server.world.chunks.is_loaded(ChunkPos::new(x, z)))
        })
    }

    fn is_village(&self, pos: BlockPos) -> bool {
        use rspigot_ai::AiWorld;
        self.server
            .mob_world(sky_darken(&self.server.world.level))
            .is_village(pos)
    }

    fn is_position_entity_ticking(&self, pos: BlockPos) -> bool {
        self.server.entity_ticking(pos.chunk())
    }

    fn surface_height(&self, x: i32, z: i32) -> i32 {
        self.server.top_height(SpawnHeightmap::WorldSurface, x, z)
    }

    fn spawn_position_ok(&self, pos: BlockPos) -> bool {
        is_spawn_position_ok(&*self.server, EntityKind::Ravager, pos)
    }

    fn snow_on_air(&self, pos: BlockPos) -> bool {
        let chunks = &self.server.world.chunks;
        chunks
            .block(pos.offset(0, -1, 0))
            .and_then(BlockStateId::kind)
            .is_some_and(|kind| kind == BlockKind::Snow)
            && chunks.block(pos).is_some_and(BlockStateId::is_air)
    }

    fn players(&self) -> Vec<PlayerStatus> {
        self.server
            .players
            .values()
            .map(|player| PlayerStatus {
                id: player.id,
                position: player.position.block(),
                alive: !player.vitals.is_dead(),
            })
            .collect()
    }

    fn raider(&self, id: EntityId) -> Option<RaiderStatus> {
        if let Some(entity) = self.pending.get(&id) {
            return Some(RaiderStatus {
                removed: false,
                same_dimension: true,
                position: entity.base.position.block(),
                tick_count: 0,
                no_action_time: 0,
                ticks_outside_raid: 0,
                health: living::health(&entity.base),
                registered: true,
                wave: 0,
                leader: false,
            });
        }
        if let Some(entity) = self.server.entities.store.get(id) {
            return self.status_of(entity);
        }
        self.server.spawning_now(id).then(|| RaiderStatus {
            removed: false,
            same_dimension: true,
            position: BlockPos::new(0, 0, 0),
            tick_count: 0,
            no_action_time: 0,
            ticks_outside_raid: 0,
            health: 0.0,
            registered: true,
            wave: 0,
            leader: false,
        })
    }

    fn detach_raider(&mut self, id: EntityId) {
        if let Some(state) = self.raider_state_mut(id) {
            state.raid_id = None;
        }
    }

    fn set_ticks_outside_raid(&mut self, id: EntityId, ticks: i32) {
        if let Some(state) = self.raider_state_mut(id) {
            state.ticks_outside_raid = ticks;
        }
    }

    fn create_raider(&mut self, kind: EntityKind) -> Option<NewRaider> {
        if !crate::spawning::has_ai(kind) {
            return None;
        }
        let mut entity = new_mob(self.server.new_entity_base(kind));
        let id = entity.base.id;
        let health = entity
            .living_mut()
            .map_or(0.0, |living| living.max_health() as f32);
        self.pending.insert(id, entity);
        Some(NewRaider { id, health })
    }

    fn join_raider(&mut self, id: EntityId, info: JoinInfo) {
        match self.pending.remove(&id) {
            Some(entity) => self.place(entity, info),
            None => {
                if let Some(state) = self.raider_state_mut(id) {
                    state.raid_id = Some(info.raid);
                    state.wave = info.wave;
                    state.can_join_raid = true;
                    state.ticks_outside_raid = 0;
                }
            }
        }
    }

    fn apply_hero_of_the_village(&mut self, uuid: Uuid, amplifier: i32) -> Option<PlayerId> {
        let player = self
            .server
            .players
            .values()
            .find(|player| player.id.0 == uuid)
            .filter(|player| player.game_mode != GameMode::Spectator)?;
        let id = player.id;
        let mut instance = EffectInstance::new(
            EffectKind::HeroOfTheVillage.id(),
            EffectDuration::Ticks(raid::HERO_OF_THE_VILLAGE_TICKS as u32),
            amplifier.clamp(0, i32::from(u8::MAX)) as u8,
            false,
        );
        instance.show_icon = true;
        self.server
            .apply_effect(id, instance, Some(EntityPotionEffectCause::Unknown));
        Some(id)
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_raids(&mut self) {
        let Some(nbt) = self.world.level.raids.take() else {
            return;
        };
        let seed = self.world.level.seed;
        let mut random = StdRngSource(&mut self.rng);
        let loaded = Raids::from_nbt(&nbt, || random.uuid(), seed);
        self.raids.set = loaded;
        self.world.level.raids = Some(nbt);
    }

    fn with_raids<R>(
        &mut self,
        run: impl FnOnce(&mut Raids, &mut Adapter<'_, L>) -> R,
    ) -> (R, Vec<RaidOutput>) {
        let mut set = std::mem::take(&mut self.raids.set);
        let mut adapter = Adapter {
            server: self,
            pending: BTreeMap::new(),
        };
        let result = run(&mut set, &mut adapter);
        let outputs = set.drain_outputs();
        self.raids.set = set;
        (result, outputs)
    }

    pub(crate) fn raid_at(&self, pos: BlockPos) -> Option<i32> {
        self.raids.set.raid_at(pos)
    }

    pub(crate) fn tick_raids(&mut self) {
        let enabled = self.config.rules.raids;
        let (mut outputs, drained) = self.with_raids(|set, adapter| set.tick(adapter, enabled));
        outputs.extend(drained);
        self.deliver_raid_outputs(outputs);
        if self.raids.set.take_dirty() {
            self.world.level.raids = Some(self.raids.set.to_nbt());
        }
        if self.raids.views.len() > 256 {
            self.raids.views.clear();
        }
    }

    pub(crate) fn save_raids(&mut self) {
        self.world.level.raids = Some(self.raids.set.to_nbt());
    }

    fn deliver_raid_outputs(&mut self, outputs: Vec<RaidOutput>) {
        for output in outputs {
            self.deliver_bar_messages(output.bar_id, output.messages);
            for event in output.events {
                self.deliver_raid_event(output.bar_id, event);
            }
        }
    }

    fn deliver_bar_messages(&mut self, bar: Uuid, messages: BossMessages) {
        self.send_boss_messages(bar, messages);
    }

    fn deliver_raid_event(&mut self, bar: Uuid, event: RaidEvent) {
        let world = self.world_name();
        match event {
            RaidEvent::Horn(origin) => self.raid_horn(bar, origin),
            RaidEvent::Finish { winners } => {
                let winners = winners
                    .into_iter()
                    .filter_map(|player| self.snapshot_of(player))
                    .collect();
                self.notify_now(EventPayload::RaidFinish(RaidFinish { winners, world }));
            }
            RaidEvent::Stop(reason) => {
                self.notify_now(EventPayload::RaidStop(RaidStop {
                    reason: stop_reason(reason),
                    world,
                }));
            }
            RaidEvent::SpawnWave { leader, raiders } => {
                let view = |server: &Self, id: EntityId| {
                    server
                        .entity_view(id)
                        .or_else(|| server.raids.views.get(&id).cloned())
                };
                let patrol_leader = leader.and_then(|id| view(self, id));
                let raiders = raiders.iter().filter_map(|id| view(self, *id)).collect();
                self.notify_now(EventPayload::RaidSpawnWave(RaidSpawnWave {
                    patrol_leader,
                    raiders,
                    world,
                }));
            }
        }
    }

    fn raid_horn(&mut self, bar: Uuid, origin: BlockPos) {
        let target = origin.center();
        let seed = self.rng.random::<i64>();
        let viewers: Vec<(PlayerId, Vec3)> = self
            .players
            .values()
            .map(|player| (player.id, player.position))
            .collect();
        let watching: Vec<PlayerId> = self
            .raids
            .set
            .iter()
            .find(|raid| raid.bar().id() == bar)
            .map(|raid| raid.bar().players().collect())
            .unwrap_or_default();
        for (player, position) in viewers {
            let dx = target.x - position.x;
            let dz = target.z - position.z;
            let distance = (dx * dx + dz * dz).sqrt();
            if distance > raid::HORN_HEARING_RANGE && !watching.contains(&player) {
                continue;
            }
            let scale = raid::HORN_DISTANCE / distance;
            let at = Vec3::new(position.x + scale * dx, position.y, position.z + scale * dz);
            self.push(
                Audience::One(player),
                ClientEvent::Sound {
                    sound: raid::HORN_SOUND,
                    source: SoundSource::Neutral,
                    position: at,
                    volume: raid::HORN_HEARING_RANGE as f32,
                    pitch: 1.0,
                    seed,
                },
            );
        }
    }

    pub(crate) fn raider_view_facts(&self, entity: &Entity) -> Option<RaiderFacts> {
        let living = entity.living()?;
        let state = living.mob.as_ref()?.species.raider()?;
        let raid = state.raid_id.and_then(|id| self.raids.set.get(id));
        let leader_alive = raid
            .and_then(|raid| raid.leader(state.wave))
            .is_some_and(|leader| {
                self.entities.store.get(leader).is_some_and(|entity| {
                    !entity.base.is_removed()
                        && entity
                            .living()
                            .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living))
                })
            });
        let wearing_banner = living
            .equipment(EquipmentSlot::Head)
            .is_some_and(is_ominous_banner);
        Some(RaiderFacts {
            patrolling: state.patrolling,
            patrol_leader: state.patrol_leader,
            patrol_target: state.patrol_target,
            raid: raid.map(facts_of),
            can_be_leader: raid::can_be_leader(entity.base.kind),
            wearing_banner,
            leader_alive,
            johnny: false,
            blind: false,
        })
    }

    pub(crate) fn raider_registered(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.species.raider())
            .copied()
        else {
            return;
        };
        let Some(raid_id) = state.raid_id else {
            return;
        };
        let health = living::health(&entity.base);
        let known = self.raids.set.get(raid_id).is_some();
        if !known {
            if let Some(state) = self.raider_state_of(id) {
                state.raid_id = None;
            }
            return;
        }
        let (_, outputs) = self.with_raids(|set, adapter| {
            if let Some(raid) = set.get_mut(raid_id) {
                raid.add_wave_mob(state.wave, id, health, false, adapter);
                if state.patrol_leader {
                    raid.set_leader(state.wave, id);
                }
            }
        });
        self.deliver_raid_outputs(outputs);
    }

    pub(crate) fn raider_state_of(&mut self, id: EntityId) -> Option<&mut raider::RaiderState> {
        self.entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()?
            .species
            .raider_mut()
    }

    pub(crate) fn raider_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        if living::is_dead_or_dying(&entity.base, living) {
            return;
        }
        let Some(state) = living
            .mob
            .as_ref()
            .and_then(|mob| mob.species.raider())
            .copied()
        else {
            return;
        };
        if !state.can_join_raid {
            return;
        }
        let no_action_time = living.no_action_time;
        let health = living::health(&entity.base);
        let position = entity.base.position.block();
        if state.raid_id.is_none() {
            if self.world.level.game_time % 20 != 0 {
                return;
            }
            let Some(raid_id) = self.raid_at(position) else {
                return;
            };
            if !Raids::can_join_raid(true, true, no_action_time) {
                return;
            }
            let (_, outputs) = self.with_raids(|set, adapter| {
                if let Some(raid) = set.get_mut(raid_id) {
                    let wave = raid.groups_spawned();
                    raid.join_raid(adapter, wave, NewRaider { id, health }, None, false);
                }
            });
            self.deliver_raid_outputs(outputs);
            return;
        }
        let attacks_defenders = self.mob_target(id).is_some_and(|target| {
            self.players
                .values()
                .any(|player| player.entity_id == target)
                || self
                    .entities
                    .store
                    .get(target)
                    .is_some_and(|entity| entity.base.kind == EntityKind::IronGolem)
        });
        if attacks_defenders
            && let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut)
        {
            living.no_action_time = 0;
        }
    }

    pub(crate) fn raider_hurt(&mut self, id: EntityId) {
        let Some(raid_id) = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.species.raider())
            .and_then(|state| state.raid_id)
        else {
            return;
        };
        let (_, outputs) = self.with_raids(|set, adapter| {
            if let Some(raid) = set.get_mut(raid_id)
                && raid.is_active()
            {
                raid.refresh_bossbar(adapter);
            }
        });
        self.deliver_raid_outputs(outputs);
    }

    pub(crate) fn raider_died(&mut self, id: EntityId, killer: Option<PlayerId>) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let health = living::health(&entity.base);
        let Some(state) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.species.raider())
            .copied()
        else {
            return;
        };
        let Some(raid_id) = state.raid_id else {
            return;
        };
        let hero = killer.map(|player| player.0);
        let (_, outputs) = self.with_raids(|set, adapter| {
            let Some(raid) = set.get_mut(raid_id) else {
                return;
            };
            if state.patrol_leader {
                raid.remove_leader(state.wave);
            }
            if let Some(hero) = hero {
                raid.add_hero_of_the_village(hero);
            }
            raid.remove_from_raid(adapter, id, state.wave, false, health);
        });
        self.deliver_raid_outputs(outputs);
    }

    pub(crate) fn raider_recruit(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(raid_id) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.species.raider())
            .and_then(|state| state.raid_id)
        else {
            return;
        };
        let area = entity.base.bounding_box().inflate(raid::RECRUIT_RANGE);
        let recruits: Vec<(EntityId, f32)> = self
            .entities
            .store
            .iter()
            .filter(|other| other.base.kind.extends("Raider") && !other.base.is_removed())
            .filter(|other| other.base.bounding_box().intersects(&area))
            .filter_map(|other| {
                let living = other.living()?;
                let state = living.mob.as_ref()?.species.raider()?;
                let active = state
                    .raid_id
                    .and_then(|raid| self.raids.set.get(raid))
                    .is_some_and(raid::Raid::is_active);
                (!active
                    && Raids::can_join_raid(
                        !living::is_dead_or_dying(&other.base, living),
                        state.can_join_raid,
                        living.no_action_time,
                    ))
                .then(|| (other.base.id, living::health(&other.base)))
            })
            .collect();
        let (_, outputs) = self.with_raids(|set, adapter| {
            let Some(raid) = set.get_mut(raid_id) else {
                return;
            };
            if !raid.is_active() {
                return;
            }
            for (recruit, health) in &recruits {
                let wave = raid.groups_spawned();
                raid.join_raid(
                    adapter,
                    wave,
                    NewRaider {
                        id: *recruit,
                        health: *health,
                    },
                    None,
                    false,
                );
            }
        });
        self.deliver_raid_outputs(outputs);
    }

    pub(crate) fn raider_pick_up_banner(&mut self, id: EntityId, item: EntityId) {
        let Some(stack) = self
            .entities
            .store
            .get(item)
            .and_then(|entity| rspigot_domain::entity::item::stack(&entity.base))
            .filter(|stack| is_ominous_banner(stack))
            .cloned()
        else {
            return;
        };
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let Some(state) = living
            .mob
            .as_ref()
            .and_then(|mob| mob.species.raider())
            .copied()
        else {
            return;
        };
        let Some(raid_id) = state.raid_id else {
            return;
        };
        let leaderless = self
            .raids
            .set
            .get(raid_id)
            .is_some_and(|raid| raid.is_active() && raid.leader(state.wave).is_none());
        if !leaderless {
            return;
        }
        let head = slot_index(EquipmentSlot::Head);
        let chance = living
            .mob
            .as_ref()
            .map_or(0.0, |mob| mob.drop_chances[head]);
        let previous = living.equipment(EquipmentSlot::Head).cloned();
        let roll = (self.mob_random(id).next_float() - 0.1).max(0.0);
        if let Some(previous) = previous
            && roll < chance
        {
            self.drop_at_location(id, 0.0, previous, Vec3::ZERO);
        }
        if let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut) {
            set_item_slot(living, EquipmentSlot::Head, Some(stack));
        }
        self.remove_entity(
            item,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Pickup),
        );
        if let Some(state) = self.raider_state_of(id) {
            state.patrol_leader = true;
        }
        let (_, outputs) = self.with_raids(|set, _| {
            if let Some(raid) = set.get_mut(raid_id) {
                raid.set_leader(state.wave, id);
            }
        });
        self.deliver_raid_outputs(outputs);
    }

    pub(crate) fn bad_omen_tick(&mut self, id: PlayerId, amplifier: u8) -> bool {
        let Some(player) = self.players.get(&id) else {
            return true;
        };
        if player.game_mode == GameMode::Spectator || self.world.difficulty == Difficulty::Peaceful
        {
            return true;
        }
        let pos = player.position.block();
        let village = {
            use rspigot_ai::AiWorld;
            self.mob_world(sky_darken(&self.world.level))
                .is_village(pos)
        };
        if !village {
            return true;
        }
        if let Some(raid) = self.raid_at(pos).and_then(|raid| self.raids.set.get(raid))
            && raid::MAX_RAID_OMEN_LEVEL <= raid.raid_omen_level()
        {
            return true;
        }
        let omen = EffectInstance::new(
            EffectKind::RaidOmen.id(),
            EffectDuration::Ticks(raid::OMEN_EFFECT_TICKS as u32),
            amplifier,
            true,
        );
        self.apply_effect(id, omen, Some(EntityPotionEffectCause::Unknown));
        self.raids.omen_positions.insert(id, pos);
        false
    }

    pub(crate) fn raid_omen_tick(&mut self, id: PlayerId, amplifier: u8) -> bool {
        let spectator = self
            .players
            .get(&id)
            .is_none_or(|player| player.game_mode == GameMode::Spectator);
        if spectator {
            return true;
        }
        let Some(pos) = self.raids.omen_positions.remove(&id) else {
            return true;
        };
        self.raid_trigger(id, pos, amplifier);
        false
    }

    pub(crate) fn raid_trigger(&mut self, id: PlayerId, pos: BlockPos, amplifier: u8) {
        let world = self.world_name();
        let snapshot = self.snapshot_of(id);
        self.fire(
            id,
            Continuation::RaidTrigger {
                player: id,
                pos,
                amplifier,
            },
            move |_| {
                snapshot.map(|player| EventPayload::RaidTrigger(RaidTrigger { player, world }))
            },
        );
    }

    pub(crate) fn finish_raid_trigger(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        amplifier: u8,
        cancelled: bool,
    ) {
        let spectator = self
            .players
            .get(&id)
            .is_none_or(|player| player.game_mode == GameMode::Spectator);
        if spectator || !self.config.rules.raids {
            return;
        }
        let pois = self.village_poi_positions(pos);
        let bar = self.random_uuid();
        let seed = self.rng.random::<i64>();
        let difficulty = self.world.difficulty;
        let amplifier = i32::from(amplifier);
        let (trigger, outputs) = self.with_raids(|set, _| {
            set.create_or_extend(
                &Omen {
                    pos,
                    village_pois: &pois,
                    difficulty,
                    bar_id: bar,
                    seed,
                    amplifier: Some(amplifier),
                },
                |_| !cancelled,
            )
        });
        self.deliver_raid_outputs(outputs);
        if trigger == Trigger::Refused {
            self.remove_effects(
                id,
                Some(EffectKind::RaidOmen.id()),
                EntityPotionEffectCause::Plugin,
            );
        }
    }

    fn village_poi_positions(&self, pos: BlockPos) -> Vec<BlockPos> {
        use rspigot_ai::AiWorld;
        self.mob_world(sky_darken(&self.world.level))
            .pois_in_range(
                pos,
                raid::POI_SEARCH_RANGE,
                rspigot_domain::poi::Occupancy::IsOccupied,
            )
            .into_iter()
            .filter(|record| record.kind().is_village())
            .map(|record| record.pos())
            .collect()
    }
}

struct StdRngSource<'a>(&'a mut rand::rngs::StdRng);

impl StdRngSource<'_> {
    fn uuid(&mut self) -> Uuid {
        Uuid::from_u128(self.0.random())
    }
}
