use rspigot_plugin::prelude::*;

struct Shop;

#[listeners]
impl Shop {
    #[task(every = "5")]
    async fn no_unit(&self) {}

    #[task(every = "10ms")]
    async fn too_fast(&self) {}

    #[task]
    async fn no_period(&self) {}
}

fn main() {}
