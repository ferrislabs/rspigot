use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_nbt::{Compound, Tag};

use crate::console::console_text;
use crate::events::ClientEvent;
use crate::plugins::events::{PlayerSignOpen, PlayerSignOpenCause, SignChange, SignChangePatch};
use crate::plugins::steps::Continuation;
use crate::plugins::{EventPayload, SignData};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const LINES: usize = 4;
const FRONT_TEXT: &str = "front_text";
const BACK_TEXT: &str = "back_text";
const MESSAGES: &str = "messages";
const WAXED: &str = "is_waxed";

#[derive(Debug, Clone, Copy)]
pub(crate) struct SignOpenStep {
    pub(crate) player: PlayerId,
    pub(crate) pos: BlockPos,
    pub(crate) front: bool,
    pub(crate) cause: PlayerSignOpenCause,
}

#[derive(Debug)]
pub(crate) struct SignEditStep {
    pub(crate) pos: BlockPos,
    pub(crate) front: bool,
    pub(crate) lines: Vec<String>,
}

pub(crate) fn is_sign(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| kind.name().ends_with("_sign"))
}

fn entity_kind(state: BlockStateId) -> &'static str {
    match state.kind() {
        Some(kind) if kind.name().contains("hanging_sign") => "hanging_sign",
        _ => "sign",
    }
}

fn yaw_of(state: BlockStateId) -> f32 {
    if let Some(rotation) = state
        .property("rotation")
        .and_then(|value| value.parse::<f32>().ok())
    {
        return rotation * 22.5;
    }
    match state.property("facing") {
        Some("west") => 90.0,
        Some("north") => 180.0,
        Some("east") => 270.0,
        _ => 0.0,
    }
}

