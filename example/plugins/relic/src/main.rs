use rspigot_plugin::prelude::*;

const KIND: &str = "relic:kind";
const CHARGES: &str = "relic:charges";
const BOOST_TICKS: i32 = 200;
const MAX_CHARGES: i64 = 3;

fn lore(charges: i64) -> [String; 2] {
    [
        "Right click to ignite your step".to_owned(),
        format!("{}Charges: {charges}/{MAX_CHARGES}", ChatColor::Gray),
    ]
}

fn relic() -> ItemStack {
    ItemStack::new(Material::GoldenSword, 1)
        .name(format!("{}Relic of Fire", ChatColor::Gold))
        .lore(lore(MAX_CHARGES))
        .unbreakable(true)
        .hide_flags([ItemFlag::HideUnbreakable, ItemFlag::HideAttributes])
        .attribute_modifier(AttributeModifier::new(
            Attribute::AttackDamage,
            "relic:fire_edge",
            4.0,
            Operation::AddValue,
            EquipmentSlot::Hand,
        ))
        .rarity(Rarity::Epic)
        .with_meta(KIND, "fire")
        .with_meta(CHARGES, MAX_CHARGES)
}

#[derive(Plugin, Default)]
struct Relic;

#[listeners]
impl Relic {
    #[command("relic")]
    async fn relic(&self, ctx: Context) {
        ctx.sender.inventory.add(relic());
        ctx.reply("The relic is yours");
    }

    async fn on_player_interact(&self, event: &mut PlayerInteract) {
        if !matches!(
            event.action(),
            Action::RightClickAir | Action::RightClickBlock
        ) {
            return;
        }
        let Some(item) = event.item() else {
            return;
        };
        if item.meta_get(KIND).as_ref().and_then(MetaValue::as_str) != Some("fire") {
            return;
        }
        let player = event.player();
        let charges = item.meta_get(CHARGES).and_then(|value| value.as_i64());
        match charges {
            Some(left) if left > 0 => {
                let mut spent = item.clone().lore(lore(left - 1));
                spent.meta_set(CHARGES, left - 1);
                let slot = match event.hand() {
                    Some(EquipmentSlot::OffHand) => Some(InventorySlot::Offhand),
                    _ => player
                        .inventory
                        .get_items()
                        .await
                        .ok()
                        .map(|state| InventorySlot::Hotbar(state.held_slot)),
                };
                if let Some(slot) = slot {
                    player.inventory.set(SlotItem::new(slot, Some(spent)));
                }
                player
                    .effects
                    .add(Effect::new(EffectType::Speed, BOOST_TICKS, 1));
                player
                    .message
                    .send(format!("The relic flares, {} charges left", left - 1));
            }
            _ => player.message.send("The relic is spent"),
        }
        event.cancel();
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(Relic).await
}
