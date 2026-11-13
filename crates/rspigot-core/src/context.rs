use std::hash::{BuildHasher, Hasher};
use std::net::IpAddr;
use std::time::SystemTime;

use rspigot_domain::effect::{EffectId, EffectInstance};
use rspigot_domain::entity::spawn;
use rspigot_domain::entity::{Aabb, Entity, EntityState, living, save};
use rspigot_domain::item::PLAYER_INVENTORY_SIZE;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, ItemId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::admin::{AdminLists, Ban, BanEntry, IpBanEntry, ListEntry};
use crate::chunks::{ChunkJob, Priority};
use crate::commands::selector::{self, Selector, SelectorBase, Sort, TypeMatch};
use crate::commands::{CommandContext, CommandSender, Target, Weather};
use crate::events::ClientEvent;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, PlayerTeleportCause, TimeSkipReason};
use crate::plugins::events::{EntityPotionEffectCause, EntityTeleport, EntityTeleportPatch};
use crate::plugins::steps::RelocateStep;
use crate::plugins::{Birth, Holder, Potion, ShulkerHop, Spawned, name_of_entity};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> CommandContext for Server<L> {
    fn reply(&mut self, sender: CommandSender, message: Component) {
        match sender {
            CommandSender::Console => tracing::info!("{}", crate::console::console_text(&message)),
            CommandSender::Player(id) => self.push(Audience::One(id), ClientEvent::system(message)),
        }
    }

    fn broadcast(&mut self, message: Component) {
        tracing::info!("{}", crate::console::console_text(&message));
        self.push(Audience::All, ClientEvent::system(message));
    }

    fn is_operator(&self, sender: CommandSender) -> bool {
        match sender {
            CommandSender::Console => true,
            CommandSender::Player(id) => self.players.get(&id).is_some_and(|p| p.operator),
        }
    }

    fn sender_name(&self, sender: CommandSender) -> String {
        match sender {
            CommandSender::Console => "Server".to_owned(),
            CommandSender::Player(id) => self.player_name(id).unwrap_or_default(),
        }
    }

    fn find_player(&self, name: &str) -> Option<PlayerId> {
        self.players
            .values()
            .find(|p| p.profile.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
    }

    fn player_name(&self, player: PlayerId) -> Option<String> {
        self.players.get(&player).map(|p| p.profile.name.clone())
    }

    fn player_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .players
            .values()
            .map(|p| p.profile.name.clone())
            .collect();
        names.sort_by_key(|n| n.to_ascii_lowercase());
        names
    }

    fn plugin_names(&self) -> Vec<String> {
        self.plugins
            .summary(&self.config.plugins)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect()
    }

    fn max_players(&self) -> u32 {
        self.config.max_players
    }

    fn player_position(&self, player: PlayerId) -> Option<Vec3> {
        self.players.get(&player).map(|p| p.position)
    }

    fn set_game_mode(
        &mut self,
        player: PlayerId,
        mode: GameMode,
        replies: Vec<(CommandSender, Component)>,
    ) {
        self.change_game_mode(player, mode, replies);
    }

    fn teleport(&mut self, player: PlayerId, position: Vec3) {
        self.ask_teleport(player, position, None, PlayerTeleportCause::Command, None);
    }

    fn day_time(&self) -> i64 {
        self.world.level.day_time
    }

    fn game_time(&self) -> i64 {
        self.world.level.game_time
    }

    fn set_day_time(&mut self, time: i64) {
        self.change_day_time(time, None);
    }

    fn skip_day_time(&mut self, time: i64) {
        self.change_day_time(time, Some(TimeSkipReason::Command));
    }

    fn give(&mut self, player: PlayerId, item: ItemId, count: u32) -> bool {
        let Some(p) = self.players.get_mut(&player) else {
            return false;
        };
        let outcome = p.inventory.insert(item, count, item.max_stack_size());
        let remaining = outcome.remaining;
        self.send_inventory(player);
        self.broadcast_held_item(player);
        remaining < count
    }

    fn clear_inventory(&mut self, player: PlayerId) -> u32 {
        let Some(p) = self.players.get_mut(&player) else {
            return 0;
        };
        let mut removed = 0u32;
        for slot in 0..PLAYER_INVENTORY_SIZE {
            if let Some(stack) = p.inventory.get(slot) {
                removed += u32::from(stack.count.get());
                p.inventory.set(slot, None);
            }
        }
        self.send_inventory(player);
        self.broadcast_held_item(player);
        removed
    }

    fn kick(&mut self, player: PlayerId, reason: Component) {
        self.disconnect_player(player, reason);
    }

    fn set_operator(&mut self, name: &str, operator: bool) -> bool {
        let online = self.find_player(name);
        let entry = online
            .and_then(|id| self.players.get(&id))
            .map_or_else(|| ListEntry::named(name), |p| ListEntry::of(&p.profile));
        let changed = if operator {
            AdminLists::add(&mut self.admin.operators, entry)
        } else {
            AdminLists::remove(&mut self.admin.operators, name)
        };
        if changed {
            self.publish_admin();
        }
        if let Some(id) = online {
            if let Some(p) = self.players.get_mut(&id) {
                p.operator = operator;
                p.link.send(ClientEvent::OperatorLevel {
                    entity: p.entity_id,
                    level: if operator { 4 } else { 0 },
                });
            }
            self.send_commands(id);
        }
        changed
    }

    fn set_whitelist_enabled(&mut self, enabled: bool) -> bool {
        if self.admin.whitelist_enabled == enabled {
            return false;
        }
        self.admin.whitelist_enabled = enabled;
        self.publish_admin();
        true
    }

    fn whitelist_add(&mut self, name: &str) -> bool {
        let entry = self
            .find_player(name)
            .and_then(|id| self.players.get(&id))
            .map_or_else(|| ListEntry::named(name), |p| ListEntry::of(&p.profile));
        let changed = AdminLists::add(&mut self.admin.whitelist, entry);
        if changed {
            self.publish_admin();
        }
        changed
    }

    fn whitelist_remove(&mut self, name: &str) -> bool {
        let changed = AdminLists::remove(&mut self.admin.whitelist, name);
        if changed {
            self.publish_admin();
        }
        changed
    }

    fn whitelist_names(&self) -> Vec<String> {
        self.admin
            .whitelist
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    fn ban_profile(
        &mut self,
        target: ListEntry,
        source: &str,
        reason: Option<String>,
        expires: Option<SystemTime>,
    ) -> bool {
        let now = self.prune_expired_bans();
        if self.admin.ban_of(&target, now).is_some() {
            return false;
        }
        let online = target
            .uuid
            .map(PlayerId)
            .filter(|id| self.players.contains_key(id));
        self.admin.bans.push(Ban {
            target,
            created: now,
            source: source.to_owned(),
            expires,
            reason,
        });
        self.publish_admin();
        if let Some(id) = online {
            self.kick(
                id,
                Component::translatable("multiplayer.disconnect.banned", Vec::new()),
            );
        }
        true
    }

    fn pardon_profile(&mut self, target: &ListEntry) -> bool {
        self.prune_expired_bans();
        let changed = self.admin.remove_ban(target);
        if changed {
            self.publish_admin();
        }
        changed
    }

    fn ban_address(
        &mut self,
        address: IpAddr,
        source: &str,
        reason: Option<String>,
        expires: Option<SystemTime>,
    ) -> Option<Vec<String>> {
        let now = self.prune_expired_bans();
        let address = address.to_canonical();
        if self.admin.ip_ban(address, now).is_some() {
            return None;
        }
        self.admin.ip_bans.push(Ban {
            target: address,
            created: now,
            source: source.to_owned(),
            expires,
            reason,
        });
        self.publish_admin();
        let mut affected: Vec<(EntityId, PlayerId)> = self
            .players
            .values()
            .filter(|p| p.address == address)
            .map(|p| (p.entity_id, p.id))
            .collect();
        affected.sort_by_key(|(entity, _)| entity.0);
        let mut kicked = Vec::with_capacity(affected.len());
        for (_, id) in affected {
            kicked.extend(self.player_name(id));
            self.kick(
                id,
                Component::translatable("multiplayer.disconnect.ip_banned", Vec::new()),
            );
        }
        Some(kicked)
    }

    fn pardon_address(&mut self, address: IpAddr) -> bool {
        self.prune_expired_bans();
        let address = address.to_canonical();
        let before = self.admin.ip_bans.len();
        self.admin.ip_bans.retain(|ban| ban.target != address);
        let changed = self.admin.ip_bans.len() != before;
        if changed {
            self.publish_admin();
        }
        changed
    }

    fn player_address(&self, player: PlayerId) -> Option<IpAddr> {
        self.players.get(&player).map(|p| p.address)
    }

    fn ban_entries(&self) -> Vec<BanEntry> {
        let now = self.now();
        self.admin
            .bans
            .iter()
            .filter(|ban| !ban.has_expired(now))
            .cloned()
            .collect()
    }

    fn ip_ban_entries(&self) -> Vec<IpBanEntry> {
        let now = self.now();
        self.admin
            .ip_bans
            .iter()
            .filter(|ban| !ban.has_expired(now))
            .cloned()
            .collect()
    }

    fn select_players(
        &self,
        sender: CommandSender,
        selector: &str,
    ) -> Result<Vec<PlayerId>, Component> {
        if selector.contains('[') {
            let parsed = selector::parse(selector)?;
            return Ok(self
                .evaluate_selector(sender, &parsed)
                .into_iter()
                .filter_map(|t| match t {
                    Target::Player(id) => Some(id),
                    Target::Entity(_) => None,
                })
                .collect());
        }
        let Some(kind) = selector.strip_prefix('@') else {
            return Ok(self.find_player(selector).into_iter().collect());
        };
        if let Some((_, options)) = kind.split_once('[') {
            let option = options.split(['=', ']']).next().unwrap_or_default();
            return Err(Component::translatable(
                "argument.entity.options.unknown",
                vec![Component::text(option)],
            ));
        }
        let origin = match sender {
            CommandSender::Player(id) => self
                .players
                .get(&id)
                .map_or(self.world.spawn, |p| p.position),
            CommandSender::Console => self.world.spawn,
        };
        let mut nearest: Vec<(f64, PlayerId)> = self
            .players
            .values()
            .map(|p| (p.position.distance_squared(origin), p.id))
            .collect();
        nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
        match kind {
            "s" => Ok(match sender {
                CommandSender::Player(id) => vec![id],
                CommandSender::Console => Vec::new(),
            }),
            "p" => Ok(nearest.first().map(|(_, id)| *id).into_iter().collect()),
            "a" | "e" => Ok(nearest.into_iter().map(|(_, id)| id).collect()),
            "r" => {
                let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
                hasher.write_u64(self.world.level.game_time as u64);
                let index = (hasher.finish() as usize).checked_rem(nearest.len());
                Ok(index.map(|i| nearest[i].1).into_iter().collect())
            }
            other => Err(Component::translatable(
                "argument.entity.selector.unknown",
                vec![Component::text(format!("@{other}"))],
            )),
        }
    }

    fn give_effect(&mut self, player: PlayerId, effect: EffectInstance) -> bool {
        self.apply_effect(player, effect, Some(EntityPotionEffectCause::Command))
    }

    fn clear_effect(&mut self, player: PlayerId, effect: Option<EffectId>) -> bool {
        self.remove_effects(player, effect, EntityPotionEffectCause::Command)
    }

    fn kill(&mut self, player: PlayerId) -> bool {
        Server::kill(self, player)
    }

    fn seed(&self) -> i64 {
        self.world.level.seed
    }

    fn difficulty(&self) -> Difficulty {
        self.world.difficulty
    }

    fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.world.difficulty = difficulty;
        self.push(
            Audience::All,
            ClientEvent::Difficulty {
                difficulty,
                locked: false,
            },
        );
    }

    fn set_weather(&mut self, weather: Weather) {
        self.change_weather(weather != Weather::Clear, weather == Weather::Thunder);
    }

    fn set_world_spawn(&mut self, pos: BlockPos) {
        self.change_spawn(pos);
    }

    fn save_all(&mut self) -> usize {
        self.world_saving();
        self.save_all_entities();
        self.persist_all_players();
        let count = self.world.chunks.save_dirty(&self.workers);
        self.workers.submit(Priority::BACKGROUND, ChunkJob::Flush);
        count
    }

    fn tps(&self) -> [f64; 3] {
        Server::tps(self)
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn sender_rotation(&self, sender: CommandSender) -> Rotation {
        match sender {
            CommandSender::Player(id) => self
                .players
                .get(&id)
                .map_or_else(Rotation::default, |p| p.rotation),
            CommandSender::Console => Rotation::default(),
        }
    }

    fn sender_origin(&self, sender: CommandSender) -> Vec3 {
        match sender {
            CommandSender::Player(id) => self.players.get(&id).map_or(Vec3::ZERO, |p| p.position),
            CommandSender::Console => {
                let spawn = self.world.spawn.block();
                Vec3::new(f64::from(spawn.x), f64::from(spawn.y), f64::from(spawn.z))
            }
        }
    }

    fn select_entities(&self, sender: CommandSender, text: &str) -> Result<Vec<Target>, Component> {
        let selector = selector::parse(text)?;
        Ok(self.evaluate_selector(sender, &selector))
    }

    fn target_name(&self, target: Target) -> Component {
        match target {
            Target::Player(id) => Component::text(self.player_name(id).unwrap_or_default()),
            Target::Entity(id) => self.entity_name(id).unwrap_or_else(|| Component::text("")),
        }
    }

    fn target_position(&self, target: Target) -> Option<Vec3> {
        match target {
            Target::Player(id) => self.player_position(id),
            Target::Entity(id) => self.entities.store.get(id).map(|e| e.base.position),
        }
    }

    fn teleport_target(&mut self, target: Target, position: Vec3) {
        match target {
            Target::Player(id) => CommandContext::teleport(self, id, position),
            Target::Entity(id) => self.relocate(id, position, true),
        }
    }

    fn kill_target(&mut self, target: Target) -> bool {
        match target {
            Target::Player(id) => Server::kill(self, id),
            Target::Entity(id) => self.kill_entity(id),
        }
    }

    fn give_effect_to(&mut self, target: Target, effect: EffectInstance) -> bool {
        let cause = EntityPotionEffectCause::Command;
        match target {
            Target::Player(id) => self.apply_effect(id, effect, Some(cause)),
            Target::Entity(id) => self.apply_entity_effect(id, effect, cause),
        }
    }

    fn clear_effect_of(&mut self, target: Target, effect: Option<EffectId>) -> bool {
        let cause = EntityPotionEffectCause::Command;
        match target {
            Target::Player(id) => self.remove_effects(id, effect, cause),
            Target::Entity(id) => self.remove_entity_effects(id, effect, cause),
        }
    }

    fn summon(
        &mut self,
        sender: CommandSender,
        kind: Kind,
        position: Vec3,
        nbt: Option<Compound>,
    ) -> Result<(), Component> {
        let failed = |key: &str| Component::translatable(key, Vec::new());
        let (entity, random) = self
            .summon_entity(kind, position, nbt, SpawnReason::Command)
            .map_err(failed)?;
        let name = name_of_entity(&entity);
        let mut birth = Birth::of(CreatureSpawnReason::Command)
            .with_random(random)
            .then(Spawned::Summoned { sender, name });
        if let CommandSender::Player(player) = sender {
            birth = birth.by(player);
        }
        self.spawn_entity(entity, birth);
        Ok(())
    }
}

pub(crate) const SUMMON_FAILED: &str = "commands.summon.failed";
pub(crate) const SUMMON_DUPLICATE_UUID: &str = "commands.summon.failed.uuid";

struct Candidate<'a> {
    target: Target,
    kind: Kind,
    position: Vec3,
    bounds: Aabb,
    name: String,
    tags: &'a [String],
    alive: bool,
}

