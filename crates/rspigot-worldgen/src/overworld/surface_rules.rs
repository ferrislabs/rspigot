use std::cell::Cell;

use rspigot_data::multi_noise::overworld_biomes;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use crate::buffer::ChunkBuffer;
use crate::climate::{NoiseStack, Positional, Xoroshiro};
use crate::datapack::{block_state, noise_parameters, state_named, worldgen};
use crate::resources::{BlockKind, Palette};
use crate::temperature::{BiomeClimate, TemperatureNoise};

const WAY_BELOW: i32 = i32::MIN / 2;
const NO_WATER: i32 = i32::MIN;
const BAND_COUNT: usize = 192;
const DEEP_LIMIT: i32 = 48;
const SURFACE_NOISE_COUNT: usize = 9;

#[derive(Debug, Clone, Copy)]
enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

#[derive(Debug, Clone)]
enum Cond {
    Never,
    Biome(u128),
    Noise {
        index: usize,
        min: f64,
        max: f64,
        three_d: bool,
    },
    StoneDepth {
        offset: i32,
        add_surface: bool,
        range: i32,
        ceiling: bool,
    },
    Y {
        anchor: Anchor,
        multiplier: i32,
        add_stone: bool,
    },
    Water {
        offset: i32,
        multiplier: i32,
        add_stone: bool,
    },
    Not(Box<Cond>),
    Hole,
    Steep,
    AbovePreliminary,
    Gradient {
        factory: Positional,
        below: Anchor,
        above: Anchor,
    },
}

#[derive(Debug, Clone)]
enum Rule {
    Sequence(Vec<Rule>),
    If(Cond, Box<Rule>),
    Block(Option<BlockStateId>),
    Bands,
}

struct Compiler<'a> {
    names: Vec<&'static str>,
    noise_names: Vec<String>,
    seed_random: &'a Positional,
}

impl Compiler<'_> {
    fn anchor(value: &Value) -> Anchor {
        let get = |key: &str| value.get(key).and_then(Value::as_i64).map(|n| n as i32);
        if let Some(n) = get("absolute") {
            Anchor::Absolute(n)
        } else if let Some(n) = get("above_bottom") {
            Anchor::AboveBottom(n)
        } else {
            Anchor::BelowTop(get("below_top").unwrap_or(0))
        }
    }

    fn biome_set(&self, value: &Value) -> u128 {
        let mut set = 0u128;
        let mut add = |name: &str| {
            if let Some(index) = self.names.iter().position(|candidate| *candidate == name) {
                set |= 1u128 << index;
            }
        };
        match value {
            Value::String(name) => add(name),
            Value::Array(items) => items.iter().filter_map(Value::as_str).for_each(add),
            _ => {}
        }
        set
    }

    fn noise_index(&mut self, name: &str) -> usize {
        if let Some(index) = self.noise_names.iter().position(|n| n == name) {
            return index;
        }
        self.noise_names.push(name.to_owned());
        self.noise_names.len() - 1
    }

    fn int(value: &Value, key: &str) -> i32 {
        value.get(key).and_then(Value::as_i64).unwrap_or(0) as i32
    }

    fn flag(value: &Value, key: &str) -> bool {
        value.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    fn condition(&mut self, value: &Value) -> Cond {
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "minecraft:biome" => {
                Cond::Biome(self.biome_set(value.get("biome_is").unwrap_or(&Value::Null)))
            }
            "minecraft:noise_threshold" => {
                let name = value.get("noise").and_then(Value::as_str).unwrap_or("");
                Cond::Noise {
                    index: self.noise_index(name),
                    min: value
                        .get("min_threshold")
                        .and_then(Value::as_f64)
                        .unwrap_or(f64::MIN),
                    max: value
                        .get("max_threshold")
                        .and_then(Value::as_f64)
                        .unwrap_or(f64::MAX),
                    three_d: Self::flag(value, "is_3d"),
                }
            }
            "minecraft:stone_depth" => Cond::StoneDepth {
                offset: Self::int(value, "offset"),
                add_surface: Self::flag(value, "add_surface_depth"),
                range: Self::int(value, "secondary_depth_range"),
                ceiling: value.get("surface_type").and_then(Value::as_str) == Some("ceiling"),
            },
            "minecraft:y_above" => Cond::Y {
                anchor: Self::anchor(value.get("anchor").unwrap_or(&Value::Null)),
                multiplier: Self::int(value, "surface_depth_multiplier"),
                add_stone: Self::flag(value, "add_stone_depth"),
            },
            "minecraft:water" => Cond::Water {
                offset: Self::int(value, "offset"),
                multiplier: Self::int(value, "surface_depth_multiplier"),
                add_stone: Self::flag(value, "add_stone_depth"),
            },
            "minecraft:not" => Cond::Not(Box::new(
                self.condition(value.get("invert").unwrap_or(&Value::Null)),
            )),
            "minecraft:hole" => Cond::Hole,
            "minecraft:steep" => Cond::Steep,
            "minecraft:above_preliminary_surface" => Cond::AbovePreliminary,
            "minecraft:vertical_gradient" => {
                let name = value
                    .get("random_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Cond::Gradient {
                    factory: self.seed_random.named(name).fork_positional(),
                    below: Self::anchor(value.get("true_at_and_below").unwrap_or(&Value::Null)),
                    above: Self::anchor(value.get("false_at_and_above").unwrap_or(&Value::Null)),
                }
            }
            _ => Cond::Never,
        }
    }

    fn rule(&mut self, value: &Value) -> Rule {
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "minecraft:sequence" => Rule::Sequence(
                value
                    .get("sequence")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(|item| self.rule(item)).collect())
                    .unwrap_or_default(),
            ),
            "minecraft:condition" => Rule::If(
                self.condition(value.get("if_true").unwrap_or(&Value::Null)),
                Box::new(self.rule(value.get("then_run").unwrap_or(&Value::Null))),
            ),
            "minecraft:block" => Rule::Block(value.get("result_state").and_then(block_state)),
            "minecraft:bandlands" => Rule::Bands,
            _ => Rule::Sequence(Vec::new()),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SurfaceSystem {
    surface: Rule,
    underground: Rule,
    noises: Vec<NoiseStack>,
    surface_noise: NoiseStack,
    secondary: NoiseStack,
    clay_offset: NoiseStack,
    pillar: NoiseStack,
    pillar_roof: NoiseStack,
    badlands: NoiseStack,
    iceberg_pillar: NoiseStack,
    iceberg_roof: NoiseStack,
    iceberg_surface: NoiseStack,
    bands: Vec<BlockStateId>,
    random: Positional,
    stone: BlockStateId,
    water: BlockStateId,
    lava: BlockStateId,
    packed_ice: BlockStateId,
    snow_block: BlockStateId,
    eroded_badlands: Option<u8>,
    frozen_oceans: [Option<u8>; 2],
    climates: Vec<BiomeClimate>,
    temperature: TemperatureNoise,
}

