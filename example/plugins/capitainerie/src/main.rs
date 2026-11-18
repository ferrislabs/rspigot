mod api;
mod bridge;
mod config;
mod model;
mod present;

use std::sync::Arc;

use rspigot_plugin::prelude::*;
use rspigot_plugin::{Dialog, DialogButton, DialogInput, EquipmentSlot};

use bridge::{Bridge, CANCEL_ID, Clicked, RUN_PREFIX, Turn};
use config::Config;
use present::{CAPTAIN_NAME, click_slug};

const MAX_TITLE: usize = 120;
const MAX_DESCRIPTION: usize = 2000;

#[derive(Plugin)]
struct Capitainerie {
    bridge: Arc<Bridge>,
}

fn explain(error: &api::ApiError) -> String {
    let reason = match error.code() {
        Some("agent_paused") => "l'agent est en pause",
        Some("agent_archived") => "l'agent est archivé",
        Some("card_without_agent") => "la carte n'a pas d'agent",
        Some("agent_without_connector") => "l'agent n'a aucun connecteur",
        Some("no_provider") => "aucun fournisseur de modèle n'est configuré",
        Some("not_connected") => "l'intégration n'est pas connectée",
        Some("execution_already_active") => "une exécution est déjà en cours",
        Some("turn_in_progress") => "le Capitaine répond déjà",
        Some("conversation_full") => "la conversation est pleine, quittez puis reprenez",
        Some("captain_not_configured" | "captain_unavailable") => "le Capitaine est indisponible",
        _ => "Capitainerie a refusé la demande",
    };
    format!("§c{reason}")
}

fn field(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[listeners]
impl Capitainerie {
    async fn on_enable(&self, server: &Server) {
        let (bridge, server) = (Arc::clone(&self.bridge), server.clone());
        tokio::spawn(async move { bridge.run(server).await });
    }

    async fn on_player_interact_at_entity(&self, event: &PlayerInteractAtEntity) {
        if event.hand() != EquipmentSlot::Hand {
            return;
        }
        let player = event.player();
        match self.bridge.target_of(event.right_clicked().entity_id) {
            Some(Clicked::Agent(agent)) => {
                if agent.paused {
                    player
                        .message
                        .send(format!("§c{} est en pause", agent.name));
                    return;
                }
                let intro = agent
                    .role
                    .clone()
                    .unwrap_or_else(|| "Décrivez la tâche à confier.".to_owned());
                player.dialog.show(
                    Dialog::new(format!("§6Tâche pour {}", agent.name))
                        .line(intro)
                        .input(DialogInput::new("title", "Titre").max_length(MAX_TITLE as u32))
                        .input(
                            DialogInput::new("description", "Description")
                                .multiline()
                                .max_length(MAX_DESCRIPTION as u32),
                        )
                        .button(DialogButton::new(
                            "§aLancer",
                            format!("{RUN_PREFIX}{}", click_slug(&agent.slug)),
                        ))
                        .button(DialogButton::new("§cAnnuler", CANCEL_ID)),
                );
            }
            Some(Clicked::Captain) => {
                if self.bridge.toggle_chat(player.uuid) {
                    player.message.send(format!(
                        "§6Vous parlez au {CAPTAIN_NAME}. §7Écrivez dans le chat, §fquitter§7 pour sortir."
                    ));
                } else {
                    player.message.send("§7Conversation terminée");
                }
            }
            None => {}
        }
    }

    async fn on_player_custom_click(&self, event: &PlayerCustomClick) {
        let id = event.click_id();
        let Some(slug) = id.strip_prefix(RUN_PREFIX) else {
            return;
        };
        let player = event.player().clone();
        let Some(agent) = self.bridge.agent_by_click_slug(slug) else {
            player.message.send("§cCet agent n'est plus disponible");
            return;
        };
        let payload: serde_json::Value = event
            .data()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default();
        let title = field(&payload, "title");
        let description = field(&payload, "description");
        if title.is_empty() {
            player
                .message
                .send("§cIl faut un titre pour lancer une tâche");
            return;
        }
        let bridge = Arc::clone(&self.bridge);
        tokio::spawn(async move {
            match bridge.start_task(&agent, &title, &description).await {
                Ok(_) => player
                    .message
                    .send(format!("§a{} prend la tâche : §f{title}", agent.name)),
                Err(error) => {
                    tracing::warn!("Could not start a task for {}: {error}", agent.slug);
                    player.message.send(explain(&error));
                }
            }
        });
    }

    async fn on_async_player_chat(&self, event: &mut AsyncPlayerChat) {
        let player = event.player().clone();
        if !self.bridge.in_chat(player.uuid) {
            return;
        }
        event.cancel();
        let text = event.message().trim().to_owned();
        if text.is_empty() {
            return;
        }
        if Bridge::is_quit(&text) {
            self.bridge.leave_chat(player.uuid);
            player.message.send("§7Conversation terminée");
            return;
        }
        let conversation = match self.bridge.begin_turn(player.uuid) {
            Turn::Busy => {
                player
                    .message
                    .send(format!("§7{CAPTAIN_NAME} répond encore, patientez"));
                return;
            }
            Turn::Ready(conversation) => conversation,
        };
        player.message.send(format!("§7Vous§8: §f{text}"));
        let bridge = Arc::clone(&self.bridge);
        tokio::spawn(async move {
            match bridge.chat(&player, conversation.as_deref(), &text).await {
                Ok(outcome) => {
                    for (_, title) in &outcome.cards {
                        player
                            .message
                            .send(format!("§a+ Carte proposée : §f{title}"));
                    }
                    bridge.end_turn(player.uuid, Some(outcome.conversation));
                }
                Err(error) => {
                    tracing::warn!("The conversation failed: {error}");
                    player.message.send(explain(&error));
                    bridge.end_turn(player.uuid, None);
                }
            }
        });
    }

    async fn on_player_leave(&self, event: &PlayerLeave) {
        self.bridge.leave_chat(event.player().uuid);
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().init();
    dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/.env")).ok();
    dotenvy::dotenv().ok();
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("{error}");
            std::process::exit(2);
        }
    };
    tracing::info!("Bridging {}", config.base_url);
    rspigot_plugin::run(Capitainerie {
        bridge: Arc::new(Bridge::new(config)),
    })
    .await
}
