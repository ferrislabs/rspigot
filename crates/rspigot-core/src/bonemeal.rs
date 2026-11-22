use std::num::NonZeroU8;

use rand::Rng;
use rspigot_blocks::{BlockKind, Face, StateFlags, Support};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, placement};
use rspigot_items::ItemKind;

use crate::plugins::events::TreeType;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::level;

const SEAGRASS_SUCCESS: f32 = 0.45;
const MUSHROOM_SUCCESS: f32 = 0.4;
const FUNGUS_SUCCESS: f32 = 0.4;
const NETHER_VINE_DECAY: f64 = 0.826;
const VINE_MAX_AGE: i32 = 25;
const BAMBOO_MAX_HEIGHT: i32 = 16;
const HORIZONTAL: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn set(state: BlockStateId, name: &str, value: &str) -> BlockStateId {
    state.with_property(name, value).unwrap_or(state)
}

fn flag(state: BlockStateId, name: &str) -> bool {
    state.property(name) == Some("true")
}

fn is_water(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Water)
}

fn is_air(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Air)
}

fn plant_growth(kind: BlockKind) -> Option<(BlockKind, Face)> {
    match kind {
        BlockKind::Kelp | BlockKind::KelpPlant => Some((BlockKind::Kelp, Face::Up)),
        BlockKind::TwistingVines | BlockKind::TwistingVinesPlant => {
            Some((BlockKind::TwistingVines, Face::Up))
        }
        BlockKind::WeepingVines | BlockKind::WeepingVinesPlant => {
            Some((BlockKind::WeepingVines, Face::Down))
        }
        _ => None,
    }
}

