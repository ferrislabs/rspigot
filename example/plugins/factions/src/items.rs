use rspigot_plugin::prelude::*;

use crate::zones::Zones;

const KIND: &str = "factions:kind";
const WAND: &str = "claim_wand";

fn wand() -> ItemStack {
    ItemStack::new(Material::BlazeRod, 1)
        .name(format!("{}Claim Wand", ChatColor::Gold))
        .lore(["Right click a block to claim the land around it"])
        .with_meta(KIND, WAND)
}

pub struct Items {
    zones: Zones,
}

impl Items {
    pub const fn new(zones: Zones) -> Self {
        Self { zones }
    }
}

#[listeners]
impl Items {
    #[command("faction wand")]
    async fn give_wand(&self, ctx: Context) {
        ctx.sender.inventory.add(wand());
        ctx.reply("The claim wand is yours");
    }

    async fn on_player_interact(&self, event: &mut PlayerInteract) {
        if event.action() != Action::RightClickBlock {
            return;
        }
        let Some(item) = event.item() else {
            return;
        };
        if item.meta_get(KIND).as_ref().and_then(MetaValue::as_str) != Some(WAND) {
            return;
        }
        let Some(block) = event.clicked_block() else {
            return;
        };
        self.zones.claim(block.pos);
        event.player().message.send(format!(
            "Claimed the land around {}, {}, {}",
            block.pos.x, block.pos.y, block.pos.z
        ));
        event.cancel();
    }
}
