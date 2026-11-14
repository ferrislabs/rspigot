use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::SoundSource;

use crate::crops::is_bone_meal;
use crate::events::Hand;
use crate::plugins::events::{EquipmentSlot, PlayerHarvestBlock, PlayerHarvestBlockPatch};
use crate::plugins::steps::{Grief, HarvestStep};
use crate::plugins::{Continuation, EventPayload, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;

const BERRY_TABLE: &str = "minecraft:harvest/sweet_berry_bush";
const GLOW_TABLE: &str = "minecraft:harvest/cave_vine";
const BERRY_SOUND: &str = "minecraft:block.sweet_berry_bush.pick_berries";
const GLOW_SOUND: &str = "minecraft:block.cave_vines.pick_berries";
const RIPE_AGE: i32 = 3;
const PICKABLE_AGE: i32 = 2;
const PICKED_AGE: &str = "1";

fn bush_age(state: BlockStateId) -> Option<i32> {
    state.property("age")?.parse().ok()
}

fn is_cave_vine(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::CaveVines | BlockKind::CaveVinesPlant)
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_berries(
        &mut self,
        id: PlayerId,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        let handled = if clicked.kind() == Some(BlockKind::SweetBerryBush) {
            let Some(age) = bush_age(clicked) else {
                return false;
            };
            let bone_meal = held.is_some_and(|stack| is_bone_meal(stack.item));
            if age < PICKABLE_AGE || (bone_meal && age != RIPE_AGE) {
                return false;
            }
            self.ask_harvest(id, pos, clicked, false);
            true
        } else if is_cave_vine(clicked) && clicked.property("berries") == Some("true") {
            self.pick_glow_berries(id, pos, clicked);
            true
        } else {
            false
        };
        if handled {
            self.player_swing(id, Hand::Main, false);
            self.ack_block(id, sequence);
        }
        handled
    }

    fn pick_glow_berries(&mut self, id: PlayerId, pos: BlockPos, clicked: BlockStateId) {
        let Some(to) = clicked.with_property("berries", "false") else {
            return;
        };
        let Some(view) = self
            .players
            .get(&id)
            .and_then(|player| self.subject_view(player.entity_id))
        else {
            return;
        };
        self.change_block(view, pos, to, Grief::Vines { player: id, pos }, false);
    }

    pub(crate) fn finish_vines(&mut self, player: PlayerId, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if is_cave_vine(state) && state.property("berries") == Some("true") {
            self.ask_harvest(player, pos, state, true);
        }
    }

    fn ask_harvest(&mut self, id: PlayerId, pos: BlockPos, state: BlockStateId, glow: bool) {
        let table = rspigot_data::loot::table(if glow { GLOW_TABLE } else { BERRY_TABLE });
        let drops = table
            .map(|table| {
                crate::loot::table_drops(table, &crate::loot::LootContext::default(), &mut self.rng)
            })
            .unwrap_or_default();
        let items = drops.iter().map(spec_of).collect::<Vec<_>>();
        let step = HarvestStep {
            pos,
            state,
            drops,
            glow,
        };
        self.fire(id, Continuation::Harvest(Box::new(step)), move |server| {
            Some(EventPayload::PlayerHarvestBlock(PlayerHarvestBlock {
                hand: EquipmentSlot::Hand,
                harvested_block: pos,
                items_harvested: items,
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(crate) fn finish_harvest(
        &mut self,
        step: HarvestStep,
        cancelled: bool,
        patch: PlayerHarvestBlockPatch,
    ) {
        if cancelled || self.world.chunks.block(step.pos) != Some(step.state) {
            return;
        }
        let specs = patch
            .items_harvested
            .apply(step.drops.iter().map(spec_of).collect());
        for spec in &specs {
            if let Ok(Some(stack)) = stack_of(spec) {
                self.spawn_item(step.pos, stack);
            }
        }
        let pitch = 0.8 + self.rng.random::<f32>() * 0.4;
        let at = Vec3::new(
            f64::from(step.pos.x) + 0.5,
            f64::from(step.pos.y) + 0.5,
            f64::from(step.pos.z) + 0.5,
        );
        let sound = if step.glow { GLOW_SOUND } else { BERRY_SOUND };
        self.play_sound(None, at, sound, SoundSource::Blocks, 1.0, pitch);
        let (property, value) = if step.glow {
            ("berries", "false")
        } else {
            ("age", PICKED_AGE)
        };
        if let Some(next) = step.state.with_property(property, value) {
            self.set_block(step.pos, next);
        }
    }
}
