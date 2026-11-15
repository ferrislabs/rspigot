mod bodies;
mod place;
mod prov;
mod tree;

use std::collections::HashMap;
use std::sync::Arc;

use rspigot_data::multi_noise::overworld_biomes;
use serde_json::{Map, Value};

use self::bodies::Compiler;
use self::place::{Placed, Run};
use super::region::Region;
use super::surface_rules::SurfaceSystem;
use crate::climate::Xoroshiro;
use crate::datapack::worldgen;

const STEPS: usize = 11;

#[derive(Debug, Clone)]
struct Entry {
    placed: Arc<Placed>,
}

#[derive(Debug, Clone)]
pub(crate) struct Features {
    steps: Vec<Vec<Entry>>,
    first_id: Vec<usize>,
    biome_has: Vec<Vec<bool>>,
    universal: Vec<bool>,
}

fn ordered(lists: &[Vec<String>]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for list in lists {
        for name in list {
            if !index.contains_key(name.as_str()) {
                index.insert(name, names.len());
                names.push(name.clone());
            }
        }
    }
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    let mut incoming = vec![0usize; names.len()];
    for list in lists {
        for pair in list.windows(2) {
            let (a, b) = (index[pair[0].as_str()], index[pair[1].as_str()]);
            if a != b && !successors[a].contains(&b) {
                successors[a].push(b);
                incoming[b] += 1;
            }
        }
    }
    let mut ready: std::collections::BTreeSet<usize> =
        (0..names.len()).filter(|i| incoming[*i] == 0).collect();
    let mut result = Vec::with_capacity(names.len());
    while let Some(next) = ready.pop_first() {
        result.push(names[next].clone());
        for follower in successors[next].clone() {
            incoming[follower] -= 1;
            if incoming[follower] == 0 {
                ready.insert(follower);
            }
        }
    }
    for (i, name) in names.iter().enumerate() {
        if incoming[i] > 0 {
            result.push(name.clone());
        }
    }
    result
}

