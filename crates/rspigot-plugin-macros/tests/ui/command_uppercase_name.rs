use rspigot_plugin::prelude::*;

#[derive(Plugin)]
struct Shop;

#[listeners]
impl Shop {
    #[command("Give")]
    async fn give(&self, ctx: Context) {
        let _ = ctx;
    }
}

fn main() {}
