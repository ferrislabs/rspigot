use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_domain::crop;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::mobs::ServerRandom;
use crate::plugins::Lane;
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Default, Debug, PartialEq, Eq)]
struct Growth {
    grow: Vec<(BlockPos, BlockStateId)>,
    attach: Option<(BlockPos, BlockStateId)>,
}

const CROP_LIGHT: i32 = 9;
const PITCHER_LIGHT: i32 = 8;
const GROWTH_FACTOR: f32 = 25.0;
const STEM_MAX_AGE: i32 = 7;
const PITCHER_MAX_AGE: i32 = 4;
const NETHER_WART_MAX_AGE: i32 = 3;
const COCOA_MAX_AGE: i32 = 2;
const BERRY_MAX_AGE: i32 = 3;
const HORIZONTAL: [(&str, i32, i32); 4] = [
    ("north", 0, -1),
    ("east", 1, 0),
    ("south", 0, 1),
    ("west", -1, 0),
];

fn number(state: BlockStateId, name: &str) -> i32 {
    state
        .property(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn is_dirt(state: BlockStateId) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", "minecraft:dirt")
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn growth_speed(kind: BlockKind, pos: BlockPos, block: &impl Fn(BlockPos) -> BlockStateId) -> f32 {
    let mut speed = 1.0;
    for dx in -1..=1 {
        for dz in -1..=1 {
            let soil = block(pos.offset(dx, -1, dz));
            let mut bonus = 0.0;
            if soil.kind() == Some(BlockKind::Farmland) {
                bonus = if number(soil, "moisture") > 0 {
                    3.0
                } else {
                    1.0
                };
            }
            if dx != 0 || dz != 0 {
                bonus /= 4.0;
            }
            speed += bonus;
        }
    }
    let same = |dx: i32, dz: i32| block(pos.offset(dx, 0, dz)).kind() == Some(kind);
    let horizontal = same(-1, 0) || same(1, 0);
    let vertical = same(0, -1) || same(0, 1);
    let diagonal = same(-1, -1) || same(1, -1) || same(-1, 1) || same(1, 1);
    if horizontal && vertical || diagonal {
        speed /= 2.0;
    }
    speed
}

fn rolls_growth(
    kind: BlockKind,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
    random: &mut impl RandomSource,
) -> bool {
    let speed = growth_speed(kind, pos, block);
    random.next_int((GROWTH_FACTOR / speed) as i32 + 1) == 0
}

fn single(pos: BlockPos, state: Option<BlockStateId>) -> Growth {
    Growth {
        grow: state.map(|state| (pos, state)).into_iter().collect(),
        attach: None,
    }
}

fn plan_crop(
    kind: BlockKind,
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
    light: &impl Fn(BlockPos) -> i32,
    random: &mut impl RandomSource,
) -> Growth {
    let skipped = matches!(kind, BlockKind::Beetroots | BlockKind::TorchflowerCrop)
        && random.next_int(3) == 0;
    if skipped || light(pos) < CROP_LIGHT {
        return Growth::default();
    }
    let age = number(state, "age");
    let max = if kind == BlockKind::TorchflowerCrop {
        2
    } else {
        crop::max_age(kind).unwrap_or(0)
    };
    if age >= max || !rolls_growth(kind, pos, block, random) {
        return Growth::default();
    }
    let grown = if kind == BlockKind::TorchflowerCrop && age + 1 >= max {
        Some(BlockKind::Torchflower.default_state())
    } else {
        crop::state_for_age(state, age + 1)
    };
    single(pos, grown)
}

fn plan_pitcher(
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
    light: &impl Fn(BlockPos) -> i32,
    random: &mut impl RandomSource,
) -> Growth {
    let age = number(state, "age");
    if state.property("half") != Some("lower") || age >= PITCHER_MAX_AGE {
        return Growth::default();
    }
    if !rolls_growth(BlockKind::PitcherCrop, pos, block, random) {
        return Growth::default();
    }
    let next = age + 1;
    let above = pos.offset(0, 1, 0);
    let above_state = block(above);
    let room =
        next <= 2 || above_state.is_air() || above_state.kind() == Some(BlockKind::PitcherCrop);
    if !room || light(pos) < PITCHER_LIGHT {
        return Growth::default();
    }
    let mut grow = Vec::new();
    if let Some(lower) = crop::state_for_age(state, next) {
        grow.push((pos, lower));
        if next >= 3
            && let Some(upper) = crop::state_for_age(state, next)
                .and_then(|lower| lower.with_property("half", "upper"))
        {
            grow.push((above, upper));
        }
    }
    Growth { grow, attach: None }
}

fn plan_stem(
    kind: BlockKind,
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
    light: &impl Fn(BlockPos) -> i32,
    random: &mut impl RandomSource,
) -> Growth {
    if light(pos) < CROP_LIGHT || !rolls_growth(kind, pos, block, random) {
        return Growth::default();
    }
    let age = number(state, "age");
    if age < STEM_MAX_AGE {
        return single(pos, crop::state_for_age(state, age + 1));
    }
    let (fruit, attached) = if kind == BlockKind::PumpkinStem {
        (BlockKind::Pumpkin, BlockKind::AttachedPumpkinStem)
    } else {
        (BlockKind::Melon, BlockKind::AttachedMelonStem)
    };
    let (facing, dx, dz) = HORIZONTAL[random.next_int(HORIZONTAL.len() as i32) as usize];
    let target = pos.offset(dx, 0, dz);
    let soil = block(target.offset(0, -1, 0));
    if !block(target).is_air() || soil.kind() != Some(BlockKind::Farmland) && !is_dirt(soil) {
        return Growth::default();
    }
    Growth {
        grow: vec![(target, fruit.default_state())],
        attach: attached
            .default_state()
            .with_property("facing", facing)
            .map(|state| (pos, state)),
    }
}

fn plan(
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
    light: &impl Fn(BlockPos) -> i32,
    random: &mut impl RandomSource,
) -> Option<Growth> {
    let kind = state.kind()?;
    Some(match kind {
        BlockKind::Wheat
        | BlockKind::Carrots
        | BlockKind::Potatoes
        | BlockKind::Beetroots
        | BlockKind::TorchflowerCrop => plan_crop(kind, pos, state, block, light, random),
        BlockKind::PitcherCrop => plan_pitcher(pos, state, block, light, random),
        BlockKind::PumpkinStem | BlockKind::MelonStem => {
            plan_stem(kind, pos, state, block, light, random)
        }
        BlockKind::NetherWart => {
            let age = number(state, "age");
            single(
                pos,
                (age < NETHER_WART_MAX_AGE && random.next_int(10) == 0)
                    .then(|| crop::state_for_age(state, age + 1))
                    .flatten(),
            )
        }
        BlockKind::Cocoa => {
            let age = number(state, "age");
            single(
                pos,
                (random.next_int(5) == 0 && age < COCOA_MAX_AGE)
                    .then(|| crop::state_for_age(state, age + 1))
                    .flatten(),
            )
        }
        BlockKind::SweetBerryBush => {
            let age = number(state, "age");
            single(
                pos,
                (age < BERRY_MAX_AGE
                    && random.next_int(5) == 0
                    && light(pos.offset(0, 1, 0)) >= CROP_LIGHT)
                    .then(|| crop::state_for_age(state, age + 1))
                    .flatten(),
            )
        }
        _ => return None,
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn random_tick_crop(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let seed: u64 = self.rng.random();
        let growth = {
            let block = |at: BlockPos| self.block_or_air(at);
            let light = |at: BlockPos| self.raw_brightness(at);
            let mut rng = StdRng::seed_from_u64(seed);
            let mut random = ServerRandom(&mut rng);
            plan(pos, state, &block, &light, &mut random)
        };
        let Some(growth) = growth else {
            return false;
        };
        let companions: Vec<_> = growth.attach.into_iter().collect();
        let mut grows = growth.grow.into_iter();
        if let Some((at, grown)) = grows.next() {
            self.grow_block_with(Lane::Block(pos), at, grown, companions);
        }
        for (at, grown) in grows {
            self.grow_block(Lane::Block(pos), at, grown);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct Script(Vec<i32>);

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            let value = if self.0.is_empty() {
                0
            } else {
                self.0.remove(0)
            };
            assert!(value < bound, "{value} >= {bound}");
            value
        }
        fn next_double(&mut self) -> f64 {
            0.0
        }
        fn next_boolean(&mut self) -> bool {
            false
        }
        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    const POS: BlockPos = BlockPos::new(0, 64, 0);

    fn state(kind: BlockKind, props: &[(&str, &str)]) -> BlockStateId {
        props.iter().fold(kind.default_state(), |state, (k, v)| {
            state.with_property(k, v).expect("property")
        })
    }

    fn farmland(moisture: &str) -> BlockStateId {
        state(BlockKind::Farmland, &[("moisture", moisture)])
    }

    fn world(cells: &[(BlockPos, BlockStateId)]) -> HashMap<BlockPos, BlockStateId> {
        cells.iter().copied().collect()
    }

    fn run(
        cells: &HashMap<BlockPos, BlockStateId>,
        at: BlockPos,
        light: i32,
        rolls: &[i32],
    ) -> Option<Growth> {
        let block = |p: BlockPos| cells.get(&p).copied().unwrap_or(BlockStateId::AIR);
        let lit = |_: BlockPos| light;
        let state = block(at);
        plan(at, state, &block, &lit, &mut Script(rolls.to_vec()))
    }

    fn crop(kind: BlockKind, age: &str) -> BlockStateId {
        state(kind, &[("age", age)])
    }

    fn base(kind: BlockKind, age: &str, soil: BlockStateId) -> HashMap<BlockPos, BlockStateId> {
        let mut cells = HashMap::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                cells.insert(POS.offset(dx, -1, dz), soil);
            }
        }
        cells.insert(POS, crop(kind, age));
        cells
    }

    #[test]
    fn unrelated_blocks_are_not_handled() {
        let cells = world(&[(POS, BlockKind::Stone.default_state())]);
        assert_eq!(run(&cells, POS, 15, &[]), None);
    }

    #[test]
    fn wheat_on_wet_farmland_grows_with_speed_ten() {
        let cells = base(BlockKind::Wheat, "0", farmland("7"));
        let grown = run(&cells, POS, 15, &[0]).unwrap();
        assert_eq!(grown.grow, [(POS, crop(BlockKind::Wheat, "1"))]);
        assert_eq!(run(&cells, POS, 15, &[1]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[2]).unwrap(), Growth::default());
    }

    #[test]
    fn wheat_needs_light_nine_and_is_handled_either_way() {
        let cells = base(BlockKind::Wheat, "0", farmland("7"));
        assert_eq!(run(&cells, POS, 8, &[0]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 9, &[0]).unwrap().grow.len(), 1);
    }

    #[test]
    fn mature_wheat_does_not_grow() {
        let cells = base(BlockKind::Wheat, "7", farmland("7"));
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap(), Growth::default());
    }

    #[test]
    fn dry_farmland_is_slower() {
        let cells = base(BlockKind::Wheat, "0", farmland("0"));
        assert_eq!(run(&cells, POS, 15, &[6]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap().grow.len(), 1);
    }

    #[test]
    fn same_crop_on_both_axes_halves_the_speed() {
        let mut cells = base(BlockKind::Wheat, "0", farmland("7"));
        cells.insert(POS.offset(1, 0, 0), crop(BlockKind::Wheat, "0"));
        cells.insert(POS.offset(0, 0, 1), crop(BlockKind::Wheat, "0"));
        assert!(run(&cells, POS, 15, &[4]).unwrap().grow.is_empty());
        assert_eq!(run(&cells, POS, 15, &[4]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap().grow.len(), 1);
    }

    #[test]
    fn same_crop_diagonal_halves_the_speed() {
        let mut cells = base(BlockKind::Wheat, "0", farmland("7"));
        cells.insert(POS.offset(1, 0, 1), crop(BlockKind::Wheat, "0"));
        assert_eq!(run(&cells, POS, 15, &[4]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[3]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[1]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap().grow.len(), 1);
    }

    #[test]
    fn beetroots_skip_one_tick_in_three() {
        let cells = base(BlockKind::Beetroots, "0", farmland("7"));
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 15, &[1, 1]).unwrap(), Growth::default());
        let grown = run(&cells, POS, 15, &[1, 0]).unwrap();
        assert_eq!(grown.grow, [(POS, crop(BlockKind::Beetroots, "1"))]);
    }

    #[test]
    fn torchflower_crop_ripens_into_the_flower() {
        let cells = base(BlockKind::TorchflowerCrop, "1", farmland("7"));
        let grown = run(&cells, POS, 15, &[1, 0]).unwrap();
        assert_eq!(grown.grow, [(POS, BlockKind::Torchflower.default_state())]);
        let cells = base(BlockKind::TorchflowerCrop, "0", farmland("7"));
        let grown = run(&cells, POS, 15, &[1, 0]).unwrap();
        assert_eq!(grown.grow, [(POS, crop(BlockKind::TorchflowerCrop, "1"))]);
    }

    #[test]
    fn pitcher_crop_grows_lower_half_and_adds_upper_from_age_three() {
        let lower = |age: &str| state(BlockKind::PitcherCrop, &[("age", age), ("half", "lower")]);
        let upper = |age: &str| state(BlockKind::PitcherCrop, &[("age", age), ("half", "upper")]);
        let mut cells = base(BlockKind::PitcherCrop, "0", farmland("7"));
        cells.insert(POS, lower("1"));
        let grown = run(&cells, POS, 15, &[0]).unwrap();
        assert_eq!(grown.grow, [(POS, lower("2"))]);
        cells.insert(POS, lower("2"));
        let grown = run(&cells, POS, 15, &[0]).unwrap();
        assert_eq!(
            grown.grow,
            [(POS, lower("3")), (POS.offset(0, 1, 0), upper("3"))]
        );
        cells.insert(POS.offset(0, 1, 0), BlockKind::Stone.default_state());
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap(), Growth::default());
        cells.insert(POS, lower("4"));
        cells.remove(&POS.offset(0, 1, 0));
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap(), Growth::default());
        cells.insert(POS, upper("3"));
        assert_eq!(run(&cells, POS, 15, &[0]).unwrap(), Growth::default());
    }

    #[test]
    fn nether_wart_grows_one_in_ten_up_to_three() {
        let mut cells = world(&[(POS, crop(BlockKind::NetherWart, "0"))]);
        assert_eq!(
            run(&cells, POS, 0, &[0]).unwrap().grow,
            [(POS, crop(BlockKind::NetherWart, "1"))]
        );
        assert_eq!(run(&cells, POS, 0, &[1]).unwrap(), Growth::default());
        cells.insert(POS, crop(BlockKind::NetherWart, "3"));
        assert_eq!(run(&cells, POS, 0, &[0]).unwrap(), Growth::default());
    }

    #[test]
    fn cocoa_grows_one_in_five_up_to_two() {
        let pod = |age: &str| state(BlockKind::Cocoa, &[("age", age), ("facing", "north")]);
        let mut cells = world(&[(POS, pod("0"))]);
        assert_eq!(run(&cells, POS, 0, &[0]).unwrap().grow, [(POS, pod("1"))]);
        assert_eq!(run(&cells, POS, 0, &[4]).unwrap(), Growth::default());
        cells.insert(POS, pod("2"));
        assert_eq!(run(&cells, POS, 0, &[0]).unwrap(), Growth::default());
    }

    #[test]
    fn sweet_berry_bush_needs_light_nine_above() {
        let bush = |age: &str| crop(BlockKind::SweetBerryBush, age);
        let mut cells = world(&[(POS, bush("0"))]);
        assert_eq!(run(&cells, POS, 9, &[0]).unwrap().grow, [(POS, bush("1"))]);
        assert_eq!(run(&cells, POS, 8, &[0]).unwrap(), Growth::default());
        assert_eq!(run(&cells, POS, 9, &[1]).unwrap(), Growth::default());
        cells.insert(POS, bush("3"));
        assert_eq!(run(&cells, POS, 9, &[0]).unwrap(), Growth::default());
    }

    fn stem_world(kind: BlockKind, age: &str) -> HashMap<BlockPos, BlockStateId> {
        let mut cells = base(kind, age, BlockKind::Stone.default_state());
        cells.insert(POS.offset(0, -1, 0), farmland("7"));
        cells.insert(POS.offset(1, -1, 0), BlockKind::Dirt.default_state());
        cells
    }

    #[test]
    fn stem_ages_until_seven() {
        let cells = stem_world(BlockKind::PumpkinStem, "3");
        let grown = run(&cells, POS, 15, &[0]).unwrap();
        assert_eq!(grown.grow, [(POS, crop(BlockKind::PumpkinStem, "4"))]);
        assert_eq!(run(&cells, POS, 8, &[0]).unwrap(), Growth::default());
    }

    #[test]
    fn mature_stem_places_the_fruit_and_attaches() {
        let cells = stem_world(BlockKind::PumpkinStem, "7");
        let grown = run(&cells, POS, 15, &[0, 1]).unwrap();
        assert_eq!(
            grown.grow,
            [(POS.offset(1, 0, 0), BlockKind::Pumpkin.default_state())]
        );
        assert_eq!(
            grown.attach,
            Some((
                POS,
                state(BlockKind::AttachedPumpkinStem, &[("facing", "east")])
            ))
        );
        let cells = stem_world(BlockKind::MelonStem, "7");
        let grown = run(&cells, POS, 15, &[0, 1]).unwrap();
        assert_eq!(
            grown.grow,
            [(POS.offset(1, 0, 0), BlockKind::Melon.default_state())]
        );
    }

    #[test]
    fn mature_stem_needs_free_target_above_soil() {
        let mut cells = stem_world(BlockKind::PumpkinStem, "7");
        assert_eq!(run(&cells, POS, 15, &[0, 0]).unwrap(), Growth::default());
        cells.insert(POS.offset(1, 0, 0), BlockKind::Stone.default_state());
        assert_eq!(run(&cells, POS, 15, &[0, 1]).unwrap(), Growth::default());
    }
}
