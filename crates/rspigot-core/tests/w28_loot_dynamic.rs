use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::BlockKind;
use rspigot_core::loot::{self, LootContext};
use rspigot_nbt::{Compound, Tag};

fn pot(cracked: &str) -> rspigot_domain::BlockStateId {
    BlockKind::DecoratedPot
        .state_with(&[
            ("cracked", cracked),
            ("facing", "north"),
            ("waterlogged", "false"),
        ])
        .expect("a decorated pot state")
}

fn sherds() -> Compound {
    Compound::new().with(
        "sherds",
        Tag::List(vec![
            Tag::String("minecraft:brick".to_owned()),
            Tag::String("minecraft:angler_pottery_sherd".to_owned()),
            Tag::String("minecraft:brick".to_owned()),
            Tag::String("minecraft:arms_up_pottery_sherd".to_owned()),
        ]),
    )
}

#[test]
fn a_cracked_pot_drops_its_sherds_and_no_pot() {
    let data = sherds();
    let context = LootContext::block(pot("true"), None, None).with_block_entity(Some(&data));
    let drops = loot::block_drops(&context, &mut StdRng::seed_from_u64(1));
    let mut names: Vec<&str> = drops
        .iter()
        .map(|stack| stack.item.kind().expect("an item").name())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "minecraft:angler_pottery_sherd",
            "minecraft:arms_up_pottery_sherd",
            "minecraft:brick",
            "minecraft:brick"
        ]
    );
}

#[test]
fn an_intact_pot_drops_itself_with_its_decorations() {
    let data = sherds();
    let context = LootContext::block(pot("false"), None, None).with_block_entity(Some(&data));
    let drops = loot::block_drops(&context, &mut StdRng::seed_from_u64(1));
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].item.kind().expect("an item").name(),
        "minecraft:decorated_pot"
    );
}
