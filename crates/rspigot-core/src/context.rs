use std::hash::{BuildHasher, Hasher};
use std::net::IpAddr;
use std::time::{Duration, SystemTime};

use rspigot_domain::effect::{EffectId, EffectInstance};
use rspigot_domain::entity::spawn;
use rspigot_domain::entity::{Aabb, Entity, EntityState, living, save};
use rspigot_domain::item::PLAYER_INVENTORY_SIZE;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Identifier, ItemId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::admin::{AdminLists, Ban, BanEntry, IpBanEntry, ListEntry, MAX_PERMISSION_LEVEL};
use crate::chunks::{ChunkJob, Priority};
use crate::commands::selector::{self, Selector, SelectorBase, Sort, TypeMatch};
use crate::commands::source::{Anchor, DataTarget, Relation, Source};
use crate::commands::{CommandContext, CommandSender, Target, TitleAction, Weather};
use crate::events::ClientEvent;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, PlayerTeleportCause, TimeSkipReason};
use crate::plugins::events::{EntityPotionEffectCause, EntityTeleport, EntityTeleportPatch};
use crate::plugins::steps::RelocateStep;
use crate::plugins::{Birth, Holder, Potion, ShulkerHop, Spawned, name_of_entity};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::chunk::HeightmapKind;

impl<L: ClientLink> CommandContext for Server<L> {
    fn reply(&mut self, sender: CommandSender, message: Component) {
        if sender == CommandSender::Console && self.capture_command_reply(&message) {
            return;
        }
        if self.functions.runtime.silent {
            return;
        }
        if matches!(sender, CommandSender::Player(_))
            && !self.config.rules.send_command_feedback
            && !crate::command_block::is_failure(&message)
        {
            return;
        }
        match sender {
            CommandSender::Console => tracing::info!("{}", crate::console::console_text(&message)),
            CommandSender::Player(id) => self.push(Audience::One(id), ClientEvent::system(message)),
        }
    }

    fn broadcast(&mut self, message: Component) {
        tracing::info!("{}", crate::console::console_text(&message));
        self.push(Audience::All, ClientEvent::system(message));
    }

    fn permission_level(&self, sender: CommandSender) -> u8 {
        match sender {
            CommandSender::Console => MAX_PERMISSION_LEVEL,
            CommandSender::Player(id) => self
                .players
                .get(&id)
                .map_or(0, |player| player.permission_level()),
        }
    }

    fn sender_name(&self, sender: CommandSender) -> String {
        match sender {
            CommandSender::Console => self.running_command_name().unwrap_or("Server").to_owned(),
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
        let dimension = self.command_dimension.clone();
        self.ask_teleport_in(
            player,
            dimension,
            position,
            None,
            PlayerTeleportCause::Command,
            None,
        );
    }

    fn place_block(
        &mut self,
        pos: BlockPos,
        state: rspigot_blocks::StateId,
        nbt: Option<&Compound>,
    ) -> bool {
        let changed = self.set_block(pos, state);
        let Some(nbt) = nbt else {
            return changed;
        };
        let Some(mut data) = self.block_entity_nbt(pos) else {
            return changed;
        };
        for (key, value) in nbt
            .iter()
            .filter(|(key, _)| !matches!(*key, "x" | "y" | "z"))
        {
            data.insert(key, value.clone());
        }
        let _ = self.set_command_data(&DataTarget::Block(pos), data);
        true
    }

    fn destroy_block(&mut self, pos: BlockPos) -> bool {
        self.break_block_naturally(pos)
    }

    fn in_world_height(&self, y: i32) -> bool {
        self.world.dimension.height.contains(y)
    }

    fn default_game_mode(&self) -> GameMode {
        self.config.default_game_mode
    }

    fn set_default_game_mode(&mut self, mode: GameMode) {
        self.config.default_game_mode = mode;
    }

    fn unknown_command_text(&self) -> Option<String> {
        Some(self.config.messages.unknown_command.clone()).filter(|text| !text.is_empty())
    }

    fn game_rule_text(&self, name: &str) -> Option<String> {
        Server::game_rule_text(self, name)
    }

    fn set_game_rule_text(&mut self, name: &str, value: &str) -> Result<(), &'static str> {
        self.apply_game_rule(name, value)
    }

    fn experience_state(&self, player: PlayerId) -> Option<(i32, i32)> {
        self.experience_levels_and_points(player)
    }

    fn add_experience(&mut self, player: PlayerId, amount: i32, levels: bool) {
        if levels {
            self.give_experience_levels(player, amount);
        } else {
            self.give_experience(player, amount);
        }
    }

    fn set_experience(&mut self, player: PlayerId, amount: i32, levels: bool) -> bool {
        if levels {
            let current = self
                .experience_levels_and_points(player)
                .map_or(0, |(level, _)| level);
            self.give_experience_levels(player, amount - current);
            true
        } else {
            self.set_experience_points(player, amount)
        }
    }

