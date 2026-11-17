use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Shop;

#[listeners]
impl Shop {
    #[command("give")]
    async fn give(&self) {}
}

fn main() {}
