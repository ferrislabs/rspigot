use rspigot_blocks::BlockKind;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Rotation, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::events::{Direction, Hand};
use crate::game_events::GameEventContext;
use crate::plugins::direction_of;
use crate::plugins::steps::UseOnStep;
use crate::ports::ClientLink;
use crate::server::Server;

const WAXED_PREFIX: &str = "waxed_";
const WAX_ON_EVENT: i32 = 3003;
const WAX_ON_SOUND: &str = "minecraft:item.honeycomb.wax_on";
const SPLASH_SOUND: &str = "minecraft:entity.generic.splash";
const SNIP_SOUND: &str = "minecraft:item.shears.snip";
const CONVERTIBLE_TO_MUD: &str = "minecraft:convertible_to_mud";
const MAX_PLANT_AGE: &str = "25";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const WATER_POTION: &str = "minecraft:water";
const RECIPES: &str = "minecraft:recipes";

pub(crate) fn waxed_of(state: BlockStateId) -> Option<BlockStateId> {
    let name = state.kind()?.name().trim_start_matches("minecraft:");
    if name.starts_with(WAXED_PREFIX) || !name.contains("copper") {
        return None;
    }
    let waxed = BlockKind::from_name(&format!("{WAXED_PREFIX}{name}"))?;
    let properties: Vec<(&str, &str)> = state.properties().collect();
    waxed.state_with(&properties)
}

fn is_liquid(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava))
}

fn mud_of(state: BlockStateId) -> Option<BlockStateId> {
    let kind = state.kind()?;
    rspigot_data::tags::tag("minecraft:block", CONVERTIBLE_TO_MUD)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
        .then(|| BlockKind::Mud.default_state())
}

fn max_age_of(state: BlockStateId) -> Option<BlockStateId> {
    let growing = matches!(
        state.kind()?,
        BlockKind::Kelp | BlockKind::CaveVines | BlockKind::WeepingVines | BlockKind::TwistingVines
    );
    if !growing || state.property("age") == Some(MAX_PLANT_AGE) {
        return None;
    }
    state.with_property("age", MAX_PLANT_AGE)
}

fn is_water_bottle(stack: &ItemStack) -> bool {
    stack.item == ItemKind::Potion.id()
        && stack
            .components
            .get_compound(POTION_CONTENTS)
            .and_then(|contents| contents.get_str("potion"))
            .is_some_and(|potion| potion == WATER_POTION)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_water_bottle_on_dirt(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if !is_water_bottle(&stack) {
            return false;
        }
        let Some(mud) = mud_of(clicked) else {
            return false;
        };
        if self.players.get(&id).is_none_or(|player| {
            matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
        }) {
            return false;
        }
        self.set_block(pos, mud);
        self.play_sound(
            None,
            pos.center(),
            SPLASH_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.block_game_event(GameEvent::FluidPlace, pos, GameEventContext::of(None));
        self.count_used(id, &stack);
        let bottle = ItemStack::new(ItemKind::GlassBottle.id(), std::num::NonZeroU8::MIN);
        self.replace_held_with(id, hand, &stack, bottle);
        true
    }

    pub(crate) fn use_shears_on_plant(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item != ItemKind::Shears.id() {
            return false;
        }
        let Some(grown) = max_age_of(clicked) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator) {
            return false;
        }
        let entity = player.entity_id;
        self.set_block(pos, grown);
        self.play_sound(
            Some(id),
            pos.center(),
            SNIP_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::entity_in(Some(entity), grown),
        );
        self.hurt_and_break(id, hand, 1);
        self.player_swing(id, hand, false);
        true
    }

    pub(crate) fn use_knowledge_book(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item != ItemKind::KnowledgeBook.id() {
            return false;
        }
        let recipes: Vec<String> = stack
            .components
            .get_list(RECIPES)
            .unwrap_or_default()
            .iter()
            .filter_map(|recipe| recipe.as_str().map(str::to_owned))
            .collect();
        if recipes.is_empty() {
            return true;
        }
        let names: Vec<&str> = recipes.iter().map(String::as_str).collect();
        self.award_recipes(id, &names);
        self.count_used(id, &stack);
        self.consume_held(id, hand, 1);
        self.refresh_hands(id);
        true
    }

    fn replace_held_with(&mut self, id: PlayerId, hand: Hand, used: &ItemStack, result: ItemStack) {
        if self.infinite_materials(id) {
            self.refresh_hands(id);
            return;
        }
        let Some(entity) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        match std::num::NonZeroU8::new(used.count.get() - 1) {
            None => self.set_held(id, hand, Some(result)),
            Some(count) => {
                self.set_held(
                    id,
                    hand,
                    Some(ItemStack {
                        count,
                        ..used.clone()
                    }),
                );
                let mut spare = Some(result);
                if let Some(player) = self.players.get_mut(&id) {
                    player.inventory.add(&mut spare, false);
                }
                if let Some(spare) = spare {
                    self.drop_at_location(entity, 1.0, spare, Vec3::ZERO);
                }
            }
        }
        self.refresh_hands(id);
    }

    pub(crate) fn use_honeycomb_on_block(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item != ItemKind::Honeycomb.id() {
            return false;
        }
        let Some(waxed) = waxed_of(clicked) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator) {
            return false;
        }
        let entity = player.entity_id;
        self.set_block(pos, waxed);
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::entity_in(Some(entity), waxed),
        );
        self.level_event(WAX_ON_EVENT, pos, 0);
        self.play_sound(
            Some(id),
            pos.center(),
            WAX_ON_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.count_used(id, &stack);
        self.consume_held(id, hand, 1);
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        true
    }

    pub(crate) fn use_place_on_water(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
        sequence: i32,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if !matches!(
            stack.item.kind(),
            Some(ItemKind::LilyPad | ItemKind::Frogspawn)
        ) {
            return false;
        }
        let Some(hit) = self.look_source(id, rotation) else {
            return false;
        };
        if !self.world.chunks.block(hit.pos).is_some_and(is_liquid) {
            return false;
        }
        let above = hit.pos.offset(0, 1, 0);
        let Some(clicked) = self
            .world
            .chunks
            .block(above)
            .filter(|state| state.is_air())
        else {
            return false;
        };
        let (Some(slot), Some(game_mode)) = (
            self.hand_slot_of(id, hand),
            self.players.get(&id).map(|player| player.game_mode),
        ) else {
            return false;
        };
        let step = UseOnStep {
            player: id,
            hand,
            slot,
            pos: above,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.0, 0.5),
            sequence,
            stack: Some(stack),
            clicked,
            game_mode,
            block_denied: false,
        };
        self.use_item_on_now(&step, true, true);
        true
    }

    pub(crate) fn use_spawn_egg_on_liquid(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if rspigot_entities::EntityKind::from_spawn_egg(stack.item.0).is_none() {
            return false;
        }
        let Some(hit) = self.look_source(id, rotation) else {
            return false;
        };
        let Some(clicked) = self
            .world
            .chunks
            .block(hit.pos)
            .filter(|state| is_liquid(*state))
        else {
            return false;
        };
        let Some(slot) = self.hand_slot_of(id, hand) else {
            return false;
        };
        if self
            .players
            .get(&id)
            .is_some_and(|player| player.game_mode == GameMode::Spectator)
        {
            return true;
        }
        self.spawn_from_egg_on_block(id, slot, &stack, hit.pos, direction_of(hit.face), clicked);
        true
    }
}
