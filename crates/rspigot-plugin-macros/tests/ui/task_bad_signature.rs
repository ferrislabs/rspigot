use rspigot_plugin::prelude::*;

struct Shop;

#[listeners]
impl Shop {
    #[task(every = "5s")]
    async fn takes_a_number(&self, count: u32) {
        let _ = count;
    }

    #[task(every = "5s")]
    fn not_async(&self) {}
}

fn main() {}