fn stack(random: &Positional, name: &str) -> NoiseStack {
    let parameters = noise_parameters(name).unwrap_or_else(|| {
        panic!("noise {name} missing from the worldgen data");
    });
    NoiseStack::create(&parameters, &mut random.named(name))
}

fn generate_bands(random: &mut Xoroshiro, palette: &Palette) -> Vec<BlockStateId> {
    let band = |name: &str| state_named(name, &[]).unwrap_or(palette[BlockKind::Stone]);
    let terracotta = band("minecraft:terracotta");
    let mut bands = vec![terracotta; BAND_COUNT];
    let mut i = 0usize;
    while i < BAND_COUNT {
        i += random.next_int(5) as usize + 1;
        if i < BAND_COUNT {
            bands[i] = band("minecraft:orange_terracotta");
        }
        i += 1;
    }
    let mut make = |random: &mut Xoroshiro, base: i32, state: BlockStateId| {
        let count = random.between(6, 15);
        for _ in 0..count {
            let width = base + random.next_int(3) as i32;
            let start = random.next_int(BAND_COUNT as u32) as usize;
            let mut offset = 0;
            while start + offset < BAND_COUNT && (offset as i32) < width {
                bands[start + offset] = state;
                offset += 1;
            }
        }
    };
    make(random, 1, band("minecraft:yellow_terracotta"));
    make(random, 2, band("minecraft:brown_terracotta"));
    make(random, 1, band("minecraft:red_terracotta"));
    let whites = random.between(9, 15);
    let (white, gray) = (
        band("minecraft:white_terracotta"),
        band("minecraft:light_gray_terracotta"),
    );
    let mut position = 0usize;
    for _ in 0..whites {
        if position >= BAND_COUNT {
            break;
        }
        bands[position] = white;
        if position > 1 && random.next_boolean() {
            bands[position - 1] = gray;
        }
        if position + 1 < BAND_COUNT && random.next_boolean() {
            bands[position + 1] = gray;
        }
        position += random.next_int(16) as usize + 4;
    }
    bands
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) min_y: i32,
    pub(crate) max_y: i32,
    pub(crate) sea_level: i32,
}

