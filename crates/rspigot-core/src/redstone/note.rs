use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;

use super::step;
use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::plugins::events::{Instrument, NotePlay};
use crate::plugins::{Continuation, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::signal::{flag, level};

const NOTE_VOLUME: f32 = 3.0;
const NOTE_RANGE: f64 = 48.0;
const NOTES: i32 = 25;
const MIDDLE: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteStep {
    pub(crate) pos: BlockPos,
    pub(crate) instrument: &'static str,
    pub(crate) note: i32,
}

const INSTRUMENTS: [(&str, Instrument, &str, bool); 27] = [
    (
        "harp",
        Instrument::Piano,
        "minecraft:block.note_block.harp",
        true,
    ),
    (
        "basedrum",
        Instrument::BassDrum,
        "minecraft:block.note_block.basedrum",
        true,
    ),
    (
        "snare",
        Instrument::SnareDrum,
        "minecraft:block.note_block.snare",
        true,
    ),
    (
        "hat",
        Instrument::Sticks,
        "minecraft:block.note_block.hat",
        true,
    ),
    (
        "bass",
        Instrument::BassGuitar,
        "minecraft:block.note_block.bass",
        true,
    ),
    (
        "flute",
        Instrument::Flute,
        "minecraft:block.note_block.flute",
        true,
    ),
    (
        "bell",
        Instrument::Bell,
        "minecraft:block.note_block.bell",
        true,
    ),
    (
        "guitar",
        Instrument::Guitar,
        "minecraft:block.note_block.guitar",
        true,
    ),
    (
        "chime",
        Instrument::Chime,
        "minecraft:block.note_block.chime",
        true,
    ),
    (
        "xylophone",
        Instrument::Xylophone,
        "minecraft:block.note_block.xylophone",
        true,
    ),
    (
        "iron_xylophone",
        Instrument::IronXylophone,
        "minecraft:block.note_block.iron_xylophone",
        true,
    ),
    (
        "cow_bell",
        Instrument::CowBell,
        "minecraft:block.note_block.cow_bell",
        true,
    ),
    (
        "didgeridoo",
        Instrument::Didgeridoo,
        "minecraft:block.note_block.didgeridoo",
        true,
    ),
    (
        "bit",
        Instrument::Bit,
        "minecraft:block.note_block.bit",
        true,
    ),
    (
        "banjo",
        Instrument::Banjo,
        "minecraft:block.note_block.banjo",
        true,
    ),
    (
        "pling",
        Instrument::Pling,
        "minecraft:block.note_block.pling",
        true,
    ),
    (
        "trumpet",
        Instrument::Trumpet,
        "minecraft:block.note_block.trumpet",
        true,
    ),
    (
        "trumpet_exposed",
        Instrument::TrumpetExposed,
        "minecraft:block.note_block.trumpet_exposed",
        true,
    ),
    (
        "trumpet_oxidized",
        Instrument::TrumpetOxidized,
        "minecraft:block.note_block.trumpet_oxidized",
        true,
    ),
    (
        "trumpet_weathered",
        Instrument::TrumpetWeathered,
        "minecraft:block.note_block.trumpet_weathered",
        true,
    ),
    (
        "zombie",
        Instrument::Zombie,
        "minecraft:block.note_block.imitate.zombie",
        false,
    ),
    (
        "skeleton",
        Instrument::Skeleton,
        "minecraft:block.note_block.imitate.skeleton",
        false,
    ),
    (
        "creeper",
        Instrument::Creeper,
        "minecraft:block.note_block.imitate.creeper",
        false,
    ),
    (
        "dragon",
        Instrument::Dragon,
        "minecraft:block.note_block.imitate.ender_dragon",
        false,
    ),
    (
        "wither_skeleton",
        Instrument::WitherSkeleton,
        "minecraft:block.note_block.imitate.wither_skeleton",
        false,
    ),
    (
        "piglin",
        Instrument::Piglin,
        "minecraft:block.note_block.imitate.piglin",
        false,
    ),
    (
        "custom_head",
        Instrument::CustomHead,
        "minecraft:block.note_block.imitate.zombie",
        false,
    ),
];

fn entry_of(name: &str) -> (&'static str, Instrument, &'static str, bool) {
    INSTRUMENTS
        .iter()
        .find(|(known, ..)| *known == name)
        .copied()
        .unwrap_or(INSTRUMENTS[0])
}

pub(crate) fn pitch_of(note: i32) -> f32 {
    2.0_f32.powf((note - MIDDLE) as f32 / 12.0)
}

fn works_above(name: &str) -> bool {
    matches!(
        name,
        "zombie" | "skeleton" | "creeper" | "dragon" | "wither_skeleton" | "piglin" | "custom_head"
    )
}

fn instrument_above(state: BlockStateId) -> Option<&'static str> {
    let name = state.kind()?.name();
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(match name {
        "zombie_head" | "zombie_wall_head" => "zombie",
        "skeleton_skull" | "skeleton_wall_skull" => "skeleton",
        "creeper_head" | "creeper_wall_head" => "creeper",
        "dragon_head" | "dragon_wall_head" => "dragon",
        "wither_skeleton_skull" | "wither_skeleton_wall_skull" => "wither_skeleton",
        "piglin_head" | "piglin_wall_head" => "piglin",
        "player_head" | "player_wall_head" => "custom_head",
        _ => return None,
    })
}

