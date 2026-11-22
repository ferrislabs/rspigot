use rspigot_plugin::prelude::*;

struct Shop;

impl Shop {
    #[task(every = "5s")]
    async fn sweep(&self) {}
}

fn main() {}
