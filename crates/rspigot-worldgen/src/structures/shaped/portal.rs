use rspigot_blocks::{BlockKind as Kind, Face, Support};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::prov::BlockSet;
use crate::overworld::features::structure::{Rotation, commit};
use crate::overworld::region::Heightmap;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Blueprint;
use crate::structures::jigsaw::{Base, Ground, Heightmap as Map};
use crate::structures::library::Library;
use crate::structures::placement::{BiomeProbe, large_feature_seed};
use crate::structures::processors::{Rule, Step, Subject, Test, parse_state};

const PORTALS: [&str; 10] = [
    "minecraft:ruined_portal/portal_1",
    "minecraft:ruined_portal/portal_2",
    "minecraft:ruined_portal/portal_3",
    "minecraft:ruined_portal/portal_4",
    "minecraft:ruined_portal/portal_5",
    "minecraft:ruined_portal/portal_6",
    "minecraft:ruined_portal/portal_7",
    "minecraft:ruined_portal/portal_8",
    "minecraft:ruined_portal/portal_9",
    "minecraft:ruined_portal/portal_10",
];
const GIANT_PORTALS: [&str; 3] = [
    "minecraft:ruined_portal/giant_portal_1",
    "minecraft:ruined_portal/giant_portal_2",
    "minecraft:ruined_portal/giant_portal_3",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    OnLandSurface,
    PartlyBuried,
    OnOceanFloor,
    InMountain,
    Underground,
    InNether,
}

impl Placement {
    fn parse(name: &str) -> Self {
        match name {
            "partly_buried" => Self::PartlyBuried,
            "on_ocean_floor" => Self::OnOceanFloor,
            "in_mountain" => Self::InMountain,
            "underground" => Self::Underground,
            "in_nether" => Self::InNether,
            _ => Self::OnLandSurface,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::OnLandSurface => "on_land_surface",
            Self::PartlyBuried => "partly_buried",
            Self::OnOceanFloor => "on_ocean_floor",
            Self::InMountain => "in_mountain",
            Self::Underground => "underground",
            Self::InNether => "in_nether",
        }
    }

