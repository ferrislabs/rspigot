mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};
use rspigot_domain::BlockPos;
use seeded::{Seed, contents, world_configured};

const CHEST: BlockPos = BlockPos::new(3, 64, 3);
const HOPPER: BlockPos = BlockPos::new(3, 65, 3);
const FLOOR_HOPPER: BlockPos = BlockPos::new(0, 63, 0);

fn pushing(amount: i32) -> Scene {
    let hopper = Seed::block(HOPPER, BlockKind::Hopper)
        .with("facing", "down")
        .holding("minecraft:stone", 10);
    world_configured(
        vec![Seed::block(CHEST, BlockKind::Chest), hopper],
        Vec::new(),
        false,
        |config| config.world_keys.hopper_amount = amount,
    )
}

#[test]
fn hopper_amount_moves_several_items_per_transfer() {
    let mut s = pushing(3);
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 3))]
    );
    assert_eq!(
        contents(&mut s, &alice, HOPPER, 1),
        [Some(("minecraft:stone", 7))]
    );
}

#[test]
fn a_default_hopper_still_moves_one_item() {
    let mut s = pushing(1);
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 1))]
    );
}

#[test]
fn hopper_amount_never_moves_more_than_the_stack_holds() {
    let hopper = Seed::block(HOPPER, BlockKind::Hopper)
        .with("facing", "down")
        .holding("minecraft:stone", 2);
    let mut s = world_configured(
        vec![Seed::block(CHEST, BlockKind::Chest), hopper],
        Vec::new(),
        false,
        |config| config.world_keys.hopper_amount = 64,
    );
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 2))]
    );
}

fn drop_at_feet(s: &mut Scene, key: &str, count: i32) {
    let alice = s.players[0].clone();
    for slot in (0..=35).chain([40]) {
        s.act(Order::Inventory {
            player: alice.id,
            order: InventoryOrder::Set {
                slot,
                item: Some(ItemSpec::of("minecraft:cobblestone", 64)),
            },
        });
    }
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Add(ItemSpec::of(key, count)),
    });
}

fn ticks_to_suck(check: i32) -> u32 {
    let mut s = world_configured(
        vec![Seed::block(FLOOR_HOPPER, BlockKind::Hopper).with("facing", "down")],
        Vec::new(),
        false,
        |config| config.world_keys.hopper_check = check,
    );
    let alice = s.players[0].clone();
    s.tick(2);
    drop_at_feet(&mut s, "minecraft:diamond", 5);
    for waited in 1..=30 {
        s.tick(1);
        if contents(&mut s, &alice, FLOOR_HOPPER, 1)[0].is_some() {
            return waited;
        }
    }
    u32::MAX
}

#[test]
fn hopper_check_spaces_the_pickup_scans_of_an_idle_hopper() {
    let eager = ticks_to_suck(1);
    let lazy = ticks_to_suck(10);
    assert!(eager <= 3, "{eager}");
    assert!(lazy >= 5 && lazy != u32::MAX, "{lazy}");
}
