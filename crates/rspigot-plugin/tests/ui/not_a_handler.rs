use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Guard;

#[listeners]
impl Guard {
    async fn on_player_join(&self, event: &PlayerJoin) {
        event.player().message().send(self.greeting());
    }

    fn greeting(&self) -> &'static str {
        "Welcome!"
    }
}

fn main() {}