    fn map(self) -> Map {
        if self == Self::OnOceanFloor {
            Map::OceanFloorWg
        } else {
            Map::WorldSurfaceWg
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Setup {
    placement: Placement,
    air_pocket: f32,
    mossiness: f32,
    overgrown: bool,
    vines: bool,
    can_be_cold: bool,
    blackstone: bool,
    weight: f32,
}

pub(crate) fn setups(value: &Value) -> Vec<Setup> {
    let number = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_f64)
            .map_or(0.0, |n| n as f32)
    };
    let flag = |item: &Value, key: &str| item.get(key).and_then(Value::as_bool).unwrap_or(false);
    value
        .get("setups")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| Setup {
                    placement: Placement::parse(
                        item.get("placement").and_then(Value::as_str).unwrap_or(""),
                    ),
                    air_pocket: number(item, "air_pocket_probability"),
                    mossiness: number(item, "mossiness"),
                    overgrown: flag(item, "overgrown"),
                    vines: flag(item, "vines"),
                    can_be_cold: flag(item, "can_be_cold"),
                    blackstone: flag(item, "replace_with_blackstone"),
                    weight: number(item, "weight"),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mirror {
    None,
    FrontBack,
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) name: &'static str,
    blueprint: &'static Blueprint,
    pub(crate) origin: Pos,
    pub(crate) rotation: Rotation,
    pub(crate) mirror: Mirror,
    pivot: (i32, i32),
    placement: Placement,
    cold: bool,
    air_pocket: bool,
    mossiness: f32,
    overgrown: bool,
    vines: bool,
    blackstone: bool,
    pub(crate) bbox: Bbox,
}

fn transform((x, y, z): Pos, mirror: Mirror, rotation: Rotation, (px, pz): (i32, i32)) -> Pos {
    let x = if mirror == Mirror::FrontBack { -x } else { x };
    match rotation {
        Rotation::Counterclockwise90 => (px - pz + z, y, px + pz - x),
        Rotation::Clockwise90 => (px + pz - z, y, pz - px + x),
        Rotation::Clockwise180 => (px * 2 - x, y, pz * 2 - z),
        Rotation::None => (x, y, z),
    }
}

fn bounds(size: Pos, origin: Pos, mirror: Mirror, rotation: Rotation, pivot: (i32, i32)) -> Bbox {
    let a = transform((0, 0, 0), mirror, rotation, pivot);
    let b = transform(
        (size.0 - 1, size.1 - 1, size.2 - 1),
        mirror,
        rotation,
        pivot,
    );
    Bbox::from_corners(a, b).moved(origin.0, origin.1, origin.2)
}

fn center(bbox: &Bbox) -> Pos {
    (
        bbox.min.0 + (bbox.max.0 - bbox.min.0 + 1) / 2,
        bbox.min.1 + (bbox.max.1 - bbox.min.1 + 1) / 2,
        bbox.min.2 + (bbox.max.2 - bbox.min.2 + 1) / 2,
    )
}

fn between(random: &mut JavaRandom, low: i32, high: i32) -> i32 {
    random.next_int(high - low + 1) + low
}

fn within(random: &mut JavaRandom, preferred: i32, max: i32) -> i32 {
    if preferred < max {
        between(random, preferred, max)
    } else {
        max
    }
}

fn sample(random: &mut JavaRandom, limit: f32) -> bool {
    if limit == 0.0 {
        false
    } else if limit == 1.0 {
        true
    } else {
        random.next_float() < limit
    }
}

fn suitable_y<G: Ground>(
    random: &mut JavaRandom,
    ground: &G,
    placement: Placement,
    air_pocket: bool,
    surface: i32,
    span: i32,
    bbox: &Bbox,
) -> i32 {
    let floor = ground.min_y() + 15;
    let mut y = match placement {
        Placement::InNether => {
            if air_pocket {
                between(random, 32, 100)
            } else if random.next_float() >= 0.5 {
                between(random, 29, 100)
            } else {
                between(random, 27, 29)
            }
        }
        Placement::InMountain => within(random, 70, surface - span),
        Placement::Underground => within(random, floor, surface - span),
        Placement::PartlyBuried => surface - span + between(random, 2, 8),
        _ => surface,
    };
    let corners = [
        (bbox.min.0, bbox.min.2),
        (bbox.max.0, bbox.min.2),
        (bbox.min.0, bbox.max.2),
        (bbox.max.0, bbox.max.2),
    ];
    let surface_map = placement == Placement::OnOceanFloor;
    loop {
        if y <= floor {
            return y;
        }
        let mut count = 0;
        for (x, z) in corners {
            let state = ground.base(x, y, z);
            let opaque = if surface_map {
                state == Base::Solid
            } else {
                state != Base::Air
            };
            if opaque {
                count += 1;
                if count == 3 {
                    return y;
                }
            }
        }
        y -= 1;
    }
}

pub(crate) fn generate<G: Ground>(
    setups: &[Setup],
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Piece> {
    let mut random = JavaRandom::new(0);
    large_feature_seed(&mut random, seed, chunk.0, chunk.1);
    let setup = if setups.len() < 2 {
        setups.first()?
    } else {
        let total: f32 = setups.iter().map(|setup| setup.weight).sum();
        let mut roll = random.next_float();
        let mut chosen = None;
        for setup in setups {
            roll -= setup.weight / total;
            if roll < 0.0 {
                chosen = Some(setup);
                break;
            }
        }
        chosen?
    };
    let air_pocket = sample(&mut random, setup.air_pocket);
    let name: &'static str = if random.next_float() < 0.05 {
        GIANT_PORTALS[random.next_int(3) as usize]
    } else {
        PORTALS[random.next_int(10) as usize]
    };
    let blueprint = library.blueprint(name)?;
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let mirror = if random.next_float() < 0.5 {
        Mirror::None
    } else {
        Mirror::FrontBack
    };
    let pivot = (blueprint.size.0 / 2, blueprint.size.2 / 2);
    let origin = (chunk.0 * 16, 0, chunk.1 * 16);
    let bbox = bounds(blueprint.size, origin, mirror, rotation, pivot);
    let middle = center(&bbox);
    let surface = ground.first_free_height(middle.0, middle.2, setup.placement.map()) - 1;
    let y = suitable_y(
        &mut random,
        ground,
        setup.placement,
        air_pocket,
        surface,
        bbox.y_span(),
        &bbox,
    );
    let at = (origin.0, y, origin.2);
    let biome = probe.at(at.0 >> 2, at.1 >> 2, at.2 >> 2);
    if !valid.contains(&biome) {
        return None;
    }
    let cold = setup.can_be_cold && ground.cold(biome, at.0, at.1, at.2);
    Some(Piece {
        name,
        blueprint,
        origin: at,
        rotation,
        mirror,
        pivot,
        placement: setup.placement,
        cold,
        air_pocket,
        mossiness: setup.mossiness,
        overgrown: setup.overgrown,
        vines: setup.vines,
        blackstone: setup.blackstone,
        bbox: bounds(blueprint.size, at, mirror, rotation, pivot),
    })
}

fn mirrored(state: BlockStateId, mirror: Mirror) -> BlockStateId {
    if mirror == Mirror::None {
        return state;
    }
    let Some(kind) = state.kind() else {
        return state;
    };
    let name = kind.name();
    let swap = |state: BlockStateId, first: &str, second: &str| match (
        state.property(first),
        state.property(second),
    ) {
        (Some(a), Some(b)) => state
            .with_property(first, b)
            .and_then(|next| next.with_property(second, a))
            .unwrap_or(state),
        _ => state,
    };
    if name.ends_with("_stairs") {
        let facing = state.property("facing").unwrap_or("north");
        if !matches!(facing, "east" | "west") {
            return state;
        }
        let turned = Rotation::Clockwise180.state(state);
        let shape = match state.property("shape") {
            Some("outer_left") => "outer_right",
            Some("outer_right") => "outer_left",
            Some(other) => other,
            None => "straight",
        };
        return turned.with_property("shape", shape).unwrap_or(turned);
    }
    if name.ends_with("_wall") || kind == Kind::IronBars || kind == Kind::IronChain {
        return swap(state, "east", "west");
    }
    match state.property("facing") {
        Some("east" | "west") => Rotation::Clockwise180.state(state),
        _ => state,
    }
}

fn steps(piece: &Piece) -> Vec<Step> {
    let replace = |kind: Kind, chance: Option<f32>, output: Kind| {
        Rule::replace(
            chance.map_or(Test::Block(kind), |p| Test::RandomBlock(kind, p)),
            output.default_state(),
        )
    };
    let mut rules = vec![replace(Kind::GoldBlock, Some(0.3), Kind::Air)];
    rules.push(match (piece.placement, piece.cold) {
        (Placement::OnOceanFloor, _) => replace(Kind::Lava, None, Kind::MagmaBlock),
        (_, false) => replace(Kind::Lava, Some(0.2), Kind::MagmaBlock),
        (_, true) => replace(Kind::Lava, None, Kind::Netherrack),
    });
    if !piece.cold {
        rules.push(replace(Kind::Netherrack, Some(0.07), Kind::MagmaBlock));
    }
    let mut steps = vec![
        if piece.air_pocket {
            Step::IgnoreStructureBlock
        } else {
            Step::IgnoreAir
        },
        Step::Rules(rules),
        Step::BlockAge {
            mossiness: piece.mossiness,
        },
        Step::Protected(cannot_replace()),
        Step::LavaSubmerged,
    ];
    if piece.blackstone {
        steps.push(Step::Blackstone);
    }
    steps
}

fn cannot_replace() -> BlockSet {
    BlockSet::compile(&Value::String(
        "#minecraft:features_cannot_replace".to_owned(),
    ))
}

const HORIZONTAL: [(&str, (i32, i32)); 4] = [
    ("north", (0, -1)),
    ("east", (1, 0)),
    ("south", (0, 1)),
    ("west", (-1, 0)),
];

impl Piece {
    pub(crate) fn tag_extra(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Template", self.name.to_owned()),
            ("VerticalPlacement", self.placement.name().to_owned()),
        ]
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let middle = center(&self.bbox);
        if !write.contains(middle) {
            return;
        }
        let mut area = *write;
        area.encapsulate(&self.bbox);
        self.template(run, &area);
        self.spread(run);
        self.drip_below(run);
        if self.vines || self.overgrown {
            let b = self.bbox;
            for z in b.min.2..=b.max.2 {
                for y in b.min.1..=b.max.1 {
                    for x in b.min.0..=b.max.0 {
                        if self.vines {
                            self.add_vines(run, (x, y, z));
                        }
                        if self.overgrown {
                            self.add_leaves(run, (x, y, z));
                        }
                    }
                }
            }
        }
    }

    fn template(&self, run: &mut Run, area: &Bbox) {
        let steps = steps(self);
        let mut placed: Vec<(Pos, BlockStateId)> = Vec::new();
        let mut loot: Vec<(Pos, Value)> = Vec::new();
        for block in &self.blueprint.blocks {
            let (rx, ry, rz) = transform(block.pos, self.mirror, self.rotation, self.pivot);
            let mut subject = Subject {
                local: block.pos,
                at: (rx + self.origin.0, ry + self.origin.1, rz + self.origin.2),
                state: block.state,
                nbt: block.nbt.as_ref(),
            };
            if !steps.iter().all(|step| step.process(run, &mut subject)) {
                continue;
            }
            if !area.contains(subject.at) {
                continue;
            }
            placed.push((subject.at, mirrored(subject.state, self.mirror)));
            if let Some(nbt) = subject.nbt {
                loot.push((subject.at, nbt.clone()));
            }
        }
        commit(run, placed, self.rotation, true);
        for (at, mut nbt) in loot {
            if let Value::Object(map) = &mut nbt
                && map.contains_key("LootTable")
            {
                let seed = run.rng.next_long() as i64;
                map.insert(
                    "LootTableSeed".to_owned(),
                    serde_json::json!({ "$l": seed }),
                );
            }
            run.region.block_entities.push((at, nbt));
        }
        for block in &self.blueprint.blocks {
            if block.state.kind() != Some(Kind::Jigsaw) {
                continue;
            }
            let Some(nbt) = &block.nbt else {
                continue;
            };
            let name = nbt
                .get("final_state")
                .and_then(Value::as_str)
                .unwrap_or("minecraft:air");
            let state = parse_state(name).unwrap_or(BlockStateId::AIR);
            let (x, y, z) = transform(block.pos, self.mirror, self.rotation, self.pivot);
            run.region.set(
                x + self.origin.0,
                y + self.origin.1,
                z + self.origin.2,
                state,
            );
        }
    }

    fn surface(&self, run: &mut Run, x: i32, z: i32) -> i32 {
        let map = if self.placement == Placement::OnOceanFloor {
            Heightmap::OceanFloor
        } else {
            Heightmap::WorldSurface
        };
        run.region.height_at(map, x, z) - 1
    }

    fn replaceable(&self, state: BlockStateId) -> bool {
        !matches!(state.kind(), Some(Kind::Air | Kind::Obsidian))
            && !cannot_replace().contains(state)
            && (self.placement == Placement::InNether || state.kind() != Some(Kind::Lava))
    }

    fn place_rack(&self, run: &mut Run, at: Pos) {
        let state = run.region.get(at.0, at.1, at.2);
        if !self.replaceable(state) {
            return;
        }
        let block = if !self.cold && run.rng.next_float() < 0.07 {
            Kind::MagmaBlock
        } else {
            Kind::Netherrack
        };
        run.region.set(at.0, at.1, at.2, block.default_state());
    }

    fn drip_column(&self, run: &mut Run, mut at: Pos) {
        self.place_rack(run, at);
        let mut left = 8;
        while left > 0 && run.rng.next_float() < 0.5 {
            at.1 -= 1;
            left -= 1;
            self.place_rack(run, at);
        }
    }

    fn spread(&self, run: &mut Run) {
        let on_surface = matches!(
            self.placement,
            Placement::OnLandSurface | Placement::OnOceanFloor
        );
        let middle = center(&self.bbox);
        const CHANCES: [f32; 14] = [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.9, 0.8, 0.7, 0.6, 0.4, 0.2,
        ];
        let length = CHANCES.len() as i32;
        let span =
            (self.bbox.max.0 - self.bbox.min.0 + 1) + (self.bbox.max.2 - self.bbox.min.2 + 1);
        let offset = run.rng.below((8 - span / 4).max(1));
        for x in middle.0 - length..=middle.0 + length {
            for z in middle.2 - length..=middle.2 + length {
                let distance = (x - middle.0).abs() + (z - middle.2).abs();
                let index = (distance + offset).max(0);
                if index >= length {
                    continue;
                }
                if run.rng.next_double() >= f64::from(CHANCES[index as usize]) {
                    continue;
                }
                let mut y = self.surface(run, x, z);
                if !on_surface {
                    y = y.min(self.bbox.min.1);
                }
                let at = (x, y, z);
                let state = run.region.get(x, y, z);
                if (y - self.bbox.min.1).abs() < 4 && self.replaceable(state) {
                    self.place_rack(run, at);
                    if self.overgrown {
                        self.add_leaves(run, at);
                    }
                    self.drip_column(run, (x, y - 1, z));
                }
            }
        }
    }

    fn drip_below(&self, run: &mut Run) {
        let b = self.bbox;
        for x in b.min.0 + 1..b.max.0 {
            for z in b.min.2 + 1..b.max.2 {
                let state = run.region.get(x, b.min.1, z);
                if state.kind() == Some(Kind::Netherrack) {
                    self.drip_column(run, (x, b.min.1 - 1, z));
                }
            }
        }
    }

    fn add_vines(&self, run: &mut Run, at: Pos) {
        let state = run.region.get(at.0, at.1, at.2);
        if state.is_air() || state.kind() == Some(Kind::Vine) {
            return;
        }
        let (name, (dx, dz)) = HORIZONTAL[run.rng.below(4) as usize];
        let near = (at.0 + dx, at.1, at.2 + dz);
        if !run.region.get(near.0, near.1, near.2).is_air() {
            return;
        }
        let face = match name {
            "north" => Face::North,
            "east" => Face::East,
            "south" => Face::South,
            _ => Face::West,
        };
        if state.is_face_sturdy(face, Support::Full) {
            let opposite = match name {
                "north" => "south",
                "east" => "west",
                "south" => "north",
                _ => "east",
            };
            let vine = Kind::Vine
                .state_with(&[(opposite, "true")])
                .unwrap_or(Kind::Vine.default_state());
            run.region.set(near.0, near.1, near.2, vine);
        }
    }

    fn add_leaves(&self, run: &mut Run, at: Pos) {
        if run.rng.next_float() >= 0.5 {
            return;
        }
        if run.region.get(at.0, at.1, at.2).kind() != Some(Kind::Netherrack) {
            return;
        }
        if run.region.get(at.0, at.1 + 1, at.2).is_air() {
            let leaves = Kind::JungleLeaves
                .state_with(&[("persistent", "true")])
                .unwrap_or(Kind::JungleLeaves.default_state());
            run.region.set(at.0, at.1 + 1, at.2, leaves);
        }
    }
}
