use rspigot_domain::BlockPos;

use crate::heap::BinaryHeap;
use crate::path::{Path, PathNode, manhattan};
use crate::walk::{Node, Walker, distance, distance_xz};
use crate::{BlockView, MobProfile, MobState};

const FUDGING: f32 = 1.5;
const REQUIRED_PATH_LENGTH: f32 = 16.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PathFinder {
    profile: MobProfile,
    max_visited_nodes: i32,
    max_path_length: f32,
    visited_multiplier: f32,
}

struct Target {
    pos: BlockPos,
    best_heuristic: f32,
    best_node: Option<usize>,
}

impl Target {
    fn update_best(&mut self, heuristic: f32, node: usize) {
        if heuristic < self.best_heuristic {
            self.best_heuristic = heuristic;
            self.best_node = Some(node);
        }
    }
}

impl PathFinder {
    pub fn new(profile: MobProfile) -> Self {
        let max_visited_nodes = (f64::from(profile.follow_range) * 16.0).floor() as i32;
        let max_path_length = profile.follow_range.max(REQUIRED_PATH_LENGTH);
        Self {
            profile,
            max_visited_nodes,
            max_path_length,
            visited_multiplier: 1.0,
        }
    }

    pub fn profile(&self) -> &MobProfile {
        &self.profile
    }

    pub fn max_visited_nodes(&self) -> i32 {
        self.max_visited_nodes
    }

    pub fn set_max_visited_nodes(&mut self, nodes: i32) {
        self.max_visited_nodes = nodes;
    }

    pub fn max_path_length(&self) -> f32 {
        self.max_path_length
    }

    pub fn set_required_path_length(&mut self, length: f32) {
        self.max_path_length = self.profile.follow_range.max(length);
        self.max_visited_nodes = (self.max_path_length * 16.0).floor() as i32;
    }

    pub fn set_max_path_length(&mut self, length: f32) {
        self.max_path_length = length;
    }

    pub fn set_visited_multiplier(&mut self, multiplier: f32) {
        self.visited_multiplier = multiplier;
    }

    pub fn find_path(
        &self,
        view: &impl BlockView,
        mob: &MobState,
        targets: &[BlockPos],
        reach: i32,
    ) -> Option<Path> {
        let mut unique: Vec<BlockPos> = Vec::with_capacity(targets.len());
        for target in targets {
            if !unique.contains(target) {
                unique.push(*target);
            }
        }
        if unique.is_empty() {
            return None;
        }
        let profile = self.profile.prepared_for_search();
        let mut walker = Walker::new(view, &profile, *mob);
        let start = walker.start();
        let mut targets: Vec<Target> = unique
            .into_iter()
            .map(|pos| Target {
                pos,
                best_heuristic: f32::MAX,
                best_node: None,
            })
            .collect();
        let start_pos = walker.nodes[start].pos;
        let heuristic = best_heuristic(&walker.nodes, start, &mut targets);
        let node = &mut walker.nodes[start];
        node.g = 0.0;
        node.h = heuristic;
        node.f = heuristic;
        let mut open = BinaryHeap::default();
        open.insert(&mut walker.nodes, start);
        let mut reached: Vec<usize> = Vec::new();
        let mut neighbors = Vec::with_capacity(8);
        let limit = (self.max_visited_nodes as f32 * self.visited_multiplier) as i32;
        let max_range = self.max_path_length;
        let mut visited = 0;
        let mut popped = 0;
        while !open.is_empty() {
            visited += 1;
            if visited >= limit {
                break;
            }
            let Some(current) = open.pop(&mut walker.nodes) else {
                break;
            };
            popped += 1;
            walker.nodes[current].closed = true;
            let current_pos = walker.nodes[current].pos;
            for (index, target) in targets.iter().enumerate() {
                if manhattan(current_pos, target.pos) <= reach as f32 && !reached.contains(&index) {
                    reached.push(index);
                }
            }
            if !reached.is_empty() {
                break;
            }
            if distance(current_pos, start_pos) >= max_range {
                continue;
            }
            walker.neighbors(current, &mut neighbors);
            for &neighbor in &neighbors {
                let neighbor_pos = walker.nodes[neighbor].pos;
                let step = if self.profile.horizontal_step_cost {
                    distance_xz(current_pos, neighbor_pos)
                } else {
                    distance(current_pos, neighbor_pos)
                };
                let walked = walker.nodes[current].walked_distance + step;
                let g = walker.nodes[current].g + step + walker.nodes[neighbor].cost_malus;
                walker.nodes[neighbor].walked_distance = walked;
                let node = &walker.nodes[neighbor];
                if walked < max_range && (!node.in_open_set() || g < node.g) {
                    let h = best_heuristic(&walker.nodes, neighbor, &mut targets) * FUDGING;
                    let node = &mut walker.nodes[neighbor];
                    node.came_from = Some(current);
                    node.g = g;
                    node.h = h;
                    if node.in_open_set() {
                        open.change_cost(&mut walker.nodes, neighbor, g + h);
                    } else {
                        node.f = g + h;
                        open.insert(&mut walker.nodes, neighbor);
                    }
                }
            }
        }
        let best = if reached.is_empty() {
            targets
                .iter()
                .filter_map(|target| target.best_node.map(|node| (node, target.pos)))
                .map(|(node, pos)| reconstruct(&walker.nodes, node, pos, false))
                .min_by(|left, right| {
                    left.dist_to_target()
                        .total_cmp(&right.dist_to_target())
                        .then(left.node_count().cmp(&right.node_count()))
                })
        } else {
            reached
                .iter()
                .filter_map(|index| {
                    let target = &targets[*index];
                    target.best_node.map(|node| (node, target.pos))
                })
                .map(|(node, pos)| reconstruct(&walker.nodes, node, pos, true))
                .min_by_key(Path::node_count)
        };
        best.map(|path| path.with_visited(popped))
    }
}

fn best_heuristic(nodes: &[Node], node: usize, targets: &mut [Target]) -> f32 {
    let pos = nodes[node].pos;
    let mut best = f32::MAX;
    for target in targets {
        let heuristic = distance(pos, target.pos);
        target.update_best(heuristic, node);
        best = best.min(heuristic);
    }
    best
}

fn reconstruct(nodes: &[Node], end: usize, target: BlockPos, reached: bool) -> Path {
    let mut chain = Vec::new();
    let mut cursor = Some(end);
    while let Some(index) = cursor.filter(|_| chain.len() < nodes.len()) {
        let node = &nodes[index];
        chain.push(PathNode {
            pos: node.pos,
            path_type: node.path_type,
            cost_malus: node.cost_malus,
            walked_distance: node.walked_distance,
        });
        cursor = node.came_from;
    }
    chain.reverse();
    Path::new(chain, target, reached)
}
