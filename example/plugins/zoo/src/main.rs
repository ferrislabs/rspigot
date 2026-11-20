use rspigot_plugin::prelude::*;

const SPAWN_AHEAD: f64 = 3.0;

#[derive(Plugin, Default)]
struct Zoo;

#[listeners]
impl Zoo {
    #[command("zoo")]
    async fn zoo(&self, ctx: Context) {
        let player = ctx.sender();
        let world = &player.world;
        let at = Position {
            x: player.position.x + SPAWN_AHEAD,
            ..player.position
        };
        let zombie = match world.spawn(EntityType::Zombie, at).await {
            Ok(zombie) => zombie,
            Err(error) => {
                ctx.reply(format!("Could not spawn the zombie: {error}"));
                return;
            }
        };
        zombie.name.set(format!("{}Bob", ChatColor::Gold));
        zombie.name.set_visible(true);
        zombie.equipment.set(
            EquipmentSlot::Head,
            &ItemStack::new(Material::IronHelmet, 1),
        );
        zombie.equipment.set(
            EquipmentSlot::Chest,
            &ItemStack::new(Material::IronChestplate, 1),
        );
        zombie
            .equipment
            .set(EquipmentSlot::Hand, &ItemStack::new(Material::IronSword, 1));
        zombie.ai.set(false);

        let base = BlockPos {
            x: at.x.floor() as i32 + 2,
            y: at.y.floor() as i32,
            z: at.z.floor() as i32,
        };
        let chest = world.block(base);
        chest.set(BlockType::Chest);
        chest.inventory.add(&ItemStack::new(Material::Diamond, 3));
        chest
            .inventory
            .add(&ItemStack::new(Material::GoldIngot, 16));

        let sign = world.block(BlockPos {
            z: base.z + 2,
            ..base
        });
        sign.set(BlockType::OakSign);
        sign.sign
            .set_lines(Side::Front, &["Welcome to", "the zoo", "", "Bob is calm"]);
        ctx.reply("The zoo is open");
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Zoo).await
}
