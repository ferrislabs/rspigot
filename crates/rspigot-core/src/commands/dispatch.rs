use std::net::IpAddr;
use std::time::SystemTime;

use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::scoreboard::{Scoreboard, ScoreboardMessage};
use rspigot_domain::text::{Component, NamedColor};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Identifier, ItemId, Rotation, Vec3};
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use super::selector::split_argument;
use super::source::{Anchor, DataTarget, Relation, Source};
use crate::admin::{Ban, BanEntry, IpBanEntry, ListEntry, ban_reason};
use rspigot_domain::chunk::HeightmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Player(PlayerId),
    Entity(EntityId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSender {
    Console,
    Player(PlayerId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TitleAction {
    Title(Component),
    Subtitle(Component),
    Times {
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    Clear,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Thunder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    Success,
    Failure,
    Unknown,
    Denied,
}

pub trait CommandContext {
    fn reply(&mut self, sender: CommandSender, message: Component);
    fn broadcast(&mut self, message: Component);
    fn permission_level(&self, sender: CommandSender) -> u8;
    fn sender_name(&self, sender: CommandSender) -> String;
    fn find_player(&self, name: &str) -> Option<PlayerId>;
    fn player_name(&self, player: PlayerId) -> Option<String>;
    fn player_names(&self) -> Vec<String>;
    fn plugin_names(&self) -> Vec<String>;
    fn max_players(&self) -> u32;
    fn player_position(&self, player: PlayerId) -> Option<Vec3>;
    fn set_game_mode(
        &mut self,
        player: PlayerId,
        mode: GameMode,
        replies: Vec<(CommandSender, Component)>,
    );
    fn teleport(&mut self, player: PlayerId, position: Vec3);
    fn day_time(&self) -> i64;
    fn game_time(&self) -> i64;
    fn set_day_time(&mut self, time: i64);
    fn skip_day_time(&mut self, time: i64);
    fn give(&mut self, player: PlayerId, item: ItemId, count: u32) -> bool;
    fn clear_inventory(&mut self, player: PlayerId) -> u32;
    fn kick(&mut self, player: PlayerId, reason: Component);
    fn set_operator(&mut self, name: &str, operator: bool) -> bool;
    fn set_whitelist_enabled(&mut self, enabled: bool) -> bool;
    fn whitelist_add(&mut self, name: &str) -> bool;
    fn whitelist_remove(&mut self, name: &str) -> bool;
    fn whitelist_names(&self) -> Vec<String>;
    fn ban_profile(
        &mut self,
        target: ListEntry,
        source: &str,
        reason: Option<String>,
        expires: Option<SystemTime>,
    ) -> bool;
    fn pardon_profile(&mut self, target: &ListEntry) -> bool;
    fn ban_address(
        &mut self,
        address: IpAddr,
        source: &str,
        reason: Option<String>,
        expires: Option<SystemTime>,
    ) -> Option<Vec<String>>;
    fn pardon_address(&mut self, address: IpAddr) -> bool;
    fn player_address(&self, player: PlayerId) -> Option<IpAddr>;
    fn ban_entries(&self) -> Vec<BanEntry>;
    fn ip_ban_entries(&self) -> Vec<IpBanEntry>;
    fn select_players(
        &self,
        sender: CommandSender,
        selector: &str,
    ) -> Result<Vec<PlayerId>, Component>;
    fn give_effect(&mut self, player: PlayerId, effect: EffectInstance) -> bool;
    fn clear_effect(&mut self, player: PlayerId, effect: Option<EffectId>) -> bool;
    fn kill(&mut self, player: PlayerId) -> bool;
    fn seed(&self) -> i64;
    fn difficulty(&self) -> Difficulty;
    fn set_difficulty(&mut self, difficulty: Difficulty);
    fn set_weather(&mut self, weather: Weather, duration: Option<i32>);
    fn set_world_spawn(&mut self, pos: BlockPos);
    fn save_all(&mut self, flush: bool) -> bool;
    fn request_reload(&mut self, _whitelist_only: bool) -> bool {
        false
    }
    fn tps(&self) -> [f64; 3];
    fn stop(&mut self);
    fn restart(&mut self) -> bool {
        false
    }
    fn timings_enabled(&self) -> bool {
        false
    }
    fn timings_enable(&mut self, _enabled: bool) {}
    fn timings_verbose(&mut self, _enabled: bool) {}
    fn timings_reset(&mut self) {}
    fn timings_report(&mut self) -> Option<String> {
        None
    }

    fn predicate_holds(&self, _sender: CommandSender, _id: &Identifier) -> Option<bool> {
        None
    }

    fn sender_rotation(&self, _sender: CommandSender) -> Rotation {
        Rotation::default()
    }

    fn player_entry(&self, player: PlayerId) -> Option<ListEntry> {
        self.player_name(player).map(|name| ListEntry {
            name,
            uuid: Some(player.0),
            level: None,
            bypasses_player_limit: false,
        })
    }

    fn profile_entry(&self, name: &str) -> ListEntry {
        self.find_player(name)
            .and_then(|id| self.player_entry(id))
            .unwrap_or_else(|| ListEntry::named(name))
    }

    fn sender_origin(&self, sender: CommandSender) -> Vec3 {
        match sender {
            CommandSender::Player(id) => self.player_position(id).unwrap_or(Vec3::ZERO),
            CommandSender::Console => Vec3::ZERO,
        }
    }

    fn select_entities(
        &self,
        sender: CommandSender,
        selector: &str,
    ) -> Result<Vec<Target>, Component> {
        self.select_players(sender, selector)
            .map(|players| players.into_iter().map(Target::Player).collect())
    }

    fn target_name(&self, target: Target) -> Component {
        match target {
            Target::Player(id) => Component::text(self.player_name(id).unwrap_or_default()),
            Target::Entity(_) => Component::text(""),
        }
    }

    fn target_position(&self, target: Target) -> Option<Vec3> {
        match target {
            Target::Player(id) => self.player_position(id),
            Target::Entity(_) => None,
        }
    }

    fn run_in_dimension(
        &mut self,
        _dimension: &Identifier,
        _swap: bool,
        _run: impl FnOnce(&mut Self),
    ) -> bool {
        false
    }

    fn teleport_target(&mut self, target: Target, position: Vec3) {
        if let Target::Player(id) = target {
            self.teleport(id, position);
        }
    }

    fn kill_target(&mut self, target: Target) -> bool {
        match target {
            Target::Player(id) => self.kill(id),
            Target::Entity(_) => false,
        }
    }

    fn give_effect_to(&mut self, target: Target, effect: EffectInstance) -> bool {
        match target {
            Target::Player(id) => self.give_effect(id, effect),
            Target::Entity(_) => false,
        }
    }

    fn clear_effect_of(&mut self, target: Target, effect: Option<EffectId>) -> bool {
        match target {
            Target::Player(id) => self.clear_effect(id, effect),
            Target::Entity(_) => false,
        }
    }

    fn summon(
        &mut self,
        _sender: CommandSender,
        _entity: rspigot_entities::EntityKind,
        _position: Vec3,
        _nbt: Option<Compound>,
    ) -> Result<Option<EntityId>, Component> {
        Err(Component::translatable(
            "commands.summon.failed",
            Vec::new(),
        ))
    }

    fn place_block(
        &mut self,
        _pos: BlockPos,
        _state: rspigot_blocks::StateId,
        _nbt: Option<&Compound>,
    ) -> bool {
        false
    }

    fn destroy_block(&mut self, _pos: BlockPos) -> bool {
        false
    }

    fn in_world_height(&self, _y: i32) -> bool {
        true
    }

    fn default_game_mode(&self) -> GameMode {
        GameMode::Survival
    }

    fn set_default_game_mode(&mut self, _mode: GameMode) {}

    fn unknown_command_text(&self) -> Option<String> {
        None
    }

    fn game_rule_text(&self, _name: &str) -> Option<String> {
        None
    }

    fn set_game_rule_text(&mut self, _name: &str, _value: &str) -> Result<(), &'static str> {
        Err("argument.rule.invalid")
    }

    fn experience_state(&self, _player: PlayerId) -> Option<(i32, i32)> {
        None
    }

    fn add_experience(&mut self, _player: PlayerId, _amount: i32, _levels: bool) {}

    fn set_experience(&mut self, _player: PlayerId, _amount: i32, _levels: bool) -> bool {
        false
    }

    fn tags_of(&self, _target: Target) -> Vec<String> {
        Vec::new()
    }

    fn add_tag(&mut self, _target: Target, _tag: &str) -> bool {
        false
    }

    fn remove_tag(&mut self, _target: Target, _tag: &str) -> bool {
        false
    }

    fn set_saving(&mut self, _enabled: bool) -> bool {
        false
    }

    fn random_between(&mut self, min: i32, _max: i32) -> i32 {
        min
    }

    fn set_spawn_point(
        &mut self,
        _player: PlayerId,
        _dimension: Identifier,
        _pos: BlockPos,
        _angle: f32,
    ) {
    }

    fn tell_player(&mut self, _player: PlayerId, _message: Component, _overlay: bool) {}

    fn title_player(&mut self, _player: PlayerId, _action: TitleAction) {}

    fn local_origin(&self, sender: CommandSender) -> Vec3 {
        self.sender_origin(sender)
    }

    fn command_source(&self, sender: CommandSender) -> Source {
        Source {
            executor: match sender {
                CommandSender::Player(id) => Some(Target::Player(id)),
                CommandSender::Console => None,
            },
            position: self.sender_origin(sender),
            rotation: self.sender_rotation(sender),
            anchor: Anchor::Feet,
            dimension: None,
        }
    }

    fn run_with_source(&mut self, _source: &Source, run: impl FnOnce(&mut Self)) {
        run(self);
    }

    fn current_dimension(&self) -> Option<Identifier> {
        None
    }

    fn block_state_at(&self, _pos: BlockPos) -> Option<rspigot_blocks::StateId> {
        None
    }

    fn block_entity_data(&self, _pos: BlockPos) -> Option<Compound> {
        None
    }

    fn is_position_loaded(&self, _pos: BlockPos) -> bool {
        false
    }

    fn biome_name_at(&self, _pos: BlockPos) -> Option<String> {
        None
    }

    fn surface_height(&self, _kind: HeightmapKind, _x: i32, _z: i32) -> Option<i32> {
        None
    }

    fn target_rotation(&self, _target: Target) -> Option<Rotation> {
        None
    }

    fn target_eye_height(&self, _target: Target) -> f64 {
        0.0
    }

    fn target_dimension(&self, _target: Target) -> Option<Identifier> {
        None
    }

    fn related_targets(&self, _target: Target, _relation: Relation) -> Vec<Target> {
        Vec::new()
    }

    fn record_result(&mut self, _value: i32) {}

    fn take_result(&mut self) -> Option<i32> {
        None
    }

    fn read_data(&self, _target: &DataTarget) -> Result<Compound, Component> {
        Err(Component::translatable(
            "commands.data.entity.invalid",
            Vec::new(),
        ))
    }

    fn write_data(&mut self, _target: &DataTarget, _data: Compound) -> Result<(), Component> {
        Err(Component::translatable(
            "commands.data.entity.invalid",
            Vec::new(),
        ))
    }

    fn set_boss_bar_value(&mut self, id: &Identifier, maximum: bool, value: i32) -> bool {
        self.change_boss_bar(id, |bar| {
            let messages = if maximum {
                bar.set_max(value)
            } else {
                bar.set_value(value)
            };
            ((), messages)
        })
        .is_some()
    }

    fn boss_bar(&self, _id: &Identifier) -> Option<rspigot_domain::custom_boss::CustomBossBar> {
        None
    }

    fn boss_bars(&self) -> Vec<rspigot_domain::custom_boss::CustomBossBar> {
        Vec::new()
    }

    fn create_boss_bar(&mut self, _id: Identifier, _name: Component) -> bool {
        false
    }

    fn change_boss_bar<R>(
        &mut self,
        _id: &Identifier,
        _change: impl FnOnce(
            &mut rspigot_domain::custom_boss::CustomBossBar,
        ) -> (R, rspigot_domain::boss_event::BossMessages),
    ) -> Option<R> {
        None
    }

    fn remove_boss_bar(&mut self, _id: &Identifier) -> bool {
        false
    }

    fn world_border(&self) -> rspigot_domain::world_border::WorldBorder;

    fn world_game_time(&self) -> i64;

    fn change_world_border<R>(
        &mut self,
        change: impl FnOnce(&mut rspigot_domain::world_border::WorldBorder, i64) -> R,
    ) -> R;

    fn tick_state(&self) -> rspigot_domain::tick_rate::TickRate;

    fn change_tick_rate<R>(
        &mut self,
        change: impl FnOnce(&mut rspigot_domain::tick_rate::TickRate) -> R,
    ) -> R;

    fn scoreboard(&self) -> &Scoreboard;

    fn change_scoreboard<R>(
        &mut self,
        change: impl FnOnce(&mut Scoreboard) -> (R, Vec<ScoreboardMessage>),
    ) -> R;

    fn score_holder_name(&self, _target: Target) -> Option<String> {
        None
    }

    fn holder_display_name(&self, _holder: &str) -> Option<Component> {
        None
    }

    fn function_exists(&self, id: &Identifier) -> bool {
        self.function_state().library.function(id).is_some()
    }

    fn function_state(&self) -> &crate::functions::FunctionState;

    fn function_state_mut(&mut self) -> &mut crate::functions::FunctionState;

    fn reload_datapacks(&mut self) -> bool {
        false
    }

    fn datapack_lists_changed(&mut self, _lists: crate::functions::DataPackLists) {}

    fn server_function_permission(&self) -> u8 {
        2
    }

    fn server_function_source(&self) -> Source {
        self.command_source(CommandSender::Console)
    }

    fn apply_packs(&mut self, packs: crate::functions::Packs)
    where
        Self: Sized,
    {
        self.function_state_mut().apply(packs);
        let lists = self.function_state().packs.lists();
        self.datapack_lists_changed(lists);
        super::function_commands::run_server_tag(self, &Identifier::minecraft("load"));
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        Vec::new()
    }

    fn is_living(&self, _target: Target) -> bool {
        false
    }

    fn world_height(&self) -> (i32, i32) {
        (-64, 319)
    }

    fn transfer_player(&mut self, _player: PlayerId, _host: &str, _port: i32) {}

    fn set_idle_timeout(&mut self, _minutes: u32) {}

    fn start_profiler(&mut self) -> bool {
        false
    }

    fn stop_profiler(&mut self) -> Option<(f64, i64)> {
        None
    }

    fn fill_biome(
        &mut self,
        _min: BlockPos,
        _max: BlockPos,
        _biome: u16,
        _replaceable: Option<&[u16]>,
    ) -> Result<i32, super::FillBiomeProblem> {
        Err(super::FillBiomeProblem::NotLoaded)
    }

    fn random_seed(&mut self) -> u64 {
        0x2545_F491_4F6C_DD1D
    }

    fn set_chunk_forced(&mut self, _x: i32, _z: i32, _forced: bool) -> bool {
        false
    }

    fn forced_chunks(&self) -> Vec<(i32, i32)> {
        Vec::new()
    }

    fn is_chunk_forced(&self, _x: i32, _z: i32) -> bool {
        false
    }

    fn vehicle_of(&self, _target: Target) -> Option<Target> {
        None
    }

    fn would_loop(&self, _rider: Target, _vehicle: Target) -> bool {
        false
    }

    fn start_riding_target(&mut self, _rider: Target, _vehicle: Target) -> bool {
        false
    }

    fn stop_riding_target(&mut self, _rider: Target) {}

    fn rotate_to(&mut self, _target: Target, _yaw: f32, _pitch: f32) {}

    fn swing_target(
        &mut self,
        _target: Target,
        _hand: crate::events::Hand,
        _style: crate::events::SwingStyle,
        _duration: i32,
    ) -> bool {
        false
    }

    fn is_spectator(&self, _player: PlayerId) -> bool {
        false
    }

    fn can_be_camera(&self, _target: Target) -> bool {
        false
    }

    fn set_camera_of(&mut self, _player: PlayerId, _target: Option<Target>) {}

    fn attribute_state(
        &mut self,
        _target: Target,
        _attribute: &str,
    ) -> Option<rspigot_domain::effect::AttributeState> {
        None
    }

    fn default_attribute_base(&self, _target: Target, _attribute: &str) -> Option<f64> {
        None
    }

    fn set_attribute_base(&mut self, _target: Target, _attribute: &str, _base: f64) {}

    fn add_attribute_modifier(
        &mut self,
        _target: Target,
        _attribute: &str,
        _modifier: rspigot_domain::effect::AttributeModifier,
    ) -> bool {
        false
    }

    fn remove_attribute_modifier(&mut self, _target: Target, _attribute: &str, _id: &str) -> bool {
        false
    }

    fn damage_target(
        &mut self,
        _target: Target,
        _spec: super::entity_commands::DamageSpec,
        _amount: f32,
    ) -> bool {
        false
    }

    fn main_hand(&self, _target: Target) -> Option<rspigot_domain::item::ItemStack> {
        None
    }

    fn set_main_hand(&mut self, _target: Target, _stack: rspigot_domain::item::ItemStack) {}

    fn play_sound_to(
        &mut self,
        _player: PlayerId,
        _sound: &str,
        _source: rspigot_entities::SoundSource,
        _position: Vec3,
        _volume: f32,
        _pitch: f32,
    ) {
    }

    fn stop_sound_for(
        &mut self,
        _player: PlayerId,
        _source: Option<rspigot_entities::SoundSource>,
        _sound: Option<String>,
    ) {
    }

    fn send_particle_to(
        &mut self,
        _player: PlayerId,
        _particle: rspigot_domain::particle::Particle,
        _force: bool,
    ) -> bool {
        false
    }

    fn advancement_criteria(&self, _player: PlayerId, _advancement: &str) -> Vec<&'static str> {
        Vec::new()
    }

    fn advancement_done(&self, _player: PlayerId, _advancement: &str) -> bool {
        false
    }

    fn grant_criterion(&mut self, _player: PlayerId, _advancement: &str, _criterion: &str) -> bool {
        false
    }

    fn revoke_criterion(
        &mut self,
        _player: PlayerId,
        _advancement: &str,
        _criterion: &str,
    ) -> bool {
        false
    }

    fn give_recipes(&mut self, _player: PlayerId, _recipes: &[&str]) -> usize {
        0
    }

    fn take_recipes_of(&mut self, _player: PlayerId, _recipes: &[&str]) -> usize {
        0
    }

    fn read_slot(
        &self,
        _holder: super::slots::SlotHolder,
        _slot: i32,
    ) -> Option<Option<rspigot_domain::item::ItemStack>> {
        None
    }

    fn write_slot(
        &mut self,
        _holder: super::slots::SlotHolder,
        _slot: i32,
        _stack: Option<rspigot_domain::item::ItemStack>,
    ) -> bool {
        false
    }

    fn container_slots(&self, _pos: BlockPos) -> usize {
        0
    }

    fn insert_stack(
        &mut self,
        _pos: BlockPos,
        _stack: rspigot_domain::item::ItemStack,
    ) -> Option<Option<rspigot_domain::item::ItemStack>> {
        None
    }

    fn give_stack(&mut self, _player: PlayerId, _stack: rspigot_domain::item::ItemStack) {}

    fn drop_stack(&mut self, _at: Vec3, _stack: rspigot_domain::item::ItemStack) {}

    fn roll_loot(
        &mut self,
        _sender: CommandSender,
        _source: &super::loot_source::LootSource,
    ) -> Result<Vec<rspigot_domain::item::ItemStack>, Component> {
        Ok(Vec::new())
    }

    fn apply_item_modifier(
        &mut self,
        _sender: CommandSender,
        _functions: &[rspigot_data::loot::Function],
        stack: &rspigot_domain::item::ItemStack,
    ) -> Option<rspigot_domain::item::ItemStack> {
        Some(stack.clone())
    }

    fn start_world_query(
        &mut self,
        _sender: CommandSender,
        _query: crate::chunks::WorldQuery,
        _kind: crate::locate::QueryKind,
        _label: String,
    ) -> bool {
        false
    }

    fn nearest_poi(&self, _origin: Vec3, _types: &[String]) -> Option<(String, BlockPos)> {
        None
    }

    fn item_modifier(&self, _id: &str) -> Option<&'static [rspigot_data::loot::Function]> {
        None
    }

    fn stopwatch_started(&self, _id: &str) -> Option<i64> {
        None
    }

    fn set_stopwatch(&mut self, _id: &str, _started: Option<i64>) {}

    fn stopwatch_ids(&self) -> Vec<String> {
        Vec::new()
    }
}

pub fn execute<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    line: &str,
) -> CommandOutcome {
    let line = line.trim().trim_start_matches('/');
    let mut parts = line.split_whitespace();
    let Some(label) = parts.next() else {
        return CommandOutcome::Unknown;
    };
    let label = label.to_ascii_lowercase();
    let label = label
        .strip_prefix("minecraft:")
        .or_else(|| label.strip_prefix("bukkit:"))
        .unwrap_or(&label)
        .to_owned();
    let args: Vec<&str> = parts.collect();
    let level = ctx.permission_level(sender);
    let level = ctx
        .function_state()
        .runtime
        .permission_cap
        .map_or(level, |cap| level.min(cap));
    if !super::permission::may_use(&label, level) {
        return refuse(ctx, sender, line);
    }
    let rest = line
        .split_once(char::is_whitespace)
        .map_or("", |(_, r)| r.trim());
    let outcome = match label.as_str() {
        "help" | "?" => help(ctx, sender),
        "list" => list(ctx, sender),
        "seed" => {
            let seed = ctx.seed();
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.seed.success",
                    vec![Component::text(format!("[{seed}]")).color(NamedColor::Green)],
                ),
            );
            CommandOutcome::Success
        }
        "version" | "ver" | "about" => {
            ctx.reply(
                sender,
                Component::text(format!(
                    "This server is running rspigot version {} (MC: {})",
                    env!("CARGO_PKG_VERSION"),
                    rspigot_data::VERSION_NAME
                )),
            );
            CommandOutcome::Success
        }
        "plugins" | "pl" => {
            let names = ctx.plugin_names();
            ctx.reply(
                sender,
                Component::text(format!("Plugins ({}): {}", names.len(), names.join(", ")))
                    .color(NamedColor::White),
            );
            CommandOutcome::Success
        }
        "tps" => {
            let [a, b, c] = ctx.tps();
            ctx.reply(
                sender,
                Component::text(format!("TPS from last 1m, 5m, 15m: {a:.1}, {b:.1}, {c:.1}"))
                    .color(NamedColor::Gold),
            );
            CommandOutcome::Success
        }
        "me" => {
            let name = ctx.sender_name(sender);
            ctx.broadcast(Component::text(format!("* {name} {rest}")));
            CommandOutcome::Success
        }
        "say" => {
            let name = ctx.sender_name(sender);
            ctx.broadcast(Component::text(format!("[{name}] {rest}")));
            CommandOutcome::Success
        }
        "msg" | "tell" | "w" => private_message(ctx, sender, &args),
        "gamemode" => gamemode(ctx, sender, &args),
        "tp" | "teleport" => teleport(ctx, sender, rest).unwrap_or_else(|outcome| outcome),
        "execute" => super::execute::run(ctx, sender, rest),
        "summon" => summon(ctx, sender, rest).unwrap_or_else(|outcome| outcome),
        "time" => time(ctx, sender, &args),
        "give" => give(ctx, sender, &args),
        "clear" => clear(ctx, sender, &args),
        "kick" => kick(ctx, sender, &args),
        "op" | "deop" => operator(ctx, sender, &args, label == "op"),
        "whitelist" => whitelist(ctx, sender, &args),
        "ban" => ban(ctx, sender, &args),
        "ban-ip" => ban_ip(ctx, sender, &args),
        "pardon" => pardon(ctx, sender, &args),
        "pardon-ip" => pardon_ip(ctx, sender, &args),
        "banlist" => banlist(ctx, sender, &args),
        "effect" => effect(ctx, sender, rest),
        "kill" => kill(ctx, sender, rest).unwrap_or_else(|outcome| outcome),
        "difficulty" => difficulty(ctx, sender, &args),
        "defaultgamemode" | "gamerule" | "experience" | "xp" | "tag" | "save-on" | "save-off"
        | "random" | "spawnpoint" => super::server_commands::run(ctx, sender, &label, rest),
        "tellraw" | "title" => super::text_commands::run(ctx, sender, &label, rest),
        "playsound" | "stopsound" | "particle" => {
            super::sound_commands::run(ctx, sender, &label, rest)
        }
        "attribute" | "damage" | "enchant" => {
            super::entity_commands::run(ctx, sender, &label, rest)
        }
        "ride" | "rotate" | "spectate" | "swing" => {
            super::movement_commands::run(ctx, sender, &label, rest)
        }
        "tick" => super::tick_commands::run(ctx, sender, rest),
        "transfer" | "setidletimeout" | "debug" => {
            super::admin_commands::run(ctx, sender, &label, rest)
        }
        "forceload" | "worldborder" | "spreadplayers" | "fillbiome" => {
            super::world_commands::run(ctx, sender, &label, rest)
        }
        "function" | "schedule" | "return" | "datapack" => {
            super::function_commands::run(ctx, sender, &label, rest)
        }
        "advancement" | "recipe" | "item" | "loot" | "waypoint" | "stopwatch" | "locate"
        | "place" => super::game_commands::run(ctx, sender, &label, rest),
        "bossbar" => super::boss_commands::run(ctx, sender, rest),
        "scoreboard" | "trigger" => super::scoreboard_commands::run(ctx, sender, &label, rest),
        "team" | "teammsg" | "tm" => super::team_commands::run(ctx, sender, &label, rest),
        "data" => super::data::run(ctx, sender, rest),
        "setblock" => super::blocks::setblock(ctx, sender, rest),
        "fill" => super::blocks::fill(ctx, sender, rest),
        "clone" => super::blocks::clone(ctx, sender, rest),
        "weather" => weather(ctx, sender, &args),
        "setworldspawn" => set_world_spawn(ctx, sender, &args),
        "save-all" => save_all(ctx, sender, rest),
        "reload" => {
            let datapacks = ctx.reload_datapacks();
            if !ctx.request_reload(false) && !datapacks {
                return fail(ctx, sender, "commands.reload.failure", Vec::new());
            }
            ctx.reply(
                sender,
                if datapacks {
                    Component::text("Reloaded!")
                } else {
                    Component::translatable("commands.reload.success", Vec::new())
                },
            );
            CommandOutcome::Success
        }
        "stop" => {
            ctx.reply(sender, Component::text("Stopping the server"));
            ctx.stop();
            CommandOutcome::Success
        }
        "restart" | "spigot" | "timings" => super::bukkit_commands::run(ctx, sender, &label, rest),
        _ => CommandOutcome::Unknown,
    };
    if outcome == CommandOutcome::Unknown {
        let text = ctx
            .unknown_command_text()
            .unwrap_or_else(|| "Unknown command. Type \"/help\" for help.".to_owned());
        ctx.reply(sender, Component::text(text));
    }
    outcome
}

