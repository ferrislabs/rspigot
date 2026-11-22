use rspigot_plugin::prelude::*;

#[derive(Plugin)]
#[plugin(name = "shop", depends = "economy")]
struct Shop {
    #[module]
    sweep: Sweep,
}

struct Sweep;

#[listeners]
impl Sweep {}

fn main() {}
