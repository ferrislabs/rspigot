use std::sync::Mutex;

use rspigot_plugin::prelude::*;

const LEAVE_SLOT: u16 = 4;

#[derive(Plugin, Default)]
struct Welcome {
    menu: Mutex<Option<CustomInventory>>,
}

impl Welcome {
    fn menu(&self) -> Option<CustomInventory> {
        self.menu.lock().ok().and_then(|menu| menu.clone())
    }
}

#[listeners]
impl Welcome {
    async fn on_enable(&self, server: &Server) {
        let menu = server.create_inventory(1, format!("{}Menu", ChatColor::Gold));
        let leave =
            ItemStack::new(Material::Compass, 1).name(format!("{}Leave", ChatColor::Yellow));
        menu.set(LEAVE_SLOT, Some(leave));
        if let Ok(mut held) = self.menu.lock() {
            *held = Some(menu);
        }
    }

    async fn on_player_join(&self, event: &PlayerJoin) {
        let player = event.player();
        player.message().send(format!("Welcome {}!", player.name));
        player
            .title()
            .send(Title::new(format!("{}Welcome", ChatColor::Gold)).subtitle(&player.name));
        player
            .inventory()
            .add(ItemStack::new(Material::Compass, 1).name("§eMenu"));

        match player.server().resolve().await {
            Ok(server) => tracing::info!(
                "{} joined, {} of {} players online",
                player.name,
                server.online,
                server.max_players
            ),
            Err(error) => tracing::warn!("Could not read the server: {error}"),
        }

        let world = player.world();
        let feet = player.position;
        world
            .sound()
            .play(SoundCue::new(Sound::EntityPlayerLevelup).at(feet));
        let below = BlockPos {
            x: feet.x.floor() as i32,
            y: feet.y.floor() as i32 - 1,
            z: feet.z.floor() as i32,
        };
        match world.block(below).resolve().await {
            Ok(block) => tracing::info!("{} stands on {}", player.name, block.kind.key()),
            Err(error) => tracing::warn!("Could not read the block under {}: {error}", player.name),
        }

        match world.spawn(EntityType::Chicken, feet).await {
            Ok(chicken) => chicken.add_effect(Effect::new(EffectType::Glowing, 200, 0)),
            Err(error) => tracing::warn!("No chicken for {}: {error}", player.name),
        }

        if let Some(menu) = self.menu() {
            player.inventory().open(&menu);
        }
    }

    async fn on_inventory_click(&self, event: &InventoryClick) {
        let menu = self.menu();
        let in_menu = menu.is_some()
            && event.clicked_inventory().and_then(InventoryRef::custom) == menu.as_ref();
        if in_menu && event.slot() == i32::from(LEAVE_SLOT) {
            let player = event.player();
            player.inventory().close();
            player.message().send("§aSee you around");
        }
    }

    #[priority(High)]
    async fn on_player_move(&self, event: &mut PlayerMove) {
        event.cancel();
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Welcome::default()).await
}
