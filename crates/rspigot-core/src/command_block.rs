use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::{component_from_tag, component_to_tag};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::{Color, Component, NamedColor};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;
use rustc_hash::FxHashMap;

use crate::commands::source::{Anchor, Source};
use crate::commands::{CommandOutcome, CommandSender, Target, execute};
use crate::console::console_text;
use crate::events::{ClientEvent, CommandBlockMode};
use crate::ports::ClientLink;
use crate::redstone::{facing, step};
use crate::server::{Audience, Server};
use crate::signal::flag;

const ENTITY: &str = "command_block";
const COMMAND: &str = "Command";
const CUSTOM_NAME: &str = "CustomName";
const LAST_OUTPUT: &str = "LastOutput";
const TRACK_OUTPUT: &str = "TrackOutput";
const SUCCESS_COUNT: &str = "SuccessCount";
const CONDITION_MET: &str = "conditionMet";
const POWERED: &str = "powered";
const AUTO: &str = "auto";
const UPDATE_LAST_EXECUTION: &str = "UpdateLastExecution";
const LAST_EXECUTION: &str = "LastExecution";
const NO_LAST_EXECUTION: i64 = -1;
const DEFAULT_NAME: &str = "@";
const MINECART_NAME: &str = "Minecart with Command Block";
const SECRET_COMMAND: &str = "Searge";
const SECRET_OUTPUT: &str = "#itzlipofutzli";
const GAMEMASTER: u8 = 2;
const TICK_DELAY: u64 = 1;
const MINECART_PERIOD: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Sequence,
    Auto,
    Redstone,
}

impl Mode {
    fn of(kind: BlockKind) -> Self {
        match kind {
            BlockKind::ChainCommandBlock => Self::Sequence,
            BlockKind::RepeatingCommandBlock => Self::Auto,
            _ => Self::Redstone,
        }
    }

    fn from_input(mode: CommandBlockMode) -> Self {
        match mode {
            CommandBlockMode::Sequence => Self::Sequence,
            CommandBlockMode::Auto => Self::Auto,
            CommandBlockMode::Redstone => Self::Redstone,
        }
    }

    const fn block(self) -> BlockKind {
        match self {
            Self::Sequence => BlockKind::ChainCommandBlock,
            Self::Auto => BlockKind::RepeatingCommandBlock,
            Self::Redstone => BlockKind::CommandBlock,
        }
    }
}

pub(crate) fn is_command_block(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(
            BlockKind::CommandBlock
                | BlockKind::ChainCommandBlock
                | BlockKind::RepeatingCommandBlock
        )
    )
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fields {
    pub(crate) command: String,
    pub(crate) last_output: Option<Component>,
    pub(crate) track_output: bool,
    pub(crate) success_count: i32,
    pub(crate) update_last_execution: bool,
    pub(crate) last_execution: i64,
}

impl Fields {
    pub(crate) fn new(track_output: bool) -> Self {
        Self {
            command: String::new(),
            last_output: None,
            track_output,
            success_count: 0,
            update_last_execution: true,
            last_execution: NO_LAST_EXECUTION,
        }
    }

    pub(crate) fn read(data: &Compound) -> Self {
        Self {
            command: data.get_str(COMMAND).unwrap_or_default().to_owned(),
            last_output: data.get(LAST_OUTPUT).and_then(component_from_tag),
            track_output: data.get_bool(TRACK_OUTPUT).unwrap_or(true),
            success_count: data.get_i32(SUCCESS_COUNT).unwrap_or(0),
            update_last_execution: data.get_bool(UPDATE_LAST_EXECUTION).unwrap_or(true),
            last_execution: data.get_i64(LAST_EXECUTION).unwrap_or(NO_LAST_EXECUTION),
        }
    }

