use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_nbt::Compound;

use crate::advancements::AdvancementEvent;
use crate::beacon_beam::sections as beam_sections;
use crate::events::ClientEvent;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const LEVELS: &str = "Levels";
const PRIMARY: &str = "primary_effect";
const SECONDARY: &str = "secondary_effect";
const BASE_BLOCKS: &str = "minecraft:beacon_base_blocks";
const MAX_LEVELS: i32 = 4;
const SECONDARY_LEVELS: i32 = 4;
const RECHECK_TICKS: i64 = 80;
const HALF_WIDTH: f64 = 0.3;
const PLAYER_HEIGHT: f64 = 1.8;
const ACTIVATE_SOUND: &str = "minecraft:block.beacon.activate";
const DEACTIVATE_SOUND: &str = "minecraft:block.beacon.deactivate";
const AMBIENT_SOUND: &str = "minecraft:block.beacon.ambient";
const LEVELS_DATA: i16 = 0;
const PRIMARY_DATA: i16 = 1;
const SECONDARY_DATA: i16 = 2;
const TIERS: [&[&str]; 4] = [
    &["minecraft:speed", "minecraft:haste"],
    &["minecraft:resistance", "minecraft:jump_boost"],
    &["minecraft:strength"],
    &["minecraft:regeneration"],
];

pub(crate) fn is_beacon(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Beacon)
}

fn required_levels(effect: Option<&str>) -> i32 {
    let Some(effect) = effect else {
        return 0;
    };
    TIERS
        .iter()
        .position(|tier| tier.contains(&effect))
        .map_or(i32::MAX, |index| index as i32 + 1)
}

fn valid_choice(primary: Option<&str>, secondary: Option<&str>, levels: i32) -> bool {
    if secondary.is_some() && levels < SECONDARY_LEVELS {
        return false;
    }
    let needed_primary = required_levels(primary);
    let needed_secondary = required_levels(secondary);
    if needed_primary > levels || needed_secondary > levels || needed_primary >= MAX_LEVELS {
        return false;
    }
    needed_secondary == 0 || needed_secondary >= MAX_LEVELS || primary == secondary
}

fn encode(effect: Option<&str>) -> i16 {
    effect
        .and_then(EffectId::from_name)
        .and_then(|id| i16::try_from(id.0).ok())
        .map_or(0, |id| id + 1)
}

fn name_of(id: i32) -> Option<&'static str> {
    u16::try_from(id).ok().and_then(|id| EffectId(id).name())
}