fn body_of(head: BlockKind) -> BlockKind {
    match head {
        BlockKind::Kelp => BlockKind::KelpPlant,
        BlockKind::TwistingVines => BlockKind::TwistingVinesPlant,
        _ => BlockKind::WeepingVinesPlant,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn fertilized(
        &mut self,
        who: Option<PlayerId>,
        origin: BlockPos,
        blocks: Vec<(BlockPos, BlockStateId)>,
    ) {
        match who {
            Some(player) => self.fertilize(player, origin, blocks),
            None => {
                for (pos, state) in blocks {
                    self.set_block(pos, state);
                }
            }
        }
    }

    fn pop_block_item(&mut self, pos: BlockPos, kind: BlockKind) {
        if let Some(item) = ItemKind::from_name(kind.name()) {
            self.spawn_item(pos, ItemStack::new(item.id(), NonZeroU8::MIN));
        }
    }

    pub(crate) fn can_survive_at(&self, state: BlockStateId, pos: BlockPos) -> bool {
        placement::can_survive(state, pos, &|at| self.shape_view(at))
    }

    fn spreadable_neighbour(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockPos> {
        let mut faces = HORIZONTAL;
        for index in (1..faces.len()).rev() {
            faces.swap(index, self.rng.random_range(0..=index));
        }
        faces
            .into_iter()
            .map(|face| step(pos, face))
            .find(|at| is_air(self.block_or_air(*at)) && self.can_survive_at(state, *at))
    }

    fn has_spreadable_neighbour(&self, pos: BlockPos, state: BlockStateId) -> bool {
        HORIZONTAL
            .into_iter()
            .map(|face| step(pos, face))
            .any(|at| is_air(self.block_or_air(at)) && self.can_survive_at(state, at))
    }

    fn vine_head(&self, pos: BlockPos, kind: BlockKind) -> Option<(BlockPos, BlockStateId)> {
        let (head, face) = plant_growth(kind)?;
        let mut at = pos;
        for _ in 0..512 {
            let state = self.block_or_air(at);
            if state.kind() == Some(head) {
                return Some((at, state));
            }
            if state.kind() != Some(body_of(head)) {
                return None;
            }
            at = step(at, face);
        }
        None
    }

    fn can_grow_into(&self, head: BlockKind, pos: BlockPos) -> bool {
        let state = self.block_or_air(pos);
        if head == BlockKind::Kelp {
            is_water(state)
        } else {
            is_air(state)
        }
    }

    fn grow_vines(&mut self, who: Option<PlayerId>, pos: BlockPos, kind: BlockKind) -> bool {
        let Some((head_kind, face)) = plant_growth(kind) else {
            return false;
        };
        let Some((head_pos, head)) = self.vine_head(pos, kind) else {
            return false;
        };
        if !self.can_grow_into(head_kind, step(head_pos, face)) {
            return false;
        }
        let count = if head_kind == BlockKind::Kelp {
            1
        } else {
            let mut chance = 1.0_f64;
            let mut grown = 0;
            while self.rng.random::<f64>() < chance {
                grown += 1;
                chance *= NETHER_VINE_DECAY;
            }
            grown
        };
        let mut blocks = Vec::new();
        let mut age = (level(head, "age") + 1).min(VINE_MAX_AGE);
        let mut at = step(head_pos, face);
        for _ in 0..count {
            if !self.can_grow_into(head_kind, at) {
                break;
            }
            blocks.push((at, set(head, "age", &age.to_string())));
            at = step(at, face);
            age = (age + 1).min(VINE_MAX_AGE);
        }
        if blocks.is_empty() {
            return true;
        }
        blocks.insert(0, (head_pos, body_of(head_kind).default_state()));
        self.fertilized(who, head_pos, blocks);
        true
    }

    fn bonemeal_bamboo(&mut self, who: Option<PlayerId>, pos: BlockPos) -> bool {
        let mut above = 0;
        while self.block_or_air(pos.offset(0, above + 1, 0)).kind() == Some(BlockKind::Bamboo)
            && above < BAMBOO_MAX_HEIGHT
        {
            above += 1;
        }
        let mut below = 0;
        while self.block_or_air(pos.offset(0, -(below + 1), 0)).kind() == Some(BlockKind::Bamboo)
            && below < BAMBOO_MAX_HEIGHT
        {
            below += 1;
        }
        let top = pos.offset(0, above, 0);
        let top_state = self.block_or_air(top);
        if above + below + 1 >= BAMBOO_MAX_HEIGHT
            || top_state.property("stage") == Some("1")
            || !is_air(self.block_or_air(top.offset(0, 1, 0)))
        {
            return false;
        }
        let mut head = top;
        let mut blocks: Vec<(BlockPos, BlockStateId)> = Vec::new();
        let rounds = 1 + self.rng.random_range(0..2);
        for height in (above + below + 1..).take(rounds) {
            let state = blocks
                .iter()
                .find(|(at, _)| *at == head)
                .map_or_else(|| self.block_or_air(head), |(_, state)| *state);
            if height > BAMBOO_MAX_HEIGHT
                || state.kind() != Some(BlockKind::Bamboo)
                || !is_air(self.block_or_air(head.offset(0, 1, 0)))
            {
                break;
            }
            let below1 = self.block_or_air(head.offset(0, -1, 0));
            let below2_pos = head.offset(0, -2, 0);
            let below2 = self.block_or_air(below2_pos);
            let mut leaves = "none";
            if below > 0 || height > 1 {
                if below1.kind() == Some(BlockKind::Bamboo)
                    && below1.property("leaves") != Some("none")
                {
                    leaves = "large";
                    if below2.kind() == Some(BlockKind::Bamboo) {
                        blocks.push((head.offset(0, -1, 0), set(below1, "leaves", "small")));
                        blocks.push((below2_pos, set(below2, "leaves", "none")));
                    }
                } else {
                    leaves = "small";
                }
            }
            let age =
                if state.property("age") != Some("1") && below2.kind() != Some(BlockKind::Bamboo) {
                    "0"
                } else {
                    "1"
                };
            let stage = if (height >= 11 && self.rng.random::<f32>() < 0.25) || height == 15 {
                "1"
            } else {
                "0"
            };
            let next = BlockKind::Bamboo
                .state_with(&[("age", age), ("leaves", leaves), ("stage", stage)])
                .unwrap_or_else(|| BlockKind::Bamboo.default_state());
            let up = head.offset(0, 1, 0);
            blocks.push((up, next));
            head = up;
        }
        if blocks.is_empty() {
            return false;
        }
        self.fertilized(who, pos, blocks);
        true
    }

    fn tall_pair(&self, kind: BlockKind, lower: BlockPos) -> Vec<(BlockPos, BlockStateId)> {
        let mut pair = Vec::new();
        for (at, half) in [(lower, "lower"), (step(lower, Face::Up), "upper")] {
            if let Some(state) = kind.state_with(&[("half", half)]) {
                pair.push((at, state));
            }
        }
        pair
    }

    fn grow_pitcher(&mut self, who: Option<PlayerId>, pos: BlockPos, state: BlockStateId) -> bool {
        let lower = if state.property("half") == Some("upper") {
            step(pos, Face::Down)
        } else {
            pos
        };
        let state = self.block_or_air(lower);
        if state.kind() != Some(BlockKind::PitcherCrop) || state.property("half") != Some("lower") {
            return false;
        }
        let age = level(state, "age");
        if age >= 4 {
            return false;
        }
        let next = age + 1;
        let upper = step(lower, Face::Up);
        let mut blocks = vec![(lower, set(state, "age", &next.to_string()))];
        if next >= 3 {
            let above = self.block_or_air(upper);
            if !is_air(above) && above.kind() != Some(BlockKind::PitcherCrop) {
                return false;
            }
            blocks.push((
                upper,
                set(set(state, "age", &next.to_string()), "half", "upper"),
            ));
        }
        self.fertilized(who, lower, blocks);
        true
    }

    fn moss_topper(&self, pos: BlockPos) -> Option<BlockStateId> {
        let mut topper = BlockKind::PaleMossCarpet
            .state_with(&[("bottom", "false")])
            .unwrap_or_else(|| BlockKind::PaleMossCarpet.default_state());
        let mut any = false;
        for face in HORIZONTAL {
            let side = self.block_or_air(step(pos, face));
            if side.is_face_sturdy(face.opposite(), Support::Full) {
                topper = set(topper, face.name(), "low");
                any = true;
            }
        }
        any.then_some(topper)
    }

    pub(crate) fn bone_meal_apply(&mut self, who: Option<PlayerId>, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        let Some(kind) = state.kind() else {
            return false;
        };
        let above = step(pos, Face::Up);
        let below = step(pos, Face::Down);
        match kind {
            BlockKind::PumpkinStem | BlockKind::MelonStem => {
                let age = level(state, "age");
                if age >= 7 {
                    return false;
                }
                let grown = (age + self.rng.random_range(2..=5)).min(7);
                self.fertilized(who, pos, vec![(pos, set(state, "age", &grown.to_string()))]);
                true
            }
            BlockKind::Cocoa => {
                let age = level(state, "age");
                if age >= 2 {
                    return false;
                }
                self.fertilized(
                    who,
                    pos,
                    vec![(pos, set(state, "age", &(age + 1).to_string()))],
                );
                true
            }
            BlockKind::SweetBerryBush => {
                let age = level(state, "age");
                if age >= 3 {
                    return false;
                }
                self.fertilized(
                    who,
                    pos,
                    vec![(pos, set(state, "age", &(age + 1).to_string()))],
                );
                true
            }
            BlockKind::TorchflowerCrop => {
                let next = if level(state, "age") == 0 {
                    set(state, "age", "1")
                } else {
                    BlockKind::Torchflower.default_state()
                };
                self.fertilized(who, pos, vec![(pos, next)]);
                true
            }
            BlockKind::PitcherCrop => self.grow_pitcher(who, pos, state),
            BlockKind::Seagrass => {
                let water = self.block_or_air(above);
                if !is_water(water) || level(water, "level") != 0 {
                    return false;
                }
                if self.rng.random::<f32>() < SEAGRASS_SUCCESS {
                    let blocks = self.tall_pair(BlockKind::TallSeagrass, pos);
                    self.fertilized(who, pos, blocks);
                }
                true
            }
            BlockKind::Kelp
            | BlockKind::KelpPlant
            | BlockKind::TwistingVines
            | BlockKind::TwistingVinesPlant
            | BlockKind::WeepingVines
            | BlockKind::WeepingVinesPlant => self.grow_vines(who, pos, kind),
            BlockKind::CaveVines | BlockKind::CaveVinesPlant => {
                if flag(state, "berries") {
                    return false;
                }
                self.fertilized(who, pos, vec![(pos, set(state, "berries", "true"))]);
                true
            }
            BlockKind::BambooSapling => {
                if !is_air(self.block_or_air(above)) {
                    return false;
                }
                self.fertilized(who, pos, vec![(above, BlockKind::Bamboo.default_state())]);
                true
            }
            BlockKind::Bamboo => self.bonemeal_bamboo(who, pos),
            BlockKind::RootedDirt => {
                if !is_air(self.block_or_air(below)) {
                    return false;
                }
                self.fertilized(
                    who,
                    pos,
                    vec![(below, BlockKind::HangingRoots.default_state())],
                );
                true
            }
            BlockKind::MangroveLeaves => {
                if !is_air(self.block_or_air(below)) {
                    return false;
                }
                let propagule = BlockKind::MangrovePropagule
                    .state_with(&[("hanging", "true"), ("age", "0")])
                    .unwrap_or_else(|| BlockKind::MangrovePropagule.default_state());
                self.fertilized(who, pos, vec![(below, propagule)]);
                true
            }
            BlockKind::MangrovePropagule => {
                if flag(state, "hanging") {
                    let age = level(state, "age");
                    if age >= 4 {
                        return false;
                    }
                    self.fertilized(
                        who,
                        pos,
                        vec![(pos, set(state, "age", &(age + 1).to_string()))],
                    );
                    true
                } else {
                    if self.rng.random::<f32>() < 0.45 {
                        self.advance_tree(pos, state, who);
                    }
                    true
                }
            }
            BlockKind::PaleHangingMoss => {
                let mut tip = pos;
                while self.block_or_air(step(tip, Face::Down)).kind() == Some(kind) {
                    tip = step(tip, Face::Down);
                }
                let next = step(tip, Face::Down);
                if !is_air(self.block_or_air(next)) {
                    return false;
                }
                let tip_state = self.block_or_air(tip);
                self.fertilized(
                    who,
                    tip,
                    vec![
                        (tip, set(tip_state, "tip", "false")),
                        (next, set(state, "tip", "true")),
                    ],
                );
                true
            }
            BlockKind::ShelfMushroom => {
                let age = level(state, "age");
                if age >= 1 {
                    return false;
                }
                self.fertilized(who, pos, vec![(pos, set(state, "age", "1"))]);
                true
            }
            BlockKind::PinkPetals | BlockKind::Wildflowers => {
                let amount = level(state, "flower_amount");
                if amount < 4 {
                    self.fertilized(
                        who,
                        pos,
                        vec![(pos, set(state, "flower_amount", &(amount + 1).to_string()))],
                    );
                } else {
                    self.pop_block_item(pos, kind);
                }
                true
            }
            BlockKind::Sunflower | BlockKind::Lilac | BlockKind::RoseBush | BlockKind::Peony => {
                self.pop_block_item(pos, kind);
                true
            }
            BlockKind::ShortGrass | BlockKind::Fern => {
                let double = if kind == BlockKind::Fern {
                    BlockKind::LargeFern
                } else {
                    BlockKind::TallGrass
                };
                let lower = double.state_with(&[("half", "lower")]);
                if !is_air(self.block_or_air(above))
                    || lower.is_none_or(|lower| !self.can_survive_at(lower, pos))
                {
                    return false;
                }
                let blocks = self.tall_pair(double, pos);
                self.fertilized(who, pos, blocks);
                true
            }
            BlockKind::ShortDryGrass => {
                self.fertilized(
                    who,
                    pos,
                    vec![(pos, BlockKind::TallDryGrass.default_state())],
                );
                true
            }
            BlockKind::TallDryGrass | BlockKind::Bush | BlockKind::FireflyBush => {
                let placed = if kind == BlockKind::TallDryGrass {
                    BlockKind::ShortDryGrass.default_state()
                } else {
                    state
                };
                if !self.has_spreadable_neighbour(pos, placed) {
                    return false;
                }
                if let Some(at) = self.spreadable_neighbour(pos, placed) {
                    self.fertilized(who, at, vec![(at, placed)]);
                }
                true
            }
            BlockKind::Netherrack => self.bone_meal_netherrack(who, pos),
            BlockKind::PaleMossCarpet => {
                let Some(topper) = self.moss_topper(above) else {
                    return false;
                };
                if !is_air(self.block_or_air(above)) {
                    return false;
                }
                self.fertilized(who, above, vec![(above, topper)]);
                true
            }
            BlockKind::GlowLichen => self.grow_lichen(who, pos, state),
            BlockKind::BrownMushroom | BlockKind::RedMushroom => {
                if self.rng.random::<f32>() < MUSHROOM_SUCCESS {
                    let (feature, tree) = if kind == BlockKind::RedMushroom {
                        ("huge_red_mushroom", TreeType::RedMushroom)
                    } else {
                        ("huge_brown_mushroom", TreeType::BrownMushroom)
                    };
                    self.begin_feature_growth(feature, pos, vec![pos], who, Some(tree));
                }
                true
            }
            BlockKind::CrimsonFungus | BlockKind::WarpedFungus => {
                let (base, feature, tree) = if kind == BlockKind::CrimsonFungus {
                    (
                        BlockKind::CrimsonNylium,
                        "crimson_fungus_planted",
                        TreeType::CrimsonFungus,
                    )
                } else {
                    (
                        BlockKind::WarpedNylium,
                        "warped_fungus_planted",
                        TreeType::WarpedFungus,
                    )
                };
                if self.block_or_air(below).kind() != Some(base) {
                    return false;
                }
                if self.rng.random::<f32>() < FUNGUS_SUCCESS {
                    self.begin_feature_growth(feature, pos, vec![pos], who, Some(tree));
                }
                true
            }
            BlockKind::CrimsonNylium | BlockKind::WarpedNylium => {
                if !is_air(self.block_or_air(above)) {
                    return false;
                }
                self.begin_feature_growth("nylium_bonemeal", above, Vec::new(), who, None);
                true
            }
            BlockKind::MossBlock | BlockKind::PaleMossBlock => {
                if !is_air(self.block_or_air(above)) {
                    return false;
                }
                let feature = if kind == BlockKind::MossBlock {
                    "moss_patch_bonemeal"
                } else {
                    "pale_moss_patch_bonemeal"
                };
                self.begin_feature_growth(feature, above, Vec::new(), who, None);
                true
            }
            _ => false,
        }
    }

    fn bone_meal_netherrack(&mut self, who: Option<PlayerId>, pos: BlockPos) -> bool {
        let above = self.block_or_air(step(pos, Face::Up));
        if above.flags().contains(StateFlags::FULL_COLLISION_CUBE) || above.flags().has_fluid() {
            return false;
        }
        let mut warped = false;
        let mut crimson = false;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    match self.block_or_air(pos.offset(dx, dy, dz)).kind() {
                        Some(BlockKind::WarpedNylium) => warped = true,
                        Some(BlockKind::CrimsonNylium) => crimson = true,
                        _ => {}
                    }
                }
            }
        }
        let chosen = match (warped, crimson) {
            (true, true) if self.rng.random::<bool>() => BlockKind::WarpedNylium,
            (true, true) | (false, true) => BlockKind::CrimsonNylium,
            (true, false) => BlockKind::WarpedNylium,
            (false, false) => return false,
        };
        self.fertilized(who, pos, vec![(pos, chosen.default_state())]);
        true
    }

    fn grow_lichen(&mut self, who: Option<PlayerId>, pos: BlockPos, state: BlockStateId) -> bool {
        let faces: Vec<Face> = Face::ALL
            .into_iter()
            .filter(|face| flag(state, face.name()))
            .collect();
        if faces.is_empty() {
            return false;
        }
        let start = faces[self.rng.random_range(0..faces.len())];
        let mut directions = Face::ALL;
        for index in (1..directions.len()).rev() {
            directions.swap(index, self.rng.random_range(0..=index));
        }
        for toward in directions {
            if toward == start || toward == start.opposite() {
                continue;
            }
            let neighbour = step(pos, toward);
            let there = self.block_or_air(neighbour);
            let host = self.block_or_air(step(neighbour, start));
            if is_air(there) && host.is_face_sturdy(start.opposite(), Support::Full) {
                let lichen = set(BlockKind::GlowLichen.default_state(), start.name(), "true");
                self.fertilized(who, neighbour, vec![(neighbour, lichen)]);
                return true;
            }
            if there.kind() == Some(BlockKind::GlowLichen)
                && !flag(there, start.name())
                && host.is_face_sturdy(start.opposite(), Support::Full)
            {
                self.fertilized(
                    who,
                    neighbour,
                    vec![(neighbour, set(there, start.name(), "true"))],
                );
                return true;
            }
        }
        false
    }
}
