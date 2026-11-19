use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_plugin::prelude::*;
use rspigot_plugin::{EquipmentSlot, Npc, NpcPose, NpcSlot, NpcSpec};

const KEY: &str = "scout";
const NAME: &str = "Scout";
const START_SLOT: u16 = 2;
const CANCEL_SLOT: u16 = 6;
const STEPS: u32 = 5;
const STEP: Duration = Duration::from_secs(2);
const GAZE: f64 = 8.0;

#[derive(Plugin, Default)]
struct AgentNpc {
    npc: Mutex<Option<Npc>>,
    menu: Mutex<Option<CustomInventory>>,
    run: Arc<AtomicU64>,
}

impl AgentNpc {
    fn npc(&self) -> Option<Npc> {
        self.npc.lock().ok().and_then(|npc| npc.clone())
    }

    fn menu(&self) -> Option<CustomInventory> {
        self.menu.lock().ok().and_then(|menu| menu.clone())
    }

    fn start(&self, player: &Player) {
        let Some(npc) = self.npc() else { return };
        let run = self.run.fetch_add(1, Ordering::SeqCst) + 1;
        player
            .message
            .send(format!("{}{NAME} starts a task", ChatColor::Green));
        npc.set_pose(NpcPose::Crouching);
        tokio::spawn(progress(npc, run, Arc::clone(&self.run)));
    }

    fn cancel(&self, player: &Player) {
        let Some(npc) = self.npc() else { return };
        self.run.fetch_add(1, Ordering::SeqCst);
        npc.set_pose(NpcPose::Standing);
        npc.set_label(format!("{}Idle", ChatColor::Gray));
        player
            .message
            .send(format!("{}{NAME} stops", ChatColor::Yellow));
    }
}

async fn progress(npc: Npc, run: u64, current: Arc<AtomicU64>) {
    for step in 1..=STEPS {
        if current.load(Ordering::SeqCst) != run {
            return;
        }
        let percent = step * 100 / STEPS;
        npc.set_label(format!(
            "{}Working {}{percent}%",
            ChatColor::Yellow,
            ChatColor::White
        ));
        npc.swing_main_hand();
        tokio::time::sleep(STEP).await;
    }
    npc.set_pose(NpcPose::Standing);
    npc.set_label(format!("{}Done", ChatColor::Green));
}

#[listeners]
impl AgentNpc {
    async fn on_enable(&self, server: &Server) {
        match server
            .create_inventory(1, format!("{}Task", ChatColor::Gold))
            .await
        {
            Ok(menu) => {
                menu.set(
                    START_SLOT,
                    Some(
                        ItemStack::new(Material::Book, 1)
                            .name(format!("{}Start", ChatColor::Green)),
                    ),
                );
                menu.set(
                    CANCEL_SLOT,
                    Some(
                        ItemStack::new(Material::Barrier, 1)
                            .name(format!("{}Cancel", ChatColor::Red)),
                    ),
                );
                if let Ok(mut held) = self.menu.lock() {
                    *held = Some(menu);
                }
            }
            Err(error) => tracing::warn!("No menu: {error}"),
        }

        let spawn = match server.world.get_state().await {
            Ok(state) => state.spawn,
            Err(error) => {
                tracing::warn!("No world: {error}");
                return;
            }
        };
        let at = Position {
            x: f64::from(spawn.x) + 3.5,
            y: f64::from(spawn.y),
            z: f64::from(spawn.z) + 0.5,
        };
        let spec = NpcSpec::new(KEY, NAME, at);
        match server.world.npcs.create(spec).await {
            Ok(npc) => {
                npc.set_equipment(NpcSlot::Head, Some(ItemStack::new(Material::IronHelmet, 1)));
                npc.look_at_nearest(GAZE);
                npc.set_label(format!("{}Idle", ChatColor::Gray));
                if let Ok(mut held) = self.npc.lock() {
                    *held = Some(npc);
                }
            }
            Err(error) => tracing::warn!("No npc: {error}"),
        }
    }

    async fn on_disable(&self) {
        if let Some(npc) = self.npc() {
            npc.remove();
        }
    }

    async fn on_player_interact_at_entity(&self, event: &PlayerInteractAtEntity) {
        if event.hand() != EquipmentSlot::Hand {
            return;
        }
        let Some(npc) = self.npc() else { return };
        if event.right_clicked().entity_id != npc.entity_id {
            return;
        }
        let player = event.player();
        if let Some(menu) = self.menu()
            && let Err(error) = player.inventory.open(&menu).await
        {
            tracing::warn!("Could not open the menu for {}: {error}", player.name);
        }
    }

    async fn on_inventory_click(&self, event: &InventoryClick) {
        let menu = self.menu();
        let in_menu = menu.is_some()
            && event.clicked_inventory().and_then(InventoryRef::custom) == menu.as_ref();
        if !in_menu {
            return;
        }
        let player = event.player();
        match u16::try_from(event.slot()) {
            Ok(START_SLOT) => {
                player.inventory.close();
                self.start(player);
            }
            Ok(CANCEL_SLOT) => {
                player.inventory.close();
                self.cancel(player);
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(AgentNpc::default()).await
}