    pub(crate) fn write(&self, data: &mut Compound) {
        data.insert(COMMAND, self.command.as_str());
        data.insert(SUCCESS_COUNT, self.success_count);
        data.insert(TRACK_OUTPUT, self.track_output);
        match (&self.last_output, self.track_output) {
            (Some(output), true) => {
                data.insert(LAST_OUTPUT, component_to_tag(output));
            }
            _ => {
                data.remove(LAST_OUTPUT);
            }
        }
        data.insert(UPDATE_LAST_EXECUTION, self.update_last_execution);
        data.insert(LAST_EXECUTION, self.last_execution);
    }

    fn shows_differently(&self, other: &Self) -> bool {
        self.command != other.command || self.last_output != other.last_output
    }
}

#[derive(Debug, Clone)]
struct Origin {
    name: String,
    position: Vec3,
    rotation: Rotation,
    executor: Option<Target>,
}

#[derive(Debug)]
struct Capture {
    name: String,
    replies: Vec<Component>,
}

#[derive(Debug, Default, Clone, Copy)]
struct CartClock {
    age: i32,
    last: i32,
}

#[derive(Debug, Default)]
pub(crate) struct Runtime {
    capture: Option<Capture>,
    carts: FxHashMap<EntityId, CartClock>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BlockEdit {
    pub(crate) pos: BlockPos,
    pub(crate) command: String,
    pub(crate) mode: CommandBlockMode,
    pub(crate) track_output: bool,
    pub(crate) conditional: bool,
    pub(crate) automatic: bool,
}

fn name_text(name: Option<&Component>, fallback: &str) -> String {
    name.map_or_else(|| fallback.to_owned(), console_text)
}

fn stamped(message: &Component) -> Component {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0));
    let clock = chrono::DateTime::from_timestamp(seconds, 0)
        .map(|time| time.format("%H:%M:%S").to_string())
        .unwrap_or_default();
    Component::text(format!("[{clock}] ")).append(message.clone())
}

