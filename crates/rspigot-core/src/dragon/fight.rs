use rspigot_blocks::BlockKind;
use rspigot_domain::boss_event::{BossColor, BossOverlay, ServerBossEvent};
use rspigot_domain::dragon::fight::{
    self as rules, FightData, GatewayBlock, RespawnStage, RespawnStep,
};
use rspigot_domain::dragon::phase::DragonPhase;
use rspigot_domain::dragon::podium::{self, Podium};
use rspigot_domain::dragon::spikes::{self, EndSpike, SpikeBlock};
use rspigot_domain::entity::species::ender_dragon;
use rspigot_domain::entity::{crystal, living};
use rspigot_domain::spawn::{SpawnLevel, SpawnReason};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SpawnHeightmap};
use uuid::Uuid;

use super::CrystalFacts;
use super::ai::state_of;
use crate::health::DamageSource;
use crate::mobs::ServerRandom;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::damage::DamageType;
use rspigot_domain::random::RandomSource;

use crate::explosion::BlockInteraction;

#[derive(Debug)]
pub(crate) struct DragonFight {
    pub(crate) data: FightData,
    pub(crate) origin: BlockPos,
    pub(crate) alive_crystals: i32,
    pub(crate) bar: ServerBossEvent,
    pub(crate) spikes: Vec<EndSpike>,
    pub(crate) dirty: bool,
    scan_ticks: i32,
    seen_ticks: i32,
    crystal_ticks: i32,
}

fn bar_name() -> Component {
    Component::translatable(rules::NAME_KEY, Vec::new())
}

fn block(kind: BlockKind) -> BlockStateId {
    kind.default_state()
}