    fn tags_of(&self, target: Target) -> Vec<String> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|p| p.tags.clone()),
            Target::Entity(id) => self.entities.store.get(id).map(|e| e.base.tags.clone()),
        }
        .unwrap_or_default()
    }

    fn add_tag(&mut self, target: Target, tag: &str) -> bool {
        let tags = match target {
            Target::Player(id) => self.players.get_mut(&id).map(|p| &mut p.tags),
            Target::Entity(id) => self.entities.store.get_mut(id).map(|e| &mut e.base.tags),
        };
        match tags {
            Some(tags) if tags.len() < 1024 && !tags.iter().any(|t| t == tag) => {
                tags.push(tag.to_owned());
                true
            }
            _ => false,
        }
    }

    fn remove_tag(&mut self, target: Target, tag: &str) -> bool {
        let tags = match target {
            Target::Player(id) => self.players.get_mut(&id).map(|p| &mut p.tags),
            Target::Entity(id) => self.entities.store.get_mut(id).map(|e| &mut e.base.tags),
        };
        tags.is_some_and(|tags| {
            let before = tags.len();
            tags.retain(|t| t != tag);
            tags.len() != before
        })
    }

    fn set_saving(&mut self, enabled: bool) -> bool {
        let changed = self.saving_enabled != enabled;
        self.saving_enabled = enabled;
        changed
    }

    fn random_between(&mut self, min: i32, max: i32) -> i32 {
        rand::Rng::random_range(&mut self.rng, min..=max)
    }

    fn set_spawn_point(
        &mut self,
        player: PlayerId,
        dimension: Identifier,
        pos: BlockPos,
        angle: f32,
    ) {
        self.set_respawn_position(
            player,
            Some(rspigot_domain::bed::RespawnConfig {
                dimension,
                pos,
                yaw: angle,
                pitch: 0.0,
                forced: true,
            }),
            false,
            crate::plugins::events::PlayerSpawnChangeCause::Command,
        );
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        let mut ids: Vec<PlayerId> = self.players.keys().copied().collect();
        ids.sort();
        ids
    }

    fn is_living(&self, target: Target) -> bool {
        match target {
            Target::Player(id) => self.players.contains_key(&id),
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .is_some_and(|e| e.living().is_some()),
        }
    }

    fn attribute_state(
        &mut self,
        target: Target,
        attribute: &str,
    ) -> Option<rspigot_domain::effect::AttributeState> {
        match target {
            Target::Player(id) => {
                let player = self.players.get(&id)?;
                let (name, default) = Kind::Player
                    .default_attributes()
                    .iter()
                    .copied()
                    .find(|(candidate, _)| *candidate == attribute)?;
                let derived = crate::health::attributes(player).state(name);
                let edit = player.attribute_edits.iter().find(|e| e.attribute == name);
                Some(rspigot_domain::effect::AttributeState {
                    attribute: name,
                    base: edit.map_or(default, |e| e.base),
                    modifiers: derived.modifiers,
                })
            }
            Target::Entity(id) => {
                let living = self.entities.store.get_mut(id)?.living_mut()?;
                living.attributes.instance(attribute)?;
                living.attributes.state(attribute, &living.effects)
            }
        }
    }

    fn default_attribute_base(&self, target: Target, attribute: &str) -> Option<f64> {
        let kind = match target {
            Target::Player(_) => Kind::Player,
            Target::Entity(id) => self.entities.store.get(id)?.base.kind,
        };
        kind.default_attributes()
            .iter()
            .find(|(candidate, _)| *candidate == attribute)
            .map(|(_, base)| *base)
    }

    fn set_attribute_base(&mut self, target: Target, attribute: &str, base: f64) {
        let base = rspigot_domain::attribute::sanitize(attribute, base);
        match target {
            Target::Player(id) => {
                let Some(name) = self.default_attribute_name(Kind::Player, attribute) else {
                    return;
                };
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                match player
                    .attribute_edits
                    .iter_mut()
                    .find(|e| e.attribute == name)
                {
                    Some(edit) => edit.base = base,
                    None => player.attribute_edits.push(
                        rspigot_domain::entity::living::AttributeInstance {
                            attribute: name,
                            base,
                            modifiers: Vec::new(),
                        },
                    ),
                }
                self.after_player_attribute_change(id, name);
            }
            Target::Entity(id) => {
                if let Some(living) = self.entities.store.get_mut(id).and_then(|e| e.living_mut()) {
                    living.attributes.set_base(attribute, base);
                }
                self.after_entity_attribute_change(id, attribute);
            }
        }
    }

    fn add_attribute_modifier(
        &mut self,
        target: Target,
        attribute: &str,
        modifier: rspigot_domain::effect::AttributeModifier,
    ) -> bool {
        match target {
            Target::Player(id) => {
                let Some(name) = self.default_attribute_name(Kind::Player, attribute) else {
                    return false;
                };
                let default = self
                    .default_attribute_base(target, attribute)
                    .unwrap_or_default();
                let Some(player) = self.players.get_mut(&id) else {
                    return false;
                };
                let derived = crate::health::attributes(player).state(name);
                if derived.modifiers.iter().any(|m| m.id == modifier.id) {
                    return false;
                }
                match player
                    .attribute_edits
                    .iter_mut()
                    .find(|e| e.attribute == name)
                {
                    Some(edit) => edit.modifiers.push(modifier),
                    None => player.attribute_edits.push(
                        rspigot_domain::entity::living::AttributeInstance {
                            attribute: name,
                            base: default,
                            modifiers: vec![modifier],
                        },
                    ),
                }
                self.after_player_attribute_change(id, name);
                true
            }
            Target::Entity(id) => {
                let added = self
                    .entities
                    .store
                    .get_mut(id)
                    .and_then(|e| e.living_mut())
                    .is_some_and(|living| living.attributes.add_modifier(attribute, modifier));
                if added {
                    self.after_entity_attribute_change(id, attribute);
                }
                added
            }
        }
    }

    fn remove_attribute_modifier(&mut self, target: Target, attribute: &str, id: &str) -> bool {
        match target {
            Target::Player(player_id) => {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return false;
                };
                let Some(edit) = player
                    .attribute_edits
                    .iter_mut()
                    .find(|e| e.attribute == attribute)
                else {
                    return false;
                };
                let before = edit.modifiers.len();
                edit.modifiers.retain(|m| m.id != id);
                let removed = edit.modifiers.len() != before;
                if removed {
                    let name = edit.attribute;
                    self.after_player_attribute_change(player_id, name);
                }
                removed
            }
            Target::Entity(entity) => {
                let removed = self
                    .entities
                    .store
                    .get_mut(entity)
                    .and_then(|e| e.living_mut())
                    .is_some_and(|living| living.attributes.remove_modifier(attribute, id));
                if removed {
                    self.after_entity_attribute_change(entity, attribute);
                }
                removed
            }
        }
    }

    fn damage_target(
        &mut self,
        target: Target,
        spec: crate::commands::DamageSpec,
        amount: f32,
    ) -> bool {
        let mut source = crate::health::DamageSource::new(spec.kind);
        source.position = spec.position;
        let describe = |server: &Self, who: Target| match who {
            Target::Player(id) => server.source_of(id),
            Target::Entity(id) => server.source_for(id),
        };
        source.direct = spec.direct.and_then(|who| describe(self, who));
        source.causing = spec.cause.and_then(|who| describe(self, who));
        match target {
            Target::Player(id) => self.hurt(id, &source, amount),
            Target::Entity(id) => self.hurt_entity(id, &source, amount),
        }
    }

    fn main_hand(&self, target: Target) -> Option<rspigot_domain::item::ItemStack> {
        match target {
            Target::Player(id) => self.players.get(&id)?.inventory.held().cloned(),
            Target::Entity(id) => self
                .entities
                .store
                .get(id)?
                .living()?
                .equipment(rspigot_items::EquipmentSlot::Mainhand)
                .cloned(),
        }
    }

    fn set_main_hand(&mut self, target: Target, stack: rspigot_domain::item::ItemStack) {
        match target {
            Target::Player(id) => {
                let Some(player) = self.players.get(&id) else {
                    return;
                };
                let slot = rspigot_domain::item::HOTBAR_START
                    + usize::from(player.inventory.selected_hotbar);
                self.set_slot_by_player(id, slot, Some(stack));
            }
            Target::Entity(id) => {
                let slot = rspigot_items::EquipmentSlot::Mainhand;
                let Some(living) = self.entities.store.get_mut(id).and_then(|e| e.living_mut())
                else {
                    return;
                };
                living.equipment[rspigot_domain::entity::living::slot_index(slot)] =
                    Some(stack.clone());
                self.push(
                    Audience::Trackers(id),
                    ClientEvent::EntityEquipment {
                        entity: id,
                        slots: vec![(crate::entities::wire_slot(slot), Some(stack))],
                    },
                );
            }
        }
    }

    fn advancement_criteria(&self, player: PlayerId, advancement: &str) -> Vec<&'static str> {
        self.obtained_criteria(player, advancement)
    }

    fn advancement_done(&self, player: PlayerId, advancement: &str) -> bool {
        self.advancement_is_done(player, advancement)
    }

    fn grant_criterion(&mut self, player: PlayerId, advancement: &str, criterion: &str) -> bool {
        self.grant_advancement_criterion(player, advancement, criterion)
    }

    fn revoke_criterion(&mut self, player: PlayerId, advancement: &str, criterion: &str) -> bool {
        self.revoke_advancement_criterion(player, advancement, criterion)
    }

    fn give_recipes(&mut self, player: PlayerId, recipes: &[&str]) -> usize {
        self.award_recipes(player, recipes)
    }

    fn take_recipes_of(&mut self, player: PlayerId, recipes: &[&str]) -> usize {
        self.take_recipes(player, recipes)
    }

    fn read_slot(
        &self,
        holder: crate::commands::slots::SlotHolder,
        slot: i32,
    ) -> Option<Option<rspigot_domain::item::ItemStack>> {
        self.read_command_slot(holder, slot)
    }

    fn write_slot(
        &mut self,
        holder: crate::commands::slots::SlotHolder,
        slot: i32,
        stack: Option<rspigot_domain::item::ItemStack>,
    ) -> bool {
        self.write_command_slot(holder, slot, stack)
    }

    fn container_slots(&self, pos: BlockPos) -> usize {
        self.command_slot_count(crate::commands::slots::SlotHolder::Block(pos))
    }

    fn insert_stack(
        &mut self,
        pos: BlockPos,
        stack: rspigot_domain::item::ItemStack,
    ) -> Option<Option<rspigot_domain::item::ItemStack>> {
        self.insert_into_container(pos, stack)
    }

    fn give_stack(&mut self, player: PlayerId, stack: rspigot_domain::item::ItemStack) {
        self.give_command_stack(player, stack);
    }

    fn drop_stack(&mut self, at: Vec3, stack: rspigot_domain::item::ItemStack) {
        self.drop_command_stack(at, stack);
    }

    fn roll_loot(
        &mut self,
        sender: CommandSender,
        source: &crate::commands::loot_source::LootSource,
    ) -> Result<Vec<rspigot_domain::item::ItemStack>, Component> {
        self.roll_command_loot(sender, source)
    }

    fn apply_item_modifier(
        &mut self,
        _sender: CommandSender,
        functions: &[rspigot_data::loot::Function],
        stack: &rspigot_domain::item::ItemStack,
    ) -> Option<rspigot_domain::item::ItemStack> {
        crate::loot::modified_stack(
            functions,
            stack,
            &crate::loot::LootContext::default(),
            &mut self.rng,
        )
    }

    fn start_world_query(
        &mut self,
        sender: CommandSender,
        query: crate::chunks::WorldQuery,
        kind: crate::locate::QueryKind,
        label: String,
    ) -> bool {
        self.begin_world_query(sender, query, kind, label);
        true
    }

    fn nearest_poi(&self, origin: Vec3, types: &[String]) -> Option<(String, BlockPos)> {
        let center = BlockPos::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );
        let poi = self.world.chunks.poi();
        let wanted = |kind: rspigot_domain::poi::PoiType| types.iter().any(|t| t == kind.name());
        let found = poi.find_closest(wanted, center, 256, rspigot_domain::poi::Occupancy::Any)?;
        Some((poi.type_at(found)?.name().to_owned(), found))
    }

    fn item_modifier(&self, id: &str) -> Option<&'static [rspigot_data::loot::Function]> {
        self.functions.data.item_modifier(id)
    }

    fn stopwatch_started(&self, id: &str) -> Option<i64> {
        self.stopwatches.get(id).copied()
    }

    fn set_stopwatch(&mut self, id: &str, started: Option<i64>) {
        match started {
            Some(at) => {
                self.stopwatches.insert(id.to_owned(), at);
            }
            None => {
                self.stopwatches.remove(id);
            }
        }
    }

    fn stopwatch_ids(&self) -> Vec<String> {
        self.stopwatches.keys().cloned().collect()
    }

    fn world_border(&self) -> rspigot_domain::world_border::WorldBorder {
        self.world.border.clone()
    }

    fn world_game_time(&self) -> i64 {
        self.world.level.game_time
    }

    fn change_world_border<R>(
        &mut self,
        change: impl FnOnce(&mut rspigot_domain::world_border::WorldBorder, i64) -> R,
    ) -> R {
        let now = self.world.level.game_time;
        let before = self.world.border.view(now);
        let result = change(&mut self.world.border, now);
        let after = self.world.border.view(now);
        if after != before {
            self.broadcast_border_change(&before, &after);
        }
        result
    }

    fn tick_state(&self) -> rspigot_domain::tick_rate::TickRate {
        self.tick_rate.clone()
    }

    fn change_tick_rate<R>(
        &mut self,
        change: impl FnOnce(&mut rspigot_domain::tick_rate::TickRate) -> R,
    ) -> R {
        let before = (self.tick_rate.rate(), self.tick_rate.is_frozen());
        let steps = self.tick_rate.pending_steps();
        let result = change(&mut self.tick_rate);
        let after = (self.tick_rate.rate(), self.tick_rate.is_frozen());
        if before != after {
            self.push(
                Audience::All,
                ClientEvent::TickingState {
                    rate: after.0,
                    frozen: after.1,
                },
            );
        }
        if self.tick_rate.pending_steps() != steps {
            self.push(
                Audience::All,
                ClientEvent::TickingStep(self.tick_rate.pending_steps()),
            );
        }
        result
    }

    fn transfer_player(&mut self, player: PlayerId, host: &str, port: i32) {
        self.push(
            Audience::One(player),
            ClientEvent::Transfer {
                host: host.to_owned(),
                port,
            },
        );
    }

    fn set_idle_timeout(&mut self, minutes: u32) {
        self.config.player_idle_timeout = minutes;
    }

    fn start_profiler(&mut self) -> bool {
        if self.profiler.is_some() {
            return false;
        }
        self.profiler = Some((std::time::Instant::now(), self.tick));
        true
    }

    fn stop_profiler(&mut self) -> Option<(f64, i64)> {
        let (started, first_tick) = self.profiler.take()?;
        Some((
            started.elapsed().as_secs_f64(),
            i64::try_from(self.tick.saturating_sub(first_tick)).unwrap_or(i64::MAX),
        ))
    }

    fn world_height(&self) -> (i32, i32) {
        let height = self.world.dimension.height;
        (height.min_y, height.min_y + height.height as i32 - 1)
    }

    fn fill_biome(
        &mut self,
        min: BlockPos,
        max: BlockPos,
        biome: u16,
        replaceable: Option<&[u16]>,
    ) -> Result<i32, crate::commands::FillBiomeProblem> {
        use crate::commands::FillBiomeProblem;
        let (low, high) = (min.chunk(), max.chunk());
        let mut chunks = Vec::new();
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                let pos = rspigot_domain::ChunkPos::new(x, z);
                if !self.world.chunks.is_loaded(pos) {
                    return Err(FillBiomeProblem::NotLoaded);
                }
                chunks.push(pos);
            }
        }
        let new = rspigot_domain::BiomeId(biome);
        let mut changed = 0;
        let mut touched = Vec::new();
        for pos in chunks {
            let Some(column) = self.world.chunks.chunk_mut(pos) else {
                continue;
            };
            let base_y = column.height().min_y;
            let mut here = 0;
            for (index, section) in column.sections_mut().iter_mut().enumerate() {
                for by in 0..4usize {
                    for bz in 0..4usize {
                        for bx in 0..4usize {
                            let block = BlockPos::new(
                                pos.min_block_x() + bx as i32 * 4,
                                base_y + index as i32 * 16 + by as i32 * 4,
                                pos.min_block_z() + bz as i32 * 4,
                            );
                            let inside = (min.x..=max.x).contains(&block.x)
                                && (min.y..=max.y).contains(&block.y)
                                && (min.z..=max.z).contains(&block.z);
                            if !inside {
                                continue;
                            }
                            let old = section.biome(bx, by, bz);
                            if old == new || replaceable.is_some_and(|only| !only.contains(&old.0))
                            {
                                continue;
                            }
                            section.set_biome(bx, by, bz, new);
                            here += 1;
                        }
                    }
                }
            }
            if here > 0 {
                changed += here;
                touched.push(pos);
            }
        }
        if changed == 0 {
            return Err(FillBiomeProblem::NoChange);
        }
        self.resend_biomes(&touched);
        Ok(changed)
    }

    fn random_seed(&mut self) -> u64 {
        rand::Rng::random::<u64>(&mut self.rng)
    }

    fn set_chunk_forced(&mut self, x: i32, z: i32, forced: bool) -> bool {
        self.world
            .chunks
            .set_forced(rspigot_domain::ChunkPos::new(x, z), forced, &self.workers)
    }

    fn forced_chunks(&self) -> Vec<(i32, i32)> {
        self.world
            .chunks
            .forced_chunks()
            .into_iter()
            .map(|pos| (pos.x, pos.z))
            .collect()
    }

    fn is_chunk_forced(&self, x: i32, z: i32) -> bool {
        self.world
            .chunks
            .is_forced(rspigot_domain::ChunkPos::new(x, z))
    }

    fn vehicle_of(&self, target: Target) -> Option<Target> {
        let rider = self.entity_id_of(target)?;
        let vehicle = self.entities.rides.vehicle_of(rider)?;
        self.target_of_entity(vehicle)
    }

    fn would_loop(&self, rider: Target, vehicle: Target) -> bool {
        let (Some(rider), Some(vehicle)) = (self.entity_id_of(rider), self.entity_id_of(vehicle))
        else {
            return false;
        };
        rider == vehicle || self.entities.rides.has_indirect_passenger(rider, vehicle)
    }

    fn start_riding_target(&mut self, rider: Target, vehicle: Target) -> bool {
        let (Some(rider), Some(vehicle)) = (self.entity_id_of(rider), self.entity_id_of(vehicle))
        else {
            return false;
        };
        self.start_riding(rider, vehicle, true, true)
    }

    fn stop_riding_target(&mut self, rider: Target) {
        if let Some(rider) = self.entity_id_of(rider) {
            self.stop_riding(rider);
        }
    }

    fn rotate_to(&mut self, target: Target, yaw: f32, pitch: f32) {
        match target {
            Target::Player(id) => {
                let Some(player) = self.players.get_mut(&id) else {
                    return;
                };
                player.rotation = Rotation::new(yaw, pitch);
                let entity = player.entity_id;
                self.push(
                    Audience::One(id),
                    ClientEvent::PlayerRotation { yaw, pitch },
                );
                self.push(
                    Audience::Trackers(entity),
                    ClientEvent::EntityHeadRotation { entity, yaw },
                );
            }
            Target::Entity(id) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    entity.base.set_rotation(yaw, pitch);
                    entity.base.head_yaw = yaw;
                }
            }
        }
    }

    fn face_target(&mut self, target: Target, facing: &crate::commands::FaceRequest) {
        let Target::Player(id) = target else {
            self.rotate_to(target, facing.rotation.yaw, facing.rotation.pitch);
            return;
        };
        let anchor = |anchor| match anchor {
            crate::commands::source::Anchor::Feet => crate::events::LookAnchor::Feet,
            crate::commands::source::Anchor::Eyes => crate::events::LookAnchor::Eyes,
        };
        let entity = facing
            .entity
            .and_then(|(looked, to)| self.entity_id_of(looked).map(|looked| (looked, anchor(to))));
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.rotation = facing.rotation;
        let tracked = player.entity_id;
        self.push(
            Audience::One(id),
            ClientEvent::LookAt {
                from: anchor(facing.from),
                target: facing.at,
                entity,
            },
        );
        self.push(
            Audience::Trackers(tracked),
            ClientEvent::EntityHeadRotation {
                entity: tracked,
                yaw: facing.rotation.yaw,
            },
        );
    }

    fn swing_target(
        &mut self,
        target: Target,
        hand: crate::events::Hand,
        style: crate::events::SwingStyle,
        duration: i32,
    ) -> bool {
        let (entity, own) = match target {
            Target::Player(id) => match self.players.get(&id) {
                Some(player) => (player.entity_id, Some(id)),
                None => return false,
            },
            Target::Entity(id) => {
                if !self
                    .entities
                    .store
                    .get(id)
                    .is_some_and(|e| e.living().is_some())
                {
                    return false;
                }
                (id, None)
            }
        };
        let event = ClientEvent::EntitySwing {
            entity,
            hand,
            style,
            duration,
        };
        if let Some(id) = own {
            self.push(Audience::One(id), event.clone());
        }
        self.push(Audience::Trackers(entity), event);
        true
    }

    fn is_spectator(&self, player: PlayerId) -> bool {
        self.players
            .get(&player)
            .is_some_and(|p| p.game_mode == GameMode::Spectator)
    }

    fn can_be_camera(&self, target: Target) -> bool {
        match target {
            Target::Player(_) => true,
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .is_some_and(|e| e.base.kind.tracking_range() != 0),
        }
    }

    fn set_camera_of(&mut self, player: PlayerId, target: Option<Target>) {
        let Some(own) = self.players.get(&player).map(|p| p.entity_id) else {
            return;
        };
        let camera = target.and_then(|t| self.entity_id_of(t)).unwrap_or(own);
        self.push(Audience::One(player), ClientEvent::Camera(camera));
        if camera != own
            && let Some(at) = target.and_then(|t| self.target_position(t))
        {
            self.teleport_player(player, at);
        }
    }

    fn play_sound_to(
        &mut self,
        player: PlayerId,
        sound: &str,
        source: rspigot_entities::SoundSource,
        position: Vec3,
        volume: f32,
        pitch: f32,
    ) {
        let seed = rand::Rng::random::<i64>(&mut self.rng);
        self.push(
            Audience::One(player),
            ClientEvent::NamedSound {
                sound: sound.to_owned(),
                source,
                position,
                volume,
                pitch,
                seed,
            },
        );
    }

    fn stop_sound_for(
        &mut self,
        player: PlayerId,
        source: Option<rspigot_entities::SoundSource>,
        sound: Option<String>,
    ) {
        self.push(
            Audience::One(player),
            ClientEvent::StopSound { source, sound },
        );
    }

    fn send_particle_to(
        &mut self,
        player: PlayerId,
        particle: rspigot_domain::particle::Particle,
        _force: bool,
    ) -> bool {
        if !self.players.contains_key(&player) {
            return false;
        }
        self.push(Audience::One(player), ClientEvent::Particle(particle));
        true
    }

    fn tell_player(&mut self, player: PlayerId, message: Component, overlay: bool) {
        self.push(
            Audience::One(player),
            ClientEvent::SystemChat { message, overlay },
        );
    }

    fn title_player(&mut self, player: PlayerId, action: TitleAction) {
        let event = match action {
            TitleAction::Title(text) => ClientEvent::TitleText(text),
            TitleAction::Subtitle(text) => ClientEvent::SubtitleText(text),
            TitleAction::Times {
                fade_in,
                stay,
                fade_out,
            } => ClientEvent::TitleTimes {
                fade_in,
                stay,
                fade_out,
            },
            TitleAction::Clear => ClientEvent::ClearTitles { reset: false },
            TitleAction::Reset => ClientEvent::ClearTitles { reset: true },
        };
        self.push(Audience::One(player), event);
    }

    fn local_origin(&self, sender: CommandSender) -> Vec3 {
        let origin = self.sender_origin(sender);
        match &self.command_source {
            Some(source) if source.anchor == Anchor::Eyes => match source.executor {
                Some(executor) => Vec3::new(
                    origin.x,
                    origin.y + self.target_eye_height(executor),
                    origin.z,
                ),
                None => origin,
            },
            _ => origin,
        }
    }

    fn run_with_source(&mut self, source: &Source, run: impl FnOnce(&mut Self)) {
        let previous = self.command_source.replace(source.clone());
        run(self);
        self.command_source = previous;
    }

    fn current_dimension(&self) -> Option<Identifier> {
        Some(
            self.command_dimension
                .clone()
                .unwrap_or_else(|| self.world.dimension.key.clone()),
        )
    }

    fn block_state_at(&self, pos: BlockPos) -> Option<rspigot_blocks::StateId> {
        self.world.chunks.block(pos)
    }

    fn block_entity_data(&self, pos: BlockPos) -> Option<Compound> {
        self.block_entity_nbt(pos)
    }

    fn is_position_loaded(&self, pos: BlockPos) -> bool {
        self.world.chunks.is_loaded(pos.chunk())
    }

    fn biome_name_at(&self, pos: BlockPos) -> Option<String> {
        rspigot_data::registries::biome_name(self.biome_at(pos).0).map(str::to_owned)
    }

    fn surface_height(&self, kind: HeightmapKind, x: i32, z: i32) -> Option<i32> {
        let column = self.world.chunks.get(BlockPos::new(x, 0, z).chunk())?;
        let value = column
            .heightmaps
            .get(kind)?
            .get((x & 15) as usize, (z & 15) as usize);
        Some(column.height().min_y + i32::from(value))
    }

    fn target_rotation(&self, target: Target) -> Option<Rotation> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|p| p.rotation),
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .map(|e| Rotation::new(e.base.yaw, e.base.pitch)),
        }
    }

    fn target_eye_height(&self, target: Target) -> f64 {
        match target {
            Target::Player(id) => self.players.get(&id).map_or(0.0, |p| p.eye_height()),
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .map_or(0.0, |e| f64::from(e.base.eye_height())),
        }
    }

    fn target_dimension(&self, target: Target) -> Option<Identifier> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|p| p.dim.clone()),
            Target::Entity(_) => self.current_dimension(),
        }
    }

    fn related_targets(&self, target: Target, relation: Relation) -> Vec<Target> {
        self.relations_of(target, relation)
    }

    fn scoreboard(&self) -> &rspigot_domain::scoreboard::Scoreboard {
        &self.scoreboard.board
    }

    fn boss_bar(&self, id: &Identifier) -> Option<rspigot_domain::custom_boss::CustomBossBar> {
        self.boss_bar_view(id)
    }

    fn boss_bars(&self) -> Vec<rspigot_domain::custom_boss::CustomBossBar> {
        self.boss_bar_views()
    }

    fn create_boss_bar(&mut self, id: Identifier, name: Component) -> bool {
        self.create_custom_boss_bar(Some(id), name).is_ok()
    }

    fn change_boss_bar<R>(
        &mut self,
        id: &Identifier,
        change: impl FnOnce(
            &mut rspigot_domain::custom_boss::CustomBossBar,
        ) -> (R, rspigot_domain::boss_event::BossMessages),
    ) -> Option<R> {
        let handle = self.custom_bars.handle_of(id)?;
        self.change_custom_boss_bar(handle, change)
    }

    fn remove_boss_bar(&mut self, id: &Identifier) -> bool {
        self.custom_bars
            .handle_of(id)
            .is_some_and(|handle| self.remove_custom_boss_bar(handle))
    }

    fn change_scoreboard<R>(
        &mut self,
        change: impl FnOnce(
            &mut rspigot_domain::scoreboard::Scoreboard,
        ) -> (R, Vec<rspigot_domain::scoreboard::ScoreboardMessage>),
    ) -> R {
        let (result, messages) = change(&mut self.scoreboard.board);
        self.send_scoreboard_messages(messages);
        result
    }

    fn score_holder_name(&self, target: Target) -> Option<String> {
        self.score_holder_of(target)
    }

    fn holder_display_name(&self, holder: &str) -> Option<Component> {
        Server::holder_display_name(self, holder)
    }

    fn record_result(&mut self, value: i32) {
        self.command_result = Some(value);
    }

    fn function_state(&self) -> &crate::functions::FunctionState {
        &self.functions
    }

    fn function_state_mut(&mut self) -> &mut crate::functions::FunctionState {
        &mut self.functions
    }

    fn reload_datapacks(&mut self) -> bool {
        self.reload_data_packs()
    }

    fn datapack_lists_changed(&mut self, lists: crate::functions::DataPackLists) {
        self.world.level.data_packs = Some(lists);
    }

    fn server_function_permission(&self) -> u8 {
        self.config.function_permission_level
    }

    fn server_function_source(&self) -> Source {
        Source {
            executor: None,
            position: self.world.spawn,
            rotation: Rotation::default(),
            anchor: Anchor::Feet,
            dimension: None,
        }
    }

    fn take_result(&mut self) -> Option<i32> {
        self.command_result.take()
    }

    fn read_data(&self, target: &DataTarget) -> Result<Compound, Component> {
        self.command_data(target)
    }

    fn write_data(&mut self, target: &DataTarget, data: Compound) -> Result<(), Component> {
        self.set_command_data(target, data)
    }

    fn run_in_dimension(
        &mut self,
        dimension: &Identifier,
        swap: bool,
        run: impl FnOnce(&mut Self),
    ) -> bool {
        if !self.ensure_dimension(dimension) {
            return false;
        }
        let previous = self.command_dimension.replace(dimension.clone());
        if swap {
            self.in_dimension(dimension, run);
        } else {
            run(self);
        }
        self.command_dimension = previous;
        true
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

    fn profile_entry(&self, name: &str) -> ListEntry {
        self.find_player(name)
            .and_then(|id| self.players.get(&id))
            .map_or_else(|| self.offline_entry(name), |p| ListEntry::of(&p.profile))
    }

    fn set_operator(&mut self, name: &str, operator: bool) -> bool {
        let online = self.find_player(name);
        let level = self.config.op_permission_level;
        let entry = online
            .and_then(|id| self.players.get(&id))
            .map_or_else(|| self.offline_entry(name), |p| ListEntry::of(&p.profile))
            .operator(level);
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
                p.op_level = level;
                p.link.send(ClientEvent::OperatorLevel {
                    entity: p.entity_id,
                    level: p.permission_level(),
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
        self.enforce_whitelist();
        true
    }

    fn whitelist_add(&mut self, name: &str) -> bool {
        let entry = self
            .find_player(name)
            .and_then(|id| self.players.get(&id))
            .map_or_else(|| self.offline_entry(name), |p| ListEntry::of(&p.profile));
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
            self.enforce_whitelist();
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
        self.apply_difficulty(difficulty, true);
    }

    fn set_weather(&mut self, weather: Weather, duration: Option<i32>) {
        self.command_weather(weather, duration);
    }

    fn set_world_spawn(&mut self, pos: BlockPos) {
        self.change_spawn(pos);
    }

    fn save_all(&mut self, flush: bool) -> bool {
        self.world_saving();
        self.save_all_entities();
        self.save_world_data();
        self.persist_all_players();
        self.store_scheduled_ticks();
        self.world.chunks.save_dirty(&self.workers);
        if flush {
            return self.workers.flush_blocking(Duration::from_secs(60));
        }
        self.workers.submit(Priority::BACKGROUND, ChunkJob::Flush);
        true
    }

    fn request_reload(&mut self, whitelist_only: bool) -> bool {
        self.ask_reload(whitelist_only)
    }

    fn tps(&self) -> [f64; 3] {
        Server::tps(self)
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn restart(&mut self) -> bool {
        self.request_restart();
        true
    }

    fn timings_enabled(&self) -> bool {
        self.timings.enabled
    }

    fn timings_enable(&mut self, enabled: bool) {
        self.timings.enabled = enabled;
        self.timings.reset();
    }

    fn timings_verbose(&mut self, enabled: bool) {
        self.timings.verbose = enabled;
    }

    fn timings_reset(&mut self) {
        self.timings.reset();
    }

    fn timings_report(&mut self) -> Option<String> {
        Server::timings_report(self)
    }

    fn sender_rotation(&self, sender: CommandSender) -> Rotation {
        if let Some(source) = &self.command_source {
            return source.rotation;
        }
        match sender {
            CommandSender::Player(id) => self
                .players
                .get(&id)
                .map_or_else(Rotation::default, |p| p.rotation),
            CommandSender::Console => Rotation::default(),
        }
    }

    fn sender_origin(&self, sender: CommandSender) -> Vec3 {
        if let Some(source) = &self.command_source {
            return source.position;
        }
        match sender {
            CommandSender::Player(id) => self.players.get(&id).map_or(Vec3::ZERO, |p| p.position),
            CommandSender::Console => {
                let spawn = self.world.spawn.block();
                Vec3::new(f64::from(spawn.x), f64::from(spawn.y), f64::from(spawn.z))
            }
        }
    }

    fn predicate_holds(&self, sender: CommandSender, id: &Identifier) -> Option<bool> {
        let executor = match (&self.command_source, sender) {
            (Some(source), _) => source.executor,
            (None, CommandSender::Player(id)) => Some(Target::Player(id)),
            (None, CommandSender::Console) => None,
        };
        self.predicate_result(id.as_str(), executor, self.sender_origin(sender))
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
    ) -> Result<Option<EntityId>, Component> {
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
        let id = entity.base.id;
        self.spawn_entity(entity, birth);
        Ok(Some(id))
    }
}

pub(crate) const SUMMON_FAILED: &str = "commands.summon.failed";
pub(crate) const SUMMON_DUPLICATE_UUID: &str = "commands.summon.failed.uuid";

fn advancement_matches(
    progress: &crate::advancements::PlayerAdvancements,
    id: &str,
    want: &selector::AdvancementWant,
) -> bool {
    let Some((index, found)) = rspigot_data::advancements::find(id) else {
        return false;
    };
    match want {
        selector::AdvancementWant::Done(done) => progress.is_done(index) == *done,
        selector::AdvancementWant::Criteria(criteria) => criteria.iter().all(|(name, flag)| {
            found.criteria.iter().any(|known| known.name == name)
                && progress.has(index, name) == *flag
        }),
    }
}

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
    fn resend_biomes(&mut self, chunks: &[rspigot_domain::ChunkPos]) {
        let columns: Vec<_> = chunks
            .iter()
            .filter_map(|pos| Some((*pos, self.world.chunks.get(*pos).cloned()?)))
            .collect();
        for player in self.players.values() {
            let visible: Vec<_> = columns
                .iter()
                .filter(|(pos, _)| player.streamer.has_sent(*pos))
                .map(|(_, chunk)| std::sync::Arc::clone(chunk))
                .collect();
            if !visible.is_empty() {
                player.link.send(ClientEvent::ChunksBiomes(visible));
            }
        }
    }

    fn default_attribute_name(&self, kind: Kind, attribute: &str) -> Option<&'static str> {
        kind.default_attributes()
            .iter()
            .find(|(candidate, _)| *candidate == attribute)
            .map(|(name, _)| *name)
    }

    fn after_player_attribute_change(&mut self, id: PlayerId, attribute: &'static str) {
        self.clamp_vitals(id);
        self.sync_attributes(id, &[attribute]);
    }

    fn after_entity_attribute_change(&mut self, id: EntityId, attribute: &str) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let base = &mut entity.base;
        let Some(living) = (match &mut entity.state {
            EntityState::Living(living) => Some(living),
            _ => None,
        }) else {
            return;
        };
        if attribute == living::MAX_HEALTH {
            let health = living::health(base);
            living::set_health(base, living, health);
        }
        let Some(state) = living.attributes.state(attribute, &living.effects) else {
            return;
        };
        living.attributes.take_dirty();
        if rspigot_domain::entity::living::is_client_syncable(state.attribute) {
            self.push(
                Audience::Trackers(id),
                ClientEvent::Attributes {
                    entity: id,
                    attributes: vec![state],
                },
            );
        }
    }

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
        self.stop_riding_forced(id);
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
        self.position_riders(id);
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
        let id = self.allocate_entity_id_for(kind);
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
        if let Some(entity) = self.players.get(&player).map(|p| p.entity_id) {
            self.stop_riding_forced(entity);
        }
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
                tags: &p.tags,
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

    fn state_matches(&self, selector: &Selector, candidate: &Candidate<'_>) -> bool {
        let player = match candidate.target {
            Target::Player(id) => self.players.get(&id),
            Target::Entity(_) => None,
        };
        if let Some((min, max)) = selector.level {
            let Some(player) = player else {
                return false;
            };
            let level = player.vitals.experience.level;
            if min.is_some_and(|m| level < m) || max.is_some_and(|m| level > m) {
                return false;
            }
        }
        if !selector.game_modes.is_empty() {
            let Some(player) = player else {
                return false;
            };
            if !selector
                .game_modes
                .iter()
                .all(|(negated, mode)| (player.game_mode == *mode) != *negated)
            {
                return false;
            }
        }
        let (pitch, yaw) = match (candidate.target, player) {
            (_, Some(player)) => (player.rotation.pitch, player.rotation.yaw),
            (Target::Entity(id), None) => self
                .entities
                .store
                .get(id)
                .map_or((0.0, 0.0), |e| (e.base.pitch, e.base.yaw)),
            (Target::Player(_), None) => (0.0, 0.0),
        };
        if selector
            .x_rotation
            .is_some_and(|range| !selector::wrapped_range_matches(range, f64::from(pitch)))
            || selector
                .y_rotation
                .is_some_and(|range| !selector::wrapped_range_matches(range, f64::from(yaw)))
        {
            return false;
        }
        let holder = self.score_holder_of(candidate.target);
        if !selector.teams.is_empty() {
            let team_name = holder
                .as_deref()
                .and_then(|holder| self.scoreboard.board.team_of(holder))
                .map_or("", |team| team.name.as_str());
            if !selector
                .teams
                .iter()
                .all(|(negated, team)| (team_name == team) != *negated)
            {
                return false;
            }
        }
        if !selector.scores.is_empty() {
            let Some(holder) = holder.as_deref() else {
                return false;
            };
            let in_range = |(name, (min, max)): &(String, (Option<i32>, Option<i32>))| {
                self.scoreboard
                    .board
                    .score(holder, name)
                    .is_some_and(|score| {
                        min.is_none_or(|min| score.value >= min)
                            && max.is_none_or(|max| score.value <= max)
                    })
            };
            if !selector.scores.iter().all(in_range) {
                return false;
            }
        }
        if !selector.predicates.iter().all(|(negated, name)| {
            self.predicate_result(name, Some(candidate.target), candidate.position)
                .unwrap_or(false)
                != *negated
        }) {
            return false;
        }
        if !selector.advancements.is_empty() {
            let Some(player) = player else {
                return false;
            };
            if !selector
                .advancements
                .iter()
                .all(|(id, want)| advancement_matches(&player.advancements, id, want))
            {
                return false;
            }
        }
        selector.nbt.iter().all(|(negated, expected)| {
            let actual = match (candidate.target, player) {
                (_, Some(player)) => Some(crate::player_data::PlayerSave::capture(player).to_nbt()),
                (Target::Entity(id), None) => self
                    .entities
                    .store
                    .get(id)
                    .and_then(|entity| self.entity_nbt(entity, None)),
                (Target::Player(_), None) => None,
            };
            actual.is_some_and(|actual| {
                rspigot_nbt::Tag::Compound(expected.clone())
                    .contains(&rspigot_nbt::Tag::Compound(actual))
                    != *negated
            })
        })
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
            let tags_match = selector.tags.iter().all(|(negated, tag)| {
                let has = if tag.is_empty() {
                    candidate.tags.is_empty()
                } else {
                    candidate.tags.iter().any(|t| t == tag)
                };
                has != *negated
            });
            tags_match && self.state_matches(selector, candidate)
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
            SelectorBase::Sender => {
                let executor = match (&self.command_source, sender) {
                    (Some(source), _) => source.executor,
                    (None, CommandSender::Player(id)) => Some(Target::Player(id)),
                    (None, CommandSender::Console) => None,
                };
                match executor {
                    Some(executor) => self
                        .candidates(true)
                        .into_iter()
                        .filter(|c| c.target == executor && matches(c))
                        .collect(),
                    None => Vec::new(),
                }
            }
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
        self.mob_effect_started(id, effect);
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
            self.mob_effect_ended(id);
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
