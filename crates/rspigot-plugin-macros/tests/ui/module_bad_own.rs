use rspigot_plugin::prelude::*;

struct Leaf;

#[listeners]
impl Leaf {}

#[derive(Module)]
#[module(mine)]
struct Node {
    #[module]
    leaf: Leaf,
}

fn main() {}
