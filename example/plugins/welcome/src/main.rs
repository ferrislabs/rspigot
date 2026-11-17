use std::sync::Mutex;

use rspigot_plugin::prelude::*;
use rspigot_plugin::{Dimension, Generator, WorldOptions};

const LEAVE_SLOT: u16 = 4;
const PLACES: [&str; 3] = ["lobby", "hub", "zombie"];

#[derive(Plugin, Default)]
struct Welcome {
    menu: Mutex<Option<CustomInventory>>,
    lobby: Mutex<Option<World>>,
}

impl Welcome {
    fn menu(&self) -> Option<CustomInventory> {
        self.menu.lock().ok().and_then(|menu| menu.clone())
    }

    fn lobby(&self) -> Option<World> {
        self.lobby.lock().ok().and_then(|lobby| lobby.clone())
    }

    async fn places(&self, _player: &Player, typed: &str) -> Vec<String> {
        PLACES
            .iter()
            .filter(|place| place.starts_with(typed))
            .map(|place| (*place).to_owned())
            .collect()
    }
}

#[listeners]
impl Welcome {
    async fn on_enable(&self, server: &Server) {
        let menu = match server
            .create_inventory(1, format!("{}Menu", ChatColor::Gold))
            .await
        {
            Ok(menu) => menu,
            Err(error) => {
                tracing::warn!("No menu: {error}");
                return;
            }
        };
        let leave =
            ItemStack::new(Material::Compass, 1).name(format!("{}Leave", ChatColor::Yellow));
        menu.set(LEAVE_SLOT, Some(leave));
        if let Ok(mut held) = self.menu.lock() {
            *held = Some(menu);
        }

        let options = WorldOptions {
            name: "lobby".to_owned(),
            dimension: Dimension::Overworld,
            seed: 0,
            generator: Generator::Void,
            structures: false,
        };
        match server.world().create(options).await {
            Ok(lobby) => {
                if let Ok(mut held) = self.lobby.lock() {
                    *held = Some(lobby);
                }
            }
            Err(error) => tracing::warn!("No lobby: {error}"),
        }
    }

    async fn on_disable(&self) {
        let Some(lobby) = self.lobby() else { return };
        match lobby.entities.get_entities().await {
            Ok(entities) if entities.iter().all(|entity| entity.player().is_none()) => {
                if let Err(error) = lobby.delete(true).await {
                    tracing::warn!("Could not delete the lobby: {error}");
                }
            }
            Ok(_) => tracing::info!("The lobby keeps its players"),
            Err(error) => tracing::warn!("Could not read the lobby: {error}"),
        }
    }

    #[command("welcome", permission = "welcome.use")]
    async fn welcome(&self, ctx: Context, #[suggest(Self::places)] place: String) {
        let player = ctx.sender();
        match place.as_str() {
            "lobby" => match self.lobby() {
                Some(lobby) => {
                    if let Err(error) = player.location().teleport_to_world(&lobby).await {
                        ctx.reply(format!("Could not reach the lobby: {error}"));
                    }
                }
                None => ctx.reply("The lobby is closed"),
            },
            "hub" if player.permissions().has("welcome.hub") => {
                if let Err(error) = player.connection().transfer("127.0.0.1", 25565).await {
                    ctx.reply(format!("Transfer refused: {error}"));
                }
            }
            "hub" => ctx.reply("You may not use the hub"),
            "zombie" => match player
                .world()
                .spawn(EntityType::Zombie, player.position)
                .await
            {
                Ok(zombie) => {
                    zombie.attributes().set(Attribute::MaxHealth, 40.0);
                    if let Some(zombie) = zombie.as_zombie() {
                        zombie.set_baby(true);
                    }
                }
                Err(error) => ctx.reply(format!("No zombie: {error}")),
            },
            other => ctx.reply(format!("Unknown place {other}")),
        }
    }

    async fn on_entity_damage_by_entity(&self, event: &EntityDamageByEntity) {
        match event.player() {
            Some(attacker) => tracing::info!("{} hit {:?}", attacker.name, event.entity_type()),
            None => tracing::info!("{:?} hit {:?}", event.damager().kind, event.entity_type()),
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

        match player.server().get_state().await {
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
        match world.get_block(below).await {
            Ok(block) => tracing::info!("{} stands on {}", player.name, block.kind.key()),
            Err(error) => tracing::warn!("Could not read the block under {}: {error}", player.name),
        }

        match world.spawn(EntityType::Chicken, feet).await {
            Ok(chicken) => chicken.add_effect(Effect::new(EffectType::Glowing, 200, 0)),
            Err(error) => tracing::warn!("No chicken for {}: {error}", player.name),
        }

        if let Some(menu) = self.menu()
            && let Err(error) = player.inventory().open(&menu).await
        {
            tracing::warn!("Could not open the menu for {}: {error}", player.name);
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
