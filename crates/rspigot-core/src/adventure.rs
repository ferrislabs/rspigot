use rspigot_domain::BlockStateId;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::GameMode;
use rspigot_domain::stack_components::{self, CAN_BREAK, CAN_PLACE_ON};

const GAME_MASTER_BLOCKS: [&str; 7] = [
    "minecraft:command_block",
    "minecraft:chain_command_block",
    "minecraft:repeating_command_block",
    "minecraft:structure_block",
    "minecraft:jigsaw",
    "minecraft:test_block",
    "minecraft:test_instance_block",
];

pub(crate) fn is_game_master_block(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| GAME_MASTER_BLOCKS.contains(&kind.name()))
}

pub(crate) fn may_break_in_adventure(held: Option<&ItemStack>, state: BlockStateId) -> bool {
    matches(held, state, &CAN_BREAK)
}

pub(crate) fn may_place_in_adventure(held: Option<&ItemStack>, clicked: BlockStateId) -> bool {
    matches(held, clicked, &CAN_PLACE_ON)
}

fn matches(held: Option<&ItemStack>, state: BlockStateId, key: &stack_components::Key) -> bool {
    held.is_some_and(|stack| stack_components::adventure_allows(stack, key, state))
}

pub(crate) fn use_restricted_by_mode(
    game_mode: GameMode,
    held: Option<&ItemStack>,
    clicked: BlockStateId,
) -> bool {
    match game_mode {
        GameMode::Spectator => true,
        GameMode::Adventure => !may_place_in_adventure(held, clicked),
        GameMode::Survival | GameMode::Creative => false,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_blocks::BlockKind;
    use rspigot_items::ItemKind;
    use rspigot_nbt::{Compound, Tag};

    use super::*;

    fn state(name: &str) -> BlockStateId {
        BlockKind::from_name(name).unwrap().default_state()
    }

    fn stick(key: &str, blocks: &str) -> ItemStack {
        let mut stack = ItemStack::new(ItemKind::Stick.id(), NonZeroU8::MIN);
        stack.components.insert(
            key,
            Tag::List(vec![Tag::Compound(Compound::new().with("blocks", blocks))]),
        );
        stack
    }

    #[test]
    fn game_master_blocks_are_the_command_structure_and_jigsaw_blocks() {
        assert!(is_game_master_block(state("command_block")));
        assert!(is_game_master_block(state("structure_block")));
        assert!(is_game_master_block(state("jigsaw")));
        assert!(!is_game_master_block(state("stone")));
    }

    #[test]
    fn adventure_players_need_a_matching_predicate_on_the_held_item() {
        let pick = stick("minecraft:can_break", "minecraft:stone");
        assert!(may_break_in_adventure(Some(&pick), state("stone")));
        assert!(!may_break_in_adventure(Some(&pick), state("dirt")));
        assert!(!may_break_in_adventure(None, state("stone")));
        let place = stick("minecraft:can_place_on", "minecraft:stone");
        assert!(!use_restricted_by_mode(
            GameMode::Adventure,
            Some(&place),
            state("stone")
        ));
        assert!(use_restricted_by_mode(
            GameMode::Adventure,
            Some(&place),
            state("dirt")
        ));
        assert!(use_restricted_by_mode(
            GameMode::Spectator,
            Some(&place),
            state("stone")
        ));
        assert!(!use_restricted_by_mode(
            GameMode::Survival,
            None,
            state("stone")
        ));
    }
}
