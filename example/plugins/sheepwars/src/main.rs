use rspigot_plugin::prelude::*;

#[derive(Plugin, Default)]
struct SheepWars;

#[listeners]
impl SheepWars {}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(SheepWars).await
}