pub(crate) fn refuse<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    line: &str,
) -> CommandOutcome {
    if let Some(text) = ctx.unknown_command_text() {
        ctx.reply(sender, Component::text(text));
        return CommandOutcome::Denied;
    }
    ctx.reply(
        sender,
        Component::translatable("command.unknown.command", Vec::new()).color(NamedColor::Red),
    );
    ctx.reply(
        sender,
        Component::text("")
            .color(NamedColor::Gray)
            .append(
                Component::text(line)
                    .color(NamedColor::Red)
                    .underlined(true),
            )
            .append(
                Component::translatable("command.context.here", Vec::new())
                    .color(NamedColor::Red)
                    .italic(true),
            ),
    );
    CommandOutcome::Denied
}

pub(super) fn usage<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    text: &str,
) -> CommandOutcome {
    ctx.reply(
        sender,
        Component::text(format!("Usage: {text}")).color(NamedColor::Red),
    );
    CommandOutcome::Failure
}

fn not_found<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    ctx.reply(
        sender,
        Component::translatable("argument.entity.notfound.player", Vec::new())
            .color(NamedColor::Red),
    );
    CommandOutcome::Failure
}

fn help<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let mut lines = vec![
        "/help",
        "/list",
        "/me <action>",
        "/msg <player> <message>",
        "/tps",
        "/version",
        "/plugins",
    ];
    let level = ctx.permission_level(sender);
    if level >= super::permission::LEVEL_GAMEMASTERS {
        lines.extend([
            "/seed",
            "/say <message>",
            "/gamemode <mode> [player]",
            "/tp <x> <y> <z> | <player> [target]",
            "/time set|add|query <value>",
            "/give <player> <item> [count]",
            "/clear [player]",
            "/kill [targets]",
            "/summon <entity> [pos] [nbt]",
            "/difficulty [level]",
            "/weather <clear|rain|thunder> [duration]",
            "/setworldspawn [x y z]",
        ]);
    }
    if level >= super::permission::LEVEL_ADMINS {
        lines.extend(["/kick <player> [reason]", "/op <player>", "/deop <player>"]);
    }
    if level >= super::permission::LEVEL_OWNERS {
        lines.extend(["/save-all", "/stop"]);
    }
    ctx.reply(
        sender,
        Component::text("--------- Help ---------").color(NamedColor::Yellow),
    );
    for line in lines {
        ctx.reply(sender, Component::text(line).color(NamedColor::Gold));
    }
    CommandOutcome::Success
}

