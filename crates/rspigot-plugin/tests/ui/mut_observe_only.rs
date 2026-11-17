use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Guard;

#[listeners]
impl Guard {
    async fn on_inventory_close(&self, event: &mut InventoryClose) {
        event.player().message().send("Bye!");
    }
}

fn main() {}
