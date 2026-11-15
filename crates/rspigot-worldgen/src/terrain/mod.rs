mod aquifer;
mod eval;
mod graph;

use rspigot_data::multi_noise::ClimateVariant;
use serde_json::Value;

use crate::climate::{Positional, Xoroshiro};
use crate::datapack::worldgen;
pub(crate) use aquifer::Fluid;
use aquifer::{Aquifer, AquiferIds};
use eval::{Compiled, Sampler, lerp};
use graph::{Compiler, Id, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cell {
    Default,
    Air,
    Water,
    Lava,
}

#[derive(Debug, Clone)]
pub(crate) struct Terrain {
    compiled: Compiled,
    final_density: Id,
    chunk_surface_input: Id,
    ids: AquiferIds,
    aquifer_random: Positional,
    sea_level: i32,
    min_y: i32,
    height: i32,
}

fn variant_key(variant: ClimateVariant) -> &'static str {
    match variant {
        ClimateVariant::Normal => "normal",
        ClimateVariant::LargeBiomes => "large_biomes",
        ClimateVariant::Amplified => "amplified",
    }
}

impl Terrain {
    pub(crate) fn new(seed: i64, variant: ClimateVariant) -> Self {
        let data = &worldgen()["terrain"];
        let settings = &data["variants"][variant_key(variant)];
        let mut compiler = Compiler::new(&data["functions"]);
        let build = |compiler: &mut Compiler<'_>, value: &Value| {
            compiler
                .build(value)
                .unwrap_or_else(|error| panic!("terrain data: {error}"))
        };
        let router = &settings["router"];
        let final_density = build(&mut compiler, &router["final_density"]);
        let chunk_surface = build(&mut compiler, &router["chunk_surface_level"]);
        let aquifers = &settings["aquifers"];
        let ids = AquiferIds {
            barrier: build(&mut compiler, &aquifers["barrier"]),
            floodedness: build(&mut compiler, &aquifers["fluid_level_floodedness"]),
            spread: build(&mut compiler, &aquifers["fluid_level_spread"]),
            lava: build(&mut compiler, &aquifers["lava"]),
            exclusion: build(&mut compiler, &aquifers["exclusion"]),
            surface_level: build(&mut compiler, &aquifers["surface_level"]),
        };
        let graph = compiler.finish();
        let chunk_surface_input = match graph.nodes[chunk_surface as usize] {
            Node::Interpolated { input, .. } => input,
            _ => chunk_surface,
        };
        let roots = [
            final_density,
            chunk_surface_input,
            ids.barrier,
            ids.floodedness,
            ids.spread,
            ids.lava,
            ids.exclusion,
            ids.surface_level,
        ];
        let compiled = Compiled::new(graph, seed, &roots);
        let aquifer_random = Xoroshiro::from_seed(seed)
            .fork_positional()
            .named("minecraft:aquifer")
            .fork_positional();
        Self {
            compiled,
            final_density,
            chunk_surface_input,
            ids,
            aquifer_random,
            sea_level: settings["sea_level"].as_i64().unwrap_or(63) as i32,
            min_y: settings["min_y"].as_i64().unwrap_or(-64) as i32,
            height: settings["height"].as_i64().unwrap_or(384) as i32,
        }
    }

    pub(crate) fn sampler(&self) -> Sampler<'_> {
        Sampler::new(&self.compiled)
    }

    pub(crate) fn min_y(&self) -> i32 {
        self.min_y
    }

    pub(crate) fn height(&self) -> i32 {
        self.height
    }

    pub(crate) fn fill(&self, sampler: &mut Sampler<'_>, x0: i32, z0: i32) -> Vec<Cell> {
        sampler.begin_chunk(x0, z0, self.min_y, self.height);
        let mut aquifer = Aquifer::new(
            sampler,
            self.ids,
            self.aquifer_random,
            self.sea_level,
            x0,
            z0,
            self.min_y,
            self.height,
        );
        let mut cells = vec![Cell::Default; (16 * 16 * self.height) as usize];
        for z in 0..16 {
            for x in 0..16 {
                for yy in (0..self.height).rev() {
                    let y = self.min_y + yy;
                    let density = sampler.eval(self.final_density, x0 + x, y, z0 + z);
                    let cell =
                        match aquifer.substance(sampler, x0 + x, y, z0 + z, f64::from(density)) {
                            None => Cell::Default,
                            Some(Fluid::Air) => Cell::Air,
                            Some(Fluid::Water) => Cell::Water,
                            Some(Fluid::Lava) => Cell::Lava,
                        };
                    cells[((yy * 16 + z) * 16 + x) as usize] = cell;
                }
            }
        }
        cells
    }

    pub(crate) fn chunk_surface_levels(
        &self,
        sampler: &mut Sampler<'_>,
        x0: i32,
        z0: i32,
    ) -> [f32; 256] {
        let mut corner =
            |dx: i32, dz: i32| sampler.eval(self.chunk_surface_input, x0 + dx, 0, z0 + dz);
        let (c00, c10, c01, c11) = (corner(0, 0), corner(16, 0), corner(0, 16), corner(16, 16));
        let inv = 1.0f32 / 16.0;
        std::array::from_fn(|i| {
            let (x, z) = ((i & 15) as f32, (i >> 4) as f32);
            let tz = z * inv;
            let a = lerp(tz, c00, c01);
            let b = lerp(tz, c10, c11);
            lerp(x * inv, a, b)
        })
    }

    pub(crate) fn density_at(&self, sampler: &mut Sampler<'_>, x: i32, y: i32, z: i32) -> f32 {
        sampler.eval(self.final_density, x, y, z)
    }

    pub(crate) fn column(&self, x: i32, z: i32) -> Vec<Cell> {
        let mut sampler = self.sampler();
        let cells = self.fill(&mut sampler, x & !15, z & !15);
        let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
        (0..self.height as usize)
            .map(|yy| cells[(yy * 16 + lz) * 16 + lx])
            .collect()
    }
}

#[cfg(test)]
mod tests;