fn list<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let names = ctx.player_names();
    let max = ctx.max_players();
    ctx.reply(
        sender,
        Component::translatable(
            "commands.list.players",
            vec![
                Component::text(names.len().to_string()),
                Component::text(max.to_string()),
                Component::text(names.join(", ")),
            ],
        ),
    );
    CommandOutcome::Success
}

fn private_message<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    if args.len() < 2 {
        return usage(ctx, sender, "/msg <player> <message>");
    }
    let Some(target) = ctx.find_player(args[0]) else {
        return not_found(ctx, sender);
    };
    let from = ctx.sender_name(sender);
    let to = ctx.player_name(target).unwrap_or_default();
    let message = args[1..].join(" ");
    ctx.reply(
        CommandSender::Player(target),
        Component::text(format!("{from} whispers to you: {message}"))
            .color(NamedColor::Gray)
            .italic(true),
    );
    ctx.reply(
        sender,
        Component::text(format!("You whisper to {to}: {message}"))
            .color(NamedColor::Gray)
            .italic(true),
    );
    CommandOutcome::Success
}

fn target_or_self<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    arg: Option<&&str>,
) -> Option<PlayerId> {
    match arg {
        Some(name) if *name == "@s" => match sender {
            CommandSender::Player(id) => Some(id),
            CommandSender::Console => None,
        },
        Some(name) => ctx.find_player(name),
        None => match sender {
            CommandSender::Player(id) => Some(id),
            CommandSender::Console => None,
        },
    }
}