impl Features {
    pub(crate) fn compile() -> Self {
        let data = worldgen();
        let empty = Map::new();
        let placed = data
            .get("placed")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let configured = data
            .get("configured")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let biomes = data
            .get("biomes")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let names = overworld_biomes();
        let lists: Vec<Vec<Vec<String>>> = names
            .iter()
            .map(|name| {
                let steps = biomes
                    .get(*name)
                    .and_then(|biome| biome.get("features"))
                    .and_then(Value::as_array);
                (0..STEPS)
                    .map(|step| {
                        steps
                            .and_then(|steps| steps.get(step))
                            .and_then(Value::as_array)
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        let mut compiler = Compiler::new(placed, configured);
        let mut steps = Vec::with_capacity(STEPS);
        let mut first_id = Vec::with_capacity(STEPS);
        let mut id_of: Vec<HashMap<String, usize>> = Vec::with_capacity(STEPS);
        let mut next_id = 0;
        for step in 0..STEPS {
            let step_lists: Vec<Vec<String>> =
                lists.iter().map(|biome| biome[step].clone()).collect();
            let order = ordered(&step_lists);
            first_id.push(next_id);
            let mut ids = HashMap::new();
            let mut entries = Vec::with_capacity(order.len());
            for name in order {
                ids.insert(name.clone(), next_id);
                next_id += 1;
                entries.push(Entry {
                    placed: compiler.placed_ref(&Value::String(name)),
                });
            }
            id_of.push(ids);
            steps.push(entries);
        }
        let mut biome_has = vec![vec![false; next_id]; names.len()];
        for (biome, per_step) in lists.iter().enumerate() {
            for (step, list) in per_step.iter().enumerate() {
                for name in list {
                    if let Some(id) = id_of[step].get(name) {
                        biome_has[biome][*id] = true;
                    }
                }
            }
        }
        let universal = (0..next_id)
            .map(|id| biome_has.iter().all(|set| set[id]))
            .collect();
        Self {
            steps,
            first_id,
            biome_has,
            universal,
        }
    }

    pub(crate) fn decorate(&self, region: &mut Region, seed: i64, surface: &SurfaceSystem) {
        let center = region.center();
        let min_y = region.height().min_y;
        let mut rng = Xoroshiro::from_seed(0);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (cx, cz) = (center.x + dx, center.z + dz);
                let (ox, oz) = (cx * 16, cz * 16);
                let interest = (dx != 0 || dz != 0).then_some((
                    center.x * 16,
                    center.z * 16,
                    center.x * 16 + 15,
                    center.z * 16 + 15,
                ));
                rng.reseed(seed);
                let a = (rng.next_long() | 1) as i64;
                let b = (rng.next_long() | 1) as i64;
                let population = (i64::from(ox)
                    .wrapping_mul(a)
                    .wrapping_add(i64::from(oz).wrapping_mul(b)))
                    ^ seed;
                for (step, entries) in self.steps.iter().enumerate() {
                    for (index, entry) in entries.iter().enumerate() {
                        let id = self.first_id[step] + index;
                        rng.reseed(
                            population
                                .wrapping_add(id as i64)
                                .wrapping_add(10_000 * step as i64),
                        );
                        let mut run = Run {
                            region,
                            rng: &mut rng,
                            surface,
                            feature_id: id,
                            biome_has: &self.biome_has,
                            universal: &self.universal,
                            interest,
                        };
                        entry.placed.place(&mut run, (ox, min_y, oz));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockStateId, ChunkPos};

    use super::*;
    use crate::overworld::region::Region;
    use crate::resources::{BlockKind, Resources};

    pub(super) struct Bench {
        pub(super) resources: Resources,
        pub(super) dimension: Dimension,
        surface: SurfaceSystem,
        compiler_data: (Map<String, Value>, Map<String, Value>),
    }

    impl Bench {
        pub(super) fn new() -> Self {
            let resources = Resources::resolve();
            let data = worldgen();
            let get = |key: &str| {
                data.get(key)
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default()
            };
            Self {
                surface: SurfaceSystem::new(1, &resources.blocks),
                resources,
                dimension: Dimension::overworld(),
                compiler_data: (get("placed"), get("configured")),
            }
        }

        pub(super) fn floor<'a>(&'a self, ground: BlockKind, top: i32) -> Region<'a> {
            let ground = self.resources.blocks[ground];
            let stone = self.resources.blocks[BlockKind::Stone];
            Region::fixture(
                &self.resources,
                &self.dimension,
                ChunkPos::new(0, 0),
                0,
                |_, buffer| {
                    for y in -64..top {
                        buffer.fill_layer(y, stone);
                    }
                    buffer.fill_layer(top, ground);
                },
            )
        }

        pub(super) fn place(
            &self,
            region: &mut Region,
            name: &str,
            pos: (i32, i32, i32),
            seed: i64,
        ) -> bool {
            let mut compiler = Compiler::new(&self.compiler_data.0, &self.compiler_data.1);
            let feature = compiler.configured_ref(&Value::String(format!("minecraft:{name}")));
            let mut rng = Xoroshiro::from_seed(seed);
            let mut run = Run {
                region,
                rng: &mut rng,
                surface: &self.surface,
                feature_id: 0,
                biome_has: &[],
                universal: &[],
                interest: None,
            };
            feature.place(&mut run, pos)
        }

        pub(super) fn place_placed(
            &self,
            region: &mut Region,
            name: &str,
            pos: (i32, i32, i32),
            seed: i64,
        ) -> bool {
            let mut compiler = Compiler::new(&self.compiler_data.0, &self.compiler_data.1);
            let placed = compiler.placed_ref(&Value::String(format!("minecraft:{name}")));
            let mut rng = Xoroshiro::from_seed(seed);
            let mut run = Run {
                region,
                rng: &mut rng,
                surface: &self.surface,
                feature_id: 0,
                biome_has: &[],
                universal: &[true],
                interest: None,
            };
            placed.place(&mut run, pos)
        }

        pub(super) fn count(
            &self,
            region: &mut Region,
            area: i32,
            y0: i32,
            y1: i32,
            state: BlockStateId,
        ) -> usize {
            let mut count = 0;
            for x in -area..area {
                for z in -area..area {
                    for y in y0..y1 {
                        if region.get(x, y, z) == state {
                            count += 1;
                        }
                    }
                }
            }
            count
        }
    }

    #[test]
    fn plains_grass_patches_cover_the_ground() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place_placed(&mut region, "patch_grass_plain", (0, -64, 0), 5));
        let grass = bench.resources.blocks[BlockKind::ShortGrass];
        assert!(bench.count(&mut region, 16, 70, 71, grass) > 5);
    }

    fn named(state: &str) -> BlockStateId {
        rspigot_blocks::BlockKind::from_name(state)
            .map_or(BlockStateId::AIR, |kind| kind.default_state())
    }

    #[test]
    fn coal_veins_replace_stone_and_stay_small() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::Stone, 70);
        assert!(bench.place(&mut region, "ore_coal", (8, 40, 8), 3));
        let coal = bench.count(&mut region, 16, 30, 50, named("minecraft:coal_ore"));
        assert!((1..=40).contains(&coal), "{coal}");
    }

