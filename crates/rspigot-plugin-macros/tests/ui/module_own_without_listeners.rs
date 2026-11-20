use rspigot_plugin::prelude::*;

struct Leaf;

#[listeners]
impl Leaf {}

#[derive(Module)]
#[module(own)]
struct Node {
    #[module]
    leaf: Leaf,
}

fn main() {}
