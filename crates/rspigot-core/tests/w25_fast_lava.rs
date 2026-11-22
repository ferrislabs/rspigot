#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::dimension_rules::{GameplayRules, register_gameplay_rules};
use rspigot_domain::{BlockPos, Identifier};

const SOURCE: BlockPos = BlockPos::new(4, 64, 4);

#[test]
fn a_dimension_type_with_fast_lava_spreads_lava_after_ten_ticks_and_four_blocks_far() {
    register_gameplay_rules(
        &Identifier::minecraft("overworld"),
        GameplayRules {
            fast_lava: true,
            ..GameplayRules::default()
        },
    );
    let mut s = scene_of(Cells::default(), 3, 0);
    console(&mut s, "setblock 4 64 4 lava");
    s.tick(2);
    assert_eq!(kind_at(&s, SOURCE.offset(1, 0, 0)), Some(BlockKind::Air));
    s.tick(14);
    assert_eq!(kind_at(&s, SOURCE.offset(1, 0, 0)), Some(BlockKind::Lava));
    assert_eq!(
        property_at(&s, SOURCE.offset(1, 0, 0), "level").as_deref(),
        Some("1")
    );
    s.tick(300);
    assert_eq!(kind_at(&s, SOURCE.offset(7, 0, 0)), Some(BlockKind::Lava));
    assert_eq!(kind_at(&s, SOURCE.offset(8, 0, 0)), Some(BlockKind::Air));
}