pub(crate) fn is_failure(message: &Component) -> bool {
    message.style.color == Some(Color::Named(NamedColor::Red))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn command_block_data(&self, pos: BlockPos) -> Option<&Compound> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .filter(|entity| entity.kind.as_str() == "minecraft:command_block")
            .map(|entity| &entity.data)
    }

    fn store_command_block(&mut self, pos: BlockPos, data: Compound) {
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(ENTITY),
                data,
            });
        }
    }

    fn send_command_block(&mut self, pos: BlockPos) {
        if let Some(data) = self.command_block_data(pos).cloned() {
            self.push(
                Audience::ChunkViewers(pos.chunk()),
                ClientEvent::BlockEntityUpdate {
                    pos,
                    kind: ENTITY,
                    data,
                },
            );
        }
    }

    pub(crate) fn ensure_command_block_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind().filter(|_| is_command_block(state)) else {
            return;
        };
        if self.command_block_data(pos).is_some() {
            return;
        }
        let mut data = Compound::new();
        Fields::new(self.config.rules.send_command_feedback).write(&mut data);
        data.insert(POWERED, false);
        data.insert(CONDITION_MET, false);
        data.insert(AUTO, kind == BlockKind::ChainCommandBlock);
        self.store_command_block(pos, data);
    }

    pub(crate) fn command_block_signal(&self, pos: BlockPos) -> i32 {
        self.command_block_data(pos)
            .and_then(|data| data.get_i32(SUCCESS_COUNT))
            .unwrap_or(0)
    }

    fn set_command_flag(&mut self, pos: BlockPos, key: &str, value: bool) {
        if let Some(mut data) = self.command_block_data(pos).cloned() {
            data.insert(key, value);
            self.store_command_block(pos, data);
        }
    }

    fn command_flag(&self, pos: BlockPos, key: &str) -> bool {
        self.command_block_data(pos)
            .and_then(|data| data.get_bool(key))
            .unwrap_or(false)
    }

    fn set_command_success(&mut self, pos: BlockPos, count: i32) {
        if let Some(mut data) = self.command_block_data(pos).cloned() {
            data.insert(SUCCESS_COUNT, count);
            self.store_command_block(pos, data);
        }
    }

    fn mark_condition_met(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let met = if flag(state, "conditional") {
            facing(state).is_some_and(|face| {
                let behind = step(pos, face.opposite());
                is_command_block(self.block_or_air(behind)) && self.command_block_signal(behind) > 0
            })
        } else {
            true
        };
        self.set_command_flag(pos, CONDITION_MET, met);
        met
    }

    pub(crate) fn command_block_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        self.ensure_command_block_entity(pos, state);
        let powered = self.has_neighbor_signal(pos);
        let was_powered = self.command_flag(pos, POWERED);
        self.set_command_flag(pos, POWERED, powered);
        if !was_powered
            && powered
            && !self.command_flag(pos, AUTO)
            && Mode::of(kind) != Mode::Sequence
        {
            self.mark_condition_met(pos, state);
            self.schedule(pos, kind, TICK_DELAY);
        }
    }

    pub(crate) fn command_block_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        self.ensure_command_block_entity(pos, state);
        let Some(data) = self.command_block_data(pos) else {
            return;
        };
        let has_command = data
            .get_str(COMMAND)
            .is_some_and(|command| !command.is_empty());
        let met = data.get_bool(CONDITION_MET).unwrap_or(false);
        let conditional = flag(state, "conditional");
        match Mode::of(kind) {
            Mode::Auto => {
                self.mark_condition_met(pos, state);
                if met {
                    self.execute_block(pos, state, has_command);
                } else if conditional {
                    self.set_command_success(pos, 0);
                }
                if self.command_flag(pos, POWERED) || self.command_flag(pos, AUTO) {
                    self.schedule(pos, kind, TICK_DELAY);
                }
            }
            Mode::Redstone => {
                if met {
                    self.execute_block(pos, state, has_command);
                } else if conditional {
                    self.set_command_success(pos, 0);
                }
            }
            Mode::Sequence => {}
        }
        self.update_neighbour_for_output_signal(pos);
    }

    fn execute_block(&mut self, pos: BlockPos, state: BlockStateId, can_trigger: bool) {
        if can_trigger {
            self.perform_block_command(pos);
        } else {
            self.set_command_success(pos, 0);
        }
        if let Some(direction) = facing(state) {
            self.execute_chain(pos, direction);
        }
    }

    fn execute_chain(&mut self, origin: BlockPos, first: rspigot_blocks::Face) {
        let mut at = step(origin, first);
        let mut left = self.config.rules.max_command_sequence_length;
        while left > 0 {
            left -= 1;
            let state = self.block_or_air(at);
            if state.kind() != Some(BlockKind::ChainCommandBlock)
                || self.command_block_data(at).is_none()
            {
                return;
            }
            if self.command_flag(at, POWERED) || self.command_flag(at, AUTO) {
                if self.mark_condition_met(at, state) {
                    if !self.perform_block_command(at) {
                        return;
                    }
                    self.update_neighbour_for_output_signal(at);
                } else if flag(state, "conditional") {
                    self.set_command_success(at, 0);
                }
            }
            let Some(direction) = facing(state) else {
                return;
            };
            at = step(at, direction);
        }
        tracing::warn!(
            "Command Block chain tried to execute more than {} steps",
            self.config.rules.max_command_sequence_length
        );
    }

    fn perform_block_command(&mut self, pos: BlockPos) -> bool {
        let Some(data) = self.command_block_data(pos) else {
            return false;
        };
        let before = Fields::read(data);
        let name = name_text(
            data.get(CUSTOM_NAME).and_then(component_from_tag).as_ref(),
            DEFAULT_NAME,
        );
        let origin = Origin {
            name,
            position: pos.center(),
            rotation: Rotation::default(),
            executor: None,
        };
        let mut fields = before.clone();
        let ran = self.perform(&mut fields, &origin);
        if let Some(mut data) = self.command_block_data(pos).cloned() {
            fields.write(&mut data);
            self.store_command_block(pos, data);
            if fields.shows_differently(&before) {
                self.send_command_block(pos);
            }
        }
        ran
    }

    fn perform(&mut self, fields: &mut Fields, origin: &Origin) -> bool {
        let now = self.world.level.game_time;
        if fields.last_execution == now {
            return false;
        }
        if fields.command.eq_ignore_ascii_case(SECRET_COMMAND) {
            fields.last_output = Some(Component::text(SECRET_OUTPUT));
            fields.success_count = 1;
            return true;
        }
        fields.success_count = 0;
        if self.config.rules.command_blocks_work && !fields.command.is_empty() {
            fields.last_output = None;
            let line = fields.command.clone();
            let (success, output) = self.run_command(&line, origin, fields.track_output);
            fields.success_count = i32::from(success);
            if output.is_some() {
                fields.last_output = output;
            }
        }
        fields.last_execution = if fields.update_last_execution {
            now
        } else {
            NO_LAST_EXECUTION
        };
        true
    }

    fn run_command(
        &mut self,
        line: &str,
        origin: &Origin,
        track_output: bool,
    ) -> (bool, Option<Component>) {
        let outer = self.falls.redstone.command.capture.replace(Capture {
            name: origin.name.clone(),
            replies: Vec::new(),
        });
        let source = Source {
            executor: origin.executor,
            position: origin.position,
            rotation: origin.rotation,
            anchor: Anchor::Feet,
            dimension: None,
        };
        let previous = self.command_source.replace(source);
        let cap = self.functions.runtime.permission_cap;
        self.functions.runtime.permission_cap = Some(cap.map_or(GAMEMASTER, |c| c.min(GAMEMASTER)));
        let stale = self.command_result.take();
        let outcome = execute(self, CommandSender::Console, line);
        self.command_result = stale;
        self.functions.runtime.permission_cap = cap;
        self.command_source = previous;
        let replies = std::mem::replace(&mut self.falls.redstone.command.capture, outer)
            .map(|capture| capture.replies)
            .unwrap_or_default();
        let feedback = self.config.rules.send_command_feedback;
        let inform = self.config.rules.command_block_output;
        let mut output = None;
        for reply in replies {
            let failure = is_failure(&reply);
            if !failure && !feedback {
                continue;
            }
            if track_output {
                output = Some(stamped(&reply));
            }
            if !failure && inform {
                self.inform_operators(&origin.name, reply);
            }
        }
        (outcome == CommandOutcome::Success, output)
    }

    fn inform_operators(&mut self, name: &str, message: Component) {
        let line = Component::translatable("chat.type.admin", vec![Component::text(name), message])
            .color(NamedColor::Gray)
            .italic(true);
        tracing::info!("{}", console_text(&line));
        let operators: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.permission_level() >= GAMEMASTER)
            .map(|player| player.id)
            .collect();
        for id in operators {
            self.push(Audience::One(id), ClientEvent::system(line.clone()));
        }
    }

    pub(crate) fn run_captured_console(&mut self, name: &str, line: &str) -> Vec<Component> {
        let outer = self.falls.redstone.command.capture.replace(Capture {
            name: name.to_owned(),
            replies: Vec::new(),
        });
        execute(self, CommandSender::Console, line);
        std::mem::replace(&mut self.falls.redstone.command.capture, outer)
            .map(|capture| capture.replies)
            .unwrap_or_default()
    }

    pub(crate) fn capture_command_reply(&mut self, message: &Component) -> bool {
        match &mut self.falls.redstone.command.capture {
            Some(capture) => {
                capture.replies.push(message.clone());
                true
            }
            None => false,
        }
    }

    pub(crate) fn running_command_name(&self) -> Option<&str> {
        self.falls
            .redstone
            .command
            .capture
            .as_ref()
            .map(|capture| capture.name.as_str())
    }

    pub(crate) fn can_use_game_master_blocks(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|player| {
            player.permission_level() >= GAMEMASTER && player.game_mode == GameMode::Creative
        })
    }

    pub(crate) fn use_command_block(&mut self, id: PlayerId, state: BlockStateId) -> bool {
        is_command_block(state) && self.can_use_game_master_blocks(id)
    }

    fn refuse_command_edit(&mut self, id: PlayerId) -> bool {
        let key = if !self.config.rules.command_blocks_work {
            "advMode.notEnabled"
        } else if !self.can_use_game_master_blocks(id) {
            "advMode.notAllowed"
        } else {
            return false;
        };
        self.push(
            Audience::One(id),
            ClientEvent::system(Component::translatable(key, Vec::new())),
        );
        true
    }

    fn schedule_automatic(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        if Mode::of(kind) == Mode::Auto
            && (self.command_flag(pos, POWERED) || self.command_flag(pos, AUTO))
        {
            self.schedule(pos, kind, TICK_DELAY);
        }
    }

    pub(crate) fn handle_set_command_block(&mut self, id: PlayerId, edit: &BlockEdit) {
        if self.refuse_command_edit(id) {
            return;
        }
        let pos = edit.pos;
        let state = self.block_or_air(pos);
        let Some(old_kind) = state.kind().filter(|_| is_command_block(state)) else {
            return;
        };
        self.ensure_command_block_entity(pos, state);
        let Some(face) = facing(state) else {
            return;
        };
        let mode = Mode::from_input(edit.mode);
        let conditional = if edit.conditional { "true" } else { "false" };
        let next = mode
            .block()
            .state_with(&[("facing", face.name()), ("conditional", conditional)])
            .unwrap_or(state);
        if next != state {
            self.set_block_quiet(pos, next);
        }
        self.ensure_command_block_entity(pos, next);
        let Some(mut data) = self.command_block_data(pos).cloned() else {
            return;
        };
        let mut fields = Fields::read(&data);
        fields.command.clone_from(&edit.command);
        fields.track_output = edit.track_output;
        if !edit.track_output {
            fields.last_output = None;
        }
        fields.write(&mut data);
        let was_automatic = data.get_bool(AUTO).unwrap_or(false);
        data.insert(AUTO, edit.automatic);
        self.store_command_block(pos, data);
        if !was_automatic
            && edit.automatic
            && !self.command_flag(pos, POWERED)
            && mode != Mode::Sequence
            && let Some(kind) = next.kind()
        {
            self.schedule(pos, kind, TICK_DELAY);
        }
        if Mode::of(old_kind) != mode {
            self.schedule_automatic(pos, next);
        }
        self.send_command_block(pos);
        if !edit.command.is_empty() {
            self.push(
                Audience::One(id),
                ClientEvent::system(Component::translatable(
                    "advMode.setCommand.success",
                    vec![Component::text(edit.command.as_str())],
                )),
            );
        }
    }

    pub(crate) fn handle_set_command_minecart(
        &mut self,
        id: PlayerId,
        entity: EntityId,
        command: String,
        track_output: bool,
    ) {
        if self.refuse_command_edit(id) {
            return;
        }
        let Some(cart) = self
            .entities
            .store
            .get_mut(entity)
            .filter(|cart| cart.base.kind == EntityKind::CommandBlockMinecart)
        else {
            return;
        };
        let mut fields = Fields::read(&cart.extra);
        fields.command.clone_from(&command);
        fields.track_output = track_output;
        if !track_output {
            fields.last_output = None;
        }
        fields.write(&mut cart.extra);
        self.push(
            Audience::One(id),
            ClientEvent::system(Component::translatable(
                "advMode.setCommand.success",
                vec![Component::text(command)],
            )),
        );
    }

    pub(crate) fn command_minecart_interact(&self, id: PlayerId, target: EntityId) -> bool {
        self.entities
            .store
            .get(target)
            .is_some_and(|cart| cart.base.kind == EntityKind::CommandBlockMinecart)
            && self.can_use_game_master_blocks(id)
    }

    pub(crate) fn tick_command_minecart(&mut self, id: EntityId) {
        let Some(cart) = self.entities.store.get(id) else {
            return;
        };
        if cart.base.kind != EntityKind::CommandBlockMinecart {
            return;
        }
        let at = cart.base.position.block();
        let powered_rail = {
            let state = self.block_or_air(at);
            state.kind() == Some(BlockKind::ActivatorRail) && flag(state, "powered")
        };
        let clock = self.falls.redstone.command.carts.entry(id).or_default();
        clock.age += 1;
        if !powered_rail || clock.age - clock.last < MINECART_PERIOD {
            return;
        }
        clock.last = clock.age;
        self.perform_minecart_command(id);
    }

    fn perform_minecart_command(&mut self, id: EntityId) {
        let Some(cart) = self.entities.store.get(id) else {
            return;
        };
        let origin = Origin {
            name: name_text(cart.base.custom_name(), MINECART_NAME),
            position: cart.base.position,
            rotation: Rotation::new(cart.base.yaw, cart.base.pitch),
            executor: Some(Target::Entity(id)),
        };
        let mut fields = Fields::read(&cart.extra);
        self.perform(&mut fields, &origin);
        if let Some(cart) = self.entities.store.get_mut(id) {
            fields.write(&mut cart.extra);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_round_trip_through_their_nbt_names() {
        let mut fields = Fields::new(true);
        fields.command = "say hi".to_owned();
        fields.success_count = 3;
        fields.last_execution = 41;
        fields.last_output = Some(Component::text("[00:00:01] hi"));
        let mut data = Compound::new();
        fields.write(&mut data);
        assert_eq!(data.get_str("Command"), Some("say hi"));
        assert_eq!(data.get_i32("SuccessCount"), Some(3));
        assert_eq!(data.get_bool("TrackOutput"), Some(true));
        assert_eq!(data.get_bool("UpdateLastExecution"), Some(true));
        assert_eq!(data.get_i64("LastExecution"), Some(41));
        assert!(data.contains_key("LastOutput"));
        assert_eq!(Fields::read(&data), fields);
    }

    #[test]
    fn an_untracked_block_keeps_no_last_output() {
        let mut fields = Fields::new(false);
        fields.last_output = Some(Component::text("x"));
        let mut data = Compound::new().with("LastOutput", "old");
        fields.write(&mut data);
        assert!(!data.contains_key("LastOutput"));
    }

    #[test]
    fn missing_fields_read_as_vanilla_defaults() {
        let fields = Fields::read(&Compound::new());
        assert_eq!(fields, Fields::new(true));
        assert_eq!(fields.last_execution, -1);
    }

    #[test]
    fn the_mode_follows_the_block_and_back() {
        for (kind, mode) in [
            (BlockKind::CommandBlock, Mode::Redstone),
            (BlockKind::ChainCommandBlock, Mode::Sequence),
            (BlockKind::RepeatingCommandBlock, Mode::Auto),
        ] {
            assert_eq!(Mode::of(kind), mode);
            assert_eq!(mode.block(), kind);
        }
    }

    #[test]
    fn a_red_message_is_a_failure() {
        assert!(is_failure(&Component::text("x").color(NamedColor::Red)));
        assert!(!is_failure(&Component::text("x")));
    }

    #[test]
    fn a_stamped_output_starts_with_a_clock() {
        let text = console_text(&stamped(&Component::text("hi")));
        assert!(text.starts_with('['));
        assert!(text.ends_with("] hi"));
        assert_eq!(text.len(), "[00:00:00] hi".len());
    }
}