impl DragonFight {
    fn new(data: FightData, origin: BlockPos, seed: i64, bar_id: Uuid) -> Self {
        let mut bar =
            ServerBossEvent::new(bar_id, bar_name(), BossColor::Pink, BossOverlay::Progress);
        bar.set_play_music(true);
        bar.set_create_world_fog(true);
        let mut data = data;
        let mut dirty = false;
        if data.gateways.is_empty() {
            data.gateways = rules::shuffled_gateways(seed);
            dirty = true;
        }
        Self {
            data,
            origin,
            alive_crystals: 0,
            bar,
            spikes: spikes::spikes_for_seed(seed),
            dirty,
            scan_ticks: rules::PLAYER_SCAN_TICKS,
            seen_ticks: 0,
            crystal_ticks: 0,
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_dragon_fight(&mut self) {
        let Some(nbt) = self.world.level.dragon_fight.clone() else {
            return;
        };
        let data = FightData::from_nbt(&nbt);
        let seed = self.world.level.seed;
        let bar_id = self.random_uuid();
        self.entities.fight = Some(DragonFight::new(data, BlockPos::new(0, 0, 0), seed, bar_id));
    }

    pub(crate) fn save_dragon_fight(&mut self) {
        if let Some(fight) = &self.entities.fight {
            self.world.level.dragon_fight = Some(fight.data.to_nbt());
        }
    }

    pub(crate) fn ensure_dragon_fight(&mut self, origin: BlockPos, scan: bool) {
        if self.entities.fight.is_some() {
            return;
        }
        let seed = self.world.level.seed;
        let bar_id = self.random_uuid();
        let data = FightData {
            needs_state_scanning: scan,
            ..FightData::default()
        };
        let mut fight = DragonFight::new(data, origin, seed, bar_id);
        fight.dirty = true;
        self.entities.fight = Some(fight);
    }

    pub(super) fn dragon_registered_in_fight(&mut self, id: EntityId) {
        let Some(uuid) = self.entities.store.get(id).map(|entity| entity.base.uuid) else {
            return;
        };
        let linked = match &self.entities.fight {
            Some(fight) => fight.data.dragon_uuid == Some(uuid),
            None => false,
        };
        if linked {
            return;
        }
        if self.entities.fight.is_none() {
            self.ensure_dragon_fight(BlockPos::new(0, 0, 0), false);
            if let Some(fight) = self.entities.fight.as_mut() {
                fight.data.dragon_uuid = Some(uuid);
                fight.dirty = true;
            }
        }
    }

    pub(super) fn dragon_update_fight(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let uuid = entity.base.uuid;
        let health = living::health(&entity.base);
        let max = entity
            .living_mut()
            .map_or(1.0, |state| state.max_health() as f32);
        let name = entity.base.custom_name().cloned();
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        if fight.data.dragon_uuid != Some(uuid) {
            return;
        }
        let mut messages = fight.bar.set_progress(health / max);
        fight.seen_ticks = 0;
        if let Some(name) = name {
            messages.extend(fight.bar.set_name(name));
        }
        let bar = fight.bar.id();
        self.send_boss_messages(bar, messages);
    }

    pub(super) fn dragon_finish_fight(&mut self, id: EntityId) {
        let Some(uuid) = self.entities.store.get(id).map(|entity| entity.base.uuid) else {
            return;
        };
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        if fight.data.dragon_uuid != Some(uuid) {
            return;
        }
        let mut messages = fight.bar.set_progress(0.0);
        messages.extend(fight.bar.set_visible(false));
        let bar = fight.bar.id();
        let origin = fight.origin;
        let previously = fight.data.previously_killed;
        self.send_boss_messages(bar, messages);
        self.fight_spawn_exit_portal(true);
        self.fight_spawn_new_gateway();
        if !previously {
            let y = self.top_height(SpawnHeightmap::MotionBlocking, origin.x, origin.z);
            self.set_block(
                BlockPos::new(origin.x, y, origin.z),
                block(BlockKind::DragonEgg),
            );
        }
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.data.previously_killed = true;
            fight.data.dragon_killed = true;
            fight.dirty = true;
        }
    }

    pub(crate) fn tick_dragon_fight(&mut self) {
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        let mut messages = fight.bar.set_visible(!fight.data.dragon_killed);
        let bar = fight.bar.id();
        fight.scan_ticks += 1;
        let scan = fight.scan_ticks >= rules::PLAYER_SCAN_TICKS;
        if scan {
            fight.scan_ticks = 0;
        }
        self.send_boss_messages(bar, std::mem::take(&mut messages));
        if scan {
            self.fight_update_players();
        }
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        if fight.bar.players().next().is_none() {
            return;
        }
        if fight.data.needs_state_scanning {
            self.fight_scan_state();
        }
        self.fight_tick_respawn();
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        if fight.data.dragon_killed {
            self.fight_save_if_dirty();
            return;
        }
        fight.seen_ticks += 1;
        let find = fight.data.dragon_uuid.is_none() || fight.seen_ticks > rules::DRAGON_SEEN_TICKS;
        fight.crystal_ticks += 1;
        let count = fight.crystal_ticks >= rules::CRYSTAL_SCAN_TICKS;
        if find {
            fight.seen_ticks = 0;
        }
        if count {
            fight.crystal_ticks = 0;
        }
        if find {
            self.fight_find_or_create_dragon();
        }
        if count {
            self.fight_update_crystal_count();
        }
        self.fight_save_if_dirty();
    }

    fn fight_save_if_dirty(&mut self) {
        if self
            .entities
            .fight
            .as_ref()
            .is_some_and(|fight| fight.dirty)
        {
            self.save_dragon_fight();
            if let Some(fight) = self.entities.fight.as_mut() {
                fight.dirty = false;
            }
        }
    }

    fn fight_update_players(&mut self) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let center = Vec3::new(
            f64::from(fight.origin.x),
            f64::from(fight.origin.y + rules::PLAYER_RANGE_Y),
            f64::from(fight.origin.z),
        );
        let wanted: Vec<_> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .filter(|player| {
                player.position.distance_squared(center)
                    <= rules::PLAYER_RANGE * rules::PLAYER_RANGE
            })
            .map(|player| player.id)
            .collect();
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        let mut messages = Vec::new();
        for player in &wanted {
            messages.extend(fight.bar.add_player(*player));
        }
        let stale: Vec<_> = fight
            .bar
            .players()
            .filter(|player| !wanted.contains(player))
            .collect();
        for player in stale {
            messages.extend(fight.bar.remove_player(player));
        }
        let bar = fight.bar.id();
        self.send_boss_messages(bar, messages);
    }

