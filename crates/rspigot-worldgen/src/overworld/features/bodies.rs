use std::collections::HashMap;
use std::sync::Arc;

use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::{Modifier, Placed, Run};
use super::prov::{
    BlockSet, Int, Pos, Pred, State, flag, integer, is_fluid, is_solid, number, short, survives,
    text,
};
use crate::datapack::block_state;
use crate::overworld::region::Heightmap;

#[derive(Debug, Clone)]
pub(crate) enum RuleTest {
    Always,
    Blocks(BlockSet),
    Chance(BlockSet, f32),
    Height(i32, i32),
    AllOf(Vec<RuleTest>),
    AnyOf(Vec<RuleTest>),
    Not(Box<RuleTest>),
}

impl RuleTest {
    fn compile(value: &Value) -> Self {
        let list = |key: &str| -> Vec<RuleTest> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| items.iter().map(Self::compile).collect())
                .unwrap_or_default()
        };
        let kind = text(value, "predicate_type");
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "tag_match" => Self::Blocks(BlockSet::tag(text(value, "tag"))),
            "block_match" => Self::Blocks(BlockSet::compile(
                value.get("block").unwrap_or(&Value::Null),
            )),
            "random_block_match" => Self::Chance(
                BlockSet::compile(value.get("block").unwrap_or(&Value::Null)),
                number(value, "probability", 1.0) as f32,
            ),
            "height_match" => Self::Height(
                integer(value, "min_inclusive", i32::MIN),
                integer(value, "max_inclusive", i32::MAX),
            ),
            "all_of" => Self::AllOf(list("rules")),
            "any_of" => Self::AnyOf(list("rules")),
            "not" => Self::Not(Box::new(Self::compile(
                value.get("rule").unwrap_or(&Value::Null),
            ))),
            _ => Self::Always,
        }
    }

    fn test(&self, state: BlockStateId, y: i32, rng: &mut crate::climate::Xoroshiro) -> bool {
        match self {
            Self::Always => true,
            Self::Blocks(set) => set.contains(state),
            Self::Chance(set, chance) => set.contains(state) && rng.next_float() < *chance,
            Self::Height(min, max) => (*min..=*max).contains(&y),
            Self::AllOf(items) => items.iter().all(|item| item.test(state, y, rng)),
            Self::AnyOf(items) => items.iter().any(|item| item.test(state, y, rng)),
            Self::Not(inner) => !inner.test(state, y, rng),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Ore {
    size: i32,
    discard: f32,
    targets: Vec<(RuleTest, BlockStateId)>,
    scattered: bool,
}

impl Ore {
    fn reach(&self) -> i32 {
        if self.scattered {
            return 8;
        }
        let spread = self.size as f32 / 8.0;
        let half = (((self.size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil()) as i32;
        spread.ceil() as i32 + half
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Layer {
    height: Int,
    state: State,
}

#[derive(Debug, Clone)]
pub(crate) struct Column {
    allowed: Pred,
    up: bool,
    layers: Vec<Layer>,
    prioritize_tip: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Disk {
    half_height: i32,
    radius: Int,
    state: State,
    target: Pred,
}

#[derive(Debug, Clone)]
pub(crate) struct Spring {
    state: BlockStateId,
    valid: BlockSet,
    requires_below: bool,
    rocks: i32,
    holes: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct Lake {
    fluid: State,
    barrier: State,
    can_place: Pred,
    replace_fluid: Pred,
    replace_barrier: Pred,
}

#[derive(Debug, Clone)]
pub(crate) struct Blob {
    state: BlockStateId,
    can_place_on: Pred,
}

#[derive(Debug, Clone)]
pub(crate) enum Feature {
    NoOp,
    Simple {
        state: State,
    },
    Ore(Ore),
    Column(Column),
    Disk(Disk),
    Spring(Spring),
    Tree(Box<super::tree::Tree>),
    Lake(Box<Lake>),
    Blob(Blob),
    Bamboo(f32),
    Vines,
    Pile(State),
    BonusChest,
    VoidPlatform,
    Mushroom(Box<super::mushrooms::Mushroom>),
    Fallen(Box<super::fallen::Fallen>),
    Magma(super::magma::Magma),
    Geode(Box<super::geode::Geode>),
    MonsterRoom(super::dungeon::MonsterRoom),
    Fossil(Box<super::fossil::Fossil>),
    Template(Box<super::templated::Templated>),
    Spread(Box<super::spread::Spread>),
    Pillar(Box<super::basalt::Pillar>),
    FillLayer {
        height: i32,
        state: BlockStateId,
    },
    ReplaceSingle(Vec<(RuleTest, BlockStateId)>),
    Patchy(super::basalt::Patchy),
    Stepped(Box<super::basalt::Cluster>),
    ReplaceBlobs(Box<super::replace_blobs::ReplaceBlobs>),
    Delta(Box<super::delta::Delta>),
    Fungus(Box<super::fungus::Fungus>),
    Chorus(super::chorus::Chorus),
    EndIsland(super::end_island::EndIsland),
    Multiface(Box<super::multiface::Growth>),
    Sculk(super::sculk::Patch),
    Vegetation(Box<super::vegetation::Patch>),
    Root(Box<super::root::Root>),
    Speleothem(Box<super::speleothem::Speleothem>),
    Cluster(Box<super::speleothem::Cluster>),
    Large(Box<super::large::Large>),
    CoralTree(Arc<Placed>),
    CoralClaw(Arc<Placed>),
    Spike(Box<super::ice::Spike>),
    Iceberg(super::ice::Iceberg),
    BlueIce(super::ice::BlueIce),
    RandomSelector {
        entries: Vec<(f32, Arc<Placed>)>,
        default: Arc<Placed>,
    },
    Simple2(Vec<Arc<Placed>>),
    Weighted(Vec<(Arc<Placed>, i32)>),
    Boolean(Arc<Placed>, Arc<Placed>),
    Sequence(Vec<Arc<Placed>>),
    Overlay(Vec<Arc<Placed>>),
    Unsupported,
}

pub(crate) struct Compiler<'a> {
    pub(crate) placed: &'a serde_json::Map<String, Value>,
    pub(crate) configured: &'a serde_json::Map<String, Value>,
    placed_memo: HashMap<String, Arc<Placed>>,
    configured_memo: HashMap<String, Arc<Feature>>,
}

impl<'a> Compiler<'a> {
    pub(crate) fn new(
        placed: &'a serde_json::Map<String, Value>,
        configured: &'a serde_json::Map<String, Value>,
    ) -> Self {
        Self {
            placed,
            configured,
            placed_memo: HashMap::new(),
            configured_memo: HashMap::new(),
        }
    }

    pub(crate) fn placed_ref(&mut self, value: &Value) -> Arc<Placed> {
        if let Some(name) = value.as_str() {
            if let Some(found) = self.placed_memo.get(name) {
                return found.clone();
            }
            let result = if let Some(definition) = self.placed.get(name) {
                let definition = definition.clone();
                self.placed_inline(&definition)
            } else {
                let feature = self.configured_ref(value);
                Arc::new(Placed::new(feature, Vec::new()))
            };
            self.placed_memo.insert(name.to_owned(), result.clone());
            return result;
        }
        self.placed_inline(value)
    }

    fn placed_inline(&mut self, value: &Value) -> Arc<Placed> {
        let feature = self.configured_ref(value.get("feature").unwrap_or(&Value::Null));
        let modifiers = value
            .get("placement")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(Modifier::compile).collect())
            .unwrap_or_default();
        Arc::new(Placed::new(feature, modifiers))
    }

    pub(crate) fn configured_ref(&mut self, value: &Value) -> Arc<Feature> {
        if let Some(name) = value.as_str() {
            if let Some(found) = self.configured_memo.get(name) {
                return found.clone();
            }
            let result = match self.configured.get(name) {
                Some(definition) => {
                    let definition = definition.clone();
                    Arc::new(self.feature(&definition))
                }
                None => Arc::new(Feature::Unsupported),
            };
            self.configured_memo.insert(name.to_owned(), result.clone());
            return result;
        }
        Arc::new(self.feature(value))
    }

    fn placed_list(&mut self, value: Option<&Value>) -> Vec<Arc<Placed>> {
        value
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|item| self.placed_ref(item)).collect())
            .unwrap_or_default()
    }

    pub(crate) fn is_supported(&mut self, value: &Value) -> bool {
        !matches!(self.feature(value), Feature::Unsupported)
    }

    fn feature(&mut self, value: &Value) -> Feature {
        let kind = short(value);
        match kind {
            "no_op" => Feature::NoOp,
            "simple_block" => Feature::Simple {
                state: State::compile(value.get("to_place").unwrap_or(&Value::Null)),
            },
            "ore" | "scattered_ore" => Feature::Ore(Ore {
                size: integer(value, "size", 1),
                discard: number(value, "discard_chance_on_air_exposure", 0.0) as f32,
                targets: value
                    .get("targets")
                    .and_then(Value::as_array)
                    .map(|targets| {
                        targets
                            .iter()
                            .filter_map(|target| {
                                Some((
                                    RuleTest::compile(target.get("target")?),
                                    block_state(target.get("state")?)?,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                scattered: kind == "scattered_ore",
            }),
            "block_column" => Feature::Column(Column {
                allowed: Pred::compile(value.get("allowed_placement").unwrap_or(&Value::Null)),
                up: text(value, "direction") != "down",
                layers: value
                    .get("layers")
                    .and_then(Value::as_array)
                    .map(|layers| {
                        layers
                            .iter()
                            .map(|layer| Layer {
                                height: Int::compile(layer.get("height").unwrap_or(&Value::Null)),
                                state: State::compile(
                                    layer.get("provider").unwrap_or(&Value::Null),
                                ),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                prioritize_tip: flag(value, "prioritize_tip", false),
            }),
            "disk" => Feature::Disk(Disk {
                half_height: integer(value, "half_height", 0),
                radius: Int::compile(value.get("radius").unwrap_or(&Value::Null)),
                state: State::compile(value.get("state_provider").unwrap_or(&Value::Null)),
                target: Pred::compile(value.get("target").unwrap_or(&Value::Null)),
            }),
            "spring_feature" => Feature::Spring(Spring {
                state: value
                    .get("state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
                valid: BlockSet::compile(value.get("valid_blocks").unwrap_or(&Value::Null)),
                requires_below: flag(value, "requires_block_below", true),
                rocks: integer(value, "rock_count", 4),
                holes: integer(value, "hole_count", 1),
            }),
            "tree" => match super::tree::compile(value, self) {
                Some(tree) => Feature::Tree(Box::new(tree)),
                None => Feature::Unsupported,
            },
            "lake" => Feature::Lake(Box::new(Lake {
                fluid: State::compile(value.get("fluid").unwrap_or(&Value::Null)),
                barrier: State::compile(value.get("barrier").unwrap_or(&Value::Null)),
                can_place: Pred::compile(value.get("can_place_feature").unwrap_or(&Value::Null)),
                replace_fluid: Pred::compile(
                    value
                        .get("can_replace_with_air_or_fluid")
                        .unwrap_or(&Value::Null),
                ),
                replace_barrier: Pred::compile(
                    value
                        .get("can_replace_with_barrier")
                        .unwrap_or(&Value::Null),
                ),
            })),
            "block_blob" => Feature::Blob(Blob {
                state: value
                    .get("state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
                can_place_on: Pred::compile(value.get("can_place_on").unwrap_or(&Value::Null)),
            }),
            "bamboo" => Feature::Bamboo(number(value, "probability", 0.0) as f32),
            "vines" => Feature::Vines,
            "block_pile" => Feature::Pile(State::compile(
                value.get("state_provider").unwrap_or(&Value::Null),
            )),
            "bonus_chest" => Feature::BonusChest,
            "void_start_platform" => Feature::VoidPlatform,
            "spike" => Feature::Spike(Box::new(super::ice::Spike::compile(value))),
            "iceberg" => Feature::Iceberg(super::ice::Iceberg::compile(value)),
            "blue_ice" => Feature::BlueIce(super::ice::BlueIce),
            "coral_tree" => {
                Feature::CoralTree(self.placed_ref(value.get("feature").unwrap_or(&Value::Null)))
            }
            "coral_claw" => {
                Feature::CoralClaw(self.placed_ref(value.get("feature").unwrap_or(&Value::Null)))
            }
            "fill_layer" => Feature::FillLayer {
                height: integer(value, "height", 0),
                state: value
                    .get("state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
            },
            "replace_single_block" => Feature::ReplaceSingle(
                value
                    .get("targets")
                    .and_then(Value::as_array)
                    .map(|targets| {
                        targets
                            .iter()
                            .filter_map(|target| {
                                Some((
                                    RuleTest::compile(target.get("target")?),
                                    block_state(target.get("state")?)?,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "single_block_pillar" => {
                Feature::Pillar(Box::new(super::basalt::Pillar::compile(value, self)))
            }
            "projected_random_patchy_square" => {
                Feature::Patchy(super::basalt::Patchy::compile(value))
            }
            "stepped_column_cluster" => {
                Feature::Stepped(Box::new(super::basalt::Cluster::compile(value)))
            }
            "overlay" => Feature::Overlay(self.placed_list(value.get("features"))),
            "random_neighbor_spread" => {
                Feature::Spread(Box::new(super::spread::Spread::compile(value)))
            }
            "netherrack_replace_blobs" => {
                Feature::ReplaceBlobs(Box::new(super::replace_blobs::ReplaceBlobs::compile(value)))
            }
            "huge_fungus" => Feature::Fungus(Box::new(super::fungus::Fungus::compile(value))),
            "end_island" => Feature::EndIsland(super::end_island::EndIsland::compile()),
            "chorus_plant" => Feature::Chorus(super::chorus::Chorus::compile()),
            "delta_feature" => Feature::Delta(Box::new(super::delta::Delta::compile(value))),
            "multiface_growth" => super::multiface::Growth::compile(value)
                .map_or(Feature::Unsupported, |growth| {
                    Feature::Multiface(Box::new(growth))
                }),
            "vegetation_patch" | "waterlogged_vegetation_patch" => {
                let vegetation =
                    self.placed_ref(value.get("vegetation_feature").unwrap_or(&Value::Null));
                super::vegetation::feature(
                    value,
                    vegetation,
                    kind == "waterlogged_vegetation_patch",
                )
            }
            "speleothem" => super::speleothem::Speleothem::compile(value)
                .map_or(Feature::Unsupported, |item| {
                    Feature::Speleothem(Box::new(item))
                }),
            "speleothem_cluster" => super::speleothem::Cluster::compile(value)
                .map_or(Feature::Unsupported, |item| {
                    Feature::Cluster(Box::new(item))
                }),
            "large_dripstone" => Feature::Large(Box::new(super::large::Large::compile(value))),
            "root_system" => {
                let tree = self.placed_ref(value.get("feature").unwrap_or(&Value::Null));
                super::root::feature(value, tree)
            }
            "sculk_patch" => Feature::Sculk(super::sculk::Patch::compile(value)),
            "freeze_top_layer" => Feature::NoOp,
            "fossil" => super::fossil::Fossil::compile(value)
                .map_or(Feature::Unsupported, |item| Feature::Fossil(Box::new(item))),
            "template" => super::templated::Templated::compile(value)
                .map_or(Feature::Unsupported, |item| {
                    Feature::Template(Box::new(item))
                }),
            "monster_room" => Feature::MonsterRoom(super::dungeon::MonsterRoom::compile()),
            "geode" => Feature::Geode(Box::new(super::geode::Geode::compile(value))),
            "underwater_magma" => Feature::Magma(super::magma::Magma::compile(value)),
            "fallen_tree" => Feature::Fallen(Box::new(super::fallen::Fallen::compile(value, self))),
            "huge_red_mushroom" => {
                Feature::Mushroom(Box::new(super::mushrooms::Mushroom::compile(value, true)))
            }
            "huge_brown_mushroom" => {
                Feature::Mushroom(Box::new(super::mushrooms::Mushroom::compile(value, false)))
            }
            "random_selector" => {
                let entries = value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                (
                                    number(item, "chance", 0.0) as f32,
                                    self.placed_ref(item.get("feature").unwrap_or(&Value::Null)),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Feature::RandomSelector {
                    entries,
                    default: self.placed_ref(value.get("default").unwrap_or(&Value::Null)),
                }
            }
            "simple_random_selector" => {
                let features = value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(|item| self.placed_ref(item)).collect())
                    .unwrap_or_default();
                Feature::Simple2(features)
            }
            "weighted_random_selector" => Feature::Weighted(
                value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                (
                                    self.placed_ref(item.get("data").unwrap_or(&Value::Null)),
                                    integer(item, "weight", 1),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "random_boolean_selector" => Feature::Boolean(
                self.placed_ref(value.get("feature_true").unwrap_or(&Value::Null)),
                self.placed_ref(value.get("feature_false").unwrap_or(&Value::Null)),
            ),
            "sequence" => Feature::Sequence(self.placed_list(value.get("features"))),
            _ => Feature::Unsupported,
        }
    }
}

fn double_plant_upper(state: BlockStateId) -> Option<BlockStateId> {
    if state.property("half") == Some("lower") {
        state.with_property("half", "upper")
    } else {
        None
    }
}

impl Feature {
    pub(crate) fn reach(&self) -> i32 {
        match self {
            Self::NoOp | Self::Simple { .. } | Self::Column(_) | Self::Spring(_) => 1,
            Self::Ore(ore) => ore.reach(),
            Self::Tree(_) => 12,
            Self::Lake(_) => 16,
            Self::Blob(_) => 6,
            Self::Bamboo(_) => 5,
            Self::Vines => 1,
            Self::Pile(_) => 5,
            Self::BonusChest | Self::VoidPlatform => super::misc::REACH,
            Self::Mushroom(mushroom) => mushroom.reach(),
            Self::Fallen(fallen) => fallen.reach(),
            Self::Magma(magma) => magma.reach(),
            Self::Geode(geode) => geode.reach(),
            Self::MonsterRoom(room) => room.reach(),
            Self::Fossil(fossil) => fossil.reach(),
            Self::Template(template) => template.reach(),
            Self::Spread(spread) => spread.reach(),
            Self::Pillar(pillar) => pillar.reach(),
            Self::FillLayer { .. } => 16,
            Self::ReplaceSingle(_) => 1,
            Self::Patchy(patchy) => patchy.reach(),
            Self::Stepped(cluster) => cluster.reach(),
            Self::ReplaceBlobs(blobs) => blobs.reach(),
            Self::Delta(delta) => delta.reach(),
            Self::Fungus(fungus) => fungus.reach(),
            Self::Chorus(chorus) => chorus.reach(),
            Self::EndIsland(island) => island.reach(),
            Self::Multiface(growth) => growth.reach(),
            Self::Sculk(patch) => patch.reach(),
            Self::Vegetation(patch) => patch.reach(),
            Self::Root(root) => root.reach(),
            Self::Speleothem(item) => item.reach(),
            Self::Cluster(item) => item.reach(),
            Self::Large(item) => item.reach(),
            Self::CoralTree(placed) => super::coral::reach_of_tree(placed),
            Self::CoralClaw(placed) => super::coral::reach_of_claw(placed),
            Self::Spike(spike) => spike.reach(),
            Self::Iceberg(iceberg) => iceberg.reach(),
            Self::BlueIce(ice) => ice.reach(),
            Self::Disk(_) => 8,
            Self::RandomSelector { entries, default } => entries
                .iter()
                .map(|(_, placed)| placed.margin)
                .chain([default.margin])
                .max()
                .unwrap_or(1),
            Self::Simple2(items) | Self::Sequence(items) | Self::Overlay(items) => {
                items.iter().map(|p| p.margin).max().unwrap_or(1)
            }
            Self::Weighted(items) => items.iter().map(|(p, _)| p.margin).max().unwrap_or(1),
            Self::Boolean(a, b) => a.margin.max(b.margin),
            Self::Unsupported => 16,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, pos: Pos) -> bool {
        match self {
            Self::NoOp | Self::Unsupported => false,
            Self::Simple { state } => place_simple(state, run, pos),
            Self::Ore(ore) => place_ore(ore, run, pos),
            Self::Column(column) => place_column(column, run, pos),
            Self::Disk(disk) => place_disk(disk, run, pos),
            Self::Spring(spring) => place_spring(spring, run, pos),
            Self::Tree(tree) => tree.place(run, pos),
            Self::Lake(lake) => place_lake(lake, run, pos),
            Self::Blob(blob) => place_blob(blob, run, pos),
            Self::Bamboo(probability) => place_bamboo(*probability, run, pos),
            Self::Vines => place_vines(run, pos),
            Self::Pile(provider) => super::misc::place_pile(provider, run, pos),
            Self::BonusChest => super::misc::place_bonus_chest(run, pos),
            Self::VoidPlatform => super::misc::place_void_platform(run, pos),
            Self::Mushroom(mushroom) => mushroom.place(run, pos),
            Self::Fallen(fallen) => fallen.place(run, pos),
            Self::Magma(magma) => magma.place(run, pos),
            Self::Geode(geode) => geode.place(run, pos),
            Self::MonsterRoom(room) => room.place(run, pos),
            Self::Fossil(fossil) => fossil.place(run, pos),
            Self::Template(template) => template.place(run, pos),
            Self::Spread(spread) => spread.place(run, pos),
            Self::Pillar(pillar) => pillar.place(run, pos),
            Self::FillLayer { height, state } => {
                let y = run.region.height().min_y + height;
                for dx in 0..16 {
                    for dz in 0..16 {
                        let at = (pos.0 + dx, y, pos.2 + dz);
                        if run.region.get(at.0, at.1, at.2).is_air() {
                            run.region.set(at.0, at.1, at.2, *state);
                        }
                    }
                }
                true
            }
            Self::ReplaceSingle(targets) => {
                let current = run.region.get(pos.0, pos.1, pos.2);
                for (test, state) in targets {
                    if test.test(current, pos.1, run.rng) {
                        return run.region.set(pos.0, pos.1, pos.2, *state);
                    }
                }
                false
            }
            Self::Patchy(patchy) => patchy.place(run, pos),
            Self::Stepped(cluster) => cluster.place(run, pos),
            Self::ReplaceBlobs(blobs) => blobs.place(run, pos),
            Self::Delta(delta) => delta.place(run, pos),
            Self::Fungus(fungus) => fungus.place(run, pos),
            Self::Chorus(chorus) => chorus.place(run, pos),
            Self::EndIsland(island) => island.place(run, pos),
            Self::Multiface(growth) => growth.place(run, pos),
            Self::Sculk(patch) => patch.place(run, pos),
            Self::Vegetation(patch) => patch.place(run, pos),
            Self::Root(root) => root.place(run, pos),
            Self::Speleothem(item) => item.place(run, pos),
            Self::Cluster(item) => item.place(run, pos),
            Self::Large(item) => item.place(run, pos),
            Self::CoralTree(placed) => super::coral::tree(placed, run, pos),
            Self::CoralClaw(placed) => super::coral::claw(placed, run, pos),
            Self::Spike(spike) => spike.place(run, pos),
            Self::Iceberg(iceberg) => iceberg.place(run, pos),
            Self::BlueIce(ice) => ice.place(run, pos),
            Self::RandomSelector { entries, default } => {
                for (chance, placed) in entries {
                    if run.rng.next_float() < *chance {
                        return placed.place(run, pos);
                    }
                }
                default.place(run, pos)
            }
            Self::Simple2(items) => {
                if items.is_empty() {
                    return false;
                }
                let pick = run.rng.below(items.len() as i32) as usize;
                items[pick].place(run, pos)
            }
            Self::Weighted(items) => {
                let total: i32 = items.iter().map(|(_, weight)| weight).sum();
                if total <= 0 {
                    return false;
                }
                let mut roll = run.rng.below(total);
                for (placed, weight) in items {
                    roll -= weight;
                    if roll < 0 {
                        return placed.place(run, pos);
                    }
                }
                false
            }
            Self::Boolean(yes, no) => {
                if run.rng.next_boolean() {
                    yes.place(run, pos)
                } else {
                    no.place(run, pos)
                }
            }
            Self::Sequence(items) => items.iter().all(|placed| placed.place(run, pos)),
            Self::Overlay(items) => {
                let mut any = false;
                for placed in items {
                    any |= placed.place(run, pos);
                }
                any
            }
        }
    }
}

fn place_simple(state: &State, run: &mut Run, pos: Pos) -> bool {
    let Some(resolved) = state.resolve(run.region, run.rng, pos) else {
        return false;
    };
    if !survives(run.region, resolved, pos) {
        return false;
    }
    if resolved.kind() == Some(rspigot_blocks::BlockKind::PaleMossCarpet) {
        super::mossy::place_at(run, pos);
        return true;
    }
    if let Some(upper) = double_plant_upper(resolved) {
        if !run.region.get(pos.0, pos.1 + 1, pos.2).is_air() {
            return false;
        }
        run.region.set(pos.0, pos.1, pos.2, resolved);
        run.region.set(pos.0, pos.1 + 1, pos.2, upper);
        return true;
    }
    run.region.set(pos.0, pos.1, pos.2, resolved)
}

fn adjacent_to_air(run: &mut Run, (x, y, z): Pos) -> bool {
    [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ]
    .iter()
    .any(|(dx, dy, dz)| run.region.get(x + dx, y + dy, z + dz).is_air())
}

fn place_ore(ore: &Ore, run: &mut Run, (ox, oy, oz): Pos) -> bool {
    if ore.scattered {
        return place_scattered(ore, run, (ox, oy, oz));
    }
    let size = ore.size;
    let angle = run.rng.next_float() * std::f32::consts::PI;
    let spread = size as f32 / 8.0;
    let half = (((size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil()) as i32;
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    let (x1, x2) = (
        f64::from(ox) + sin * f64::from(spread),
        f64::from(ox) - sin * f64::from(spread),
    );
    let (z1, z2) = (
        f64::from(oz) + cos * f64::from(spread),
        f64::from(oz) - cos * f64::from(spread),
    );
    let y1 = f64::from(oy + run.rng.below(3) - 2);
    let y2 = f64::from(oy + run.rng.below(3) - 2);
    let spread_ceil = spread.ceil() as i32;
    let x0 = ox - spread_ceil - half;
    let y0 = oy - 2 - half;
    let z0 = oz - spread_ceil - half;
    let width = 2 * (spread_ceil + half);
    let height = 2 * (2 + half);
    let mut reachable = false;
    'scan: for px in x0..=x0 + width {
        for pz in z0..=z0 + width {
            if y0 <= run.region.height_at(Heightmap::OceanFloor, px, pz) {
                reachable = true;
                break 'scan;
            }
        }
    }
    if !reachable {
        return false;
    }
    let count = size as usize;
    let mut blobs = vec![0.0f64; count * 4];
    for i in 0..count {
        let t = i as f32 / size as f32;
        let td = f64::from(t);
        let radius = run.rng.next_double() * f64::from(size) / 16.0;
        let scale = (f64::from(f32::sin(std::f32::consts::PI * t) + 1.0) * radius + 1.0) / 2.0;
        blobs[i * 4] = x1 + (x2 - x1) * td;
        blobs[i * 4 + 1] = y1 + (y2 - y1) * td;
        blobs[i * 4 + 2] = z1 + (z2 - z1) * td;
        blobs[i * 4 + 3] = scale;
    }
    for i in 0..count.saturating_sub(1) {
        if blobs[i * 4 + 3] <= 0.0 {
            continue;
        }
        for k in (i + 1)..count {
            if blobs[k * 4 + 3] <= 0.0 {
                continue;
            }
            let dr = blobs[i * 4 + 3] - blobs[k * 4 + 3];
            let dx = blobs[i * 4] - blobs[k * 4];
            let dy = blobs[i * 4 + 1] - blobs[k * 4 + 1];
            let dz = blobs[i * 4 + 2] - blobs[k * 4 + 2];
            if dr * dr > dx * dx + dy * dy + dz * dz {
                if dr > 0.0 {
                    blobs[k * 4 + 3] = -1.0;
                } else {
                    blobs[i * 4 + 3] = -1.0;
                }
            }
        }
    }
    let mut done = vec![false; (width.max(1) * height.max(1) * width.max(1)) as usize + 1];
    let mut placed = false;
    for i in 0..count {
        let radius = blobs[i * 4 + 3];
        if radius < 0.0 {
            continue;
        }
        let (cx, cy, cz) = (blobs[i * 4], blobs[i * 4 + 1], blobs[i * 4 + 2]);
        let bx0 = ((cx - radius).floor() as i32).max(x0);
        let by0 = ((cy - radius).floor() as i32).max(y0);
        let bz0 = ((cz - radius).floor() as i32).max(z0);
        let bx1 = ((cx + radius).floor() as i32).max(bx0);
        let by1 = ((cy + radius).floor() as i32).max(by0);
        let bz1 = ((cz + radius).floor() as i32).max(bz0);
        for px in bx0..=bx1 {
            let dx = (f64::from(px) + 0.5 - cx) / radius;
            if dx * dx >= 1.0 {
                continue;
            }
            for py in by0..=by1 {
                let dy = (f64::from(py) + 0.5 - cy) / radius;
                if dx * dx + dy * dy >= 1.0 {
                    continue;
                }
                for pz in bz0..=bz1 {
                    let dz = (f64::from(pz) + 0.5 - cz) / radius;
                    if dx * dx + dy * dy + dz * dz >= 1.0 || !run.region.height().contains(py) {
                        continue;
                    }
                    let slot = (px - x0) + (py - y0) * width + (pz - z0) * width * height;
                    let Some(flag) = usize::try_from(slot).ok().and_then(|s| done.get_mut(s))
                    else {
                        continue;
                    };
                    if *flag {
                        continue;
                    }
                    *flag = true;
                    placed |= try_ore(ore, run, (px, py, pz));
                }
            }
        }
    }
    placed
}

fn try_ore(ore: &Ore, run: &mut Run, pos: Pos) -> bool {
    let state = run.region.get(pos.0, pos.1, pos.2);
    for (rule, result) in &ore.targets {
        if !rule.test(state, pos.1, run.rng) {
            continue;
        }
        let skip = if ore.discard <= 0.0 {
            true
        } else if ore.discard >= 1.0 {
            false
        } else {
            run.rng.next_float() >= ore.discard
        };
        if skip || !adjacent_to_air(run, pos) {
            run.region.set(pos.0, pos.1, pos.2, *result);
            return true;
        }
    }
    false
}

fn place_scattered(ore: &Ore, run: &mut Run, (ox, oy, oz): Pos) -> bool {
    let count = run.rng.below(ore.size + 1);
    let mut placed = false;
    for _ in 0..count {
        let offset = |run: &mut Run| {
            let bound = count.min(7);
            let spread = (run.rng.next_float() - run.rng.next_float()) * bound as f32;
            (spread + 0.5).floor() as i32
        };
        let dx = offset(run);
        let dy = offset(run);
        let dz = offset(run);
        placed |= try_ore(ore, run, (ox + dx, oy + dy, oz + dz));
    }
    placed
}

fn place_column(column: &Column, run: &mut Run, (x, y, z): Pos) -> bool {
    let step = if column.up { 1 } else { -1 };
    let mut heights: Vec<i32> = column
        .layers
        .iter()
        .map(|layer| layer.height.sample(run.rng))
        .collect();
    let total: i32 = heights.iter().sum();
    if total == 0 {
        return false;
    }
    let mut at = y + step;
    for k in 0..total {
        if !column.allowed.test(run.region, (x, at, z)) {
            let mut remove = total - k;
            let order: Vec<usize> = if column.prioritize_tip {
                (0..heights.len()).collect()
            } else {
                (0..heights.len()).rev().collect()
            };
            for index in order {
                let cut = heights[index].min(remove);
                remove -= cut;
                heights[index] -= cut;
            }
            break;
        }
        at += step;
    }
    let mut at = y;
    for (layer, count) in column.layers.iter().zip(&heights) {
        for _ in 0..*count {
            if let Some(state) = layer.state.resolve(run.region, run.rng, (x, at, z)) {
                run.region.set(x, at, z, state);
            }
            at += step;
        }
    }
    true
}

fn place_disk(disk: &Disk, run: &mut Run, (x, y, z): Pos) -> bool {
    let radius = disk.radius.sample(run.rng);
    let top = y + disk.half_height;
    let bottom = y - disk.half_height - 1;
    let mut placed = false;
    for px in (x - radius)..=(x + radius) {
        for pz in (z - radius)..=(z + radius) {
            let (dx, dz) = (px - x, pz - z);
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            for py in ((bottom + 1)..=top).rev() {
                if disk.target.test(run.region, (px, py, pz))
                    && let Some(state) = disk.state.resolve(run.region, run.rng, (px, py, pz))
                {
                    run.region.set(px, py, pz, state);
                    placed = true;
                }
            }
        }
    }
    placed
}

fn place_spring(spring: &Spring, run: &mut Run, (x, y, z): Pos) -> bool {
    let valid = |state: BlockStateId| spring.valid.contains(state);
    if !valid(run.region.get(x, y + 1, z)) {
        return false;
    }
    if spring.requires_below && !valid(run.region.get(x, y - 1, z)) {
        return false;
    }
    let here = run.region.get(x, y, z);
    if !here.is_air() && !valid(here) {
        return false;
    }
    let sides = [(-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1), (0, -1, 0)];
    let mut rocks = 0;
    let mut holes = 0;
    for (dx, dy, dz) in sides {
        let state = run.region.get(x + dx, y + dy, z + dz);
        if valid(state) {
            rocks += 1;
        }
        if state.is_air() {
            holes += 1;
        }
    }
    if rocks == spring.rocks && holes == spring.holes {
        run.region.set(x, y, z, spring.state);
        return true;
    }
    false
}

fn lake_cell(x: i32, z: i32, y: i32) -> usize {
    ((x * 16 + z) * 8 + y) as usize
}

fn place_lake(lake: &Lake, run: &mut Run, origin: Pos) -> bool {
    if origin.1 <= run.region.height().min_y + 4 {
        return false;
    }
    let (ox, oy, oz) = (origin.0 - 8, origin.1 - 4, origin.2 - 8);
    let mut grid = [false; 2048];
    let blobs = run.rng.below(4) + 4;
    for _ in 0..blobs {
        let sx = run.rng.next_double() * 6.0 + 3.0;
        let sy = run.rng.next_double() * 4.0 + 2.0;
        let sz = run.rng.next_double() * 6.0 + 3.0;
        let cx = run.rng.next_double() * (16.0 - sx - 2.0) + 1.0 + sx / 2.0;
        let cy = run.rng.next_double() * (8.0 - sy - 4.0) + 2.0 + sy / 2.0;
        let cz = run.rng.next_double() * (16.0 - sz - 2.0) + 1.0 + sz / 2.0;
        for x in 1..15 {
            for z in 1..15 {
                for y in 1..7 {
                    let dx = (f64::from(x) - cx) / (sx / 2.0);
                    let dy = (f64::from(y) - cy) / (sy / 2.0);
                    let dz = (f64::from(z) - cz) / (sz / 2.0);
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        grid[lake_cell(x, z, y)] = true;
                    }
                }
            }
        }
    }
    let fluid = lake.fluid.resolve(run.region, run.rng, origin);
    let adjacent = |grid: &[bool; 2048], x: i32, z: i32, y: i32| {
        !grid[lake_cell(x, z, y)]
            && ((x < 15 && grid[lake_cell(x + 1, z, y)])
                || (x > 0 && grid[lake_cell(x - 1, z, y)])
                || (z < 15 && grid[lake_cell(x, z + 1, y)])
                || (z > 0 && grid[lake_cell(x, z - 1, y)])
                || (y < 7 && grid[lake_cell(x, z, y + 1)])
                || (y > 0 && grid[lake_cell(x, z, y - 1)]))
    };
    for x in 0..16 {
        for z in 0..16 {
            for y in 0..8 {
                if !adjacent(&grid, x, z, y) {
                    continue;
                }
                let at = (ox + x, oy + y, oz + z);
                let state = run.region.get(at.0, at.1, at.2);
                if y >= 4 && is_fluid(state) {
                    return false;
                }
                if y < 4 && !is_solid(state) && Some(state) != fluid {
                    return false;
                }
                if !lake.can_place.test(run.region, at) {
                    return false;
                }
            }
        }
    }
    for x in 0..16 {
        for z in 0..16 {
            for y in 0..8 {
                if !grid[lake_cell(x, z, y)] {
                    continue;
                }
                let at = (ox + x, oy + y, oz + z);
                if lake.replace_fluid.test(run.region, at) {
                    let above = y >= 4;
                    let state = if above {
                        Some(BlockStateId::AIR)
                    } else {
                        fluid
                    };
                    if let Some(state) = state {
                        run.region.set(at.0, at.1, at.2, state);
                    }
                }
            }
        }
    }
    let barrier = lake.barrier.resolve(run.region, run.rng, origin);
    if let Some(barrier) = barrier.filter(|state| !state.is_air()) {
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..8 {
                    if !adjacent(&grid, x, z, y) {
                        continue;
                    }
                    if y < 4 || run.rng.below(2) != 0 {
                        let at = (ox + x, oy + y, oz + z);
                        if lake.replace_barrier.test(run.region, at) {
                            run.region.set(at.0, at.1, at.2, barrier);
                        }
                    }
                }
            }
        }
    }
    true
}

fn place_blob(blob: &Blob, run: &mut Run, origin: Pos) -> bool {
    let min_y = run.region.height().min_y;
    let (mut x, mut y, mut z) = origin;
    while y > min_y + 3 {
        if !run.region.get(x, y - 1, z).is_air()
            && blob.can_place_on.test(run.region, (x, y - 1, z))
        {
            break;
        }
        y -= 1;
    }
    if y <= min_y + 3 {
        return false;
    }
    for _ in 0..3 {
        let a = run.rng.below(2);
        let b = run.rng.below(2);
        let c = run.rng.below(2);
        let reach = (a + b + c) as f32 * 0.333 + 0.5;
        for px in (x - a)..=(x + a) {
            for py in (y - b)..=(y + b) {
                for pz in (z - c)..=(z + c) {
                    let (dx, dy, dz) = (px - x, py - y, pz - z);
                    if (dx * dx + dy * dy + dz * dz) as f32 <= reach * reach {
                        run.region.set(px, py, pz, blob.state);
                    }
                }
            }
        }
        x += -1 + run.rng.below(2);
        y -= run.rng.below(2);
        z += -1 + run.rng.below(2);
    }
    true
}

fn place_bamboo(probability: f32, run: &mut Run, (x, y, z): Pos) -> bool {
    use crate::datapack::state_named;
    let bamboo = |leaves: &'static str, stage: &'static str| {
        state_named(
            "minecraft:bamboo",
            &[("age", "1"), ("leaves", leaves), ("stage", stage)],
        )
        .unwrap_or(BlockStateId::AIR)
    };
    if !run.region.get(x, y, z).is_air() {
        return false;
    }
    let sapling = state_named("minecraft:bamboo", &[]).unwrap_or(BlockStateId::AIR);
    if !survives(run.region, sapling, (x, y, z)) {
        return true;
    }
    let length = run.rng.below(12) + 5;
    if run.rng.next_float() < probability {
        let radius = run.rng.below(4) + 1;
        let soil = BlockSet::tag("minecraft:dirt");
        let podzol = state_named("minecraft:podzol", &[]).unwrap_or(BlockStateId::AIR);
        for px in (x - radius)..=(x + radius) {
            for pz in (z - radius)..=(z + radius) {
                let (dx, dz) = (px - x, pz - z);
                if dx * dx + dz * dz <= radius * radius {
                    let top = run.region.height_at(Heightmap::WorldSurface, px, pz) - 1;
                    if soil.contains(run.region.get(px, top, pz)) {
                        run.region.set(px, top, pz, podzol);
                    }
                }
            }
        }
    }
    let mut top = y;
    let mut step = 0;
    while step < length && run.region.get(x, top, z).is_air() {
        run.region.set(x, top, z, bamboo("none", "0"));
        top += 1;
        step += 1;
    }
    if top - y >= 3 {
        run.region.set(x, top, z, bamboo("large", "1"));
        run.region.set(x, top - 1, z, bamboo("large", "0"));
        run.region.set(x, top - 2, z, bamboo("small", "0"));
    }
    true
}

fn place_vines(run: &mut Run, (x, y, z): Pos) -> bool {
    use rspigot_blocks::{Face, Support};
    type Side = (&'static str, Face, (i32, i32, i32), Face);
    const SIDES: [Side; 5] = [
        ("up", Face::Up, (0, 1, 0), Face::Down),
        ("north", Face::North, (0, 0, -1), Face::South),
        ("south", Face::South, (0, 0, 1), Face::North),
        ("west", Face::West, (-1, 0, 0), Face::East),
        ("east", Face::East, (1, 0, 0), Face::West),
    ];
    let max_y = run.region.height().max_y();
    for py in y..max_y {
        if !run.region.get(x, py, z).is_air() {
            continue;
        }
        for (property, _, (dx, dy, dz), opposite) in SIDES {
            let neighbour = run.region.get(x + dx, py + dy, z + dz);
            if neighbour.is_face_sturdy(opposite, Support::Full)
                && let Some(vine) =
                    crate::datapack::state_named("minecraft:vine", &[(property, "true")])
            {
                run.region.set(x, py, z, vine);
                return true;
            }
        }
    }
    false
}
