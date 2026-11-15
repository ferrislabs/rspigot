use crate::ids::play::clientbound as id;
use crate::play::wire::{List, Plain, UNBOUNDED, Var, wire_struct};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::wire::testing::roundtrip;

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
