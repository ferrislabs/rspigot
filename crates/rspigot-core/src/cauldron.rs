use std::num::NonZeroU8;

use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::potion::{self, PotionContents};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::Tag;

use crate::events::Hand;
use crate::interact::item_tag;
use crate::liquid_bucket::{
    EMPTY_LAVA_SOUND, EMPTY_POWDER_SNOW_SOUND, EMPTY_SOUND, FILL_LAVA_SOUND,
    FILL_POWDER_SNOW_SOUND, FILL_SOUND, single,
};
use crate::plugins::events::{CauldronLevelChange, CauldronLevelChangeReason as Reason};
use crate::plugins::steps::CauldronStep;
use crate::plugins::{Continuation, EventPayload, Lane, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

const BOTTLE_FILL_SOUND: &str = "minecraft:item.bottle.fill";
const BOTTLE_EMPTY_SOUND: &str = "minecraft:item.bottle.empty";
const DYED_COLOR: &str = "minecraft:dyed_color";
const BANNER_PATTERNS: &str = "minecraft:banner_patterns";
const REMOVES_DYE: &str = "minecraft:cauldron_can_remove_dye";
const BANNER_SUFFIX: &str = "_banner";
const SHULKER_SUFFIX: &str = "_shulker_box";
const MAX_LEVEL: u8 = 3;
const RAIN_FILL_CHANCE: f32 = 0.05;
const SNOW_FILL_CHANCE: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Contents {
    Empty,
    Water(u8),
    Lava,
    PowderSnow(u8),
}

impl Contents {
    pub(crate) fn of(state: BlockStateId) -> Option<Self> {
        let level = || state.property("level")?.parse::<u8>().ok();
        match state.kind()? {
            BlockKind::Cauldron => Some(Self::Empty),
            BlockKind::WaterCauldron => level().map(Self::Water),
            BlockKind::LavaCauldron => Some(Self::Lava),
            BlockKind::PowderSnowCauldron => level().map(Self::PowderSnow),
            _ => None,
        }
    }
}

fn water_level(level: u8) -> BlockStateId {
    let state = BlockKind::WaterCauldron.default_state();
    state
        .with_property("level", &level.to_string())
        .unwrap_or(state)
}

fn snow_level(level: u8) -> BlockStateId {
    let state = BlockKind::PowderSnowCauldron.default_state();
    state
        .with_property("level", &level.to_string())
        .unwrap_or(state)
}

fn lowered_water(level: u8) -> BlockStateId {
    match level.saturating_sub(1) {
        0 => BlockKind::Cauldron.default_state(),
        lower => water_level(lower),
    }
}

fn is_under_water(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Water | BlockKind::BubbleColumn)
    ) || state.property("waterlogged") == Some("true")
}

#[derive(Debug)]
pub(crate) enum Effect {
    Give(ItemStack),
    Undye,
    Nothing,
}

#[derive(Debug)]
pub(crate) struct Handled {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) held: ItemStack,
    pub(crate) sequence: i32,
    pub(crate) sound: Option<&'static str>,
    pub(crate) effect: Effect,
}

struct Change {
    state: BlockStateId,
    reason: Reason,
    sound: Option<&'static str>,
    effect: Effect,
}

enum Plan {
    Pass,
    Consume,
    Change(Change),
}

fn banner_layers(stack: &ItemStack) -> Option<&[Tag]> {
    stack
        .components
        .get(BANNER_PATTERNS)
        .and_then(Tag::as_list)
        .filter(|layers| !layers.is_empty())
}

fn item_name(stack: &ItemStack) -> &'static str {
    stack.item.kind().map_or("minecraft:air", ItemKind::name)
}

fn washed_banner(stack: &ItemStack) -> Option<ItemStack> {
    let layers = banner_layers(stack)?;
    let mut washed = ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    };
    let mut kept = layers.to_vec();
    kept.pop();
    if kept.is_empty() {
        washed.components.remove(BANNER_PATTERNS);
    } else {
        washed.components.insert(BANNER_PATTERNS, Tag::List(kept));
    }
    Some(washed)
}

fn washed_shulker(stack: &ItemStack) -> ItemStack {
    ItemStack {
        item: ItemKind::ShulkerBox.id(),
        count: NonZeroU8::MIN,
        components: stack.components.clone(),
    }
}

fn bucket_filled(full: ItemKind, sound: &'static str, level: u8) -> Option<Change> {
    (level == MAX_LEVEL).then(|| Change {
        state: BlockKind::Cauldron.default_state(),
        reason: Reason::BucketFill,
        sound: Some(sound),
        effect: Effect::Give(single(full)),
    })
}

