use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use rspigot_plugin::prelude::*;
use sheepwars::arena::Arena;
use sheepwars::domain::RoundRules;
use sheepwars::game::{Game, Session, VoidVerdict, kind_of, load_settings};
use sheepwars::hud::{Hud, bossbar_title};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

const ADMIN_PERMISSION: &str = "sheepwars.admin";
const TICK: Duration = Duration::from_millis(50);
const ACTIONS: [&str; 4] = ["join", "leave", "forcestart", "stop"];

#[derive(Plugin, Default)]
struct SheepWars {
    game: OnceLock<Arc<Game>>,
    looper: Mutex<Option<JoinHandle<()>>>,
}

impl SheepWars {
    fn game(&self) -> Option<Arc<Game>> {
        self.game.get().cloned()
    }

    async fn actions(&self, _player: &Player, typed: &str) -> Vec<String> {
        ACTIONS
            .iter()
            .filter(|action| action.starts_with(typed))
            .map(|action| (*action).to_owned())
            .collect()
    }

    fn spawn_loop(game: Arc<Game>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TICK);
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                game.tick().await;
            }
        })
    }
}

#[listeners]
impl SheepWars {
    async fn on_enable(&self, server: &Server) {
        let arena = match Arena::create(server, load_settings()).await {
            Ok(arena) => arena,
            Err(error) => {
                tracing::error!("could not build the arena: {error}");
                return;
            }
        };
        let session = Session::new(arena.layout.clone(), RoundRules::default());
        let bar = match server
            .boss_bars
            .create(
                bossbar_title(session.round()),
                BarColor::Yellow,
                BarStyle::Solid,
                &[],
            )
            .await
        {
            Ok(bar) => bar,
            Err(error) => {
                tracing::error!("could not create the boss bar: {error}");
                return;
            }
        };
        let hud = match Hud::open(server.scoreboard_manager.main_scoreboard()).await {
            Ok(hud) => hud,
            Err(error) => {
                tracing::error!("could not open the scoreboard: {error}");
                return;
            }
        };
        let game = Arc::new(Game::new(arena, session, hud, bar));
        if self.game.set(Arc::clone(&game)).is_err() {
            return;
        }
        match server.get_players().await {
            Ok(players) => {
                for player in players {
                    game.admit(player).await;
                }
            }
            Err(error) => tracing::warn!("could not list the players: {error}"),
        }
        if let Ok(mut looper) = self.looper.lock() {
            *looper = Some(Self::spawn_loop(game));
        }
    }

    async fn on_disable(&self) {
        if let Ok(mut looper) = self.looper.lock()
            && let Some(handle) = looper.take()
        {
            handle.abort();
        }
        if let Some(game) = self.game() {
            game.close();
        }
    }

    #[command("sheepwars")]
    async fn sheepwars(&self, ctx: Context, #[suggest(Self::actions)] action: String) {
        let Some(game) = self.game() else {
            ctx.reply("§cLe jeu n'est pas prêt.");
            return;
        };
        let player = ctx.sender();
        let admin = player.permissions.has(ADMIN_PERMISSION);
        match action.as_str() {
            "join" => game.join(player).await,
            "leave" => game.quit(player).await,
            "forcestart" | "stop" if !admin => ctx.reply("§cVous ne pouvez pas faire ça."),
            "forcestart" => {
                if let Err(error) = game.force_start().await {
                    ctx.reply(format!("§c{error}"));
                }
            }
            "stop" => game.stop().await,
            other => ctx.reply(format!("§cAction inconnue : {other}")),
        }
    }

    async fn on_chunk_load(&self, event: &ChunkLoad) {
        if let Some(game) = self.game() {
            game.on_chunk_load(event);
        }
    }

    async fn on_player_join(&self, event: &PlayerJoin) {
        if let Some(game) = self.game() {
            game.admit(event.player().clone()).await;
        }
    }

    async fn on_player_leave(&self, event: &PlayerLeave) {
        if let Some(game) = self.game() {
            game.release(event.player()).await;
        }
    }

    async fn on_player_interact(&self, event: &mut PlayerInteract) {
        let Some(game) = self.game() else { return };
        let right_click = matches!(
            event.action(),
            Action::RightClickAir | Action::RightClickBlock
        );
        if !right_click || event.hand() != Some(EquipmentSlot::Hand) {
            return;
        }
        let Some(kind) = kind_of(event.material()) else {
            return;
        };
        event.cancel();
        let player = event.player().clone();
        tokio::spawn(async move {
            game.throw(&player, kind).await;
        });
    }

    async fn on_player_death(&self, event: &mut PlayerDeath) {
        let Some(game) = self.game() else { return };
        event.set_keep_inventory(true);
        let player = event.player().clone();
        tokio::spawn(async move { game.died(&player).await });
    }

    async fn on_player_respawn(&self, event: &mut PlayerRespawn) {
        if let Some(game) = self.game() {
            let location = game.respawn_location(event.player());
            event.set_respawn_location(location);
        }
    }

    async fn on_player_move(&self, event: &PlayerMove) {
        let Some(game) = self.game() else { return };
        let Some(to) = event.to() else { return };
        match game.void_verdict(event.player(), to.position.y) {
            VoidVerdict::Safe => {}
            VoidVerdict::Kill => event.player().health.kill(),
            VoidVerdict::ToLobby => {
                let player = event.player().clone();
                tokio::spawn(async move { game.rescue(&player).await });
            }
        }
    }

    async fn on_entity_damage(&self, event: &mut EntityDamage) {
        let Some(game) = self.game() else { return };
        let Some(player) = event.entity().player() else {
            return;
        };
        if game.blocks_damage_to(player, shieldable(event.cause())) {
            event.cancel();
        }
    }

    async fn on_entity_damage_by_entity(&self, event: &mut EntityDamageByEntity) {
        let Some(game) = self.game() else { return };
        let victim = event.entity().player();
        if let (Some(attacker), Some(victim)) = (event.player(), victim)
            && game.blocks_hit(attacker, victim)
        {
            event.cancel();
        }
    }

    async fn on_food_level_change(&self, event: &mut FoodLevelChange) {
        event.cancel();
    }

    async fn on_block_break(&self, event: &mut BlockBreak) {
        event.cancel();
    }

    async fn on_block_place(&self, event: &mut BlockPlace) {
        event.cancel();
    }
}

const fn shieldable(cause: EntityDamageCause) -> bool {
    matches!(
        cause,
        EntityDamageCause::Fall
            | EntityDamageCause::Suffocation
            | EntityDamageCause::FlyIntoWall
            | EntityDamageCause::Contact
    )
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    rspigot_plugin::run(SheepWars::default()).await
}
