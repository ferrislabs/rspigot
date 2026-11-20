use rspigot_plugin::prelude::*;

struct Leaf;

#[listeners]
impl Leaf {}

#[derive(Module)]
struct Node(Leaf);

fn main() {}