impl<L: ClientLink> Server<L> {
    fn cauldron_plan(
        &self,
        pos: BlockPos,
        contents: Contents,
        player: PlayerId,
        held: &ItemStack,
    ) -> Plan {
        let above = self
            .world
            .chunks
            .block(pos.offset(0, 1, 0))
            .unwrap_or(BlockStateId::AIR);
        let emptied = || {
            if self.infinite_materials(player) {
                Effect::Nothing
            } else {
                Effect::Give(single(ItemKind::Bucket))
            }
        };
        let Some(kind) = held.item.kind() else {
            return Plan::Pass;
        };
        match kind {
            ItemKind::WaterBucket => Plan::Change(Change {
                state: water_level(MAX_LEVEL),
                reason: Reason::BucketEmpty,
                sound: Some(EMPTY_SOUND),
                effect: emptied(),
            }),
            ItemKind::LavaBucket if is_under_water(above) => Plan::Consume,
            ItemKind::LavaBucket => Plan::Change(Change {
                state: BlockKind::LavaCauldron.default_state(),
                reason: Reason::BucketEmpty,
                sound: Some(EMPTY_LAVA_SOUND),
                effect: emptied(),
            }),
            ItemKind::PowderSnowBucket if is_under_water(above) => Plan::Consume,
            ItemKind::PowderSnowBucket => Plan::Change(Change {
                state: snow_level(MAX_LEVEL),
                reason: Reason::BucketEmpty,
                sound: Some(EMPTY_POWDER_SNOW_SOUND),
                effect: emptied(),
            }),
            ItemKind::Bucket => match contents {
                Contents::Water(level) => bucket_filled(ItemKind::WaterBucket, FILL_SOUND, level),
                Contents::Lava => bucket_filled(ItemKind::LavaBucket, FILL_LAVA_SOUND, MAX_LEVEL),
                Contents::PowderSnow(level) => {
                    bucket_filled(ItemKind::PowderSnowBucket, FILL_POWDER_SNOW_SOUND, level)
                }
                Contents::Empty => None,
            }
            .map_or(Plan::Pass, Plan::Change),
            ItemKind::GlassBottle => match contents {
                Contents::Water(level) => Plan::Change(Change {
                    state: lowered_water(level),
                    reason: Reason::BottleFill,
                    sound: Some(BOTTLE_FILL_SOUND),
                    effect: Effect::Give(potion::stack(ItemKind::Potion, potion::WATER)),
                }),
                _ => Plan::Pass,
            },
            ItemKind::Potion if PotionContents::of(held).is(potion::WATER) => match contents {
                Contents::Empty => Plan::Change(Change {
                    state: water_level(1),
                    reason: Reason::BottleEmpty,
                    sound: Some(BOTTLE_EMPTY_SOUND),
                    effect: Effect::Give(single(ItemKind::GlassBottle)),
                }),
                Contents::Water(level) if level < MAX_LEVEL => Plan::Change(Change {
                    state: water_level(level + 1),
                    reason: Reason::BottleEmpty,
                    sound: Some(BOTTLE_EMPTY_SOUND),
                    effect: Effect::Give(single(ItemKind::GlassBottle)),
                }),
                _ => Plan::Pass,
            },
            _ => match contents {
                Contents::Water(level) => self.washing(held, level),
                _ => Plan::Pass,
            },
        }
    }

    fn washing(&self, held: &ItemStack, level: u8) -> Plan {
        let lowered = lowered_water(level);
        if item_tag(held.item, REMOVES_DYE) && held.components.contains_key(DYED_COLOR) {
            return Plan::Change(Change {
                state: lowered,
                reason: Reason::ArmorWash,
                sound: None,
                effect: Effect::Undye,
            });
        }
        let name = item_name(held);
        if name.ends_with(BANNER_SUFFIX)
            && let Some(washed) = washed_banner(held)
        {
            return Plan::Change(Change {
                state: lowered,
                reason: Reason::BannerWash,
                sound: None,
                effect: Effect::Give(washed),
            });
        }
        if name.ends_with(SHULKER_SUFFIX) {
            return Plan::Change(Change {
                state: lowered,
                reason: Reason::ShulkerWash,
                sound: None,
                effect: Effect::Give(washed_shulker(held)),
            });
        }
        Plan::Pass
    }

