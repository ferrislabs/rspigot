use std::num::NonZeroU8;

use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_items::ItemKind;

use crate::events::{Direction, Hand};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const CORAL_DELAY_BASE: u64 = 60;
const CORAL_DELAY_SPREAD: u64 = 40;
const BUDDING_ODDS: i32 = 5;
const BUBBLE_RECHECK_DELAY: u64 = 5;
const MUSHROOM_ODDS: i32 = 25;
const MUSHROOM_CAP: i32 = 5;
const PUMPKIN_SEEDS: u8 = 4;
const EGG_ATTEMPTS: i32 = 1000;

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

pub(crate) fn dead_coral_of(kind: BlockKind) -> Option<BlockKind> {
    let path = kind.name().strip_prefix("minecraft:")?;
    if path.starts_with("dead_") {
        return None;
    }
    let coral = ["_coral", "_coral_fan", "_coral_wall_fan", "_coral_block"]
        .iter()
        .any(|suffix| path.ends_with(suffix));
    if !coral {
        return None;
    }
    BlockKind::from_name(&format!("minecraft:dead_{path}"))
}

fn holds_water(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Water) || flag(state, "waterlogged")
}

pub(crate) fn is_ore(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::RedstoneOre | BlockKind::DeepslateRedstoneOre
    )
}