fn instrument_below(state: BlockStateId) -> &'static str {
    let Some(kind) = state.kind() else {
        return "harp";
    };
    let name = kind.name();
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let wood = [
        "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
        "bamboo", "crimson", "warped",
    ]
    .iter()
    .any(|tree| name.starts_with(tree))
        && !name.ends_with("_leaves")
        && !name.ends_with("_sapling")
        && !name.ends_with("_button")
        && !name.ends_with("_pressure_plate");
    if name.contains("copper") && !name.contains("chest") && !name.contains("golem") {
        return if name.contains("exposed") {
            "trumpet_exposed"
        } else if name.contains("weathered") {
            "trumpet_weathered"
        } else if name.contains("oxidized") {
            "trumpet_oxidized"
        } else if name == "copper_block" || name == "waxed_copper_block" {
            "trumpet"
        } else {
            "harp"
        };
    }
    match name {
        "gold_block" => "bell",
        "packed_ice" => "chime",
        "bone_block" => "xylophone",
        "iron_block" => "iron_xylophone",
        "soul_sand" | "soul_soil" => "cow_bell",
        "pumpkin" | "carved_pumpkin" | "jack_o_lantern" => "didgeridoo",
        "emerald_block" => "bit",
        "hay_block" => "banjo",
        "glowstone" => "pling",
        "clay" => "flute",
        "sea_lantern" | "beacon" => "hat",
        "sand" | "red_sand" | "gravel" | "suspicious_sand" | "suspicious_gravel" => "snare",
        _ if name.ends_with("concrete_powder") => "snare",
        _ if name.ends_with("_wool") || name.ends_with("_carpet") => "guitar",
        _ if name.contains("glass") => "hat",
        _ if wood
            || name.contains("planks")
            || matches!(
                name,
                "crafting_table"
                    | "bookshelf"
                    | "chiseled_bookshelf"
                    | "jukebox"
                    | "note_block"
                    | "barrel"
                    | "lectern"
                    | "loom"
                    | "fletching_table"
                    | "cartography_table"
                    | "smithing_table"
                    | "composter"
                    | "chest"
                    | "trapped_chest"
            ) =>
        {
            "bass"
        }
        _ if name.contains("redstone") || name.contains("glowstone") => "harp",
        _ if name.contains("stone")
            || name.contains("brick")
            || name.ends_with("_ore")
            || name.contains("deepslate")
            || name.contains("obsidian")
            || name.contains("netherrack")
            || name.contains("terracotta")
            || name.contains("concrete")
            || name.contains("blackstone")
            || name.contains("basalt")
            || name.contains("tuff")
            || name.contains("andesite")
            || name.contains("diorite")
            || name.contains("granite")
            || name.contains("prismarine")
            || name.contains("purpur")
            || name.contains("quartz")
            || matches!(
                name,
                "furnace"
                    | "blast_furnace"
                    | "smoker"
                    | "dispenser"
                    | "dropper"
                    | "observer"
                    | "hopper"
            ) =>
        {
            "basedrum"
        }
        _ => "harp",
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn note_instrument_at(&self, pos: BlockPos) -> &'static str {
        if let Some(above) = instrument_above(self.block_or_air(step(pos, Face::Up))) {
            return above;
        }
        let below = instrument_below(self.block_or_air(step(pos, Face::Down)));
        if works_above(below) { "harp" } else { below }
    }

    pub(crate) fn note_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        state
            .with_property("instrument", self.note_instrument_at(pos))
            .unwrap_or(state)
    }

    pub(crate) fn note_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        if !matches!(direction, Face::Up | Face::Down) {
            return;
        }
        let instrument = self.note_instrument_at(pos);
        if state.property("instrument") == Some(instrument) {
            return;
        }
        if let Some(next) = state.with_property("instrument", instrument) {
            self.set_block_quiet(pos, next);
        }
    }

    pub(crate) fn note_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let powered = self.has_neighbor_signal(pos);
        if powered == flag(state, "powered") {
            return;
        }
        let Some(next) = state.with_property("powered", if powered { "true" } else { "false" })
        else {
            return;
        };
        if powered {
            self.play_note(pos, state);
        }
        self.set_block(pos, next);
    }

    pub(crate) fn use_note_block(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let next_note = (level(state, "note") + 1) % NOTES;
        if let Some(next) = state.with_property("note", &next_note.to_string()) {
            self.set_block(pos, next);
            self.play_note(pos, next);
        }
        true
    }

    pub(crate) fn note_attacked(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.kind() == Some(BlockKind::NoteBlock) {
            self.play_note(pos, state);
        }
    }

    pub(crate) fn play_note(&mut self, pos: BlockPos, state: BlockStateId) {
        let instrument = state.property("instrument").unwrap_or("harp");
        let above_air = self.block_or_air(step(pos, Face::Up)).is_air();
        if !works_above(instrument) && !above_air {
            return;
        }
        let (name, bukkit, ..) = entry_of(instrument);
        let note = level(state, "note");
        let note_step = NoteStep {
            pos,
            instrument: name,
            note,
        };
        self.fire(Lane::Block(pos), Continuation::Note(note_step), move |_| {
            Some(EventPayload::NotePlay(NotePlay {
                block: pos,
                instrument: bukkit,
                note: note as u8,
            }))
        });
    }

    pub(crate) fn finish_note(
        &mut self,
        note_step: NoteStep,
        cancelled: bool,
        _by: Option<PlayerId>,
    ) {
        if cancelled {
            return;
        }
        let state = self.block_or_air(note_step.pos);
        if state.kind() != Some(BlockKind::NoteBlock) {
            return;
        }
        let (_, _, sound, tunable) = entry_of(note_step.instrument);
        let pitch = if tunable {
            pitch_of(note_step.note)
        } else {
            1.0
        };
        let center = note_step.pos.center() + rspigot_domain::Vec3::new(0.0, 0.5, 0.0);
        self.push(
            Audience::Nearby {
                position: center,
                range: NOTE_RANGE,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos: note_step.pos,
                action: 0,
                data: 0,
                block: BlockKind::NoteBlock.id().0,
            },
        );
        self.play_sound(
            None,
            center,
            sound,
            SoundSource::Records,
            NOTE_VOLUME,
            pitch,
        );
        self.block_game_event(
            GameEvent::NoteBlockPlay,
            note_step.pos,
            GameEventContext::block(state),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_sound_is_a_registered_sound_event() {
        for (name, _, sound, _) in INSTRUMENTS {
            assert!(
                rspigot_data::registries::static_index("minecraft:sound_event", sound).is_some(),
                "{name}: {sound}"
            );
        }
    }

    #[test]
    fn the_pitch_doubles_every_octave() {
        assert_eq!(pitch_of(12), 1.0);
        assert_eq!(pitch_of(24), 2.0);
        assert_eq!(pitch_of(0), 0.5);
    }

    #[test]
    fn the_blocks_under_a_note_block_pick_their_instruments() {
        let state = |kind: BlockKind| kind.default_state();
        assert_eq!(instrument_below(state(BlockKind::GoldBlock)), "bell");
        assert_eq!(instrument_below(state(BlockKind::OakPlanks)), "bass");
        assert_eq!(instrument_below(state(BlockKind::Stone)), "basedrum");
        assert_eq!(instrument_below(state(BlockKind::Sand)), "snare");
        assert_eq!(instrument_below(state(BlockKind::Glass)), "hat");
        assert_eq!(instrument_below(state(BlockKind::WhiteWool)), "guitar");
        assert_eq!(instrument_below(state(BlockKind::RedstoneBlock)), "harp");
        assert_eq!(instrument_below(state(BlockKind::Dirt)), "harp");
    }
}
