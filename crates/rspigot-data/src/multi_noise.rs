use crate::generated::multi_noise as table;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseParameters {
    pub id: &'static str,
    pub base_octave: i32,
    pub octave_count: u32,
    pub base_amplitude: f64,
    pub amplitude_modifiers: &'static [f64],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DensityNode {
    Constant(f32),
    Noise {
        noise: u16,
        xz_scale: f64,
        y_scale: f64,
        shift_x: u16,
        shift_z: u16,
    },
    ShiftA {
        noise: u16,
    },
    ShiftB {
        noise: u16,
    },
    Gradient {
        from: i32,
        min: i32,
        max: i32,
        from_value: f32,
        factor: f32,
    },
    Add {
        left: u16,
        right: u16,
    },
    Mul {
        left: u16,
        right: u16,
    },
    Abs {
        input: u16,
    },
    Lerp {
        alpha: u16,
        first: u16,
        second: u16,
    },
    Spline {
        spline: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplineValue {
    Constant(f32),
    Nested(u16),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplinePoint {
    pub location: f32,
    pub derivative: f32,
    pub value: SplineValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplineSpec {
    pub coordinate: u16,
    pub start: u32,
    pub length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiomePoint {
    pub low: [i16; 6],
    pub high: [i16; 6],
    pub offset: i16,
    pub biome: u8,
}

pub const NO_NODE: u16 = u16::MAX;

#[derive(Debug, Clone, Copy)]
pub struct DensityGraph {
    pub noises: &'static [NoiseParameters],
    pub nodes: &'static [DensityNode],
    pub splines: &'static [SplineSpec],
    pub spline_points: &'static [SplinePoint],
    pub router: [u16; 6],
}

pub fn overworld_graph() -> DensityGraph {
    DensityGraph {
        noises: &table::NOISES,
        nodes: &table::NODES,
        splines: &table::SPLINES,
        spline_points: &table::SPLINE_POINTS,
        router: table::ROUTER,
    }
}

pub fn overworld_points() -> &'static [BiomePoint] {
    &table::POINTS
}

pub fn overworld_biomes() -> &'static [&'static str] {
    &table::BIOMES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::biome_id;

    #[test]
    fn every_placed_biome_is_a_synced_biome() {
        for name in overworld_biomes() {
            assert!(biome_id(name).is_some(), "{name}");
        }
    }

    #[test]
    fn router_points_at_existing_nodes() {
        let graph = overworld_graph();
        for node in graph.router {
            assert!(usize::from(node) < graph.nodes.len());
        }
        assert!(overworld_points().len() > 7000);
    }
}
