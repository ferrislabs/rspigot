mod commands;
mod items;
mod mobs;
mod zones;

use rspigot_plugin::prelude::*;

use crate::commands::FactionCommands;
use crate::items::Items;
use crate::mobs::Mobs;
use crate::zones::Zones;

#[derive(Plugin)]
#[plugin(name = "factions", version = "1.0.0")]
struct Factions {
    #[module]
    items: Items,
    #[module]
    zones: Zones,
    #[module]
    commands: FactionCommands,
    #[module]
    mobs: Mobs,
}

impl Factions {
    fn new() -> Self {
        let zones = Zones::default();
        Self {
            items: Items::new(zones.clone()),
            commands: FactionCommands::new(zones.clone()),
            zones,
            mobs: Mobs::default(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Factions::new()).await
}