fn mode_key(mode: GameMode) -> String {
    format!("gameMode.{}", mode.name())
}

fn gamemode<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    let Some(mode) = args.first().and_then(|m| GameMode::from_name(m)) else {
        return usage(
            ctx,
            sender,
            "/gamemode <survival|creative|adventure|spectator> [player]",
        );
    };
    let Some(target) = target_or_self(ctx, sender, args.get(1)) else {
        return not_found(ctx, sender);
    };
    let mode_name = Component::translatable(mode_key(mode), Vec::new());
    let replies = if CommandSender::Player(target) == sender {
        vec![(
            sender,
            Component::translatable("commands.gamemode.success.self", vec![mode_name]),
        )]
    } else {
        let name = ctx.player_name(target).unwrap_or_default();
        vec![
            (
                CommandSender::Player(target),
                Component::translatable("gameMode.changed", vec![mode_name.clone()]),
            ),
            (
                sender,
                Component::translatable(
                    "commands.gamemode.success.other",
                    vec![Component::text(name), mode_name],
                ),
            ),
        ]
    };
    ctx.set_game_mode(target, mode, replies);
    CommandOutcome::Success
}

const SIN_SCALE: f32 = 10430.378;

fn mth_sin(value: f32) -> f32 {
    let index = ((value * SIN_SCALE) as i64 & 65535) as f64;
    (index * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32
}

fn mth_cos(value: f32) -> f32 {
    let index = ((value * SIN_SCALE + 16384.0) as i64 & 65535) as f64;
    (index * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32
}

fn coordinate_value(raw: &str, center: bool) -> Result<(bool, f64), Component> {
    let (relative, number) = match raw.strip_prefix('~') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    if relative && number.is_empty() {
        return Ok((true, 0.0));
    }
    let value: f64 = number.parse().map_err(|_| {
        Component::translatable("parsing.double.invalid", vec![Component::text(number)])
    })?;
    let value = if !relative && center && !number.contains('.') {
        value + 0.5
    } else {
        value
    };
    Ok((relative, value))
}

pub(crate) fn parse_vec3<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    raw: [&str; 3],
) -> Result<Vec3, Component> {
    let local = raw.iter().filter(|r| r.starts_with('^')).count();
    if local != 0 && local != 3 {
        return Err(Component::translatable("argument.pos.mixed", Vec::new()));
    }
    let origin = ctx.sender_origin(sender);
    if local == 3 {
        let mut values = [0.0f64; 3];
        for (slot, text) in values.iter_mut().zip(raw) {
            let number = &text[1..];
            if !number.is_empty() {
                *slot = number.parse().map_err(|_| {
                    Component::translatable("parsing.double.invalid", vec![Component::text(number)])
                })?;
            }
        }
        let origin = ctx.local_origin(sender);
        let rotation = ctx.sender_rotation(sender);
        let radians = 0.017_453_292f32;
        let f = mth_cos((rotation.yaw + 90.0) * radians);
        let g = mth_sin((rotation.yaw + 90.0) * radians);
        let h = mth_cos(-rotation.pitch * radians);
        let i = mth_sin(-rotation.pitch * radians);
        let j = mth_cos((-rotation.pitch + 90.0) * radians);
        let k = mth_sin((-rotation.pitch + 90.0) * radians);
        let forward = Vec3::new(f64::from(f * h), f64::from(i), f64::from(g * h));
        let up = Vec3::new(f64::from(f * j), f64::from(k), f64::from(g * j));
        let left = Vec3::new(
            -(forward.y * up.z - forward.z * up.y),
            -(forward.z * up.x - forward.x * up.z),
            -(forward.x * up.y - forward.y * up.x),
        );
        let [l, u, w] = values;
        return Ok(Vec3::new(
            origin.x + forward.x * w + up.x * u + left.x * l,
            origin.y + forward.y * w + up.y * u + left.y * l,
            origin.z + forward.z * w + up.z * u + left.z * l,
        ));
    }
    let (rx, x) = coordinate_value(raw[0], true)?;
    let (ry, y) = coordinate_value(raw[1], false)?;
    let (rz, z) = coordinate_value(raw[2], true)?;
    Ok(Vec3::new(
        if rx { origin.x + x } else { x },
        if ry { origin.y + y } else { y },
        if rz { origin.z + z } else { z },
    ))
}

pub(super) fn next_argument(input: &str) -> Option<(&str, &str)> {
    let (token, rest) = split_argument(input);
    (!token.is_empty()).then_some((token, rest))
}

pub(super) fn looks_like_coordinate(token: &str) -> bool {
    token.starts_with('~') || token.starts_with('^') || token.parse::<f64>().is_ok()
}

fn take_vec3<'a, C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    input: &'a str,
) -> Result<Option<(Vec3, &'a str)>, Component> {
    let Some((a, rest)) = next_argument(input) else {
        return Ok(None);
    };
    if !looks_like_coordinate(a) {
        return Ok(None);
    }
    let (Some((b, rest)), true) = (next_argument(rest), true) else {
        return Err(Component::translatable(
            "argument.pos3d.incomplete",
            Vec::new(),
        ));
    };
    let Some((c, rest)) = next_argument(rest) else {
        return Err(Component::translatable(
            "argument.pos3d.incomplete",
            Vec::new(),
        ));
    };
    Ok(Some((parse_vec3(ctx, sender, [a, b, c])?, rest)))
}

fn format_coordinate(value: f64) -> Component {
    Component::text(format!("{value:.6}"))
}

pub(super) fn select_one<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    selector: &str,
) -> Result<Target, CommandOutcome> {
    let targets = select(ctx, sender, Some(selector))?;
    match targets.as_slice() {
        [single] => Ok(*single),
        _ => Err(fail(ctx, sender, "argument.entity.toomany", Vec::new())),
    }
}

fn teleport<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<CommandOutcome, CommandOutcome> {
    let usage_text = "/tp <x> <y> <z> | <player> [target]";
    let Some((first, after_first)) = next_argument(rest) else {
        return Err(usage(ctx, sender, usage_text));
    };
    let executor = ctx.command_source(sender).executor;
    let self_target = || executor;
    let (targets, destination, destination_name) = if looks_like_coordinate(first) {
        let position = match take_vec3(ctx, sender, rest) {
            Ok(Some((position, _))) => position,
            Ok(None) => return Err(usage(ctx, sender, usage_text)),
            Err(message) => {
                ctx.reply(sender, message.color(NamedColor::Red));
                return Err(CommandOutcome::Failure);
            }
        };
        let Some(target) = self_target() else {
            return Err(fail(ctx, sender, "permissions.requires.entity", Vec::new()));
        };
        (vec![target], position, None)
    } else {
        match next_argument(after_first) {
            None => {
                let destination = select_one(ctx, sender, first)?;
                let Some(target) = self_target() else {
                    return Err(fail(ctx, sender, "permissions.requires.entity", Vec::new()));
                };
                let position = ctx.target_position(destination).unwrap_or(Vec3::ZERO);
                (vec![target], position, Some(ctx.target_name(destination)))
            }
            Some((second, _)) => {
                let targets = select(ctx, sender, Some(first))?;
                if looks_like_coordinate(second) {
                    let position = match take_vec3(ctx, sender, after_first) {
                        Ok(Some((position, _))) => position,
                        Ok(None) => return Err(usage(ctx, sender, usage_text)),
                        Err(message) => {
                            ctx.reply(sender, message.color(NamedColor::Red));
                            return Err(CommandOutcome::Failure);
                        }
                    };
                    (targets, position, None)
                } else {
                    let destination = select_one(ctx, sender, second)?;
                    let position = ctx.target_position(destination).unwrap_or(Vec3::ZERO);
                    (targets, position, Some(ctx.target_name(destination)))
                }
            }
        }
    };
    if !destination.is_finite()
        || destination.x.abs() > 30_000_000.0
        || destination.z.abs() > 30_000_000.0
        || destination.y.abs() >= 20_000_000.0
    {
        return Err(fail(
            ctx,
            sender,
            "commands.teleport.invalidPosition",
            Vec::new(),
        ));
    }
    for target in &targets {
        ctx.teleport_target(*target, destination);
    }
    ctx.record_result(targets.len() as i32);
    let message = match (targets.as_slice(), destination_name) {
        ([single], None) => Component::translatable(
            "commands.teleport.success.location.single",
            vec![
                ctx.target_name(*single),
                format_coordinate(destination.x),
                format_coordinate(destination.y),
                format_coordinate(destination.z),
            ],
        ),
        (many, None) => Component::translatable(
            "commands.teleport.success.location.multiple",
            vec![
                Component::text(many.len().to_string()),
                format_coordinate(destination.x),
                format_coordinate(destination.y),
                format_coordinate(destination.z),
            ],
        ),
        ([single], Some(name)) => Component::translatable(
            "commands.teleport.success.entity.single",
            vec![ctx.target_name(*single), name],
        ),
        (many, Some(name)) => Component::translatable(
            "commands.teleport.success.entity.multiple",
            vec![Component::text(many.len().to_string()), name],
        ),
    };
    ctx.reply(sender, message);
    Ok(CommandOutcome::Success)
}

pub(super) fn unknown_dimension<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    name: &str,
) -> CommandOutcome {
    fail(
        ctx,
        sender,
        "argument.dimension.invalid",
        vec![Component::text(name)],
    )
}

