use std::sync::Mutex;

use rspigot_plugin::prelude::*;

const DEFAULTS: &str = include_str!("../config.toml");
const PLUGIN: &str = "market";
const BUY: &str = "shop.buy";

#[derive(Plugin)]
#[plugin(name = "market", version = "1.0.0")]
struct Market {
    config: Mutex<Config>,
}

impl Market {
    fn new() -> Self {
        if let Err(error) = Config::save_default(PLUGIN, DEFAULTS) {
            tracing::warn!("Could not write the default config: {error}");
        }
        let config = Config::load(PLUGIN).unwrap_or_else(|error| {
            tracing::warn!("Could not read the config, using none: {error}");
            Config::open("plugins/market-empty").expect("an empty config")
        });
        Self {
            config: Mutex::new(config),
        }
    }

    fn price(&self, item: Material) -> Option<i64> {
        let key = item.key().trim_start_matches("minecraft:");
        self.config.lock().ok()?.get_i64(&format!("prices.{key}"))
    }
}

#[listeners]
impl Market {
    #[command("shop", aliases = ["s", "store"])]
    async fn shop(&self, ctx: Context) {
        ctx.reply("/shop list, /shop buy <item> <amount>, /shop reload");
    }

    #[command("shop list")]
    async fn list(&self, ctx: Context) {
        let prices = self
            .config
            .lock()
            .ok()
            .and_then(|config| config.get_keys("prices").map(|keys| keys.join(", ")))
            .unwrap_or_default();
        ctx.reply(format!("For sale: {prices}"));
    }

    #[command("shop buy", permission = "shop.buy")]
    async fn buy(&self, ctx: Context, item: Material, amount: u32) {
        let player = &ctx.sender;
        let Some(price) = self.price(item) else {
            ctx.reply("That item is not for sale");
            return;
        };
        let total = price.saturating_mul(i64::from(amount));
        let level = match player.get_state().await {
            Ok(state) => i64::from(state.level),
            Err(error) => {
                ctx.reply(format!("Could not read your level: {error}"));
                return;
            }
        };
        if level < total {
            ctx.reply(format!("That costs {total} levels, you have {level}"));
            return;
        }
        let Ok(levels) = i32::try_from(total) else {
            ctx.reply("That is too many");
            return;
        };
        player.experience.give_levels(-levels);
        for _ in 0..amount {
            player.inventory.add(ItemStack::new(item, 1));
        }
        ctx.reply(format!("Bought {amount} for {total} levels"));
    }

    #[command("shop reload", permission = "shop.admin")]
    async fn reload(&self, ctx: Context) {
        let reloaded = self
            .config
            .lock()
            .map_err(|_| "the config is locked".to_owned())
            .and_then(|mut config| config.reload().map_err(|error| error.to_string()));
        match reloaded {
            Ok(()) => ctx.reply("Reloaded"),
            Err(error) => ctx.reply(format!("Could not reload: {error}")),
        }
    }

    async fn on_enable(&self, server: &Server) {
        let voucher = Recipe::shaped(
            "market:voucher",
            ItemStack::new(Material::Paper, 3).name("Shop voucher"),
            ["EPE", " P "],
            [
                ('E', Ingredient::item(Material::Emerald)),
                ('P', Ingredient::tag("minecraft:planks")),
            ],
        )
        .group("market")
        .category(RecipeCategory::Misc)
        .auto_discover(true);
        if let Err(error) = server.recipes.add(voucher).await {
            tracing::warn!("Could not register the voucher recipe: {error}");
        }
    }

    async fn on_player_join(&self, event: &PlayerJoin) {
        let open = self
            .config
            .lock()
            .ok()
            .and_then(|config| config.get_bool("open-to-all"))
            .unwrap_or(false);
        let player = event.player();
        if open {
            player.permissions.set(BUY, true);
            player.command.refresh();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Market::new()).await
}
