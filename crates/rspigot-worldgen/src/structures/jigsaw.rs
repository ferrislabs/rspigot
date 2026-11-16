use std::collections::{BTreeMap, VecDeque};

use super::bbox::{Bbox, Pos};
use super::blueprint::Jigsaw;
use super::pool::{Element, Pool, Pools, Projection, shuffle};
use crate::legacy::JavaRandom;
use crate::overworld::features::structure::Rotation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Heightmap {
    WorldSurfaceWg,
    OceanFloorWg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Base {
    Air,
    Solid,
    Water,
    Lava,
}

pub(crate) trait Ground {
    fn base(&self, x: i32, y: i32, z: i32) -> Base {
        let _ = (x, y, z);
        Base::Air
    }
    fn cold(&self, biome: u8, x: i32, y: i32, z: i32) -> bool {
        let _ = (biome, x, y, z);
        false
    }
    fn first_free_height(&self, x: i32, z: i32, heightmap: Heightmap) -> i32;
    fn min_y(&self) -> i32;
    fn max_y(&self) -> i32;
    fn sea_level(&self) -> i32 {
        63
    }
    fn gen_depth(&self) -> i32 {
        self.max_y() - self.min_y() + 1
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Junction {
    pub(crate) source: Pos,
    pub(crate) delta_y: i32,
    pub(crate) destination: Projection,
}

#[derive(Debug)]
pub(crate) struct Piece<'a> {
    pub(crate) element: &'a Element,
    pub(crate) pos: Pos,
    pub(crate) ground_level_delta: i32,
    pub(crate) rotation: Rotation,
    pub(crate) bbox: Bbox,
    pub(crate) junctions: Vec<Junction>,
}

#[derive(Debug)]
pub(crate) struct Assembly<'a> {
    pub(crate) position: Pos,
    pub(crate) pieces: Vec<Piece<'a>>,
}

#[derive(Debug, Clone)]
pub(crate) struct JigsawConfig<'a> {
    pub(crate) start_pool: &'a str,
    pub(crate) start_jigsaw: Option<&'a str>,
    pub(crate) size: i32,
    pub(crate) expansion_hack: bool,
    pub(crate) project: Option<Heightmap>,
    pub(crate) max_horizontal: i32,
    pub(crate) max_vertical: i32,
    pub(crate) padding: (i32, i32),
}

#[derive(Debug)]
struct Shape {
    low: Pos,
    high: Pos,
    holes: Vec<Bbox>,
}

impl Shape {
    fn of(bbox: &Bbox) -> Self {
        Self {
            low: bbox.min,
            high: (bbox.max.0 + 1, bbox.max.1 + 1, bbox.max.2 + 1),
            holes: Vec::new(),
        }
    }

    fn holds(&self, candidate: &Bbox) -> bool {
        candidate.min.0 >= self.low.0
            && candidate.min.1 >= self.low.1
            && candidate.min.2 >= self.low.2
            && candidate.max.0 < self.high.0
            && candidate.max.1 < self.high.1
            && candidate.max.2 < self.high.2
            && !self.holes.iter().any(|hole| hole.intersects(candidate))
    }
}

struct State {
    piece: usize,
    free: usize,
    depth: i32,
}

#[derive(Default)]
struct Queue {
    queues: BTreeMap<i32, VecDeque<State>>,
}

impl Queue {
    fn add(&mut self, state: State, priority: i32) {
        self.queues.entry(priority).or_default().push_back(state);
    }

    fn next(&mut self) -> Option<State> {
        let key = self
            .queues
            .iter()
            .rev()
            .find(|(_, queue)| !queue.is_empty())
            .map(|(key, _)| *key)?;
        self.queues.get_mut(&key)?.pop_front()
    }
}

struct Placer<'a, 'g, G: Ground> {
    pools: &'a Pools,
    ground: &'g G,
    max_depth: i32,
    expansion_hack: bool,
    random: &'g mut JavaRandom,
    pieces: Vec<Piece<'a>>,
    shapes: Vec<Shape>,
    queue: Queue,
}