    #[test]
    fn deep_ore_turns_into_its_deepslate_variant() {
        let bench = Bench::new();
        let deepslate = bench.resources.blocks[BlockKind::Deepslate];
        let mut region = Region::fixture(
            &bench.resources,
            &bench.dimension,
            ChunkPos::new(0, 0),
            0,
            |_, buffer| {
                for y in -64..60 {
                    buffer.fill_layer(y, deepslate);
                }
            },
        );
        assert!(bench.place(&mut region, "ore_diamond_large", (8, -40, 8), 11));
        let deep = bench.count(
            &mut region,
            16,
            -60,
            -20,
            named("minecraft:deepslate_diamond_ore"),
        );
        let plain = bench.count(&mut region, 16, -60, -20, named("minecraft:diamond_ore"));
        assert!(deep >= 1, "{deep}");
        assert_eq!(plain, 0);
    }

    #[test]
    fn cacti_stack_on_sand() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::Sand, 69);
        let mut placed = false;
        for seed in 0..8 {
            placed |= bench.place(&mut region, "cactus", (4 + seed * 2, 70, 8), seed.into());
        }
        assert!(placed);
        let cactus = rspigot_blocks::BlockKind::from_name("minecraft:cactus")
            .and_then(|kind| kind.state_with(&[("age", "0")]))
            .unwrap_or(BlockStateId::AIR);
        assert!(bench.count(&mut region, 16, 70, 74, cactus) >= 2);
    }

    #[test]
    fn sand_disks_replace_dirt_and_grass_only() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place(&mut region, "disk_sand", (8, 69, 8), 2));
        let sand = bench.count(
            &mut region,
            16,
            60,
            75,
            bench.resources.blocks[BlockKind::Sand],
        );
        assert!(sand >= 5, "{sand}");
        let stone = bench.count(
            &mut region,
            16,
            0,
            60,
            bench.resources.blocks[BlockKind::Stone],
        );
        assert!(stone > 0);
    }

    #[test]
    fn springs_need_a_rocky_pocket() {
        let bench = Bench::new();
        let stone = bench.resources.blocks[BlockKind::Stone];
        let mut region = Region::fixture(
            &bench.resources,
            &bench.dimension,
            ChunkPos::new(0, 0),
            0,
            |_, buffer| {
                for y in -64..80 {
                    buffer.fill_layer(y, stone);
                }
            },
        );
        region.set(8, 40, 8, BlockStateId::AIR);
        region.set(8, 40, 9, BlockStateId::AIR);
        region.set(9, 40, 9, BlockStateId::AIR);
        assert!(bench.place(&mut region, "spring_water", (8, 40, 8), 1));
        let water = rspigot_blocks::BlockKind::from_name("minecraft:water")
            .and_then(|kind| kind.state_with(&[("level", "0")]));
        assert!(water.is_some());
        assert_eq!(
            region
                .get(8, 40, 8)
                .kind()
                .map(rspigot_blocks::BlockKind::name),
            Some("minecraft:water")
        );
        region.set(8, 40, 7, BlockStateId::AIR);
        region.set(8, 40, 8, BlockStateId::AIR);
        assert!(!bench.place(&mut region, "spring_water", (8, 40, 8), 1));
    }

    #[test]
    fn lava_lakes_carve_a_basin_with_a_stone_rim() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::Stone, 90);
        assert!(bench.place(&mut region, "lake_lava", (8, 62, 8), 4));
        let lava = bench.count(
            &mut region,
            16,
            50,
            70,
            bench.resources.blocks[BlockKind::Lava],
        );
        let air = bench.count(&mut region, 16, 50, 70, BlockStateId::AIR);
        assert!(lava > 10, "{lava}");
        assert!(air > 10, "{air}");
    }

    #[test]
    fn forest_rocks_rest_on_the_ground() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place(&mut region, "forest_rock", (8, 75, 8), 6));
        let rock = bench.count(
            &mut region,
            16,
            60,
            80,
            named("minecraft:mossy_cobblestone"),
        );
        assert!(rock >= 3, "{rock}");
        assert_eq!(
            bench.count(
                &mut region,
                16,
                60,
                69,
                named("minecraft:mossy_cobblestone")
            ),
            0
        );
    }

    #[test]
    fn bamboo_stalks_grow_with_a_leafy_top() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place(&mut region, "bamboo_no_podzol", (8, 70, 8), 3));
        let top = (70..90)
            .rev()
            .find(|y| !region.get(8, *y, 8).is_air())
            .unwrap_or(0);
        assert!(top >= 75, "{top}");
        assert_eq!(region.get(8, top, 8).property("leaves"), Some("large"));
    }

    #[test]
    fn vines_cling_to_walls() {
        let bench = Bench::new();
        let stone = bench.resources.blocks[BlockKind::Stone];
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        for y in 70..90 {
            region.set(9, y, 8, stone);
        }
        assert!(bench.place(&mut region, "vines", (8, 70, 8), 1));
        let vine = (70..90).map(|y| region.get(8, y, 8)).find(|state| {
            state.kind().map(rspigot_blocks::BlockKind::name) == Some("minecraft:vine")
        });
        assert_eq!(vine.and_then(|state| state.property("east")), Some("true"));
    }

    #[test]
    fn world_generation_is_deterministic_per_seed() {
        let bench = Bench::new();
        let run = |seed: i64| {
            let mut region = bench.floor(BlockKind::GrassBlock, 69);
            bench.place(&mut region, "oak", (8, 70, 8), seed);
            (0..16)
                .flat_map(|y| (0..16).map(move |z| (y + 70, z)))
                .map(|(y, z)| region.get(8, y, z).0)
                .collect::<Vec<_>>()
        };
        assert_eq!(run(9), run(9));
        assert_ne!(run(9), run(10));
    }

    #[test]
    fn an_oak_grows_logs_and_leaves_on_grass() {
        let bench = Bench::new();
        let mut region = bench.floor(BlockKind::GrassBlock, 69);
        assert!(bench.place(&mut region, "oak", (8, 70, 8), 5));
        let log = rspigot_blocks::BlockKind::from_name("minecraft:oak_log")
            .and_then(|k| k.state_with(&[("axis", "y")]));
        let logs = bench.count(&mut region, 16, 70, 90, log.unwrap_or(BlockStateId::AIR));
        assert!(logs >= 4, "{logs}");
        let leaves = (70..90)
            .flat_map(|y| (0..16).flat_map(move |x| (0..16).map(move |z| (x, y, z))))
            .filter(|(x, y, z)| {
                region
                    .get(*x, *y, *z)
                    .kind()
                    .map(rspigot_blocks::BlockKind::name)
                    == Some("minecraft:oak_leaves")
            })
            .count();
        assert!(leaves > 20, "{leaves}");
    }
}