    fn fight_scan_state(&mut self) {
        let Some(origin) = self.entities.fight.as_ref().map(|fight| fight.origin) else {
            return;
        };
        let active = self.fight_has_active_exit_portal(origin);
        let dragons = self.fight_dragons();
        let found = if let Some(first) = dragons.first() {
            self.entities
                .store
                .get(*first)
                .map(|entity| entity.base.uuid)
        } else {
            None
        };
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.data.needs_state_scanning = false;
            fight.dirty = true;
            fight.data.previously_killed = active;
            match found {
                Some(uuid) if active => {
                    fight.data.dragon_uuid = Some(uuid);
                    fight.data.dragon_killed = false;
                }
                Some(_) => {}
                None => fight.data.dragon_killed = true,
            }
            if !fight.data.previously_killed && fight.data.dragon_killed {
                fight.data.dragon_killed = false;
            }
        }
        if found.is_some() && !active {
            if let Some(first) = dragons.first().copied() {
                self.remove_entity(
                    first,
                    rspigot_domain::entity::RemovalReason::Discarded,
                    None,
                );
            }
            if let Some(fight) = self.entities.fight.as_mut() {
                fight.data.dragon_uuid = None;
            }
        }
        if !active && self.fight_find_exit_portal().is_none() {
            self.fight_spawn_exit_portal(false);
        }
    }

    fn fight_dragons(&self) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::EnderDragon && !entity.base.is_removed())
            .map(|entity| entity.base.id)
            .collect();
        found.sort_unstable();
        found
    }

    fn fight_has_active_exit_portal(&self, origin: BlockPos) -> bool {
        let top = self.top_height(SpawnHeightmap::MotionBlocking, origin.x, origin.z);
        let min = self.world.dimension.height.min_y;
        (min..=top + podium::PILLAR_HEIGHT).any(|y| {
            self.world
                .chunks
                .block(BlockPos::new(origin.x + 1, y, origin.z))
                .is_some_and(|state| state.0 == block(BlockKind::EndPortal).0)
        })
    }

    pub(crate) fn fight_find_exit_portal(&mut self) -> Option<BlockPos> {
        let fight = self.entities.fight.as_ref()?;
        let origin = fight.origin;
        let known = fight.data.exit_portal_location;
        if let Some(center) = known.filter(|center| self.fight_portal_matches(*center)) {
            return Some(center);
        }
        let top = self.top_height(SpawnHeightmap::MotionBlocking, origin.x, origin.z);
        let min = self.world.dimension.height.min_y;
        let found = (min..=top)
            .rev()
            .map(|y| BlockPos::new(origin.x, y, origin.z))
            .find(|center| self.fight_portal_matches(*center));
        if let (Some(center), Some(fight)) = (found, self.entities.fight.as_mut())
            && fight.data.exit_portal_location.is_none()
        {
            fight.data.exit_portal_location = Some(center);
            fight.dirty = true;
        }
        found
    }

    fn fight_portal_matches(&self, center: BlockPos) -> bool {
        let bedrock = block(BlockKind::Bedrock).0;
        podium::exit_portal_bedrock()
            .into_iter()
            .all(|(dx, dy, dz)| {
                self.world
                    .chunks
                    .block(center.offset(dx, dy, dz))
                    .is_some_and(|state| state.0 == bedrock)
            })
    }

    pub(crate) fn fight_spawn_exit_portal(&mut self, active: bool) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let origin = fight.origin;
        let location = match fight.data.exit_portal_location {
            Some(location) => location,
            None => {
                let top =
                    self.top_height(SpawnHeightmap::MotionBlockingNoLeaves, origin.x, origin.z);
                let mut at = BlockPos::new(origin.x, top - 1, origin.z);
                let bedrock = block(BlockKind::Bedrock).0;
                while self
                    .world
                    .chunks
                    .block(at)
                    .is_some_and(|state| state.0 == bedrock)
                    && at.y >= rules::PORTAL_Y_FLOOR
                {
                    at = at.offset(0, -1, 0);
                }
                let at = BlockPos::new(at.x, at.y.max(self.world.dimension.height.min_y + 1), at.z);
                if let Some(fight) = self.entities.fight.as_mut() {
                    fight.data.exit_portal_location = Some(at);
                    fight.dirty = true;
                }
                at
            }
        };
        for placed in podium::podium_blocks(location, active) {
            if placed.drop_previous {
                let wanted = match placed.block {
                    Podium::Air => BlockStateId::AIR,
                    Podium::EndPortal => block(BlockKind::EndPortal),
                    Podium::EndStone => block(BlockKind::EndStone),
                    Podium::Bedrock => block(BlockKind::Bedrock),
                    Podium::WallTorch(_) => block(BlockKind::WallTorch),
                };
                if self.world.chunks.block(placed.pos) != Some(wanted) {
                    self.destroy_block_dropping(placed.pos, None);
                }
            }
            let state = match placed.block {
                Podium::Bedrock => block(BlockKind::Bedrock),
                Podium::EndStone => block(BlockKind::EndStone),
                Podium::Air => BlockStateId::AIR,
                Podium::EndPortal => block(BlockKind::EndPortal),
                Podium::WallTorch(facing) => BlockKind::WallTorch
                    .state_with(&[("facing", facing.name())])
                    .unwrap_or_else(|| block(BlockKind::WallTorch)),
            };
            self.set_block(placed.pos, state);
        }
    }

    fn fight_spawn_new_gateway(&mut self) {
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        let Some(index) = fight.data.gateways.pop() else {
            return;
        };
        fight.dirty = true;
        let center = rules::gateway_position(index);
        self.level_event(rules::GATEWAY_EVENT, center, 0);
        for (pos, kind) in rules::gateway_blocks(center) {
            let state = match kind {
                GatewayBlock::Gateway => block(BlockKind::EndGateway),
                GatewayBlock::Bedrock => block(BlockKind::Bedrock),
                GatewayBlock::Air => BlockStateId::AIR,
            };
            self.set_block(pos, state);
        }
    }

    fn fight_find_or_create_dragon(&mut self) {
        let dragons = self.fight_dragons();
        if let Some(first) = dragons.first() {
            let uuid = self
                .entities
                .store
                .get(*first)
                .map(|entity| entity.base.uuid);
            if let Some(fight) = self.entities.fight.as_mut() {
                fight.data.dragon_uuid = uuid;
                fight.dirty = true;
            }
            return;
        }
        self.fight_create_dragon();
    }

    fn fight_create_dragon(&mut self) -> Option<EntityId> {
        let origin = self.entities.fight.as_ref()?.origin;
        let at = Vec3::new(
            f64::from(origin.x),
            f64::from(origin.y + rules::SPAWN_HEIGHT),
            f64::from(origin.z),
        );
        let (mut entity, random) = self
            .summon_entity(Kind::EnderDragon, at, None, SpawnReason::Event)
            .ok()?;
        let yaw = ServerRandom(&mut self.rng).next_float() * 360.0;
        entity.base.snap_to(at, yaw, 0.0);
        let id = entity.base.id;
        let uuid = entity.base.uuid;
        let mut base = entity.base.clone();
        if let Some(state) = state_of(&mut entity) {
            state.fight_origin = origin;
            ender_dragon::set_phase(&mut base, state, DragonPhase::HoldingPattern);
        }
        entity.base = base;
        self.spawn_entity(
            entity,
            crate::plugins::Birth::of(CreatureSpawnReason::Default).with_random(random),
        );
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.data.dragon_uuid = Some(uuid);
            fight.dirty = true;
        }
        Some(id)
    }

    fn fight_update_crystal_count(&mut self) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let boxes: Vec<_> = fight
            .spikes
            .iter()
            .map(EndSpike::top_bounding_box)
            .collect();
        let count = boxes
            .iter()
            .map(|area| {
                self.entities
                    .store
                    .iter()
                    .filter(|entity| {
                        entity.base.kind == Kind::EndCrystal && !entity.base.is_removed()
                    })
                    .filter(|entity| entity.base.bounding_box().intersects(area))
                    .count() as i32
            })
            .sum();
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.alive_crystals = count;
        }
    }

    pub(crate) fn fight_crystal_destroyed(&mut self, crystal: CrystalFacts, source: &DamageSource) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let in_sequence = fight.data.respawn_stage.is_some()
            && crystal
                .uuid
                .is_some_and(|uuid| fight.data.respawn_crystals.contains(&uuid));
        let dragon_uuid = fight.data.dragon_uuid;
        if in_sequence {
            self.fight_abort_respawn();
            return;
        }
        self.fight_update_crystal_count();
        let dragon = dragon_uuid.and_then(|uuid| self.entities.store.by_uuid(uuid));
        let Some(dragon) = dragon.filter(|id| self.entities.dragons.contains_key(id)) else {
            return;
        };
        if let Some(pos) = crystal.pos {
            self.dragon_crystal_destroyed(dragon, crystal.id, pos, source);
        }
    }

    fn fight_abort_respawn(&mut self) {
        self.fight_reset_spike_crystals();
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.data.respawn_stage = None;
            fight.data.respawn_time = 0;
            fight.dirty = true;
        }
        self.fight_spawn_exit_portal(true);
    }

    fn fight_reset_spike_crystals(&mut self) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let boxes: Vec<_> = fight
            .spikes
            .iter()
            .map(EndSpike::top_bounding_box)
            .collect();
        let ids: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::EndCrystal && !entity.base.is_removed())
            .filter(|entity| {
                boxes
                    .iter()
                    .any(|area| entity.base.bounding_box().intersects(area))
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in ids {
            if let Some(entity) = self.entities.store.get_mut(id) {
                entity.base.invulnerable = false;
                crystal::set_beam_target(&mut entity.base, None);
            }
        }
    }

    pub(crate) fn fight_try_respawn(&mut self) -> bool {
        let Some(fight) = self.entities.fight.as_ref() else {
            return false;
        };
        if !fight.data.dragon_killed || fight.data.respawn_stage.is_some() {
            return false;
        }
        let known = fight.data.exit_portal_location;
        let center = match known {
            Some(center) => center,
            None => {
                if self.fight_find_exit_portal().is_none() {
                    self.fight_spawn_exit_portal(true);
                }
                let Some(center) = self
                    .entities
                    .fight
                    .as_ref()
                    .and_then(|fight| fight.data.exit_portal_location)
                else {
                    return false;
                };
                center
            }
        };
        let above = center.offset(0, 1, 0);
        let mut found = Vec::new();
        for facing in podium::Facing::HORIZONTAL {
            let (dx, dz) = facing.offset();
            let at = above.offset(dx * rules::CRYSTAL_RING, 0, dz * rules::CRYSTAL_RING);
            let area = rspigot_domain::entity::Aabb::block(at);
            let here: Vec<EntityId> = self
                .entities
                .store
                .iter()
                .filter(|entity| entity.base.kind == Kind::EndCrystal && !entity.base.is_removed())
                .filter(|entity| entity.base.bounding_box().intersects(&area))
                .map(|entity| entity.base.id)
                .collect();
            if here.is_empty() {
                return false;
            }
            found.extend(here);
        }
        self.fight_respawn(found)
    }

    fn fight_respawn(&mut self, crystals: Vec<EntityId>) -> bool {
        let uuids: Vec<Uuid> = crystals
            .iter()
            .filter_map(|id| self.entities.store.get(*id).map(|entity| entity.base.uuid))
            .collect();
        loop {
            match self.fight_find_exit_portal() {
                None => break,
                Some(center) => {
                    for (dx, dy, dz) in podium::exit_portal_bedrock() {
                        let pos = center.offset(dx, dy, dz);
                        let state = self.world.chunks.block(pos);
                        let swap = state.is_some_and(|state| {
                            state.0 == block(BlockKind::Bedrock).0
                                || state.0 == block(BlockKind::EndPortal).0
                        });
                        if swap {
                            self.set_block(pos, block(BlockKind::EndStone));
                        }
                    }
                }
            }
        }
        if let Some(fight) = self.entities.fight.as_mut() {
            fight.data.respawn_stage = Some(RespawnStage::Start);
            fight.data.respawn_time = 0;
            fight.data.respawn_crystals = uuids;
            fight.dirty = true;
        }
        self.fight_spawn_exit_portal(false);
        true
    }

    fn fight_respawn_crystals(&self) -> Vec<EntityId> {
        let Some(fight) = self.entities.fight.as_ref() else {
            return Vec::new();
        };
        fight
            .data
            .respawn_crystals
            .iter()
            .filter_map(|uuid| self.entities.store.by_uuid(*uuid))
            .filter(|id| {
                self.entities.store.get(*id).is_some_and(|entity| {
                    entity.base.kind == Kind::EndCrystal && !entity.base.is_removed()
                })
            })
            .collect()
    }

    fn fight_tick_respawn(&mut self) {
        let Some(fight) = self.entities.fight.as_ref() else {
            return;
        };
        let Some(stage) = fight.data.respawn_stage else {
            return;
        };
        let crystals = self.fight_respawn_crystals();
        if crystals.is_empty() {
            self.fight_abort_respawn();
            return;
        }
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        let time = fight.data.respawn_time;
        fight.data.respawn_time += 1;
        fight.dirty = true;
        let count = fight.spikes.len();
        for step in rules::respawn_steps(stage, time, count) {
            self.fight_respawn_step(step, &crystals);
            if self
                .entities
                .fight
                .as_ref()
                .is_none_or(|fight| fight.data.respawn_stage != Some(stage))
            {
                break;
            }
        }
    }

    fn fight_beam_all(&mut self, crystals: &[EntityId], target: Option<BlockPos>) {
        for id in crystals {
            if let Some(entity) = self.entities.store.get_mut(*id) {
                crystal::set_beam_target(&mut entity.base, target);
            }
        }
    }

    fn fight_respawn_step(&mut self, step: RespawnStep, crystals: &[EntityId]) {
        match step {
            RespawnStep::BeamAll(target) => self.fight_beam_all(crystals, target),
            RespawnStep::Event(event) => self.level_event(event, rules::RESPAWN_TARGET, 0),
            RespawnStep::Advance(stage) => self.fight_set_stage(stage),
            RespawnStep::BeamPillar(index) => {
                let Some(spike) = self.fight_spike(index) else {
                    return;
                };
                self.fight_beam_all(
                    crystals,
                    Some(BlockPos::new(
                        spike.center_x,
                        spike.height + 1,
                        spike.center_z,
                    )),
                );
            }
            RespawnStep::BlastPillar(index) => {
                let Some(spike) = self.fight_spike(index) else {
                    return;
                };
                self.fight_blast_pillar(spike);
            }
            RespawnStep::Summon => {
                self.fight_set_stage(RespawnStage::End);
                self.fight_reset_spike_crystals();
                for id in crystals {
                    self.fight_finish_crystal(*id);
                }
            }
        }
    }

    fn fight_spike(&self, index: usize) -> Option<EndSpike> {
        self.entities
            .fight
            .as_ref()
            .and_then(|fight| fight.spikes.get(index).copied())
    }

    fn fight_finish_crystal(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        crystal::set_beam_target(&mut entity.base, None);
        let at = entity.base.position;
        let blast = DamageSource::new(DamageType::Explosion);
        self.explode(
            Some(id),
            &blast,
            at,
            rules::CRYSTAL_BLAST,
            BlockInteraction::Keep,
            false,
            Some(id),
        );
    }

    fn fight_set_stage(&mut self, stage: RespawnStage) {
        let Some(fight) = self.entities.fight.as_mut() else {
            return;
        };
        fight.data.respawn_time = 0;
        fight.dirty = true;
        if stage != RespawnStage::End {
            fight.data.respawn_stage = Some(stage);
            return;
        }
        fight.data.respawn_stage = None;
        fight.data.dragon_killed = false;
        self.fight_create_dragon();
    }

    fn fight_blast_pillar(&mut self, spike: EndSpike) {
        let center = (spike.center_x, spike.height, spike.center_z);
        for x in center.0 - rules::PILLAR_CLEAR..=center.0 + rules::PILLAR_CLEAR {
            for y in center.1 - rules::PILLAR_CLEAR..=center.1 + rules::PILLAR_CLEAR {
                for z in center.2 - rules::PILLAR_CLEAR..=center.2 + rules::PILLAR_CLEAR {
                    self.set_block(BlockPos::new(x, y, z), BlockStateId::AIR);
                }
            }
        }
        let at = Vec3::new(
            f64::from(center.0 as f32 + 0.5),
            f64::from(center.1),
            f64::from(center.2 as f32 + 0.5),
        );
        let blast = DamageSource::new(DamageType::Explosion);
        self.explode(
            None,
            &blast,
            at,
            rules::PILLAR_BLAST,
            BlockInteraction::DestroyWithDecay,
            false,
            None,
        );
        self.fight_place_pillar(&spike, true);
    }

    pub(crate) fn fight_place_pillar(&mut self, spike: &EndSpike, rebuild: bool) {
        let min = self.world.dimension.height.min_y;
        for (pos, kind) in spikes::spike_blocks(spike, min) {
            let state = match kind {
                SpikeBlock::Obsidian => block(BlockKind::Obsidian),
                SpikeBlock::Air => BlockStateId::AIR,
                SpikeBlock::Bedrock => block(BlockKind::Bedrock),
                SpikeBlock::Fire => block(BlockKind::Fire),
                SpikeBlock::IronBars {
                    north,
                    south,
                    west,
                    east,
                } => BlockKind::IronBars
                    .state_with(&[
                        ("north", bool_name(north)),
                        ("south", bool_name(south)),
                        ("west", bool_name(west)),
                        ("east", bool_name(east)),
                    ])
                    .unwrap_or_else(|| block(BlockKind::IronBars)),
            };
            self.set_block(pos, state);
        }
        let (x, y, z) = spike.crystal_spot();
        let mut entity = self.new_crystal(Vec3::new(x, y, z));
        if let Some(base) = Some(&mut entity.base) {
            base.invulnerable = rebuild;
            crystal::set_beam_target(base, rebuild.then_some(rules::RESPAWN_TARGET));
        }
        self.add_entity(entity, CreatureSpawnReason::Default);
        let below = BlockPos::new(spike.center_x, spike.height, spike.center_z);
        self.set_block(below, block(BlockKind::Bedrock));
        self.set_block(below.offset(0, 1, 0), block(BlockKind::Fire));
    }
}

fn bool_name(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