fn summon<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<CommandOutcome, CommandOutcome> {
    let Some((raw_kind, after_kind)) = next_argument(rest) else {
        return Err(usage(ctx, sender, "/summon <entity> [pos] [nbt]"));
    };
    let id = if raw_kind.contains(':') {
        raw_kind.to_owned()
    } else {
        format!("minecraft:{raw_kind}")
    };
    let Some(kind) = rspigot_entities::EntityKind::from_name(&id) else {
        return Err(fail(
            ctx,
            sender,
            "argument.resource.not_found",
            vec![
                Component::text(id),
                Component::text("minecraft:entity_type"),
            ],
        ));
    };
    if !kind.summonable() {
        return Err(fail(
            ctx,
            sender,
            "entity.not_summonable",
            vec![Component::text(kind.name())],
        ));
    }
    let (position, after_pos) = match take_vec3(ctx, sender, after_kind) {
        Ok(Some((position, rest))) => (position, rest),
        Ok(None) => (ctx.sender_origin(sender), after_kind),
        Err(message) => {
            ctx.reply(sender, message.color(NamedColor::Red));
            return Err(CommandOutcome::Failure);
        }
    };
    let nbt_text = after_pos.trim();
    let nbt = if nbt_text.is_empty() {
        None
    } else {
        match rspigot_nbt::parse_compound(nbt_text) {
            Ok((compound, consumed)) if nbt_text[consumed..].trim().is_empty() => Some(compound),
            Ok(_) => {
                return Err(fail(ctx, sender, "command.unknown.argument", Vec::new()));
            }
            Err(error) => {
                return Err(fail(ctx, sender, error.key, Vec::new()));
            }
        }
    };
    match ctx.summon(sender, kind, position, nbt) {
        Ok(_) => Ok(CommandOutcome::Success),
        Err(message) => {
            ctx.reply(sender, message.color(NamedColor::Red));
            Err(CommandOutcome::Failure)
        }
    }
}

pub(super) fn parse_time(raw: &str) -> Option<i64> {
    let (number, unit) = match raw.chars().last()? {
        'd' => (&raw[..raw.len() - 1], 24000.0),
        's' => (&raw[..raw.len() - 1], 20.0),
        't' => (&raw[..raw.len() - 1], 1.0),
        _ => (raw, 1.0),
    };
    let value: f64 = number.parse().ok()?;
    (value >= 0.0).then(|| (value * unit).round() as i64)
}

fn time<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    match (args.first().copied(), args.get(1).copied()) {
        (Some("set"), Some(value)) => {
            let ticks = match value {
                "day" => Some(1000),
                "noon" => Some(6000),
                "night" => Some(13000),
                "midnight" => Some(18000),
                other => parse_time(other),
            };
            let Some(ticks) = ticks else {
                return usage(ctx, sender, "/time set <day|noon|night|midnight|ticks>");
            };
            ctx.set_day_time(ticks);
            ctx.record_result((ticks.rem_euclid(24_000)) as i32);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.time.set",
                    vec![Component::text(ticks.to_string())],
                ),
            );
            CommandOutcome::Success
        }
        (Some("add"), Some(value)) => {
            let Some(ticks) = parse_time(value) else {
                return usage(ctx, sender, "/time add <ticks>");
            };
            let now = ctx.day_time() + ticks;
            ctx.skip_day_time(now);
            ctx.record_result(now.rem_euclid(24_000) as i32);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.time.set",
                    vec![Component::text(now.to_string())],
                ),
            );
            CommandOutcome::Success
        }
        (Some("query"), Some(kind)) => {
            let value = match kind {
                "daytime" => ctx.day_time().rem_euclid(24000),
                "gametime" => ctx.game_time(),
                "day" => ctx.day_time().div_euclid(24000),
                _ => return usage(ctx, sender, "/time query <daytime|gametime|day>"),
            };
            ctx.record_result((value % i64::from(i32::MAX)) as i32);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.time.query",
                    vec![Component::text(value.to_string())],
                ),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, "/time <set|add|query> <value>"),
    }
}

fn give<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    if args.len() < 2 {
        return usage(ctx, sender, "/give <player> <item> [count]");
    }
    let Some(target) = target_or_self(ctx, sender, args.first()) else {
        return not_found(ctx, sender);
    };
    let Some(item) = ItemKind::from_name(args[1]) else {
        ctx.reply(
            sender,
            Component::translatable("argument.item.id.invalid", vec![Component::text(args[1])])
                .color(NamedColor::Red),
        );
        return CommandOutcome::Failure;
    };
    let count = args
        .get(2)
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(1)
        .clamp(1, 6400);
    if !ctx.give(target, item.id(), count) {
        return CommandOutcome::Failure;
    }
    let name = ctx.player_name(target).unwrap_or_default();
    ctx.reply(
        sender,
        Component::translatable(
            "commands.give.success.single",
            vec![
                Component::text(count.to_string()),
                Component::text(format!("[{}]", item.name())),
                Component::text(name),
            ],
        ),
    );
    CommandOutcome::Success
}

fn clear<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let Some(target) = target_or_self(ctx, sender, args.first()) else {
        return not_found(ctx, sender);
    };
    let removed = ctx.clear_inventory(target);
    let name = ctx.player_name(target).unwrap_or_default();
    ctx.reply(
        sender,
        Component::translatable(
            "commands.clear.success.single",
            vec![Component::text(removed.to_string()), Component::text(name)],
        ),
    );
    CommandOutcome::Success
}

fn kick<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let Some(target) = args.first().and_then(|n| ctx.find_player(n)) else {
        return not_found(ctx, sender);
    };
    let reason = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "Kicked by an operator".to_owned()
    };
    let name = ctx.player_name(target).unwrap_or_default();
    ctx.kick(target, Component::text(reason.clone()));
    ctx.reply(
        sender,
        Component::translatable(
            "commands.kick.success",
            vec![Component::text(name), Component::text(reason)],
        ),
    );
    CommandOutcome::Success
}

fn operator<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
    grant: bool,
) -> CommandOutcome {
    let Some(name) = args.first() else {
        return usage(
            ctx,
            sender,
            if grant {
                "/op <player>"
            } else {
                "/deop <player>"
            },
        );
    };
    if !ctx.set_operator(name, grant) {
        let key = if grant {
            "commands.op.failed"
        } else {
            "commands.deop.failed"
        };
        ctx.reply(
            sender,
            Component::translatable(key, Vec::new()).color(NamedColor::Red),
        );
        return CommandOutcome::Failure;
    }
    let key = if grant {
        "commands.op.success"
    } else {
        "commands.deop.success"
    };
    ctx.reply(
        sender,
        Component::translatable(key, vec![Component::text(*name)]),
    );
    CommandOutcome::Success
}

const MAX_EFFECT_SECONDS: i64 = 1_000_000;
const MAX_AMPLIFIER: i64 = 255;
const DEFAULT_EFFECT_TICKS: u32 = 600;
const INSTANT_EFFECTS: [&str; 3] = [
    "minecraft:instant_health",
    "minecraft:instant_damage",
    "minecraft:saturation",
];

fn save_all<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let flush = match rest.trim() {
        "" => false,
        "flush" => true,
        _ => return usage(ctx, sender, "/save-all [flush]"),
    };
    ctx.reply(
        sender,
        Component::translatable("commands.save.saving", Vec::new()),
    );
    if !ctx.save_all(flush) {
        return fail(ctx, sender, "commands.save.failed", Vec::new());
    }
    ctx.record_result(1);
    ctx.reply(
        sender,
        Component::translatable("commands.save.success", Vec::new()),
    );
    CommandOutcome::Success
}

pub(super) fn fail<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    key: &str,
    args: Vec<Component>,
) -> CommandOutcome {
    ctx.reply(
        sender,
        Component::translatable(key, args).color(NamedColor::Red),
    );
    CommandOutcome::Failure
}

fn bounded_int<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
    min: i64,
    max: i64,
) -> Result<i64, CommandOutcome> {
    let Ok(value) = raw.parse::<i64>() else {
        return Err(fail(
            ctx,
            sender,
            "parsing.int.invalid",
            vec![Component::text(raw)],
        ));
    };
    if value < min {
        return Err(fail(
            ctx,
            sender,
            "argument.integer.low",
            vec![
                Component::text(min.to_string()),
                Component::text(value.to_string()),
            ],
        ));
    }
    if value > max {
        return Err(fail(
            ctx,
            sender,
            "argument.integer.big",
            vec![
                Component::text(max.to_string()),
                Component::text(value.to_string()),
            ],
        ));
    }
    Ok(value)
}

fn parse_effect<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<EffectId, CommandOutcome> {
    EffectId::from_name(raw).ok_or_else(|| {
        fail(
            ctx,
            sender,
            "argument.resource.not_found",
            vec![
                Component::text(raw),
                Component::text("minecraft:mob_effect"),
            ],
        )
    })
}

pub(super) fn select<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    selector: Option<&str>,
) -> Result<Vec<Target>, CommandOutcome> {
    let selector = selector.unwrap_or("@s");
    match ctx.select_entities(sender, selector) {
        Ok(targets) if targets.is_empty() => Err(fail(
            ctx,
            sender,
            "argument.entity.notfound.entity",
            Vec::new(),
        )),
        Ok(targets) => Ok(targets),
        Err(message) => {
            ctx.reply(sender, message.color(NamedColor::Red));
            Err(CommandOutcome::Failure)
        }
    }
}

fn effect_name(effect: EffectId) -> Component {
    Component::translatable(effect.translation_key().unwrap_or_default(), Vec::new())
}

fn arguments(rest: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut input = rest;
    while let Some((token, after)) = next_argument(input) {
        out.push(token);
        input = after;
    }
    out
}

fn effect<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let args = arguments(rest);
    let args = args.as_slice();
    match args.first().copied() {
        Some("give") => effect_give(ctx, sender, &args[1..]).unwrap_or_else(|outcome| outcome),
        Some("clear") => effect_clear(ctx, sender, &args[1..]).unwrap_or_else(|outcome| outcome),
        _ => usage(
            ctx,
            sender,
            "/effect <give|clear> <targets> [effect] [seconds] [amplifier] [hideParticles]",
        ),
    }
}

