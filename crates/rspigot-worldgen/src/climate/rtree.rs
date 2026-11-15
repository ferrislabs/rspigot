use rspigot_data::multi_noise::BiomePoint;

pub(crate) const DIMENSIONS: usize = 7;
const CHILDREN: usize = 19;

type Space = [(i64, i64); DIMENSIONS];

#[derive(Debug, Clone)]
enum Tree {
    Leaf(usize),
    Sub { space: Space, children: Vec<Tree> },
}

#[derive(Debug, Clone)]
pub(crate) struct Index {
    spaces: Vec<Space>,
    biomes: Vec<u8>,
    root: Tree,
}

fn space_of_point(point: &BiomePoint) -> Space {
    std::array::from_fn(|d| {
        if d < 6 {
            (i64::from(point.low[d]), i64::from(point.high[d]))
        } else {
            (i64::from(point.offset), i64::from(point.offset))
        }
    })
}

fn span(spaces: impl Iterator<Item = Space>) -> Space {
    let mut result: Option<Space> = None;
    for space in spaces {
        result = Some(match result {
            None => space,
            Some(current) => std::array::from_fn(|d| {
                (current[d].0.min(space[d].0), current[d].1.max(space[d].1))
            }),
        });
    }
    result.unwrap_or([(0, 0); DIMENSIONS])
}

fn distance_to(range: (i64, i64), value: i64) -> i64 {
    let above = value - range.1;
    let below = range.0 - value;
    if above > 0 { above } else { below.max(0) }
}

fn distance(space: &Space, target: &[i64; DIMENSIONS]) -> i64 {
    space
        .iter()
        .zip(target)
        .map(|(range, value)| {
            let d = distance_to(*range, *value);
            d * d
        })
        .sum()
}

fn middle(range: (i64, i64), absolute: bool) -> i64 {
    let mid = (range.0 + range.1) / 2;
    if absolute { mid.abs() } else { mid }
}

fn cost(space: &Space) -> i64 {
    space.iter().map(|(min, max)| (max - min).abs()).sum()
}

struct Bucket {
    space: Space,
    items: Vec<usize>,
}

fn sort_items(spaces: &[Space], items: &mut [usize], dimension: usize, absolute: bool) {
    items.sort_by_key(|item| {
        let key: [i64; DIMENSIONS] =
            std::array::from_fn(|i| middle(spaces[*item][(dimension + i) % DIMENSIONS], absolute));
        key
    });
}

fn bucketize(spaces: &[Space], items: &[usize]) -> Vec<Bucket> {
    let exponent = ((items.len() as f64 - 0.01).ln() / (CHILDREN as f64).ln()).floor();
    let size = (CHILDREN as f64).powf(exponent) as usize;
    let make = |items: Vec<usize>| Bucket {
        space: span(items.iter().map(|i| spaces[*i])),
        items,
    };
    let mut buckets = Vec::new();
    let mut current = Vec::new();
    for item in items {
        current.push(*item);
        if current.len() >= size {
            buckets.push(make(std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        buckets.push(make(current));
    }
    buckets
}

fn build(spaces: &[Space], mut items: Vec<usize>) -> Tree {
    if items.len() == 1 {
        return Tree::Leaf(items[0]);
    }
    if items.len() <= CHILDREN {
        items.sort_by_key(|item| {
            (0..DIMENSIONS)
                .map(|d| middle(spaces[*item][d], true))
                .sum::<i64>()
        });
        return Tree::Sub {
            space: span(items.iter().map(|i| spaces[*i])),
            children: items.into_iter().map(Tree::Leaf).collect(),
        };
    }
    let mut best_cost = i64::MAX;
    let mut best_dimension = 0;
    let mut best: Vec<Bucket> = Vec::new();
    for dimension in 0..DIMENSIONS {
        sort_items(spaces, &mut items, dimension, false);
        let buckets = bucketize(spaces, &items);
        let total: i64 = buckets.iter().map(|b| cost(&b.space)).sum();
        if best_cost > total {
            best_cost = total;
            best_dimension = dimension;
            best = buckets;
        }
    }
    best.sort_by_key(|bucket| {
        let key: [i64; DIMENSIONS] =
            std::array::from_fn(|i| middle(bucket.space[(best_dimension + i) % DIMENSIONS], true));
        key
    });
    let children: Vec<Tree> = best
        .into_iter()
        .map(|bucket| build(spaces, bucket.items))
        .collect();
    let space = span(children.iter().map(|c| match c {
        Tree::Leaf(i) => spaces[*i],
        Tree::Sub { space, .. } => *space,
    }));
    Tree::Sub { space, children }
}

impl Index {
    pub(crate) fn new(points: &[BiomePoint]) -> Self {
        let spaces: Vec<Space> = points.iter().map(space_of_point).collect();
        let biomes = points.iter().map(|p| p.biome).collect();
        let root = build(&spaces, (0..points.len()).collect());
        Self {
            spaces,
            biomes,
            root,
        }
    }

    pub(crate) fn find(&self, target: &[i64; 6]) -> u8 {
        let full = [
            target[0], target[1], target[2], target[3], target[4], target[5], 0,
        ];
        let leaf = self.search(&self.root, &full, None);
        self.biomes[leaf]
    }

    fn node_space<'a>(&'a self, node: &'a Tree) -> &'a Space {
        match node {
            Tree::Leaf(i) => &self.spaces[*i],
            Tree::Sub { space, .. } => space,
        }
    }

    fn search(&self, node: &Tree, target: &[i64; DIMENSIONS], candidate: Option<usize>) -> usize {
        match node {
            Tree::Leaf(i) => *i,
            Tree::Sub { children, .. } => {
                let mut best_distance =
                    candidate.map_or(i64::MAX, |c| distance(&self.spaces[c], target));
                let mut best = candidate;
                for child in children {
                    let d = distance(self.node_space(child), target);
                    if best_distance > d {
                        let leaf = self.search(child, target, best);
                        let leaf_distance = distance(&self.spaces[leaf], target);
                        if best_distance > leaf_distance {
                            best_distance = leaf_distance;
                            best = Some(leaf);
                        }
                    }
                }
                best.unwrap_or(0)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn find_brute_force(&self, target: &[i64; 6]) -> u8 {
        let full = [
            target[0], target[1], target[2], target[3], target[4], target[5], 0,
        ];
        let mut best = 0;
        let mut best_distance = i64::MAX;
        for (i, space) in self.spaces.iter().enumerate() {
            let d = distance(space, &full);
            if d < best_distance {
                best_distance = d;
                best = i;
            }
        }
        self.biomes[best]
    }
}
