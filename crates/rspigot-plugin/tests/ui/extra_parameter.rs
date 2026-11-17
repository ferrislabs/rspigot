use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Guard;

#[listeners]
impl Guard {
    async fn on_player_join(&self, event: &PlayerJoin, server: &Server) {
        let _ = server;
        event.player().message().send("Welcome!");
    }
}

fn main() {}