fn effect_give<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> Result<CommandOutcome, CommandOutcome> {
    let (Some(selector), Some(raw_effect)) = (args.first(), args.get(1)) else {
        return Err(usage(
            ctx,
            sender,
            "/effect give <targets> <effect> [seconds|infinite] [amplifier] [hideParticles]",
        ));
    };
    if args.len() > 5 {
        return Err(usage(
            ctx,
            sender,
            "/effect give <targets> <effect> [seconds|infinite] [amplifier] [hideParticles]",
        ));
    }
    let targets = select(ctx, sender, Some(selector))?;
    let effect = parse_effect(ctx, sender, raw_effect)?;
    let instant = effect
        .name()
        .is_some_and(|name| INSTANT_EFFECTS.contains(&name));
    let duration = match args.get(2).copied() {
        None => EffectDuration::Ticks(if instant { 1 } else { DEFAULT_EFFECT_TICKS }),
        Some("infinite") => EffectDuration::Infinite,
        Some(raw) => {
            let seconds = bounded_int(ctx, sender, raw, 1, MAX_EFFECT_SECONDS)? as u32;
            EffectDuration::Ticks(if instant { seconds } else { seconds * 20 })
        }
    };
    let amplifier = match args.get(3) {
        None => 0,
        Some(raw) => bounded_int(ctx, sender, raw, 0, MAX_AMPLIFIER)? as u8,
    };
    let hide_particles = match args.get(4).copied() {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => {
            return Err(fail(
                ctx,
                sender,
                "parsing.bool.invalid",
                vec![Component::text(other)],
            ));
        }
    };
    let instance = EffectInstance::new(effect, duration, amplifier, !hide_particles);
    let applied = targets
        .iter()
        .filter(|target| ctx.give_effect_to(**target, instance))
        .count();
    if applied == 0 {
        return Ok(fail(ctx, sender, "commands.effect.give.failed", Vec::new()));
    }
    let message = match targets.as_slice() {
        [single] => Component::translatable(
            "commands.effect.give.success.single",
            vec![effect_name(effect), ctx.target_name(*single)],
        ),
        many => Component::translatable(
            "commands.effect.give.success.multiple",
            vec![effect_name(effect), Component::text(many.len().to_string())],
        ),
    };
    ctx.reply(sender, message);
    Ok(CommandOutcome::Success)
}

fn effect_clear<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> Result<CommandOutcome, CommandOutcome> {
    if args.len() > 2 {
        return Err(usage(ctx, sender, "/effect clear [targets] [effect]"));
    }
    let targets = select(ctx, sender, args.first().copied())?;
    let effect = match args.get(1) {
        None => None,
        Some(raw) => Some(parse_effect(ctx, sender, raw)?),
    };
    let cleared = targets
        .iter()
        .filter(|target| ctx.clear_effect_of(**target, effect))
        .count();
    let failed = match effect {
        None => "commands.effect.clear.everything.failed",
        Some(_) => "commands.effect.clear.specific.failed",
    };
    if cleared == 0 {
        return Ok(fail(ctx, sender, failed, Vec::new()));
    }
    let message = match (effect, targets.as_slice()) {
        (None, [single]) => Component::translatable(
            "commands.effect.clear.everything.success.single",
            vec![ctx.target_name(*single)],
        ),
        (None, many) => Component::translatable(
            "commands.effect.clear.everything.success.multiple",
            vec![Component::text(many.len().to_string())],
        ),
        (Some(effect), [single]) => Component::translatable(
            "commands.effect.clear.specific.success.single",
            vec![effect_name(effect), ctx.target_name(*single)],
        ),
        (Some(effect), many) => Component::translatable(
            "commands.effect.clear.specific.success.multiple",
            vec![effect_name(effect), Component::text(many.len().to_string())],
        ),
    };
    ctx.reply(sender, message);
    Ok(CommandOutcome::Success)
}

fn kill<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<CommandOutcome, CommandOutcome> {
    let args = arguments(rest);
    if args.len() > 1 {
        return Err(usage(ctx, sender, "/kill [targets]"));
    }
    let targets = match (args.first().copied(), sender) {
        (None, CommandSender::Console) => {
            return Err(fail(ctx, sender, "permissions.requires.entity", Vec::new()));
        }
        (None, CommandSender::Player(id)) => vec![Target::Player(id)],
        (Some(selector), _) => select(ctx, sender, Some(selector))?,
    };
    let names: Vec<Component> = targets.iter().map(|t| ctx.target_name(*t)).collect();
    for target in &targets {
        ctx.kill_target(*target);
    }
    ctx.record_result(targets.len() as i32);
    let message = match names.as_slice() {
        [single] => Component::translatable("commands.kill.success.single", vec![single.clone()]),
        many => Component::translatable(
            "commands.kill.success.multiple",
            vec![Component::text(many.len().to_string())],
        ),
    };
    ctx.reply(sender, message);
    Ok(CommandOutcome::Success)
}

fn whitelist<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    let reply = |ctx: &mut C, key: &str, args: Vec<Component>| {
        ctx.reply(sender, Component::translatable(key, args))
    };
    match (args.first().copied(), args.get(1).copied()) {
        (Some("on"), _) => {
            let key = if ctx.set_whitelist_enabled(true) {
                "commands.whitelist.enabled"
            } else {
                "commands.whitelist.alreadyOn"
            };
            reply(ctx, key, Vec::new());
        }
        (Some("off"), _) => {
            let key = if ctx.set_whitelist_enabled(false) {
                "commands.whitelist.disabled"
            } else {
                "commands.whitelist.alreadyOff"
            };
            reply(ctx, key, Vec::new());
        }
        (Some("add"), Some(name)) => {
            let key = if ctx.whitelist_add(name) {
                "commands.whitelist.add.success"
            } else {
                "commands.whitelist.add.failed"
            };
            reply(ctx, key, vec![Component::text(name)]);
        }
        (Some("remove"), Some(name)) => {
            let key = if ctx.whitelist_remove(name) {
                "commands.whitelist.remove.success"
            } else {
                "commands.whitelist.remove.failed"
            };
            reply(ctx, key, vec![Component::text(name)]);
        }
        (Some("list"), _) => {
            let names = ctx.whitelist_names();
            if names.is_empty() {
                reply(ctx, "commands.whitelist.none", Vec::new());
            } else {
                reply(
                    ctx,
                    "commands.whitelist.list",
                    vec![
                        Component::text(names.len().to_string()),
                        Component::text(names.join(", ")),
                    ],
                );
            }
        }
        (Some("reload"), _) => {
            if !ctx.request_reload(true) {
                return fail(ctx, sender, "commands.reload.failure", Vec::new());
            }
            reply(ctx, "commands.whitelist.reloaded", Vec::new())
        }
        _ => {
            return usage(
                ctx,
                sender,
                "/whitelist <on|off|add|remove|list|reload> [player]",
            );
        }
    }
    CommandOutcome::Success
}

fn profile_targets<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<Vec<ListEntry>, CommandOutcome> {
    if !raw.starts_with('@') {
        return Ok(vec![ctx.profile_entry(raw)]);
    }
    match ctx.select_players(sender, raw) {
        Ok(players) if players.is_empty() => Err(not_found(ctx, sender)),
        Ok(players) => Ok(players
            .into_iter()
            .filter_map(|id| ctx.player_entry(id))
            .collect()),
        Err(message) => {
            ctx.reply(sender, message.color(NamedColor::Red));
            Err(CommandOutcome::Failure)
        }
    }
}

fn reason_argument(args: &[&str]) -> Option<String> {
    (args.len() > 1).then(|| args[1..].join(" "))
}

fn ban<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let Some(raw) = args.first() else {
        return usage(ctx, sender, "/ban <targets> [<reason>]");
    };
    let targets = match profile_targets(ctx, sender, raw) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let reason = reason_argument(args);
    let source = ctx.sender_name(sender);
    let mut banned = 0;
    for target in targets {
        let name = target.name.clone();
        if ctx.ban_profile(target, &source, reason.clone(), None) {
            banned += 1;
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.ban.success",
                    vec![Component::text(name), ban_reason(reason.as_deref())],
                ),
            );
        }
    }
    if banned == 0 {
        return fail(ctx, sender, "commands.ban.failed", Vec::new());
    }
    CommandOutcome::Success
}

fn ban_ip<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let Some(raw) = args.first() else {
        return usage(ctx, sender, "/ban-ip <address|player> [<reason>]");
    };
    let address = raw
        .parse::<IpAddr>()
        .ok()
        .or_else(|| ctx.find_player(raw).and_then(|id| ctx.player_address(id)));
    let Some(address) = address else {
        return fail(ctx, sender, "commands.banip.invalid", Vec::new());
    };
    let reason = reason_argument(args);
    let source = ctx.sender_name(sender);
    let Some(kicked) = ctx.ban_address(address, &source, reason.clone(), None) else {
        return fail(ctx, sender, "commands.banip.failed", Vec::new());
    };
    ctx.reply(
        sender,
        Component::translatable(
            "commands.banip.success",
            vec![
                Component::text(address.to_string()),
                ban_reason(reason.as_deref()),
            ],
        ),
    );
    if !kicked.is_empty() {
        ctx.reply(
            sender,
            Component::translatable(
                "commands.banip.info",
                vec![
                    Component::text(kicked.len().to_string()),
                    Component::text(kicked.join(", ")),
                ],
            ),
        );
    }
    CommandOutcome::Success
}

fn pardon<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let Some(raw) = args.first() else {
        return usage(ctx, sender, "/pardon <targets>");
    };
    let targets = match profile_targets(ctx, sender, raw) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let mut pardoned = 0;
    for target in targets {
        if ctx.pardon_profile(&target) {
            pardoned += 1;
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.pardon.success",
                    vec![Component::text(target.name)],
                ),
            );
        }
    }
    if pardoned == 0 {
        return fail(ctx, sender, "commands.pardon.failed", Vec::new());
    }
    CommandOutcome::Success
}

fn pardon_ip<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    let Some(raw) = args.first() else {
        return usage(ctx, sender, "/pardon-ip <address>");
    };
    let Ok(address) = raw.parse::<IpAddr>() else {
        return fail(ctx, sender, "commands.pardonip.invalid", Vec::new());
    };
    if !ctx.pardon_address(address) {
        return fail(ctx, sender, "commands.pardonip.failed", Vec::new());
    }
    ctx.reply(
        sender,
        Component::translatable(
            "commands.pardonip.success",
            vec![Component::text(address.to_string())],
        ),
    );
    CommandOutcome::Success
}

