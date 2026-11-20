use rspigot_plugin::prelude::*;

struct Plain;

#[derive(Module)]
struct Node {
    #[module]
    plain: Plain,
}

fn main() {}
