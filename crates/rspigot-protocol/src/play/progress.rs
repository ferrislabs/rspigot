use crate::codec::{Decode, DecodeError, Encode};
use crate::ids::play::clientbound as id;
use crate::play::slot::ItemStackTemplate;
use crate::play::wire::{List, Opt, Plain, UNBOUNDED, Var, read_var, wire_struct, write_var};
use crate::types::Text;

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatEntry {
        pub stat_type: i32 as Var,
        pub value: i32 as Var,
        pub amount: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AwardStats = id::AWARD_STATS {
        pub stats: Vec<StatEntry> as List<Plain, UNBOUNDED>,
    }
}

pub mod display_flags {
    pub const BACKGROUND: i32 = 1;
    pub const SHOW_TOAST: i32 = 2;
    pub const HIDDEN: i32 = 4;
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancementDisplay {
    pub title: Text,
    pub description: Text,
    pub icon: ItemStackTemplate,
    pub frame: i32,
    pub show_toast: bool,
    pub hidden: bool,
    pub background: Option<String>,
}

impl Encode for AdvancementDisplay {
    fn encode(&self, out: &mut Vec<u8>) {
        self.title.encode(out);
        self.description.encode(out);
        self.icon.encode(out);
        write_var(self.frame, out);
        let mut flags = 0;
        if self.background.is_some() {
            flags |= display_flags::BACKGROUND;
        }
        if self.show_toast {
            flags |= display_flags::SHOW_TOAST;
        }
        if self.hidden {
            flags |= display_flags::HIDDEN;
        }
        flags.encode(out);
        if let Some(background) = &self.background {
            background.encode(out);
        }
    }
}

impl Decode for AdvancementDisplay {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let title = Text::decode(input)?;
        let description = Text::decode(input)?;
        let icon = ItemStackTemplate::decode(input)?;
        let frame = read_var(input)?;
        let flags = i32::decode(input)?;
        let background = if flags & display_flags::BACKGROUND != 0 {
            Some(String::decode(input)?)
        } else {
            None
        };
        Ok(Self {
            title,
            description,
            icon,
            frame,
            show_toast: flags & display_flags::SHOW_TOAST != 0,
            hidden: flags & display_flags::HIDDEN != 0,
            background,
        })
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct AdvancementData {
        pub parent: Option<String> as Opt<Plain>,
        pub display: Option<AdvancementDisplay> as Opt<Plain>,
        pub requirements: Vec<Vec<String>> as List<List<Plain, UNBOUNDED>, UNBOUNDED>,
        pub telemetry: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct PositionedAdvancement {
        pub id: String,
        pub advancement: AdvancementData,
        pub x: f32,
        pub y: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CriterionEntry {
        pub name: String,
        pub obtained: Option<i64> as Opt<Plain>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProgressEntry {
        pub id: String,
        pub criteria: Vec<CriterionEntry> as List<Plain, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct UpdateAdvancements = id::UPDATE_ADVANCEMENTS {
        pub reset: bool,
        pub added: Vec<PositionedAdvancement> as List<Plain, UNBOUNDED>,
        pub removed: Vec<String> as List<Plain, UNBOUNDED>,
        pub progress: Vec<ProgressEntry> as List<Plain, UNBOUNDED>,
        pub show: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SelectAdvancementsTab = id::SELECT_ADVANCEMENTS_TAB {
        pub tab: Option<String> as Opt<Plain>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::wire::testing::roundtrip;

    #[test]
    fn an_update_lists_the_reset_flag_then_the_added_removed_and_progress() {
        let packet = UpdateAdvancements {
            reset: true,
            added: Vec::new(),
            removed: vec!["a:b".to_owned()],
            progress: vec![ProgressEntry {
                id: "a:c".to_owned(),
                criteria: vec![
                    CriterionEntry {
                        name: "x".to_owned(),
                        obtained: Some(5),
                    },
                    CriterionEntry {
                        name: "y".to_owned(),
                        obtained: None,
                    },
                ],
            }],
            show: false,
        };
        assert_eq!(
            roundtrip(&packet),
            vec![
                1, 0, 1, 3, b'a', b':', b'b', 1, 3, b'a', b':', b'c', 2, 1, b'x', 1, 0, 0, 0, 0, 0,
                0, 0, 5, 1, b'y', 0, 0
            ]
        );
    }

    #[test]
    fn a_display_carries_its_flags_and_its_background_only_when_flagged() {
        use crate::play::slot::DataComponentPatch;
        let display = AdvancementDisplay {
            title: Text(rspigot_nbt::Tag::String("t".into())),
            description: Text(rspigot_nbt::Tag::String("d".into())),
            icon: ItemStackTemplate {
                item: 3,
                count: 1,
                components: DataComponentPatch::EMPTY,
            },
            frame: 1,
            show_toast: true,
            hidden: false,
            background: Some("b".to_owned()),
        };
        let bytes = roundtrip(&display);
        assert_eq!(&bytes[bytes.len() - 7..], &[1, 0, 0, 0, 3, 1, b'b']);
    }

    #[test]
    fn award_stats_lists_type_value_and_amount() {
        let packet = AwardStats {
            stats: vec![StatEntry {
                stat_type: 8,
                value: 23,
                amount: 300,
            }],
        };
        assert_eq!(roundtrip(&packet), vec![1, 8, 23, 0xAC, 0x02]);
    }
}
