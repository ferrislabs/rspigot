use rspigot_plugin::prelude::*;

struct Leaf;

#[listeners]
impl Leaf {}

#[derive(Module)]
struct Node {
    #[module]
    leaf: Leaf,
}

#[listeners]
impl Node {
    async fn on_player_join(&self, event: &PlayerJoin) {
        let _ = event;
    }
}

fn main() {}
