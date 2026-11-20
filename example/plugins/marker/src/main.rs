use rspigot_plugin::prelude::*;

const KEY: &str = "marker:kind";
const REACH: f64 = 6.0;

#[derive(Plugin, Default)]
struct Marker;

#[listeners]
impl Marker {
    #[command("mark")]
    async fn mark(&self, ctx: Context, kind: String) {
        let player = &ctx.sender;
        let clearing = kind == "clear";
        if let Ok(Some(Target::Entity(entity))) = player.target_entity(REACH).await {
            if clearing {
                entity.meta.remove(KEY);
            } else {
                entity.meta.set(KEY, kind.as_str());
            }
            ctx.reply(format!("{:?} marked", entity.kind));
            return;
        }
        match player.target_block(REACH).await {
            Ok(Some(target)) => {
                if clearing {
                    target.block.meta.remove(KEY);
                } else {
                    target.block.meta.set(KEY, kind.as_str());
                }
                ctx.reply(format!("{} marked", target.kind.key()));
            }
            Ok(None) => ctx.reply("Look at a block or a mob"),
            Err(error) => ctx.reply(format!("Could not aim: {error}")),
        }
    }

    async fn on_block_break(&self, event: &mut BlockBreak) {
        if event.block().meta.get_str(KEY) == Some("unbreakable") {
            event.cancel();
        }
    }

    async fn on_entity_damage(&self, event: &mut EntityDamage) {
        if event.entity().meta.get_str(KEY) == Some("immortal") {
            event.cancel();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Marker).await
}