    pub(crate) fn use_cauldron(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        let (Some(contents), Some(held)) = (Contents::of(clicked), held) else {
            return false;
        };
        match self.cauldron_plan(pos, contents, id, held) {
            Plan::Pass => false,
            Plan::Consume => {
                self.player_swing(id, hand, false);
                self.ack_block(id, sequence);
                true
            }
            Plan::Change(change) => {
                let step = CauldronStep {
                    pos,
                    expected: clicked,
                    state: change.state,
                    reason: change.reason,
                    entity: self.players.get(&id).map(|player| player.entity_id),
                    extinguish: false,
                    by: Some(Handled {
                        player: id,
                        hand,
                        held: held.clone(),
                        sequence,
                        sound: change.sound,
                        effect: change.effect,
                    }),
                };
                self.fire_cauldron(Lane::Player(id), step);
                true
            }
        }
    }

    fn fire_cauldron(&mut self, lane: Lane, step: CauldronStep) {
        let (pos, state, reason, entity) = (step.pos, step.state, step.reason, step.entity);
        self.fire(
            lane,
            Continuation::Cauldron(Box::new(step)),
            move |server| {
                Some(EventPayload::CauldronLevelChange(CauldronLevelChange {
                    block: pos,
                    entity: entity.and_then(|id| server.subject_view(id)),
                    new_state: snapshot_of(pos, state),
                    reason,
                }))
            },
        );
    }

    pub(crate) fn finish_cauldron(&mut self, step: CauldronStep, cancelled: bool) {
        let CauldronStep {
            pos,
            expected,
            state,
            entity,
            extinguish,
            by,
            reason,
            ..
        } = step;
        let intact = !cancelled
            && self.world.chunks.block(pos) == Some(expected)
            && by
                .as_ref()
                .is_none_or(|by| self.held(by.player, by.hand).as_ref() == Some(&by.held));
        if intact {
            self.set_block(pos, state);
            if let Some(sound) = by.as_ref().and_then(|by| by.sound) {
                self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
            }
            if extinguish && let Some(entity) = entity {
                self.extinguish_entity(entity);
            }
        }
        let Some(by) = by else {
            return;
        };
        if intact {
            self.count_cauldron_use(by.player, reason);
            match by.effect {
                Effect::Give(result) => {
                    self.deliver_filled_bucket(by.player, by.hand, Some(result))
                }
                Effect::Undye => {
                    let mut washed = by.held;
                    washed.components.remove(DYED_COLOR);
                    self.set_held(by.player, by.hand, Some(washed));
                }
                Effect::Nothing => {}
            }
            self.refresh_hands(by.player);
        }
        self.player_swing(by.player, by.hand, false);
        self.ack_block(by.player, by.sequence);
    }

    pub(crate) fn cauldron_extinguishes(&mut self, entity: EntityId, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let next = match Contents::of(state) {
            Some(Contents::Water(level) | Contents::PowderSnow(level)) => lowered_water(level),
            _ => return,
        };
        let protected = self
            .players
            .values()
            .find(|player| player.entity_id == entity)
            .is_some_and(|player| self.is_protected(player.id, pos));
        if protected {
            return;
        }
        let step = CauldronStep {
            pos,
            expected: state,
            state: next,
            reason: Reason::Extinguish,
            entity: Some(entity),
            extinguish: true,
            by: None,
        };
        self.fire_cauldron(Lane::Block(pos), step);
    }

    pub(crate) fn cauldron_precipitation(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let Some(contents) = Contents::of(state) else {
            return;
        };
        if contents == Contents::Lava {
            return;
        }
        let snow = match self.climate_at(pos) {
            Some((true, _)) => !self.warm_enough_to_rain(pos),
            _ => return,
        };
        let chance = if snow {
            SNOW_FILL_CHANCE
        } else {
            RAIN_FILL_CHANCE
        };
        if self.rng.random::<f32>() >= chance {
            return;
        }
        let next = match contents {
            Contents::Empty if snow => {
                self.set_block(pos, BlockKind::PowderSnowCauldron.default_state());
                return;
            }
            Contents::Empty => {
                self.set_block(pos, BlockKind::WaterCauldron.default_state());
                return;
            }
            Contents::Water(level) if !snow && level < MAX_LEVEL => water_level(level + 1),
            Contents::PowderSnow(level) if snow && level < MAX_LEVEL => snow_level(level + 1),
            _ => return,
        };
        let step = CauldronStep {
            pos,
            expected: state,
            state: next,
            reason: Reason::NaturalFill,
            entity: None,
            extinguish: false,
            by: None,
        };
        self.fire_cauldron(Lane::Block(pos), step);
    }
}
