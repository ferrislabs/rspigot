use rspigot_plugin::prelude::*;

struct Leaf;

#[listeners]
impl Leaf {}

#[derive(Module)]
struct Node {
    #[module(own)]
    leaf: Leaf,
}

fn main() {}