fn bud_after(state: BlockStateId, toward: Face) -> Option<BlockKind> {
    let faces = state.property("facing") == Some(toward.name());
    match state.kind()? {
        BlockKind::SmallAmethystBud if faces => Some(BlockKind::MediumAmethystBud),
        BlockKind::MediumAmethystBud if faces => Some(BlockKind::LargeAmethystBud),
        BlockKind::LargeAmethystBud if faces => Some(BlockKind::AmethystCluster),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    fn coral_has_water(&self, pos: BlockPos, state: BlockStateId) -> bool {
        holds_water(state)
            || Face::ALL
                .into_iter()
                .any(|face| holds_water(self.block_or_air(step(pos, face))))
    }

    pub(crate) fn coral_check(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        if !self.coral_has_water(pos, state) {
            let delay = CORAL_DELAY_BASE + self.rng.random_range(0..CORAL_DELAY_SPREAD);
            self.schedule(pos, kind, delay);
        }
    }

    pub(crate) fn coral_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(dead) = state.kind().and_then(dead_coral_of) else {
            return;
        };
        if self.coral_has_water(pos, state) {
            return;
        }
        let mut next = dead.default_state();
        if let Some(facing) = state.property("facing") {
            next = next.with_property("facing", facing).unwrap_or(next);
        }
        next = next.with_property("waterlogged", "false").unwrap_or(next);
        self.set_block(pos, next);
    }

    pub(crate) fn redstone_ore_interact(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if state.kind().is_some_and(is_ore)
            && !flag(state, "lit")
            && let Some(lit) = state.with_property("lit", "true")
        {
            self.set_block(pos, lit);
        }
    }

    pub(crate) fn redstone_ore_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "lit")
            && let Some(dim) = state.with_property("lit", "false")
        {
            self.set_block(pos, dim);
        }
    }

    pub(crate) fn use_redstone_ore(
        &mut self,
        held: Option<&rspigot_domain::item::ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        if !clicked.kind().is_some_and(is_ore) {
            return false;
        }
        self.redstone_ore_interact(pos);
        held.is_none_or(|stack| BlockKind::from_item(stack.item.0).is_none())
    }

    pub(crate) fn mushroom_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.rng.random_range(0..MUSHROOM_ODDS) != 0 {
            return;
        }
        let mut left = MUSHROOM_CAP;
        for dx in -4..=4 {
            for dy in -1..=1 {
                for dz in -4..=4 {
                    if self.block_or_air(pos.offset(dx, dy, dz)).kind() == state.kind() {
                        left -= 1;
                        if left <= 0 {
                            return;
                        }
                    }
                }
            }
        }
        let mut from = pos;
        let mut target = self.mushroom_hop(from);
        for _ in 0..4 {
            if self.block_or_air(target).kind() == Some(BlockKind::Air)
                && self.can_survive_at(state, target)
            {
                from = target;
            }
            target = self.mushroom_hop(from);
        }
        if self.block_or_air(target).kind() == Some(BlockKind::Air)
            && self.can_survive_at(state, target)
        {
            self.set_block(target, state);
        }
    }

    fn mushroom_hop(&mut self, from: BlockPos) -> BlockPos {
        let dx = self.rng.random_range(0..3) - 1;
        let dy = self.rng.random_range(0..2) - self.rng.random_range(0..2);
        let dz = self.rng.random_range(0..3) - 1;
        from.offset(dx, dy, dz)
    }

    pub(crate) fn nylium_random_tick(&mut self, pos: BlockPos) {
        let above = self.block_or_air(step(pos, Face::Up));
        if above
            .flags()
            .contains(rspigot_blocks::StateFlags::SOLID_RENDER)
        {
            self.set_block(pos, BlockKind::Netherrack.default_state());
        }
    }

    pub(crate) fn use_pumpkin_shears(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        face: Direction,
    ) -> bool {
        if clicked.kind() != Some(BlockKind::Pumpkin)
            || !held.is_some_and(|stack| stack.item == ItemKind::Shears.id())
        {
            return false;
        }
        let toward = match face {
            Direction::Down => Face::Down,
            Direction::Up => Face::Up,
            Direction::North => Face::North,
            Direction::South => Face::South,
            Direction::West => Face::West,
            Direction::East => Face::East,
        };
        let facing = if toward.is_horizontal() {
            toward
        } else {
            let yaw = self
                .players
                .get(&id)
                .map_or(0.0, |player| player.rotation.yaw);
            Face::HORIZONTAL
                .into_iter()
                .find(|candidate| candidate.name() == crate::copper_statue::facing_of_yaw(yaw))
                .map_or(Face::North, Face::opposite)
        };
        let carved = BlockKind::CarvedPumpkin
            .state_with(&[("facing", facing.name())])
            .unwrap_or_else(|| BlockKind::CarvedPumpkin.default_state());
        self.set_block(pos, carved);
        if let Some(count) = NonZeroU8::new(PUMPKIN_SEEDS) {
            self.spawn_item(
                step(pos, toward),
                ItemStack::new(ItemKind::PumpkinSeeds.id(), count),
            );
        }
        self.hurt_held(id, hand, 1);
        true
    }

    pub(crate) fn dragon_egg_teleport(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::DragonEgg) {
            return;
        }
        for _ in 0..EGG_ATTEMPTS {
            let dx = self.rng.random_range(0..16) - self.rng.random_range(0..16);
            let dy = self.rng.random_range(0..8) - self.rng.random_range(0..8);
            let dz = self.rng.random_range(0..16) - self.rng.random_range(0..16);
            let target = pos.offset(dx, dy, dz);
            if self.block_or_air(target).kind() == Some(BlockKind::Air)
                && self.world.dimension.height.contains(target.y)
            {
                self.set_block(target, state);
                self.set_block(pos, BlockStateId::AIR);
                return;
            }
        }
    }

    pub(crate) fn use_dragon_egg(&mut self, pos: BlockPos, clicked: BlockStateId) -> bool {
        if clicked.kind() != Some(BlockKind::DragonEgg) {
            return false;
        }
        self.dragon_egg_teleport(pos);
        true
    }

    pub(crate) fn ice_melt(
        &mut self,
        pos: BlockPos,
        broken: BlockStateId,
        survival: bool,
        tool: Option<&ItemStack>,
    ) {
        if broken.kind() != Some(BlockKind::Ice) || !survival || self.water_evaporates() {
            return;
        }
        let silk = tool.is_some_and(|stack| {
            stack
                .enchantments()
                .iter()
                .any(|(name, level)| *level > 0 && name == "minecraft:silk_touch")
        });
        let below = self.block_or_air(step(pos, Face::Down));
        let supported = below
            .flags()
            .contains(rspigot_blocks::StateFlags::BLOCKS_MOTION)
            || below.flags().has_fluid();
        if !silk && supported {
            self.set_block(pos, BlockKind::Water.default_state());
        }
    }

    pub(crate) fn use_light_block(
        &mut self,
        id: PlayerId,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        if clicked.kind() != Some(BlockKind::Light)
            || !held.is_some_and(|stack| stack.item == ItemKind::Light.id())
            || !self.infinite_materials(id)
        {
            return false;
        }
        let level: i32 = clicked
            .property("level")
            .and_then(|level| level.parse().ok())
            .unwrap_or(15);
        if let Some(next) = clicked.with_property("level", &((level + 1) % 16).to_string()) {
            self.set_block(pos, next);
        }
        true
    }
    pub(crate) fn budding_amethyst_tick(&mut self, pos: BlockPos) {
        if self.rng.random_range(0..BUDDING_ODDS) != 0 {
            return;
        }
        let toward = Face::ALL[self.rng.random_range(0..Face::ALL.len())];
        let at = step(pos, toward);
        let present = self.block_or_air(at);
        let source_water =
            present.kind() == Some(BlockKind::Water) && present.property("level") == Some("0");
        let next = if present.kind() == Some(BlockKind::Air) || source_water {
            Some(BlockKind::SmallAmethystBud)
        } else {
            bud_after(present, toward)
        };
        let Some(kind) = next else {
            return;
        };
        let wet = if source_water || flag(present, "waterlogged") {
            "true"
        } else {
            "false"
        };
        if let Some(state) = kind.state_with(&[("facing", toward.name()), ("waterlogged", wet)]) {
            self.set_block(at, state);
        }
    }

    fn bubble_source(&self, pos: BlockPos) -> BlockStateId {
        let below = self.block_or_air(pos);
        match below.kind() {
            Some(BlockKind::BubbleColumn) => below,
            Some(BlockKind::SoulSand) => BlockKind::BubbleColumn
                .state_with(&[("drag", "false")])
                .unwrap_or_else(|| BlockKind::BubbleColumn.default_state()),
            Some(BlockKind::MagmaBlock) => BlockKind::BubbleColumn
                .state_with(&[("drag", "true")])
                .unwrap_or_else(|| BlockKind::BubbleColumn.default_state()),
            _ => BlockKind::Water.default_state(),
        }
    }

    fn bubble_can_exist(&self, pos: BlockPos) -> bool {
        let state = self.block_or_air(pos);
        state.kind() == Some(BlockKind::BubbleColumn)
            || (state.kind() == Some(BlockKind::Water) && state.property("level") == Some("0"))
    }

    pub(crate) fn bubble_update_column(&mut self, pos: BlockPos, column: BlockStateId) {
        if !self.bubble_can_exist(pos) {
            return;
        }
        let state = column;
        let mut at = pos;
        while self.bubble_can_exist(at) {
            if self.block_or_air(at) != state {
                self.set_block(at, state);
            }
            at = step(at, Face::Up);
        }
    }

    pub(crate) fn bubble_source_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        self.bubble_update_column(step(pos, Face::Up), state_source(state));
    }

    pub(crate) fn bubble_column_tick(&mut self, pos: BlockPos) {
        let column = self.bubble_source(step(pos, Face::Down));
        self.bubble_update_column(pos, column);
    }

    pub(crate) fn bubble_check(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        match kind {
            BlockKind::BubbleColumn => self.schedule(pos, kind, BUBBLE_RECHECK_DELAY),
            BlockKind::SoulSand | BlockKind::MagmaBlock
                if self.block_or_air(step(pos, Face::Up)).kind() == Some(BlockKind::Water) =>
            {
                self.schedule(pos, kind, 1);
            }
            _ => {}
        }
    }

    pub(crate) fn special_tick(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        kind: BlockKind,
    ) -> bool {
        match kind {
            BlockKind::SoulSand | BlockKind::MagmaBlock => {
                self.bubble_source_tick(pos, state);
                true
            }
            BlockKind::BubbleColumn => {
                self.bubble_column_tick(pos);
                true
            }
            _ if crate::special_plants::eyeblossom_open(kind).is_some() => {
                self.eyeblossom_scheduled(pos, state);
                true
            }
            _ if dead_coral_of(kind).is_some() => {
                self.coral_tick(pos, state);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn tick_bubble_pushes(&mut self) {
        let ids = self.entities.store.ids();
        for id in ids {
            let Some(entity) = self.entities.store.get(id) else {
                continue;
            };
            if entity.base.removal.is_some() {
                continue;
            }
            let at = entity.base.position.block();
            let state = self.block_or_air(at);
            if state.kind() != Some(BlockKind::BubbleColumn) {
                continue;
            }
            let drag = flag(state, "drag");
            let open = self.block_or_air(step(at, Face::Up)).kind() == Some(BlockKind::Air);
            let Some(entity) = self.entities.store.get_mut(id) else {
                continue;
            };
            let velocity = entity.base.velocity;
            let y = match (open, drag) {
                (true, true) => (velocity.y - 0.03).max(-0.9),
                (true, false) => (velocity.y + 0.1).min(1.8),
                (false, true) => (velocity.y - 0.03).max(-0.3),
                (false, false) => (velocity.y + 0.06).min(0.7),
            };
            entity.base.velocity = rspigot_domain::Vec3::new(velocity.x, y, velocity.z);
            entity.base.sync_velocity = true;
        }
    }
}

fn state_source(state: BlockStateId) -> BlockStateId {
    match state.kind() {
        Some(BlockKind::MagmaBlock) => BlockKind::BubbleColumn
            .state_with(&[("drag", "true")])
            .unwrap_or_else(|| BlockKind::BubbleColumn.default_state()),
        _ => BlockKind::BubbleColumn
            .state_with(&[("drag", "false")])
            .unwrap_or_else(|| BlockKind::BubbleColumn.default_state()),
    }
}