fn banlist_entry<T>(name: String, ban: &Ban<T>) -> Component {
    Component::translatable(
        "commands.banlist.entry",
        vec![
            Component::text(name),
            Component::text(ban.source.clone()),
            ban.reason_message(),
        ],
    )
}

fn banlist<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let (players, ips) = match args.first().copied() {
        None => (true, true),
        Some("players") => (true, false),
        Some("ips") => (false, true),
        Some(_) => return usage(ctx, sender, "/banlist [ips|players]"),
    };
    let mut entries = Vec::new();
    if players {
        entries.extend(
            ctx.ban_entries()
                .iter()
                .map(|ban| banlist_entry(ban.target.name.clone(), ban)),
        );
    }
    if ips {
        entries.extend(
            ctx.ip_ban_entries()
                .iter()
                .map(|ban| banlist_entry(ban.target.to_string(), ban)),
        );
    }
    if entries.is_empty() {
        ctx.reply(
            sender,
            Component::translatable("commands.banlist.none", Vec::new()),
        );
        return CommandOutcome::Success;
    }
    ctx.reply(
        sender,
        Component::translatable(
            "commands.banlist.list",
            vec![Component::text(entries.len().to_string())],
        ),
    );
    for entry in entries {
        ctx.reply(sender, entry);
    }
    CommandOutcome::Success
}

fn difficulty<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    let Some(raw) = args.first() else {
        let current = ctx.difficulty();
        ctx.reply(
            sender,
            Component::translatable(
                "commands.difficulty.query",
                vec![Component::translatable(difficulty_key(current), Vec::new())],
            ),
        );
        return CommandOutcome::Success;
    };
    let Some(level) = Difficulty::from_name(raw) else {
        return usage(ctx, sender, "/difficulty <peaceful|easy|normal|hard>");
    };
    ctx.set_difficulty(level);
    ctx.reply(
        sender,
        Component::translatable(
            "commands.difficulty.success",
            vec![Component::translatable(difficulty_key(level), Vec::new())],
        ),
    );
    CommandOutcome::Success
}

fn difficulty_key(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Peaceful => "options.difficulty.peaceful",
        Difficulty::Easy => "options.difficulty.easy",
        Difficulty::Normal => "options.difficulty.normal",
        Difficulty::Hard => "options.difficulty.hard",
    }
}

fn weather<C: CommandContext>(ctx: &mut C, sender: CommandSender, args: &[&str]) -> CommandOutcome {
    let (weather, key) = match args.first().copied() {
        Some("clear") => (Weather::Clear, "commands.weather.set.clear"),
        Some("rain") => (Weather::Rain, "commands.weather.set.rain"),
        Some("thunder") => (Weather::Thunder, "commands.weather.set.thunder"),
        _ => return usage(ctx, sender, "/weather <clear|rain|thunder> [duration]"),
    };
    let duration = match args.get(1) {
        None => None,
        Some(raw) => match parse_time(raw).filter(|ticks| *ticks >= 1) {
            Some(ticks) => Some(i32::try_from(ticks).unwrap_or(i32::MAX)),
            None => return usage(ctx, sender, "/weather <clear|rain|thunder> [duration]"),
        },
    };
    ctx.set_weather(weather, duration);
    ctx.reply(sender, Component::translatable(key, Vec::new()));
    CommandOutcome::Success
}

