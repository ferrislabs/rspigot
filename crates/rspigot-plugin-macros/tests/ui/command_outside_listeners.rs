use rspigot_plugin::prelude::*;

struct Shop;

impl Shop {
    #[command("give")]
    async fn give(&self, ctx: Context) {
        let _ = ctx;
    }
}

fn main() {}