fn english_name(kind: Kind) -> String {
    let name = kind.name();
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    path.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn type_matches(kind: Kind, matcher: &TypeMatch) -> bool {
    match matcher {
        TypeMatch::Kind(expected) => kind == *expected,
        TypeMatch::Tag(tag) => {
            let tag = if tag.contains(':') {
                tag.clone()
            } else {
                format!("minecraft:{tag}")
            };
            rspigot_data::tags::tag("minecraft:entity_type", &tag)
                .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
        }
    }
}

fn selection_box(selector: &Selector, origin: Vec3) -> Option<Aabb> {
    if selector.dx.is_none() && selector.dy.is_none() && selector.dz.is_none() {
        return None;
    }
    let edge = |d: f64| if d < 0.0 { (d, 1.0) } else { (0.0, d + 1.0) };
    let (x0, x1) = edge(selector.dx.unwrap_or(0.0));
    let (y0, y1) = edge(selector.dy.unwrap_or(0.0));
    let (z0, z1) = edge(selector.dz.unwrap_or(0.0));
    Some(Aabb::new(x0, y0, z0, x1, y1, z1).move_by(origin))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn relocate(&mut self, id: EntityId, position: Vec3, command: bool) {
        self.relocate_tracked(id, position, command, None);
    }

    pub(crate) fn relocate_tracked(
        &mut self,
        id: EntityId,
        position: Vec3,
        command: bool,
        from: Option<Vec3>,
    ) {
        if self.entities.store.get(id).is_none() {
            return;
        }
        let step = RelocateStep {
            entity: id,
            to: position,
            command,
            from,
            shulker: None,
        };
        self.ask_relocate(step);
    }

    pub(crate) fn relocate_shulker(&mut self, id: EntityId, position: Vec3, hop: ShulkerHop) {
        let step = RelocateStep {
            entity: id,
            to: position,
            command: false,
            from: None,
            shulker: Some(hop),
        };
        self.ask_relocate(step);
    }

    fn ask_relocate(&mut self, step: RelocateStep) {
        let RelocateStep {
            entity: id,
            to: position,
            ..
        } = step;
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let rotation = Rotation::new(entity.base.yaw, entity.base.pitch);
        let from_position = entity.base.position;
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::Relocate(step), move |server| {
            let entity = server.entity_view(id)?;
            Some(EventPayload::EntityTeleport(EntityTeleport {
                entity_type: entity.kind.clone(),
                entity,
                from: server.location_of(from_position, rotation),
                to: Some(server.location_of(position, rotation)),
            }))
        });
    }

    pub(crate) fn finish_relocate(
        &mut self,
        step: RelocateStep,
        cancelled: bool,
        patch: EntityTeleportPatch,
    ) {
        if cancelled || self.entities.store.get(step.entity).is_none() {
            return;
        }
        if step.command {
            self.relocate(step.entity, step.to, false);
            return;
        }
        let (position, rotation) = match patch.to {
            None => (step.to, None),
            Some(None) => return,
            Some(Some(to)) => (to.position, Some(to.rotation)),
        };
        let id = step.entity;
        if let Some(hop) = step.shulker {
            self.finish_shulker_teleport(id, position, hop);
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            let base = &mut entity.base;
            if base.kind == Kind::Shulker {
                rspigot_domain::entity::species::shulker::set_position(base, position);
            } else {
                base.position = position;
            }
            base.old_position = base.position;
            if let Some(rotation) = rotation {
                base.set_rotation(rotation.yaw, rotation.pitch);
            }
            let living = matches!(&entity.state, EntityState::Living(l) if l.fall_flying);
            if !living {
                base.velocity = base.velocity.multiply(1.0, 0.0, 1.0);
                base.on_ground = true;
            }
            base.needs_sync = true;
        }
        self.entities.store.reindex(id);
        if let Some(from) = step.from {
            self.enderman_teleported(id, from);
        }
    }

    pub(crate) fn summon_entity(
        &mut self,
        kind: Kind,
        position: Vec3,
        nbt: Option<Compound>,
        reason: SpawnReason,
    ) -> Result<(Entity, LegacyRandom), &'static str> {
        let block = position.block();
        if !(-20_000_000..20_000_000).contains(&block.y)
            || !(-30_000_000..30_000_000).contains(&block.x)
            || !(-30_000_000..30_000_000).contains(&block.z)
        {
            return Err("commands.summon.invalidPosition");
        }
        if self.world.difficulty == Difficulty::Peaceful && !kind.allowed_in_peaceful() {
            return Err("commands.summon.failed.peaceful");
        }
        if !save::is_simulated(kind) {
            return Err(SUMMON_FAILED);
        }
        let finalize = nbt.is_none();
        let mut compound = nbt.unwrap_or_default();
        compound.insert("id", kind.name());
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        let mut entity = save::load_entity(&compound, id, uuid).map_err(|_| SUMMON_FAILED)?;
        let (yaw, pitch) = (entity.base.yaw, entity.base.pitch);
        entity.base.snap_to(position, yaw, pitch);
        let mut own = LegacyRandom::new(rand::Rng::random(&mut self.rng));
        if finalize && entity.living().is_some_and(|state| state.is_mob()) {
            let environment = self.spawn_environment(entity.base.position);
            spawn::finalize_spawn(
                &mut entity,
                &environment,
                reason,
                None,
                &mut ServerRandom(&mut self.rng),
                &mut own,
            );
        }
        Ok((entity, own))
    }

    pub(crate) fn teleport_player(&mut self, player: PlayerId, position: Vec3) {
        let Some(p) = self.players.get_mut(&player) else {
            return;
        };
        let from = p.position;
        p.position = position;
        let id = p.next_teleport_id();
        p.link.send(ClientEvent::Teleport {
            id,
            position,
            rotation: p.rotation,
        });
        let chunk = position.chunk();
        p.streamer
            .move_to(chunk, &mut self.world.chunks, &self.workers, &p.link);
        let entity = p.entity_id;
        let on_ground = p.on_ground;
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityMoved {
                entity,
                from,
                to: position,
                rotation: None,
                on_ground,
            },
        );
    }

    fn candidates(&self, include_entities: bool) -> Vec<Candidate<'_>> {
        let mut out: Vec<Candidate<'_>> = self
            .players
            .values()
            .map(|p| Candidate {
                target: Target::Player(p.id),
                kind: Kind::Player,
                position: p.position,
                bounds: Aabb::of_size(p.position, 0.6, p.height() as f32),
                name: p.profile.name.clone(),
                tags: &[],
                alive: !p.vitals.is_dead(),
            })
            .collect();
        if include_entities {
            out.extend(self.entities.store.iter().map(|e| {
                Candidate {
                    target: Target::Entity(e.base.id),
                    kind: e.base.kind,
                    position: e.base.position,
                    bounds: e.base.bounding_box(),
                    name: e
                        .base
                        .custom_name()
                        .map_or_else(|| english_name(e.base.kind), |n| n.plain_text()),
                    tags: &e.base.tags,
                    alive: !e.base.is_removed()
                        && e.living().is_none_or(|_| living::health(&e.base) > 0.0),
                }
            }));
        }
        out
    }

    fn evaluate_selector(&self, sender: CommandSender, selector: &Selector) -> Vec<Target> {
        let base = self.sender_origin(sender);
        let origin = Vec3::new(
            selector.x.unwrap_or(base.x),
            selector.y.unwrap_or(base.y),
            selector.z.unwrap_or(base.z),
        );
        let bounds = selection_box(selector, origin);
        let matches = |candidate: &Candidate<'_>| {
            if selector.alive_only && !candidate.alive {
                return false;
            }
            if !selector
                .types
                .iter()
                .all(|(negated, matcher)| type_matches(candidate.kind, matcher) != *negated)
            {
                return false;
            }
            if let Some((min, max)) = selector.distance {
                let distance = candidate.position.distance_squared(origin);
                if min.is_some_and(|m| distance < m * m) || max.is_some_and(|m| distance > m * m) {
                    return false;
                }
            }
            if let Some(bounds) = &bounds
                && !bounds.intersects(&candidate.bounds)
            {
                return false;
            }
            if !selector
                .names
                .iter()
                .all(|(negated, name)| (candidate.name == *name) != *negated)
            {
                return false;
            }
            selector.tags.iter().all(|(negated, tag)| {
                let has = if tag.is_empty() {
                    candidate.tags.is_empty()
                } else {
                    candidate.tags.iter().any(|t| t == tag)
                };
                has != *negated
            })
        };
        let mut found: Vec<Candidate<'_>> = match &selector.base {
            SelectorBase::Name(name) => self
                .candidates(false)
                .into_iter()
                .filter(|c| c.name.eq_ignore_ascii_case(name))
                .collect(),
            SelectorBase::Uuid(uuid) => self
                .candidates(true)
                .into_iter()
                .filter(|c| match c.target {
                    Target::Player(id) => id.0 == *uuid,
                    Target::Entity(id) => self
                        .entities
                        .store
                        .get(id)
                        .is_some_and(|e| e.base.uuid == *uuid),
                })
                .collect(),
            SelectorBase::Sender => match sender {
                CommandSender::Player(id) => self
                    .candidates(false)
                    .into_iter()
                    .filter(|c| c.target == Target::Player(id) && matches(c))
                    .collect(),
                CommandSender::Console => Vec::new(),
            },
            SelectorBase::Search => self
                .candidates(selector.includes_entities)
                .into_iter()
                .filter(|c| matches(c))
                .collect(),
        };
        match selector.sort {
            Sort::Arbitrary => {}
            Sort::Nearest => found.sort_by(|a, b| {
                a.position
                    .distance_squared(origin)
                    .total_cmp(&b.position.distance_squared(origin))
            }),
            Sort::Furthest => found.sort_by(|a, b| {
                b.position
                    .distance_squared(origin)
                    .total_cmp(&a.position.distance_squared(origin))
            }),
            Sort::Random => {
                let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
                hasher.write_u64(self.world.level.game_time as u64);
                let mut seed = hasher.finish();
                for index in (1..found.len()).rev() {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let pick = (seed >> 33) as usize % (index + 1);
                    found.swap(index, pick);
                }
            }
        }
        found.truncate(selector.limit);
        found.into_iter().map(|c| c.target).collect()
    }

    pub(crate) fn apply_entity_effect(
        &mut self,
        id: EntityId,
        effect: EffectInstance,
        cause: EntityPotionEffectCause,
    ) -> bool {
        if self
            .entities
            .store
            .get(id)
            .and_then(|e| e.living())
            .is_none()
            || self.wither_refuses_effects(id)
            || self.nautilus_refuses_effect(id, effect.effect)
        {
            return false;
        }
        self.potion(Holder::Mob(id), Potion::Add(effect), cause)
    }

    pub(crate) fn apply_entity_effect_now(&mut self, id: EntityId, effect: EffectInstance) -> bool {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return false;
        };
        let Some(state) = entity.living_mut() else {
            return false;
        };
        if state.effects.apply(effect) == rspigot_domain::effect::ApplyOutcome::Unchanged {
            return false;
        }
        living::mark_effect_attributes(state, effect.effect);
        update_invisibility(entity);
        true
    }

    pub(crate) fn remove_entity_effects(
        &mut self,
        id: EntityId,
        effect: Option<EffectId>,
        cause: EntityPotionEffectCause,
    ) -> bool {
        let Some(state) = self.entities.store.get(id).and_then(|e| e.living()) else {
            return false;
        };
        let targets: Vec<Potion> = match effect {
            Some(effect) => state
                .effects
                .get(effect)
                .copied()
                .map(Potion::Remove)
                .into_iter()
                .collect(),
            None => state.effects.iter().copied().map(Potion::Clear).collect(),
        };
        let found = !targets.is_empty();
        for potion in targets {
            self.potion(Holder::Mob(id), potion, cause);
        }
        found
    }

    pub(crate) fn remove_entity_effect_now(&mut self, id: EntityId, effect: EffectId) -> bool {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return false;
        };
        let Some(state) = entity.living_mut() else {
            return false;
        };
        let removed = state.effects.remove(effect).is_some();
        if removed {
            living::mark_effect_attributes(state, effect);
            update_invisibility(entity);
        }
        removed
    }
}

fn update_invisibility(entity: &mut Entity) {
    let Some(invisible) = entity
        .living()
        .map(|state| state.effects.has("minecraft:invisibility"))
    else {
        return;
    };
    entity
        .base
        .set_shared_flag(rspigot_domain::entity::FLAG_INVISIBLE, invisible);
}