fn add(a: Pos, b: Pos) -> Pos {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

fn sub(a: Pos, b: Pos) -> Pos {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn shuffled_rotations(random: &mut JavaRandom) -> Vec<Rotation> {
    let mut list = Rotation::ALL.to_vec();
    shuffle(&mut list, random);
    list
}

impl<'a, G: Ground> Placer<'a, '_, G> {
    fn pool(&self, name: &str) -> Option<&'a Pool> {
        self.pools.get(name)
    }

    fn hack_height(&self, hack_box: &Bbox, jigsaw: &Jigsaw) -> i32 {
        if !hack_box.contains(jigsaw.front_pos()) {
            return 0;
        }
        let Some(pool) = self.pool(&jigsaw.pool) else {
            return 0;
        };
        let fallback = self.pool(&pool.fallback).map_or(0, Pool::max_size);
        pool.max_size().max(fallback)
    }

    fn try_placing_children(&mut self, source: usize, context_free: usize, depth: i32) {
        let (element, position, rotation, source_box, source_gld) = {
            let piece = &self.pieces[source];
            (
                piece.element,
                piece.pos,
                piece.rotation,
                piece.bbox,
                piece.ground_level_delta,
            )
        };
        let source_rigid = element.projection() == Projection::Rigid;
        let mut local_free: Option<usize> = None;
        let min_y = source_box.min.1;
        let jigsaws = element.shuffled_jigsaws(position, rotation, self.random);
        for jigsaw in &jigsaws {
            let target = jigsaw.front_pos();
            let relative_y = jigsaw.pos.1 - min_y;
            let Some(pool) = self.pool(&jigsaw.pool) else {
                continue;
            };
            if pool.size() == 0 && pool.name != "minecraft:empty" {
                continue;
            }
            let Some(fallback) = self.pool(&pool.fallback) else {
                continue;
            };
            if fallback.size() == 0 && fallback.name != "minecraft:empty" {
                continue;
            }
            let free = if source_box.contains(target) {
                *local_free.get_or_insert_with(|| {
                    self.shapes.push(Shape::of(&source_box));
                    self.shapes.len() - 1
                })
            } else {
                context_free
            };
            let mut candidates: Vec<&'a Element> = Vec::new();
            if depth != self.max_depth {
                candidates.extend(pool.shuffled_elements(self.random));
            }
            candidates.extend(fallback.shuffled_elements(self.random));
            let mut surface: Option<i32> = None;
            'candidates: for candidate in candidates {
                if candidate.is_empty() {
                    break;
                }
                for rot in shuffled_rotations(self.random) {
                    let candidate_jigsaws = candidate.shuffled_jigsaws((0, 0, 0), rot, self.random);
                    let box0 = candidate.bounds((0, 0, 0), rot);
                    let hack = if !self.expansion_hack || box0.y_span() > 16 {
                        0
                    } else {
                        candidate_jigsaws
                            .iter()
                            .map(|other| self.hack_height(&box0, other))
                            .max()
                            .unwrap_or(0)
                    };
                    for other in &candidate_jigsaws {
                        if !jigsaw.can_attach(other) {
                            continue;
                        }
                        let offset = sub(target, other.pos);
                        let mut cbox = candidate.bounds(offset, rot);
                        let candidate_rigid = candidate.projection() == Projection::Rigid;
                        let step_y = jigsaw.front.step().1;
                        let k = relative_y - other.pos.1 + step_y;
                        let j = if !(source_rigid && candidate_rigid) {
                            let top = *surface.get_or_insert_with(|| {
                                self.ground.first_free_height(
                                    jigsaw.pos.0,
                                    jigsaw.pos.2,
                                    Heightmap::WorldSurfaceWg,
                                )
                            });
                            top - other.pos.1
                        } else {
                            min_y + k
                        };
                        let dy = j - cbox.min.1;
                        cbox = cbox.moved(0, dy, 0);
                        let new_pos = add(offset, (0, dy, 0));
                        if hack > 0 {
                            let span = (hack + 1).max(cbox.max.1 - cbox.min.1);
                            cbox.encapsulate_point((cbox.min.0, cbox.min.1 + span, cbox.min.2));
                        }
                        if !self.shapes[free].holds(&cbox) {
                            continue;
                        }
                        self.shapes[free].holes.push(cbox);
                        let candidate_gld = if candidate_rigid {
                            source_gld - k
                        } else {
                            candidate.ground_level_delta()
                        };
                        let junction_y = if source_rigid {
                            min_y + relative_y
                        } else if !candidate_rigid {
                            let top = *surface.get_or_insert_with(|| {
                                self.ground.first_free_height(
                                    jigsaw.pos.0,
                                    jigsaw.pos.2,
                                    Heightmap::WorldSurfaceWg,
                                )
                            });
                            top + k / 2
                        } else {
                            j + other.pos.1
                        };
                        let source_projection = element.projection();
                        let candidate_projection = candidate.projection();
                        self.pieces[source].junctions.push(Junction {
                            source: (target.0, junction_y - relative_y + source_gld, target.2),
                            delta_y: k,
                            destination: candidate_projection,
                        });
                        let child = Piece {
                            element: candidate,
                            pos: new_pos,
                            ground_level_delta: candidate_gld,
                            rotation: rot,
                            bbox: cbox,
                            junctions: vec![Junction {
                                source: (
                                    jigsaw.pos.0,
                                    junction_y - other.pos.1 + candidate_gld,
                                    jigsaw.pos.2,
                                ),
                                delta_y: -k,
                                destination: source_projection,
                            }],
                        };
                        self.pieces.push(child);
                        if depth < self.max_depth {
                            self.queue.add(
                                State {
                                    piece: self.pieces.len() - 1,
                                    free,
                                    depth: depth + 1,
                                },
                                jigsaw.placement_priority,
                            );
                        }
                        break 'candidates;
                    }
                }
            }
        }
    }
}

