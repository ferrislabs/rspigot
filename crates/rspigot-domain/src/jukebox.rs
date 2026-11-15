use rspigot_data::registries::synced_index;
use rspigot_nbt::Tag;

use crate::item::ItemStack;

const REGISTRY: &str = "minecraft:jukebox_song";
const PLAYABLE: &str = "minecraft:jukebox_playable";
const DISC_PREFIX: &str = "music_disc_";
const TICKS_PER_SECOND: f32 = 20.0;
const TAIL_TICKS: i64 = 20;
const NOTE_PERIOD: i64 = 20;
pub const START_EVENT: i32 = 1010;
pub const STOP_EVENT: i32 = 1011;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Song {
    pub name: &'static str,
    pub seconds: f32,
    pub comparator: i32,
}

const SONGS: &[Song] = &[
    Song {
        name: "minecraft:11",
        seconds: 71.0,
        comparator: 11,
    },
    Song {
        name: "minecraft:13",
        seconds: 178.0,
        comparator: 1,
    },
    Song {
        name: "minecraft:5",
        seconds: 178.0,
        comparator: 15,
    },
    Song {
        name: "minecraft:blocks",
        seconds: 345.0,
        comparator: 3,
    },
    Song {
        name: "minecraft:bounce",
        seconds: 234.0,
        comparator: 8,
    },
    Song {
        name: "minecraft:cat",
        seconds: 185.0,
        comparator: 2,
    },
    Song {
        name: "minecraft:chirp",
        seconds: 185.0,
        comparator: 4,
    },
    Song {
        name: "minecraft:creator",
        seconds: 176.0,
        comparator: 12,
    },
    Song {
        name: "minecraft:creator_music_box",
        seconds: 73.0,
        comparator: 11,
    },
    Song {
        name: "minecraft:far",
        seconds: 174.0,
        comparator: 5,
    },
    Song {
        name: "minecraft:lava_chicken",
        seconds: 134.0,
        comparator: 9,
    },
    Song {
        name: "minecraft:mall",
        seconds: 197.0,
        comparator: 6,
    },
    Song {
        name: "minecraft:mellohi",
        seconds: 96.0,
        comparator: 7,
    },
    Song {
        name: "minecraft:otherside",
        seconds: 195.0,
        comparator: 14,
    },
    Song {
        name: "minecraft:pigstep",
        seconds: 149.0,
        comparator: 13,
    },
    Song {
        name: "minecraft:precipice",
        seconds: 299.0,
        comparator: 13,
    },
    Song {
        name: "minecraft:relic",
        seconds: 218.0,
        comparator: 14,
    },
    Song {
        name: "minecraft:stal",
        seconds: 150.0,
        comparator: 8,
    },
    Song {
        name: "minecraft:strad",
        seconds: 188.0,
        comparator: 9,
    },
    Song {
        name: "minecraft:tears",
        seconds: 175.0,
        comparator: 10,
    },
    Song {
        name: "minecraft:wait",
        seconds: 238.0,
        comparator: 12,
    },
    Song {
        name: "minecraft:ward",
        seconds: 251.0,
        comparator: 10,
    },
];

impl Song {
    pub fn named(name: &str) -> Option<&'static Song> {
        SONGS.iter().find(|song| song.name == name)
    }

    pub fn length_in_ticks(&self) -> i64 {
        (self.seconds * TICKS_PER_SECOND).ceil() as i64
    }

    pub fn has_finished(&self, ticks: i64) -> bool {
        ticks >= self.length_in_ticks() + TAIL_TICKS
    }

    pub fn registry_id(&self) -> Option<i32> {
        synced_index(REGISTRY, self.name).and_then(|index| i32::try_from(index).ok())
    }
}

pub fn emits_note(ticks: i64) -> bool {
    ticks % NOTE_PERIOD == 0
}

fn named_by_component(tag: &Tag) -> Option<&str> {
    match tag {
        Tag::String(name) => Some(name),
        Tag::Compound(compound) => compound
            .get_str("song")
            .or_else(|| compound.get_str("jukebox_song")),
        _ => None,
    }
}

pub fn is_playable(stack: &ItemStack) -> bool {
    song_of(stack).is_some()
}

pub fn song_of(stack: &ItemStack) -> Option<&'static Song> {
    if let Some(tag) = stack.components.get(PLAYABLE) {
        return named_by_component(tag).and_then(Song::named);
    }
    let name = stack.item.kind()?.name();
    let song = name
        .strip_prefix("minecraft:")
        .unwrap_or(name)
        .strip_prefix(DISC_PREFIX)?;
    Song::named(&format!("minecraft:{song}"))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_items::ItemKind;

    use super::*;

    fn disc(kind: ItemKind) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::MIN)
    }

    #[test]
    fn every_song_has_a_registry_id_in_the_synced_order() {
        for (index, song) in SONGS.iter().enumerate() {
            assert_eq!(song.registry_id(), i32::try_from(index).ok());
        }
    }

    #[test]
    fn every_synced_song_has_a_row() {
        let synced = rspigot_data::registries::synced_entries(REGISTRY).unwrap();
        assert_eq!(synced.len(), SONGS.len());
        for name in synced {
            assert!(Song::named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn a_disc_plays_the_song_of_its_name() {
        let song = song_of(&disc(ItemKind::MusicDisc13)).unwrap();
        assert_eq!(song.name, "minecraft:13");
        assert_eq!(song.comparator, 1);
        assert_eq!(song.length_in_ticks(), 3560);
    }

    #[test]
    fn a_song_ends_one_second_after_its_length() {
        let song = Song::named("minecraft:11").unwrap();
        assert!(!song.has_finished(1439));
        assert!(song.has_finished(1440));
    }

    #[test]
    fn a_plain_item_is_not_playable() {
        assert!(!is_playable(&disc(ItemKind::Stick)));
    }

    #[test]
    fn the_playable_component_names_the_song() {
        let mut stack = disc(ItemKind::Stick);
        stack
            .components
            .insert(PLAYABLE, Tag::String("minecraft:cat".to_owned()));
        assert_eq!(song_of(&stack).unwrap().name, "minecraft:cat");
    }

    #[test]
    fn notes_come_every_second() {
        assert!(emits_note(0));
        assert!(!emits_note(7));
        assert!(emits_note(40));
    }
}
