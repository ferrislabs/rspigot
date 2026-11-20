use rspigot_plugin::prelude::*;

use crate::zones::Zones;

pub struct FactionCommands {
    zones: Zones,
}

impl FactionCommands {
    pub const fn new(zones: Zones) -> Self {
        Self { zones }
    }
}

#[listeners]
impl FactionCommands {
    #[command("faction")]
    async fn faction(&self, ctx: Context) {
        ctx.reply("/faction wand, /faction claims, /faction unclaim");
    }

    #[command("faction claims")]
    async fn claims(&self, ctx: Context) {
        ctx.reply(format!("{} claimed zones", self.zones.count()));
    }

    #[command("faction unclaim", permission = "factions.admin")]
    async fn unclaim(&self, ctx: Context) {
        ctx.reply(format!("Released {} zones", self.zones.clear()));
    }
}