pub(crate) fn assemble<'a, G: Ground>(
    pools: &'a Pools,
    ground: &G,
    config: &JigsawConfig,
    random: &mut JavaRandom,
    position: Pos,
    could_exist: impl Fn(i32, i32) -> bool,
) -> Option<Assembly<'a>> {
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let pool = pools.get(config.start_pool)?;
    let element = pool.random_element(random)?;
    if element.is_empty() {
        return None;
    }
    let anchor = match config.start_jigsaw {
        Some(name) => {
            element
                .shuffled_jigsaws(position, rotation, random)
                .into_iter()
                .find(|jigsaw| jigsaw.name == name)?
                .pos
        }
        None => position,
    };
    let offset = sub(anchor, position);
    let origin = sub(position, offset);
    let ground_level_delta = element.ground_level_delta();
    let bbox = element.bounds(origin, rotation);
    let mut center = Piece {
        element,
        pos: origin,
        ground_level_delta,
        rotation,
        bbox,
        junctions: Vec::new(),
    };
    let cx = (bbox.max.0 + bbox.min.0) / 2;
    let cz = (bbox.max.2 + bbox.min.2) / 2;
    let mut y = match config.project {
        None => origin.1,
        Some(heightmap) => {
            if !could_exist(cx, cz) {
                return None;
            }
            position.1 + ground.first_free_height(cx, cz, heightmap)
        }
    };
    let dy = y - (bbox.min.1 + ground_level_delta);
    center.bbox = center.bbox.moved(0, dy, 0);
    center.pos = add(center.pos, (0, dy, 0));
    let (bottom, top) = config.padding;
    if (bottom, top) != (0, 0)
        && (center.bbox.min.1 < ground.min_y() + bottom || ground.max_y() - top < center.bbox.max.1)
    {
        return None;
    }
    y += offset.1;
    if config.size <= 0 {
        return Some(Assembly {
            position: (cx, y, cz),
            pieces: Vec::new(),
        });
    }
    let start_box = center.bbox;
    let limit = Shape {
        low: (
            cx - config.max_horizontal,
            (y - config.max_vertical).max(ground.min_y() + bottom),
            cz - config.max_horizontal,
        ),
        high: (
            cx + config.max_horizontal + 1,
            (y + config.max_vertical + 1).min(ground.max_y() + 1 - top),
            cz + config.max_horizontal + 1,
        ),
        holes: vec![start_box],
    };
    let mut placer = Placer {
        pools,
        ground,
        max_depth: config.size,
        expansion_hack: config.expansion_hack,
        random,
        pieces: vec![center],
        shapes: vec![limit],
        queue: Queue::default(),
    };
    placer.try_placing_children(0, 0, 0);
    while let Some(state) = placer.queue.next() {
        placer.try_placing_children(state.piece, state.free, state.depth);
    }
    Some(Assembly {
        position: (cx, y, cz),
        pieces: placer.pieces,
    })
}