fn set_world_spawn<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    let base = match sender {
        CommandSender::Player(id) => ctx.player_position(id).unwrap_or(Vec3::ZERO),
        CommandSender::Console => Vec3::ZERO,
    };
    let position = match args {
        [] => Some(base),
        [x, y, z] => parse_vec3(ctx, sender, [x, y, z]).ok(),
        _ => None,
    };
    let Some(position) = position else {
        return usage(ctx, sender, "/setworldspawn [x y z]");
    };
    let pos = position.block();
    ctx.set_world_spawn(pos);
    ctx.reply(
        sender,
        Component::translatable(
            "commands.setworldspawn.success",
            vec![
                Component::text(pos.x.to_string()),
                Component::text(pos.y.to_string()),
                Component::text(pos.z.to_string()),
                Component::text("0.0"),
            ],
        ),
    );
    CommandOutcome::Success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeContext {
        replies: Vec<(CommandSender, String)>,
        broadcasts: Vec<String>,
        operator: bool,
        level: Option<u8>,
        modes: Vec<(PlayerId, GameMode)>,
        teleports: Vec<(PlayerId, Vec3)>,
        day_time: i64,
        stopped: bool,
        flushed: Vec<bool>,
        effects: Vec<(PlayerId, EffectInstance)>,
        cleared: Vec<(PlayerId, Option<EffectId>)>,
        refuse_effects: bool,
        killed: Vec<PlayerId>,
        plugins: Vec<String>,
        board: Scoreboard,
        ticking: rspigot_domain::tick_rate::TickRate,
        border: rspigot_domain::world_border::WorldBorder,
        functions: crate::functions::FunctionState,
    }

    fn alice() -> PlayerId {
        PlayerId::offline("Alice")
    }

    impl CommandContext for FakeContext {
        fn function_state(&self) -> &crate::functions::FunctionState {
            &self.functions
        }
        fn function_state_mut(&mut self) -> &mut crate::functions::FunctionState {
            &mut self.functions
        }
        fn world_border(&self) -> rspigot_domain::world_border::WorldBorder {
            self.border.clone()
        }
        fn world_game_time(&self) -> i64 {
            0
        }
        fn change_world_border<R>(
            &mut self,
            change: impl FnOnce(&mut rspigot_domain::world_border::WorldBorder, i64) -> R,
        ) -> R {
            change(&mut self.border, 0)
        }
        fn tick_state(&self) -> rspigot_domain::tick_rate::TickRate {
            self.ticking.clone()
        }
        fn change_tick_rate<R>(
            &mut self,
            change: impl FnOnce(&mut rspigot_domain::tick_rate::TickRate) -> R,
        ) -> R {
            change(&mut self.ticking)
        }
        fn scoreboard(&self) -> &Scoreboard {
            &self.board
        }
        fn change_scoreboard<R>(
            &mut self,
            change: impl FnOnce(&mut Scoreboard) -> (R, Vec<ScoreboardMessage>),
        ) -> R {
            change(&mut self.board).0
        }
        fn reply(&mut self, sender: CommandSender, message: Component) {
            self.replies.push((sender, message.plain_text()));
        }
        fn broadcast(&mut self, message: Component) {
            self.broadcasts.push(message.plain_text());
        }
        fn permission_level(&self, _: CommandSender) -> u8 {
            self.level.unwrap_or(if self.operator { 4 } else { 0 })
        }
        fn sender_name(&self, sender: CommandSender) -> String {
            match sender {
                CommandSender::Console => "Server".to_owned(),
                CommandSender::Player(_) => "Alice".to_owned(),
            }
        }
        fn find_player(&self, name: &str) -> Option<PlayerId> {
            name.eq_ignore_ascii_case("alice").then(alice)
        }
        fn player_name(&self, _: PlayerId) -> Option<String> {
            Some("Alice".to_owned())
        }
        fn player_names(&self) -> Vec<String> {
            vec!["Alice".to_owned()]
        }
        fn plugin_names(&self) -> Vec<String> {
            self.plugins.clone()
        }
        fn max_players(&self) -> u32 {
            20
        }
        fn player_position(&self, _: PlayerId) -> Option<Vec3> {
            Some(Vec3::new(10.0, 64.0, 10.0))
        }
        fn set_game_mode(
            &mut self,
            player: PlayerId,
            mode: GameMode,
            replies: Vec<(CommandSender, Component)>,
        ) {
            self.modes.push((player, mode));
            for (sender, reply) in replies {
                self.reply(sender, reply);
            }
        }
        fn teleport(&mut self, player: PlayerId, position: Vec3) {
            self.teleports.push((player, position));
        }
        fn day_time(&self) -> i64 {
            self.day_time
        }
        fn game_time(&self) -> i64 {
            0
        }
        fn set_day_time(&mut self, time: i64) {
            self.day_time = time;
        }
        fn skip_day_time(&mut self, time: i64) {
            self.day_time = time;
        }
        fn give(&mut self, _: PlayerId, _: ItemId, _: u32) -> bool {
            true
        }
        fn clear_inventory(&mut self, _: PlayerId) -> u32 {
            0
        }
        fn kick(&mut self, _: PlayerId, _: Component) {}
        fn set_operator(&mut self, _: &str, _: bool) -> bool {
            true
        }
        fn set_whitelist_enabled(&mut self, _: bool) -> bool {
            true
        }
        fn whitelist_add(&mut self, _: &str) -> bool {
            true
        }
        fn whitelist_remove(&mut self, _: &str) -> bool {
            true
        }
        fn whitelist_names(&self) -> Vec<String> {
            Vec::new()
        }
        fn ban_profile(
            &mut self,
            _: ListEntry,
            _: &str,
            _: Option<String>,
            _: Option<SystemTime>,
        ) -> bool {
            true
        }
        fn pardon_profile(&mut self, _: &ListEntry) -> bool {
            true
        }
        fn ban_address(
            &mut self,
            _: IpAddr,
            _: &str,
            _: Option<String>,
            _: Option<SystemTime>,
        ) -> Option<Vec<String>> {
            Some(Vec::new())
        }
        fn pardon_address(&mut self, _: IpAddr) -> bool {
            true
        }
        fn player_address(&self, _: PlayerId) -> Option<IpAddr> {
            None
        }
        fn ban_entries(&self) -> Vec<BanEntry> {
            Vec::new()
        }
        fn ip_ban_entries(&self) -> Vec<IpBanEntry> {
            Vec::new()
        }
        fn select_players(
            &self,
            sender: CommandSender,
            selector: &str,
        ) -> Result<Vec<PlayerId>, Component> {
            match (selector, sender) {
                ("@s", CommandSender::Player(id)) => Ok(vec![id]),
                ("@s", CommandSender::Console) => Ok(Vec::new()),
                ("@a" | "@e", _) => Ok(vec![alice(), PlayerId::offline("Bob")]),
                (name, _) => Ok(self.find_player(name).into_iter().collect()),
            }
        }
        fn give_effect(&mut self, player: PlayerId, effect: EffectInstance) -> bool {
            self.effects.push((player, effect));
            !self.refuse_effects
        }
        fn clear_effect(&mut self, player: PlayerId, effect: Option<EffectId>) -> bool {
            self.cleared.push((player, effect));
            true
        }
        fn kill(&mut self, player: PlayerId) -> bool {
            self.killed.push(player);
            true
        }
        fn seed(&self) -> i64 {
            42
        }
        fn difficulty(&self) -> Difficulty {
            Difficulty::Easy
        }
        fn set_difficulty(&mut self, _: Difficulty) {}
        fn set_weather(&mut self, _: Weather, _: Option<i32>) {}
        fn set_world_spawn(&mut self, _: BlockPos) {}
        fn save_all(&mut self, flush: bool) -> bool {
            self.flushed.push(flush);
            true
        }
        fn tps(&self) -> [f64; 3] {
            [20.0; 3]
        }
        fn stop(&mut self) {
            self.stopped = true;
        }
    }

    #[test]
    fn non_operator_cannot_change_game_mode() {
        let mut ctx = FakeContext::default();
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "/gamemode creative"
            ),
            CommandOutcome::Denied
        );
        assert!(ctx.modes.is_empty());
    }

    #[test]
    fn operator_changes_own_game_mode() {
        let mut ctx = FakeContext {
            operator: true,
            ..FakeContext::default()
        };
        assert_eq!(
            execute(&mut ctx, CommandSender::Player(alice()), "gamemode c"),
            CommandOutcome::Success
        );
        assert_eq!(ctx.modes, vec![(alice(), GameMode::Creative)]);
    }

    #[test]
    fn relative_teleport_uses_sender_position() {
        let mut ctx = FakeContext {
            operator: true,
            ..FakeContext::default()
        };
        execute(&mut ctx, CommandSender::Player(alice()), "/tp ~ ~5 ~-2");
        assert_eq!(ctx.teleports, vec![(alice(), Vec3::new(10.0, 69.0, 8.0))]);
        execute(&mut ctx, CommandSender::Player(alice()), "/tp 1 2 3");
        assert_eq!(ctx.teleports[1].1, Vec3::new(1.5, 2.0, 3.5));
    }

    #[test]
    fn time_set_and_add() {
        let mut ctx = FakeContext {
            operator: true,
            ..FakeContext::default()
        };
        execute(&mut ctx, CommandSender::Console, "time set night");
        assert_eq!(ctx.day_time, 13000);
        execute(&mut ctx, CommandSender::Console, "time add 1d");
        assert_eq!(ctx.day_time, 37000);
    }

    #[test]
    fn plugins_lists_the_connected_names_in_the_order_given() {
        let mut ctx = FakeContext::default();
        execute(&mut ctx, CommandSender::Console, "plugins");
        ctx.plugins = vec!["guard".to_owned(), "welcome".to_owned()];
        execute(&mut ctx, CommandSender::Console, "pl");
        let lines: Vec<&str> = ctx.replies.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(lines, ["Plugins (0): ", "Plugins (2): guard, welcome"]);
    }

    #[test]
    fn unknown_command_replies_with_spigot_message() {
        let mut ctx = FakeContext::default();
        assert_eq!(
            execute(&mut ctx, CommandSender::Console, "/frobnicate"),
            CommandOutcome::Unknown
        );
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("Unknown command. Type \"/help\" for help.")
        );
    }

    fn operator() -> FakeContext {
        FakeContext {
            operator: true,
            ..FakeContext::default()
        }
    }

    #[test]
    fn effect_give_matches_vanilla_example() {
        let mut ctx = operator();
        let outcome = execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "/effect give @s speed 99999 250 false",
        );
        assert_eq!(outcome, CommandOutcome::Success);
        let (target, effect) = ctx.effects[0];
        assert_eq!(target, alice());
        assert_eq!(
            effect.effect,
            EffectId::from_name("minecraft:speed").unwrap()
        );
        assert_eq!(effect.amplifier, 250);
        assert_eq!(effect.duration, EffectDuration::Ticks(1_999_980));
        assert!(effect.visible && effect.show_icon && !effect.ambient);
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("commands.effect.give.success.single effect.minecraft.speed Alice")
        );
    }

    #[test]
    fn effect_give_defaults_and_infinite() {
        let mut ctx = operator();
        execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "effect give Alice minecraft:night_vision",
        );
        assert_eq!(ctx.effects[0].1.duration, EffectDuration::Ticks(600));
        assert_eq!(ctx.effects[0].1.amplifier, 0);
        execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "effect give @s jump_boost infinite 2 true",
        );
        assert_eq!(ctx.effects[1].1.duration, EffectDuration::Infinite);
        assert!(!ctx.effects[1].1.visible);
        execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "effect give @s instant_health 5",
        );
        assert_eq!(ctx.effects[2].1.duration, EffectDuration::Ticks(5));
    }

    #[test]
    fn effect_give_rejects_out_of_range_values() {
        let mut ctx = operator();
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed 1000001"
            ),
            CommandOutcome::Failure
        );
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed 10 256"
            ),
            CommandOutcome::Failure
        );
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed 0"
            ),
            CommandOutcome::Failure
        );
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed 10 1 maybe"
            ),
            CommandOutcome::Failure
        );
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s not_real"
            ),
            CommandOutcome::Failure
        );
        assert!(ctx.effects.is_empty());
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("argument.resource.not_found not_real minecraft:mob_effect")
        );
    }

    #[test]
    fn effect_give_reports_failure_when_nothing_changed() {
        let mut ctx = FakeContext {
            operator: true,
            refuse_effects: true,
            ..FakeContext::default()
        };
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed"
            ),
            CommandOutcome::Failure
        );
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("commands.effect.give.failed")
        );
    }

    #[test]
    fn effect_give_multiple_targets_and_console_self_fails() {
        let mut ctx = operator();
        execute(
            &mut ctx,
            CommandSender::Console,
            "effect give @a glowing 30",
        );
        assert_eq!(ctx.effects.len(), 2);
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("commands.effect.give.success.multiple effect.minecraft.glowing 2")
        );
        assert_eq!(
            execute(&mut ctx, CommandSender::Console, "effect give @s speed"),
            CommandOutcome::Failure
        );
    }

    #[test]
    fn effect_clear_variants() {
        let mut ctx = operator();
        execute(&mut ctx, CommandSender::Player(alice()), "effect clear");
        execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "effect clear @s speed",
        );
        assert_eq!(
            ctx.cleared,
            vec![(alice(), None), (alice(), EffectId::from_name("speed"))]
        );
    }

    #[test]
    fn effect_requires_operator() {
        let mut ctx = FakeContext::default();
        assert_eq!(
            execute(
                &mut ctx,
                CommandSender::Player(alice()),
                "effect give @s speed"
            ),
            CommandOutcome::Denied
        );
    }

    #[test]
    fn kill_defaults_to_the_sender_and_reports_vanilla_messages() {
        let mut ctx = operator();
        assert_eq!(
            execute(&mut ctx, CommandSender::Player(alice()), "/kill"),
            CommandOutcome::Success
        );
        assert_eq!(ctx.killed, vec![alice()]);
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("commands.kill.success.single Alice")
        );
        execute(&mut ctx, CommandSender::Console, "kill @a");
        assert_eq!(ctx.killed.len(), 3);
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("commands.kill.success.multiple 2")
        );
        assert_eq!(
            execute(&mut ctx, CommandSender::Console, "kill"),
            CommandOutcome::Failure
        );
        assert_eq!(
            ctx.replies.last().map(|r| r.1.as_str()),
            Some("permissions.requires.entity")
        );
        assert_eq!(
            execute(&mut ctx, CommandSender::Console, "kill @s"),
            CommandOutcome::Failure
        );
        assert_eq!(ctx.killed.len(), 3);
    }

    #[test]
    fn kill_requires_operator() {
        let mut ctx = FakeContext::default();
        assert_eq!(
            execute(&mut ctx, CommandSender::Player(alice()), "/kill"),
            CommandOutcome::Denied
        );
        assert!(ctx.killed.is_empty());
    }

    #[test]
    fn a_gamemaster_runs_gameplay_commands_but_not_admin_ones() {
        let mut ctx = FakeContext {
            level: Some(2),
            ..FakeContext::default()
        };
        let player = CommandSender::Player(alice());
        assert_eq!(
            execute(&mut ctx, player, "/say hi"),
            CommandOutcome::Success
        );
        assert_eq!(
            execute(&mut ctx, player, "/op Alice"),
            CommandOutcome::Denied
        );
        assert_eq!(
            execute(&mut ctx, player, "/ban Alice"),
            CommandOutcome::Denied
        );
        assert_eq!(execute(&mut ctx, player, "/stop"), CommandOutcome::Denied);
        assert!(!ctx.stopped);
    }

    #[test]
    fn an_admin_cannot_stop_the_server_and_the_console_can() {
        let mut ctx = FakeContext {
            level: Some(3),
            ..FakeContext::default()
        };
        let player = CommandSender::Player(alice());
        assert_eq!(execute(&mut ctx, player, "/stop"), CommandOutcome::Denied);
        assert_eq!(
            execute(&mut ctx, player, "/save-all"),
            CommandOutcome::Denied
        );
        assert_ne!(
            execute(&mut ctx, player, "/kick Alice"),
            CommandOutcome::Denied
        );
        ctx.level = Some(4);
        execute(&mut ctx, CommandSender::Console, "stop");
        assert!(ctx.stopped);
    }

    #[test]
    fn a_refusal_looks_like_vanilla() {
        let mut ctx = FakeContext::default();
        execute(
            &mut ctx,
            CommandSender::Player(alice()),
            "/gamemode creative",
        );
        let texts: Vec<&str> = ctx.replies.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(
            texts,
            [
                "command.unknown.command",
                "gamemode creativecommand.context.here"
            ]
        );
    }

    #[test]
    fn stop_stops() {
        let mut ctx = FakeContext {
            operator: true,
            ..FakeContext::default()
        };
        execute(&mut ctx, CommandSender::Console, "stop");
        assert!(ctx.stopped);
    }
}
