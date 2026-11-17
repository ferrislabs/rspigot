use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Guard;

#[listeners]
impl Guard {
    async fn on_player_join(&mut self, event: &PlayerJoin) {
        event.player().message().send("Welcome!");
    }
}

fn main() {}