impl<L: ClientLink> Server<L> {
    fn beacon_data(&self, pos: BlockPos) -> Option<&Compound> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| &entity.data)
    }

    fn beacon_effect(&self, pos: BlockPos, key: &str) -> Option<&str> {
        self.beacon_data(pos)
            .and_then(|data| data.get_str(key))
            .filter(|name| required_levels(Some(name)) != i32::MAX)
    }

    fn pyramid_levels(&self, pos: BlockPos) -> i32 {
        let floor = self.world.dimension.height.min_y;
        let mut levels = 0;
        for step in 1..=MAX_LEVELS {
            let y = pos.y - step;
            if y < floor {
                break;
            }
            let complete = (-step..=step).all(|dx| {
                (-step..=step).all(|dz| {
                    self.world
                        .chunks
                        .block(BlockPos::new(pos.x + dx, y, pos.z + dz))
                        .is_some_and(|state| has_tag(state, BASE_BLOCKS))
                })
            });
            if !complete {
                break;
            }
            levels = step;
        }
        levels
    }

    fn store_beacon(
        &mut self,
        pos: BlockPos,
        levels: i32,
        primary: Option<&str>,
        secondary: Option<&str>,
    ) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(pos)
            .map_or_else(Compound::new, |existing| existing.data.clone());
        data.insert(LEVELS, levels);
        for (key, effect) in [(PRIMARY, primary), (SECONDARY, secondary)] {
            match effect {
                Some(name) => {
                    data.insert(key, name);
                }
                None => {
                    data.remove(key);
                }
            }
        }
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft("beacon"),
            data,
        });
    }

    pub(crate) fn beacon_menu_data(&mut self, pos: BlockPos) -> Vec<i16> {
        let levels = self.pyramid_levels(pos);
        let primary = self.beacon_effect(pos, PRIMARY).map(str::to_owned);
        let secondary = self.beacon_effect(pos, SECONDARY).map(str::to_owned);
        let stored = self.stored_levels(pos);
        self.store_beacon(pos, stored, primary.as_deref(), secondary.as_deref());
        vec![
            i16::try_from(levels).unwrap_or(0),
            encode(primary.as_deref()),
            encode(secondary.as_deref()),
        ]
    }

    fn stored_levels(&self, pos: BlockPos) -> i32 {
        self.beacon_data(pos)
            .and_then(|data| data.get_i32(LEVELS))
            .unwrap_or(0)
    }

    fn beam_is_clear(&self, pos: BlockPos) -> bool {
        let height = self.world.dimension.height;
        let top = height.min_y + i32::try_from(height.height).unwrap_or(0) - 1;
        let column = (pos.y..=top).map(|y| {
            self.world
                .chunks
                .block(BlockPos::new(pos.x, y, pos.z))
                .unwrap_or(BlockStateId::AIR)
        });
        !beam_sections(column).is_empty()
    }

    pub(crate) fn recheck_beacon_levels(&mut self) {
        if self.world.level.game_time % RECHECK_TICKS != 0 {
            return;
        }
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:beacon")
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            self.beat_beacon(pos);
        }
    }

    fn beat_beacon(&mut self, pos: BlockPos) {
        if !self.world.chunks.block(pos).is_some_and(is_beacon) {
            return;
        }
        let stored = self.stored_levels(pos);
        let levels = if self.beam_is_clear(pos) {
            self.pyramid_levels(pos)
        } else {
            0
        };
        let primary = self.beacon_effect(pos, PRIMARY).map(str::to_owned);
        let secondary = self.beacon_effect(pos, SECONDARY).map(str::to_owned);
        if levels != stored {
            self.store_beacon(pos, levels, primary.as_deref(), secondary.as_deref());
            self.send_beacon_update(pos);
            self.send_menu_data(pos, LEVELS_DATA, i16::try_from(levels).unwrap_or(0));
        }
        if levels > 0 {
            self.apply_beacon_effects(pos, levels, primary.as_deref(), secondary.as_deref());
            self.play_beacon_sound(pos, AMBIENT_SOUND);
        }
        if stored == 0 && levels > 0 {
            self.play_beacon_sound(pos, ACTIVATE_SOUND);
        } else if stored > 0 && levels == 0 {
            self.play_beacon_sound(pos, DEACTIVATE_SOUND);
        }
        if (stored == 0 && levels > 0) || (stored < MAX_LEVELS && levels >= MAX_LEVELS) {
            self.construct_beacon(pos, levels);
        }
    }

    fn send_beacon_update(&mut self, pos: BlockPos) {
        let Some(data) = self.beacon_data(pos).cloned() else {
            return;
        };
        self.push(
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos,
                kind: "minecraft:beacon",
                data,
            },
        );
    }

    fn play_beacon_sound(&mut self, pos: BlockPos, sound: &'static str) {
        self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
    }

    fn players_overlapping(&self, low: Vec3, high: Vec3) -> Vec<PlayerId> {
        self.players
            .values()
            .filter(|player| {
                let at = player.position;
                at.x + HALF_WIDTH > low.x
                    && at.x - HALF_WIDTH < high.x
                    && at.z + HALF_WIDTH > low.z
                    && at.z - HALF_WIDTH < high.z
                    && at.y + PLAYER_HEIGHT > low.y
                    && at.y < high.y
            })
            .map(|player| player.id)
            .collect()
    }

    fn apply_beacon_effects(
        &mut self,
        pos: BlockPos,
        levels: i32,
        primary: Option<&str>,
        secondary: Option<&str>,
    ) {
        let Some(primary) = primary.and_then(EffectId::from_name) else {
            return;
        };
        let secondary = secondary.and_then(EffectId::from_name);
        let range = f64::from(levels * 10 + 10);
        let height = f64::from(self.world.dimension.height.height);
        let low = Vec3::new(
            f64::from(pos.x) - range,
            f64::from(pos.y) - range,
            f64::from(pos.z) - range,
        );
        let high = Vec3::new(
            f64::from(pos.x) + 1.0 + range,
            f64::from(pos.y) + 1.0 + range + height,
            f64::from(pos.z) + 1.0 + range,
        );
        let duration = EffectDuration::Ticks(u32::try_from((9 + levels * 2) * 20).unwrap_or(0));
        let doubled = levels >= MAX_LEVELS && Some(primary) == secondary;
        let boosted = EffectInstance {
            ambient: true,
            ..EffectInstance::new(primary, duration, u8::from(doubled), true)
        };
        let regeneration = secondary
            .filter(|second| levels >= MAX_LEVELS && *second != primary)
            .map(|second| EffectInstance {
                ambient: true,
                ..EffectInstance::new(second, duration, 0, true)
            });
        for player in self.players_overlapping(low, high) {
            self.apply_effect(player, boosted, Some(EntityPotionEffectCause::Beacon));
            if let Some(second) = regeneration {
                self.apply_effect(player, second, Some(EntityPotionEffectCause::Beacon));
            }
        }
    }

    fn construct_beacon(&mut self, pos: BlockPos, levels: i32) {
        let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        let low = Vec3::new(x - 10.0, y - 9.0, z - 10.0);
        let high = Vec3::new(x + 10.0, y + 5.0, z + 10.0);
        for player in self.players_overlapping(low, high) {
            let mut event = AdvancementEvent::named("minecraft:construct_beacon");
            event.amounts.push(("level", f64::from(levels)));
            self.trigger_event(player, event);
        }
    }

    pub(crate) fn handle_set_beacon(
        &mut self,
        id: PlayerId,
        primary: Option<i32>,
        secondary: Option<i32>,
    ) {
        let Some(open) = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .filter(|open| open.backing.block == BlockKind::Beacon)
        else {
            return;
        };
        let pos = open.backing.pos;
        let paid = open.local.first().is_some_and(Option::is_some);
        let chosen_primary = primary.map(name_of);
        let chosen_secondary = secondary.map(name_of);
        if !paid || chosen_primary == Some(None) || chosen_secondary == Some(None) {
            return;
        }
        let (primary, secondary) = (chosen_primary.flatten(), chosen_secondary.flatten());
        let levels = self.pyramid_levels(pos);
        if !valid_choice(primary, secondary, levels) {
            return;
        }
        let stored = self.stored_levels(pos);
        self.store_beacon(pos, stored, primary, secondary);
        self.use_up_payment(id);
        self.send_menu_data(pos, PRIMARY_DATA, encode(primary));
        self.send_menu_data(pos, SECONDARY_DATA, encode(secondary));
    }
}
