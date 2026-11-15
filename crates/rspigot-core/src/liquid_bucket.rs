use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Rotation, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::bucket::{FIZZ_SOUND, Placement, can_waterlog, slot_of};
use crate::events::{ClientEvent, Hand};
use crate::plugins::events::{PlayerBucketEmpty, PlayerBucketFill, PlayerBucketFillPatch};
use crate::plugins::steps::{BucketFillStep, LiquidEmptyStep};
use crate::plugins::{Continuation, EventPayload, direction_of, plugin_face_of, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const FILL_SOUND: &str = "minecraft:item.bucket.fill";
pub(crate) const FILL_LAVA_SOUND: &str = "minecraft:item.bucket.fill_lava";
pub(crate) const FILL_POWDER_SNOW_SOUND: &str = "minecraft:item.bucket.fill_powder_snow";
pub(crate) const EMPTY_SOUND: &str = "minecraft:item.bucket.empty";
pub(crate) const EMPTY_LAVA_SOUND: &str = "minecraft:item.bucket.empty_lava";
pub(crate) const EMPTY_POWDER_SNOW_SOUND: &str = "minecraft:item.bucket.empty_powder_snow";
const WATERLOGGED: &str = "waterlogged";

pub(crate) struct Pickup {
    pub(crate) bucket: ItemKind,
    pub(crate) after: BlockStateId,
    sound: &'static str,
}

pub(crate) fn pickup_of(state: BlockStateId) -> Option<Pickup> {
    let source = state.property("level") == Some("0");
    match state.kind()? {
        BlockKind::Water if source => Some(Pickup {
            bucket: ItemKind::WaterBucket,
            after: BlockStateId::AIR,
            sound: FILL_SOUND,
        }),
        BlockKind::Lava if source => Some(Pickup {
            bucket: ItemKind::LavaBucket,
            after: BlockStateId::AIR,
            sound: FILL_LAVA_SOUND,
        }),
        BlockKind::PowderSnow => Some(Pickup {
            bucket: ItemKind::PowderSnowBucket,
            after: BlockStateId::AIR,
            sound: FILL_POWDER_SNOW_SOUND,
        }),
        BlockKind::BubbleColumn => Some(Pickup {
            bucket: ItemKind::WaterBucket,
            after: BlockStateId::AIR,
            sound: FILL_SOUND,
        }),
        _ if state.property(WATERLOGGED) == Some("true") => Some(Pickup {
            bucket: ItemKind::WaterBucket,
            after: state.with_property(WATERLOGGED, "false")?,
            sound: FILL_SOUND,
        }),
        _ => None,
    }
}

pub(crate) fn single(item: ItemKind) -> ItemStack {
    ItemStack::new(item.id(), NonZeroU8::MIN)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_liquid_bucket(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        match stack.item.kind() {
            Some(ItemKind::Bucket) => self.use_empty_bucket(id, hand, rotation, &stack),
            Some(ItemKind::WaterBucket) => self.use_full_bucket(id, hand, rotation, &stack, false),
            Some(ItemKind::LavaBucket) => self.use_full_bucket(id, hand, rotation, &stack, true),
            _ => false,
        }
    }

    fn bucket_refused(&self, id: PlayerId, pos: BlockPos, relative: BlockPos) -> bool {
        let adventure = self
            .players
            .get(&id)
            .is_some_and(|player| player.game_mode == GameMode::Adventure);
        adventure || self.is_protected(id, pos) || self.is_protected(id, relative)
    }

    fn use_empty_bucket(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
        stack: &ItemStack,
    ) -> bool {
        let Some(hit) = self.look_source(id, rotation) else {
            return false;
        };
        let (dx, dy, dz) = direction_of(hit.face).offset();
        let relative = hit.pos.offset(dx, dy, dz);
        if self.bucket_refused(id, hit.pos, relative) {
            return true;
        }
        let Some(before) = self.world.chunks.block(hit.pos) else {
            return true;
        };
        let Some(pickup) = pickup_of(before) else {
            return true;
        };
        let filled = single(pickup.bucket);
        let spec = spec_of(&filled);
        let step = BucketFillStep {
            player: id,
            hand,
            stack: stack.clone(),
            pos: hit.pos,
            before,
            after: pickup.after,
            filled,
            sound: pickup.sound,
        };
        let (pos, face) = (hit.pos, plugin_face_of(hit.face));
        self.fire(
            id,
            Continuation::BucketFill(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerBucketFill(PlayerBucketFill {
                    block: pos,
                    block_clicked: pos,
                    block_face: face,
                    bucket: ItemKind::Bucket.name().to_owned(),
                    hand: slot_of(hand),
                    item_stack: Some(spec),
                    player: server.snapshot_of(id)?,
                }))
            },
        );
        true
    }

    pub(crate) fn finish_bucket_fill(
        &mut self,
        step: BucketFillStep,
        cancelled: bool,
        patch: PlayerBucketFillPatch,
    ) {
        let BucketFillStep {
            player,
            hand,
            stack,
            pos,
            before,
            after,
            filled,
            sound,
        } = step;
        if cancelled {
            self.send_inventory(player);
            self.push(
                Audience::One(player),
                ClientEvent::BlockUpdate { pos, state: before },
            );
            return;
        }
        if self.held(player, hand).as_ref() != Some(&stack)
            || self.world.chunks.block(pos) != Some(before)
        {
            return;
        }
        self.set_block(pos, after);
        if let Some(position) = self.players.get(&player).map(|p| p.position) {
            self.play_sound(
                Some(player),
                position,
                sound,
                SoundSource::Players,
                1.0,
                1.0,
            );
        }
        let result = match patch.item_stack {
            Some(spec) => spec.and_then(|spec| stack_of(&spec).ok().flatten()),
            None => Some(filled),
        };
        self.deliver_filled_bucket(player, hand, result);
    }

    pub(crate) fn deliver_filled_bucket(
        &mut self,
        player: PlayerId,
        hand: Hand,
        result: Option<ItemStack>,
    ) {
        if self.infinite_materials(player) {
            let Some(result) = result else {
                return;
            };
            let owned = self.players.get(&player).is_some_and(|p| {
                p.inventory
                    .slots()
                    .iter()
                    .flatten()
                    .any(|held| held.item == result.item)
            });
            if !owned {
                self.give_or_drop(player, result);
            }
            return;
        }
        self.consume_held(player, hand, 1);
        match result {
            Some(result) if self.held(player, hand).is_none() => {
                self.set_held(player, hand, Some(result));
            }
            Some(result) => self.give_or_drop(player, result),
            None => {}
        }
    }

    fn use_full_bucket(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
        stack: &ItemStack,
        lava: bool,
    ) -> bool {
        let Some(hit) = self.look_block(id, rotation) else {
            return false;
        };
        let (dx, dy, dz) = direction_of(hit.face).offset();
        let relative = hit.pos.offset(dx, dy, dz);
        let Some(clicked) = self.world.chunks.block(hit.pos) else {
            return true;
        };
        if self.bucket_refused(id, hit.pos, relative) {
            return true;
        }
        let target = if !lava && can_waterlog(clicked) {
            hit.pos
        } else {
            relative
        };
        let Some((block, placement)) = self.empty_contents(target, Some((hit.pos, hit.face)), lava)
        else {
            return true;
        };
        let step = LiquidEmptyStep {
            player: id,
            hand,
            stack: stack.clone(),
            lava,
            block,
            placement,
        };
        let name = stack.item.kind().map_or("minecraft:air", ItemKind::name);
        let (clicked_pos, face) = (hit.pos, plugin_face_of(hit.face));
        let result = spec_of(&single(ItemKind::Bucket));
        self.fire(
            id,
            Continuation::LiquidEmpty(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerBucketEmpty(PlayerBucketEmpty {
                    block,
                    block_clicked: clicked_pos,
                    block_face: face,
                    bucket: name.to_owned(),
                    hand: slot_of(hand),
                    item_stack: Some(result),
                    player: server.snapshot_of(id)?,
                }))
            },
        );
        true
    }

    pub(crate) fn finish_liquid_empty(&mut self, step: LiquidEmptyStep, cancelled: bool) {
        let LiquidEmptyStep {
            player,
            hand,
            stack,
            lava,
            block,
            placement,
        } = step;
        if cancelled {
            self.send_inventory(player);
            if let Some(state) = self.world.chunks.block(block) {
                self.push(
                    Audience::One(player),
                    ClientEvent::BlockUpdate { pos: block, state },
                );
            }
            return;
        }
        if self.held(player, hand).as_ref() != Some(&stack) {
            return;
        }
        let position = Vec3::new(
            f64::from(block.x) + 0.5,
            f64::from(block.y),
            f64::from(block.z) + 0.5,
        );
        let sound = if lava { EMPTY_LAVA_SOUND } else { EMPTY_SOUND };
        match placement {
            Placement::Evaporate => {
                self.play_sound(
                    Some(player),
                    position,
                    FIZZ_SOUND,
                    SoundSource::Blocks,
                    0.5,
                    2.6,
                );
            }
            Placement::Waterlog => {
                if let Some(state) = self
                    .world
                    .chunks
                    .block(block)
                    .and_then(|state| state.with_property(WATERLOGGED, "true"))
                {
                    self.set_block(block, state);
                }
                self.play_sound(Some(player), position, sound, SoundSource::Blocks, 1.0, 1.0);
            }
            Placement::Place { destroy } => {
                if destroy {
                    self.break_block_naturally(block);
                }
                let liquid = if lava {
                    BlockKind::Lava
                } else {
                    BlockKind::Water
                };
                self.set_block(block, liquid.default_state());
                self.play_sound(Some(player), position, sound, SoundSource::Blocks, 1.0, 1.0);
            }
        }
        if !self.infinite_materials(player) {
            self.fill_bucket_in_hand(player, hand, single(ItemKind::Bucket));
        }
    }
}
