use rspigot_proxy_plugin::wire::ArgumentKind;
use rspigot_proxy_plugin::{
    ConnectOutcome, Hook, Invocation, Player, Plugin, Proxy, Verdict, command, rich,
};

const SERVER_PERMISSION: &str = "proxy.command.server";

struct Network {
    lobby: String,
    staff: Vec<String>,
}

impl Network {
    fn from_env() -> Self {
        Self {
            lobby: std::env::var("LOBBY").unwrap_or_else(|_| "lobby".to_owned()),
            staff: std::env::var("STAFF")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    fn is_staff(&self, name: &str) -> bool {
        self.staff
            .iter()
            .any(|staff| staff.eq_ignore_ascii_case(name))
    }

    async fn hub(&self, proxy: &Proxy, player: &Player) {
        if player.server.as_deref() == Some(self.lobby.as_str()) {
            proxy.message(&player.name, "§eYou are already on the lobby.");
            return;
        }
        move_player(proxy, player, &self.lobby).await;
    }

    async fn server(&self, proxy: &Proxy, player: &Player, target: Option<&str>) {
        let Some(target) = target else {
            let servers = proxy.servers().await.unwrap_or_default();
            let mut message = rich().text(&format!(
                "§7You are on §b{}§7. Servers: ",
                player.server.as_deref().unwrap_or("?")
            ));
            for server in servers {
                message = message.command(
                    &format!("§a{} §8({}) ", server.name, server.players),
                    &format!("/server {}", server.name),
                    Some(&format!("Go to {}", server.name)),
                );
            }
            proxy.message_rich(&player.name, message.build());
            return;
        };
        move_player(proxy, player, target).await;
    }

    async fn find(&self, proxy: &Proxy, player: &Player, target: Option<&str>) {
        let Some(target) = target else {
            proxy.message(&player.name, "§cUsage: /find <player>");
            return;
        };
        match proxy.player(target).await {
            Ok(found) => proxy.message(
                &player.name,
                &format!(
                    "§b{} §7is on §b{}",
                    found.name,
                    found.server.as_deref().unwrap_or("?")
                ),
            ),
            Err(_) => proxy.message(&player.name, &format!("§c{target} is not online.")),
        };
    }
}

async fn move_player(proxy: &Proxy, player: &Player, server: &str) {
    let text = match proxy.connect(&player.id, server).await {
        Ok(result) => match result.outcome {
            ConnectOutcome::Connected => return,
            ConnectOutcome::AlreadyThere => format!("§eYou are already on {server}."),
            ConnectOutcome::UnknownServer => format!("§c{server} does not exist."),
            ConnectOutcome::Unreachable => format!("§c{server} is not reachable right now."),
            ConnectOutcome::Denied => result
                .message
                .unwrap_or_else(|| format!("§cYou cannot join {server}.")),
            _ => format!("§cCould not move you to {server}."),
        },
        Err(error) => format!("§cCould not move you to {server}: {error}"),
    };
    proxy.message(&player.id, &text);
}

impl Plugin for Network {
    const NAME: &'static str = "network";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    fn subscriptions(&self) -> Vec<(Hook, bool)> {
        vec![(Hook::Login, false), (Hook::Kicked, true)]
    }

    fn commands(&self) -> Vec<rspigot_proxy_plugin::wire::CommandSpec> {
        vec![
            command("hub").alias("lobby").build(),
            command("server")
                .permission(SERVER_PERMISSION)
                .optional("server", ArgumentKind::Server)
                .build(),
            command("find")
                .optional("player", ArgumentKind::Player)
                .build(),
        ]
    }

    async fn login(&self, proxy: &Proxy, player: &Player) {
        if self.is_staff(&player.name) {
            proxy.set_permissions(&player.id, &[SERVER_PERMISSION], &[]);
        }
    }

    async fn kicked(
        &self,
        _proxy: &Proxy,
        _player: &Player,
        server: &str,
        reason: &str,
        _fallbacks: &[String],
    ) -> Verdict {
        if server == self.lobby {
            return Verdict::Allow;
        }
        Verdict::Redirect {
            server: self.lobby.clone(),
            message: Some(format!("§eYou were sent back to the lobby: §r{reason}")),
        }
    }

    async fn command(&self, proxy: &Proxy, invocation: &Invocation) {
        let Some(player) = &invocation.player else {
            return;
        };
        let first = invocation.arguments.first().map(String::as_str);
        match invocation.command.as_str() {
            "hub" => self.hub(proxy, player).await,
            "server" => self.server(proxy, player, first).await,
            "find" => self.find(proxy, player, first).await,
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    if let Err(error) = rspigot_proxy_plugin::run(Network::from_env()).await {
        tracing::error!("{error}");
        std::process::exit(1);
    }
}