#[derive(Clone, Copy)]
struct Site {
    bounds: Bounds,
    x: usize,
    z: usize,
    world_x: i32,
    world_z: i32,
}

pub(crate) struct ColumnInput<'a, F: Fn(i32) -> u8> {
    pub(crate) bounds: Bounds,
    pub(crate) buffer: &'a mut ChunkBuffer,
    pub(crate) x: usize,
    pub(crate) z: usize,
    pub(crate) world_x: i32,
    pub(crate) world_z: i32,
    pub(crate) top: i32,
    pub(crate) gradient: (i32, i32),
    pub(crate) biome_at: F,
}

struct ColumnState<'a> {
    system: &'a SurfaceSystem,
    bounds: Bounds,
    world_x: i32,
    world_z: i32,
    surface_depth: i32,
    secondary: Cell<Option<f64>>,
    gradient: (i32, i32),
    min_surface_level: i32,
    cache: [Cell<f32>; 24],
    y: i32,
    stone_above: i32,
    stone_below: i32,
    water_height: i32,
    biome: Cell<Option<u8>>,
}

impl SurfaceSystem {
    pub(crate) fn new(seed: i64, palette: &Palette) -> Self {
        let random = Xoroshiro::from_seed(seed).fork_positional();
        let names: Vec<&'static str> = overworld_biomes().to_vec();
        let data = worldgen();
        let mut compiler = Compiler {
            names: names.clone(),
            noise_names: Vec::new(),
            seed_random: &random,
        };
        let rules = data.get("surface_rule").cloned().unwrap_or(Value::Null);
        let surface = compiler.rule(rules.get("surface").unwrap_or(&Value::Null));
        let underground = compiler.rule(rules.get("underground").unwrap_or(&Value::Null));
        let noises = compiler
            .noise_names
            .iter()
            .map(|name| stack(&random, name))
            .collect();
        let climates = names
            .iter()
            .map(|name| {
                let biome = data.get("biomes").and_then(|b| b.get(*name));
                BiomeClimate {
                    temperature: biome
                        .and_then(|b| b.get("temperature"))
                        .and_then(Value::as_f64)
                        .unwrap_or(0.5) as f32,
                    frozen: biome
                        .and_then(|b| b.get("temperature_modifier"))
                        .and_then(Value::as_str)
                        == Some("frozen"),
                    precipitation: biome
                        .and_then(|b| b.get("has_precipitation"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                }
            })
            .collect();
        let index_of = |name: &str| names.iter().position(|n| *n == name).map(|i| i as u8);
        let bands = generate_bands(&mut random.named("minecraft:clay_bands"), palette);
        let _ = SURFACE_NOISE_COUNT;
        Self {
            surface,
            underground,
            noises,
            surface_noise: stack(&random, "minecraft:surface"),
            secondary: stack(&random, "minecraft:surface_secondary"),
            clay_offset: stack(&random, "minecraft:clay_bands_offset"),
            pillar: stack(&random, "minecraft:badlands_pillar"),
            pillar_roof: stack(&random, "minecraft:badlands_pillar_roof"),
            badlands: stack(&random, "minecraft:badlands_surface"),
            iceberg_pillar: stack(&random, "minecraft:iceberg_pillar"),
            iceberg_roof: stack(&random, "minecraft:iceberg_pillar_roof"),
            iceberg_surface: stack(&random, "minecraft:iceberg_surface"),
            bands,
            random,
            stone: palette[BlockKind::Stone],
            water: palette[BlockKind::Water],
            lava: palette[BlockKind::Lava],
            packed_ice: state_named("minecraft:packed_ice", &[]).unwrap_or(BlockStateId::AIR),
            snow_block: palette[BlockKind::SnowBlock],
            eroded_badlands: index_of("minecraft:eroded_badlands"),
            frozen_oceans: [
                index_of("minecraft:frozen_ocean"),
                index_of("minecraft:deep_frozen_ocean"),
            ],
            climates,
            temperature: TemperatureNoise::new(),
        }
    }

    pub(crate) fn warm_enough_to_rain(
        &self,
        biome: u8,
        x: i32,
        y: i32,
        z: i32,
        sea_level: i32,
    ) -> bool {
        self.temperature
            .warm_enough_to_rain(self.climates[usize::from(biome)], x, y, z, sea_level)
    }

    pub(crate) fn info_noise(&self, x: f64, z: f64) -> f64 {
        self.temperature.info(x, z)
    }

    pub(crate) fn surface_depth(&self, x: i32, z: i32) -> i32 {
        let noise = f64::from(self.surface_noise.get(f64::from(x), 0.0, f64::from(z)));
        let jitter = self.random.at(x, 0, z).next_double();
        (noise * 2.75 + 3.0 + jitter * 0.25) as i32
    }

    pub(crate) fn secondary_value(&self, x: i32, z: i32) -> f64 {
        f64::from(self.secondary.get(f64::from(x), 0.0, f64::from(z)))
    }

    pub(crate) fn band(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        let offset = (f64::from(self.clay_offset.get(f64::from(x), 0.0, f64::from(z))) * 4.0 + 0.5)
            .floor() as i32;
        let length = self.bands.len() as i32;
        self.bands[(y + offset + length).rem_euclid(length) as usize]
    }

    fn is_fluid(&self, state: BlockStateId) -> bool {
        state == self.water || state == self.lava
    }

    pub(crate) fn column<F: Fn(i32) -> u8>(&self, input: ColumnInput<'_, F>) {
        let ColumnInput {
            bounds,
            buffer,
            x,
            z,
            world_x,
            world_z,
            top,
            gradient,
            biome_at,
        } = input;
        let highest = top.max(bounds.sea_level - 1);
        let start_biome = biome_at(highest + 1);
        let site = Site {
            bounds,
            x,
            z,
            world_x,
            world_z,
        };
        if self.eroded_badlands == Some(start_biome) {
            self.eroded_badlands_extension(&site, buffer, highest + 2);
        }
        let surface_depth = self.surface_depth(world_x, world_z);
        let mut state = ColumnState {
            system: self,
            bounds,
            world_x,
            world_z,
            surface_depth,
            secondary: Cell::new(None),
            gradient,
            min_surface_level: top + surface_depth - 8,
            cache: std::array::from_fn(|_| Cell::new(f32::NAN)),
            y: 0,
            stone_above: 0,
            stone_below: 0,
            water_height: NO_WATER,
            biome: Cell::new(None),
        };
        let mut run_bottom = i32::MAX;
        for y in (bounds.min_y..=highest + 1).rev() {
            let block = buffer.get(x, y, z);
            if block.is_air() {
                state.stone_above = 0;
                state.water_height = NO_WATER;
                continue;
            }
            if self.is_fluid(block) {
                if state.water_height == NO_WATER {
                    state.water_height = y + 1;
                }
                continue;
            }
            if y <= run_bottom {
                run_bottom = WAY_BELOW;
                let mut below = y - 1;
                while below >= bounds.min_y - 1 {
                    let candidate = buffer.get(x, below, z);
                    if candidate.is_air() || self.is_fluid(candidate) {
                        run_bottom = below + 1;
                        break;
                    }
                    below -= 1;
                }
            }
            state.stone_above += 1;
            state.stone_below = y - run_bottom + 1;
            if state.stone_above > DEEP_LIMIT && state.stone_below > DEEP_LIMIT {
                continue;
            }
            state.y = y;
            state.biome.set(None);
            let biome = || {
                state.biome.get().unwrap_or_else(|| {
                    let value = biome_at(y);
                    state.biome.set(Some(value));
                    value
                })
            };
            let rule = if y >= state.min_surface_level {
                &self.surface
            } else {
                &self.underground
            };
            if let Some(result) = state.apply(rule, &biome) {
                buffer.set(x, y, z, result);
            }
        }
        if self.frozen_oceans.contains(&Some(start_biome)) {
            self.frozen_ocean_extension(
                &site,
                buffer,
                (state.min_surface_level, highest + 2),
                start_biome,
            );
        }
    }

    fn eroded_badlands_extension(&self, site: &Site, buffer: &mut ChunkBuffer, height: i32) {
        let Site {
            bounds,
            x,
            z,
            world_x: wx,
            world_z: wz,
        } = *site;
        let (fx, fz) = (f64::from(wx), f64::from(wz));
        let surface = (f64::from(self.badlands.get(fx, 0.0, fz)) * 8.25).abs();
        let pillar = f64::from(self.pillar.get(fx * 0.2, 0.0, fz * 0.2)) * 15.0;
        let mut depth = surface.min(pillar);
        if depth <= 0.0 {
            return;
        }
        let roof = (f64::from(self.pillar_roof.get(fx * 0.75, 0.0, fz * 0.75)) * 1.5).abs();
        let roof = (roof * 50.0).ceil();
        depth = (depth * depth * 2.5).min(roof + 24.0);
        let top = (depth + 64.0).floor() as i32;
        if top < height {
            return;
        }
        let mut y = top;
        while y >= bounds.min_y {
            let block = buffer.get(x, y, z);
            if block == self.stone {
                break;
            }
            if block == self.water {
                return;
            }
            y -= 1;
        }
        let mut y = top;
        while y >= bounds.min_y {
            if !buffer.get(x, y, z).is_air() {
                break;
            }
            buffer.set(x, y, z, self.stone);
            y -= 1;
        }
    }

    fn frozen_ocean_extension(
        &self,
        site: &Site,
        buffer: &mut ChunkBuffer,
        (min_surface_level, height): (i32, i32),
        biome: u8,
    ) {
        let Site {
            bounds,
            x,
            z,
            world_x: wx,
            world_z: wz,
        } = *site;
        let (fx, fz) = (f64::from(wx), f64::from(wz));
        let surface = (f64::from(self.iceberg_surface.get(fx, 0.0, fz)) * 8.25).abs();
        let pillar = f64::from(self.iceberg_pillar.get(fx * 1.28, 0.0, fz * 1.28)) * 15.0;
        let mut depth = surface.min(pillar);
        if depth <= 1.8 {
            return;
        }
        let roof = (f64::from(self.iceberg_roof.get(fx * 1.17, 0.0, fz * 1.17)) * 1.5).abs();
        let roof = (roof * 40.0).ceil();
        depth = (depth * depth * 1.2).min(roof + 14.0);
        let sea = bounds.sea_level;
        let climate = self.climates[usize::from(biome)];
        if self.temperature.at(climate, wx, sea, wz, sea) > 0.1 {
            depth -= 2.0;
        }
        if depth <= 2.0 {
            return;
        }
        let lower = f64::from(sea) - depth - 7.0;
        let upper = depth + f64::from(sea);
        let mut random = self.random.at(wx, 0, wz);
        let snow_limit = random.next_int(4) as i32 + 2;
        let snow_floor = sea + 18 + random.next_int(10) as i32;
        let mut snow = 0;
        let mut y = height.max(upper as i32 + 1);
        while y >= min_surface_level {
            let block = buffer.get(x, y, z);
            let freeze = if block.is_air() {
                upper as i32 > y && random.next_double() > 0.01
            } else {
                false
            } || (block == self.water
                && (lower as i32) < y
                && y < sea
                && random.next_double() <= 0.15);
            if freeze {
                if snow <= snow_limit && snow_floor < y {
                    buffer.set(x, y, z, self.snow_block);
                    snow += 1;
                } else {
                    buffer.set(x, y, z, self.packed_ice);
                }
            }
            y -= 1;
        }
    }
}

impl ColumnState<'_> {
    fn resolve(&self, anchor: Anchor) -> i32 {
        match anchor {
            Anchor::Absolute(n) => n,
            Anchor::AboveBottom(n) => self.bounds.min_y + n,
            Anchor::BelowTop(n) => self.bounds.max_y - n,
        }
    }

    fn secondary(&self) -> f64 {
        if let Some(value) = self.secondary.get() {
            return value;
        }
        let value = self.system.secondary_value(self.world_x, self.world_z);
        self.secondary.set(Some(value));
        value
    }

    fn apply(&self, rule: &Rule, biome: &impl Fn() -> u8) -> Option<BlockStateId> {
        match rule {
            Rule::Block(state) => *state,
            Rule::Bands => Some(self.system.band(self.world_x, self.y, self.world_z)),
            Rule::If(condition, then) => {
                if self.test(condition, biome) {
                    self.apply(then, biome)
                } else {
                    None
                }
            }
            Rule::Sequence(rules) => rules.iter().find_map(|rule| self.apply(rule, biome)),
        }
    }

    fn test(&self, condition: &Cond, biome: &impl Fn() -> u8) -> bool {
        match condition {
            Cond::Never => false,
            Cond::Biome(set) => set & (1u128 << biome()) != 0,
            Cond::Noise {
                index,
                min,
                max,
                three_d,
            } => {
                let value = if *three_d {
                    f64::from(self.system.noises[*index].get(
                        f64::from(self.world_x),
                        f64::from(self.y),
                        f64::from(self.world_z),
                    ))
                } else {
                    let slot = &self.cache[*index];
                    if slot.get().is_nan() {
                        slot.set(self.system.noises[*index].get(
                            f64::from(self.world_x),
                            0.0,
                            f64::from(self.world_z),
                        ));
                    }
                    f64::from(slot.get())
                };
                value >= *min && value <= *max
            }
            Cond::StoneDepth {
                offset,
                add_surface,
                range,
                ceiling,
            } => {
                let depth = if *ceiling {
                    self.stone_below
                } else {
                    self.stone_above
                };
                let surface = if *add_surface { self.surface_depth } else { 0 };
                let secondary = if *range == 0 {
                    0
                } else {
                    let t = (self.secondary() + 1.0) / 2.0;
                    (t * f64::from(*range)) as i32
                };
                depth <= offset + 1 + surface + secondary
            }
            Cond::Y {
                anchor,
                multiplier,
                add_stone,
            } => {
                let stone = if *add_stone { self.stone_above } else { 0 };
                self.resolve(*anchor) + self.surface_depth * multiplier <= self.y + stone
            }
            Cond::Water {
                offset,
                multiplier,
                add_stone,
            } => {
                if self.water_height == NO_WATER {
                    return true;
                }
                let stone = if *add_stone { self.stone_above } else { 0 };
                self.y + stone >= self.water_height + offset + self.surface_depth * multiplier
            }
            Cond::Not(inner) => !self.test(inner, biome),
            Cond::Hole => self.surface_depth < 1,
            Cond::Steep => self.gradient.0 < -3 || self.gradient.1 > 3,
            Cond::AbovePreliminary => self.y >= self.min_surface_level,
            Cond::Gradient {
                factory,
                below,
                above,
            } => {
                let (below, above) = (self.resolve(*below), self.resolve(*above));
                if self.y <= below {
                    true
                } else if self.y < above {
                    let chance = 1.0 - f64::from(self.y - below) / f64::from(above - below);
                    let roll = factory.at(self.world_x, self.y, self.world_z).next_float();
                    f64::from(roll) < chance
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Surface = (i64, i32, i32, i32, i32, f64, &'static str);
    type Temperature = (&'static str, i32, i32, i32, bool, bool);

    const SURFACE: [Surface; 24] = [
        (
            1,
            -1015,
            -1412,
            -13,
            2,
            0.4732336699962616,
            "minecraft:terracotta",
        ),
        (
            1,
            313,
            -1746,
            44,
            1,
            -0.04796736687421799,
            "minecraft:orange_terracotta",
        ),
        (
            1,
            -1566,
            606,
            118,
            2,
            -0.07118625938892365,
            "minecraft:light_gray_terracotta",
        ),
        (
            1,
            -252,
            -1431,
            13,
            2,
            0.14719833433628082,
            "minecraft:yellow_terracotta",
        ),
        (
            1,
            -1683,
            1263,
            102,
            4,
            0.727783739566803,
            "minecraft:red_terracotta",
        ),
        (
            1,
            -766,
            -408,
            2,
            2,
            0.05043351277709007,
            "minecraft:brown_terracotta",
        ),
        (
            1,
            -1404,
            -1811,
            116,
            1,
            -0.29633066058158875,
            "minecraft:light_gray_terracotta",
        ),
        (
            1,
            -1668,
            -690,
            39,
            4,
            -0.12259416282176971,
            "minecraft:orange_terracotta",
        ),
        (
            1,
            674,
            959,
            38,
            3,
            -0.12006084620952606,
            "minecraft:terracotta",
        ),
        (
            1,
            -1847,
            -563,
            42,
            1,
            0.04957900568842888,
            "minecraft:terracotta",
        ),
        (
            1,
            -795,
            854,
            -60,
            1,
            -0.05810101702809334,
            "minecraft:orange_terracotta",
        ),
        (
            1,
            1006,
            1363,
            95,
            3,
            -0.8059679269790649,
            "minecraft:terracotta",
        ),
        (
            12345,
            251,
            -920,
            -19,
            1,
            0.1779012680053711,
            "minecraft:brown_terracotta",
        ),
        (
            12345,
            -1172,
            55,
            24,
            3,
            -0.0107201486825943,
            "minecraft:terracotta",
        ),
        (
            12345,
            -625,
            802,
            41,
            4,
            -0.18276217579841614,
            "minecraft:light_gray_terracotta",
        ),
        (
            12345,
            1389,
            1517,
            82,
            2,
            0.44732484221458435,
            "minecraft:white_terracotta",
        ),
        (
            12345,
            1390,
            806,
            -48,
            4,
            -0.22957494854927063,
            "minecraft:yellow_terracotta",
        ),
        (
            12345,
            384,
            787,
            43,
            3,
            -0.437375545501709,
            "minecraft:white_terracotta",
        ),
        (
            12345,
            532,
            1175,
            -59,
            4,
            0.6819863319396973,
            "minecraft:light_gray_terracotta",
        ),
        (
            12345,
            351,
            792,
            56,
            3,
            -0.3404501974582672,
            "minecraft:terracotta",
        ),
        (
            12345,
            428,
            781,
            -35,
            2,
            -0.10830725729465485,
            "minecraft:light_gray_terracotta",
        ),
        (
            12345,
            -57,
            1871,
            -21,
            1,
            -0.062467824667692184,
            "minecraft:terracotta",
        ),
        (
            12345,
            1729,
            1397,
            41,
            4,
            0.13049504160881042,
            "minecraft:light_gray_terracotta",
        ),
        (
            12345,
            825,
            903,
            95,
            1,
            0.05543487146496773,
            "minecraft:orange_terracotta",
        ),
    ];

    const TEMPERATURE: [Temperature; 60] = [
        ("plains", 487, 174, -908, false, true),
        ("plains", 424, 165, -494, false, true),
        ("plains", 854, 102, -309, false, true),
        ("plains", 321, 43, 831, false, true),
        ("plains", -253, 168, -440, false, true),
        ("plains", -910, 155, 978, false, true),
        ("plains", -983, 88, -983, false, true),
        ("plains", 165, 216, -192, false, true),
        ("plains", -239, 180, 402, false, true),
        ("plains", -353, 60, 715, false, true),
        ("frozen_ocean", -311, 77, 292, false, true),
        ("frozen_ocean", 129, 210, 707, true, false),
        ("frozen_ocean", -54, 149, -457, true, false),
        ("frozen_ocean", 633, 51, -116, true, false),
        ("frozen_ocean", 870, 181, 357, true, false),
        ("frozen_ocean", -802, 62, -548, false, true),
        ("frozen_ocean", -947, 185, 595, true, false),
        ("frozen_ocean", 805, 177, -288, true, false),
        ("frozen_ocean", 503, 159, 912, true, false),
        ("frozen_ocean", -100, 171, 947, true, false),
        ("deep_frozen_ocean", -901, 187, 251, true, false),
        ("deep_frozen_ocean", 326, 85, -702, false, true),
        ("deep_frozen_ocean", 733, 204, 697, true, false),
        ("deep_frozen_ocean", -482, 176, 49, false, true),
        ("deep_frozen_ocean", -342, 155, -970, true, true),
        ("deep_frozen_ocean", 57, 140, -231, false, true),
        ("deep_frozen_ocean", 413, 185, 399, false, true),
        ("deep_frozen_ocean", 73, 83, 684, false, true),
        ("deep_frozen_ocean", 156, 88, -909, false, true),
        ("deep_frozen_ocean", -494, 197, -109, false, true),
        ("snowy_plains", 843, 40, 111, true, false),
        ("snowy_plains", -899, 125, 600, true, false),
        ("snowy_plains", -973, 154, -531, true, false),
        ("snowy_plains", 60, 53, -425, true, false),
        ("snowy_plains", -160, 164, 968, true, false),
        ("snowy_plains", -851, 90, -77, true, false),
        ("snowy_plains", 764, 86, 179, true, false),
        ("snowy_plains", 55, 88, 563, true, false),
        ("snowy_plains", 98, 108, -396, true, false),
        ("snowy_plains", -189, 140, -557, true, false),
        ("taiga", 253, 104, 524, false, true),
        ("taiga", -430, 58, -362, false, true),
        ("taiga", -477, 211, -609, true, false),
        ("taiga", 292, 200, -424, true, true),
        ("taiga", -795, 108, -753, false, true),
        ("taiga", 385, 165, -684, true, true),
        ("taiga", -95, 134, 744, false, true),
        ("taiga", -501, 125, -127, false, true),
        ("taiga", 6, 209, -104, true, false),
        ("taiga", 992, 129, 764, false, true),
        ("jagged_peaks", -623, 62, 541, true, false),
        ("jagged_peaks", 761, 125, -571, true, false),
        ("jagged_peaks", 617, 122, -866, true, false),
        ("jagged_peaks", -515, 59, -81, true, false),
        ("jagged_peaks", 399, 136, 924, true, false),
        ("jagged_peaks", -815, 211, 328, true, false),
        ("jagged_peaks", -617, 66, 649, true, false),
        ("jagged_peaks", 909, 203, -953, true, false),
        ("jagged_peaks", 621, 63, -323, true, false),
        ("jagged_peaks", -23, 130, -978, true, false),
    ];

    fn system(seed: i64) -> SurfaceSystem {
        SurfaceSystem::new(seed, &Palette::resolve())
    }

    #[test]
    fn surface_depth_secondary_noise_and_bands_match_the_decompiled_vanilla() {
        for (seed, x, z, y, depth, secondary, band) in SURFACE {
            let system = system(seed);
            assert_eq!(system.surface_depth(x, z), depth, "depth {seed} {x} {z}");
            assert_eq!(
                system.secondary_value(x, z),
                secondary,
                "secondary {seed} {x} {z}"
            );
            let name = system
                .band(x, y, z)
                .kind()
                .map(rspigot_blocks::BlockKind::name);
            assert_eq!(name, Some(band), "band {seed} {x} {y} {z}");
        }
    }

    fn render(biome: &str, top: i32, sea: i32) -> Vec<&'static str> {
        let palette = Palette::resolve();
        let system = system(1);
        let index = overworld_biomes().iter().position(|n| *n == biome).unwrap() as u8;
        let mut buffer = ChunkBuffer::new(rspigot_domain::world::WorldHeight::OVERWORLD);
        for y in -64..=top {
            buffer.fill_layer(y, palette[BlockKind::Stone]);
        }
        for y in (top + 1)..sea {
            buffer.fill_layer(y, palette[BlockKind::Water]);
        }
        system.column(ColumnInput {
            bounds: Bounds {
                min_y: -64,
                max_y: 319,
                sea_level: sea,
            },
            buffer: &mut buffer,
            x: 3,
            z: 3,
            world_x: 3,
            world_z: 3,
            top,
            gradient: (0, 0),
            biome_at: |_| index,
        });
        (top - 8..=top + 1)
            .rev()
            .map(|y| buffer.get(3, y, 3).kind().map(|k| k.name()).unwrap_or("?"))
            .collect()
    }

    #[test]
    fn plains_are_grass_over_dirt_over_stone() {
        let column = render("minecraft:plains", 70, 63);
        assert_eq!(column[0], "minecraft:air");
        assert_eq!(column[1], "minecraft:grass_block");
        assert_eq!(column[2], "minecraft:dirt");
        assert_eq!(column[8], "minecraft:stone");
    }

    #[test]
    fn deserts_are_sand_over_sandstone() {
        let column = render("minecraft:desert", 70, 63);
        assert_eq!(column[1], "minecraft:sand");
        assert!(column.contains(&"minecraft:sandstone"));
    }

    #[test]
    fn badlands_show_terracotta_bands() {
        let column = render("minecraft:badlands", 90, 63);
        assert!(
            column.iter().any(|name| name.ends_with("terracotta")),
            "{column:?}"
        );
    }

    #[test]
    fn ocean_floors_are_not_grass() {
        let column = render("minecraft:ocean", 50, 63);
        assert!(!column.contains(&"minecraft:grass_block"), "{column:?}");
        assert!(column[1] != "minecraft:stone", "{column:?}");
    }

    #[test]
    fn frozen_peaks_are_snow_capped_stone() {
        let column = render("minecraft:frozen_peaks", 160, 63);
        assert!(
            column[1].contains("snow")
                || column[1].contains("ice")
                || column[1] == "minecraft:stone",
            "{column:?}"
        );
    }

    #[test]
    fn temperature_and_iceberg_melting_match_the_decompiled_vanilla() {
        let system = system(1);
        let names = overworld_biomes();
        for (name, x, y, z, cold, melt) in TEMPERATURE {
            let full = format!("minecraft:{name}");
            let Some(index) = names.iter().position(|n| *n == full) else {
                continue;
            };
            let climate = system.climates[index];
            assert_eq!(
                system.temperature.cold_enough_to_snow(climate, x, y, z, 63),
                cold,
                "{name} {x} {y} {z}"
            );
            assert_eq!(
                system.temperature.at(climate, x, y, z, 63) > 0.1,
                melt,
                "{name} {x} {y} {z}"
            );
        }
    }
}