fn wrap_degrees(value: f32) -> f32 {
    let wrapped = value.rem_euclid(360.0);
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

fn filtered(line: &str) -> String {
    line.chars()
        .filter(|c| *c >= ' ' && *c != '\u{7f}')
        .collect()
}

fn plain_lines(text: Option<&Compound>) -> Vec<String> {
    let mut lines: Vec<String> = text
        .and_then(|text| text.get_list(MESSAGES))
        .map(|messages| {
            messages
                .iter()
                .map(|message| {
                    component_from_tag(message).map_or_else(String::new, |c| console_text(&c))
                })
                .collect()
        })
        .unwrap_or_default();
    lines.resize(LINES, String::new());
    lines.truncate(LINES);
    lines
}

fn line_tag(old: Option<&Tag>, line: &str) -> Tag {
    match old {
        Some(Tag::Compound(style)) => {
            let mut next = style.clone();
            next.remove("extra");
            next.insert("text", Tag::String(line.to_owned()));
            Tag::Compound(next)
        }
        _ => Tag::String(line.to_owned()),
    }
}

impl<L: ClientLink> Server<L> {
    fn sign_data(&self, pos: BlockPos) -> Option<&Compound> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| &entity.data)
    }

    fn sign_waxed(&self, pos: BlockPos) -> bool {
        self.sign_data(pos)
            .and_then(|data| data.get_bool(WAXED))
            .unwrap_or(false)
    }

    fn sign_view(&self, pos: BlockPos) -> SignData {
        let data = self.sign_data(pos);
        SignData {
            pos,
            front: plain_lines(data.and_then(|data| data.get_compound(FRONT_TEXT))),
            back: plain_lines(data.and_then(|data| data.get_compound(BACK_TEXT))),
            waxed: self.sign_waxed(pos),
        }
    }

    fn facing_front_text(&self, id: PlayerId, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(player) = self.players.get(&id) else {
            return true;
        };
        let dx = player.position.x - (f64::from(pos.x) + 0.5);
        let dz = player.position.z - (f64::from(pos.z) + 0.5);
        let angle = (dz.atan2(dx).to_degrees() as f32) - 90.0;
        wrap_degrees(angle - yaw_of(state)).abs() <= 90.0
    }

    fn another_edits_sign(&self, id: PlayerId, pos: BlockPos) -> bool {
        self.sign_editors
            .iter()
            .any(|(editor, at)| *editor != id && *at == pos)
    }

    pub(crate) fn use_sign(&mut self, id: PlayerId, pos: BlockPos, state: BlockStateId) -> bool {
        if !is_sign(state) {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let may_build = !matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator);
        if self.sign_waxed(pos) {
            return true;
        }
        if self.another_edits_sign(id, pos) || !may_build {
            return false;
        }
        let front = self.facing_front_text(id, pos, state);
        self.open_sign_editor(SignOpenStep {
            player: id,
            pos,
            front,
            cause: PlayerSignOpenCause::Interact,
        });
        true
    }

    pub(crate) fn open_placed_sign(&mut self, id: PlayerId, pos: BlockPos, state: BlockStateId) {
        if is_sign(state) {
            self.open_sign_editor(SignOpenStep {
                player: id,
                pos,
                front: true,
                cause: PlayerSignOpenCause::Place,
            });
        }
    }

    fn open_sign_editor(&mut self, step: SignOpenStep) {
        let id = step.player;
        self.fire(id, Continuation::SignOpen(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerSignOpen(PlayerSignOpen {
                    cause: step.cause,
                    player,
                    side: side_of(step.front),
                    sign: server.sign_view(step.pos),
                })
            })
        });
    }

    pub(crate) fn finish_sign_open(&mut self, step: SignOpenStep, cancelled: bool) {
        if cancelled || self.sign_waxed(step.pos) {
            return;
        }
        self.sign_editors.insert(step.player, step.pos);
        self.push(
            Audience::One(step.player),
            ClientEvent::OpenSignEditor {
                pos: step.pos,
                front: step.front,
            },
        );
    }

    pub(crate) fn handle_sign_update(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        front: bool,
        lines: Vec<String>,
    ) {
        if self.sign_editors.get(&id) != Some(&pos) {
            return;
        }
        self.sign_editors.remove(&id);
        let is_a_sign = self.world.chunks.block(pos).is_some_and(is_sign);
        if !is_a_sign || self.sign_waxed(pos) {
            return;
        }
        let lines: Vec<String> = lines.iter().map(|line| filtered(line)).collect();
        let step = SignEditStep {
            pos,
            front,
            lines: lines.clone(),
        };
        self.fire(id, Continuation::SignEdit(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::SignChange(SignChange {
                    block: pos,
                    lines,
                    player,
                    side: side_of(front),
                })
            })
        });
    }

    pub(crate) fn finish_sign_edit(
        &mut self,
        step: SignEditStep,
        cancelled: bool,
        patch: SignChangePatch,
    ) {
        if cancelled || self.sign_waxed(step.pos) {
            return;
        }
        let Some(state) = self.world.chunks.block(step.pos).filter(|s| is_sign(*s)) else {
            return;
        };
        let lines: Vec<String> = step
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                patch
                    .lines
                    .replacement(index)
                    .map_or_else(|| line.clone(), |patched| filtered(patched))
            })
            .collect();
        let key = if step.front { FRONT_TEXT } else { BACK_TEXT };
        let Some(chunk) = self.world.chunks.chunk_mut(step.pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(step.pos)
            .map_or_else(Compound::new, |entity| entity.data.clone());
        let mut text = data
            .get_compound(key)
            .cloned()
            .unwrap_or_else(Compound::new);
        let old: Vec<Tag> = text
            .get_list(MESSAGES)
            .map(<[Tag]>::to_vec)
            .unwrap_or_default();
        let messages: Vec<Tag> = lines
            .iter()
            .enumerate()
            .map(|(index, line)| line_tag(old.get(index), line))
            .collect();
        text.insert(MESSAGES, Tag::List(messages));
        text.remove("filtered_messages");
        data.insert(key, Tag::Compound(text));
        chunk.set_block_entity(BlockEntity {
            pos: step.pos,
            kind: Identifier::minecraft(entity_kind(state)),
            data: data.clone(),
        });
        self.push(
            Audience::ChunkViewers(step.pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos: step.pos,
                kind: entity_kind(state),
                data,
            },
        );
    }

    pub(crate) fn forget_sign_editor(&mut self, id: PlayerId) {
        self.sign_editors.remove(&id);
    }
}

fn side_of(front: bool) -> crate::plugins::events::Side {
    if front {
        crate::plugins::events::Side::Front
    } else {
        crate::plugins::events::Side::Back
    }
}
