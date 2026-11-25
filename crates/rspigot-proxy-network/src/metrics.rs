use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Metrics {
    pub logins: AtomicU64,
    pub logins_refused: AtomicU64,
    pub switches: AtomicU64,
    pub switch_failures: AtomicU64,
    pub kicks: AtomicU64,
    pub backends_lost: AtomicU64,
    pub backend_unreachable: AtomicU64,
    pub proxy_commands: AtomicU64,
    pub packets_to_client: AtomicU64,
    pub packets_to_backend: AtomicU64,
}

pub(crate) fn bump(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn add(counter: &AtomicU64, amount: u64) {
    counter.fetch_add(amount, Ordering::Relaxed);
}

pub struct Gauges<'a> {
    pub online: usize,
    pub per_server: &'a [(String, usize)],
    pub shutting_down: bool,
}

impl Metrics {
    pub fn render(&self, gauges: &Gauges<'_>) -> String {
        let mut out = String::with_capacity(2048);
        let counters: [(&str, &str, &AtomicU64); 10] = [
            (
                "rspigot_proxy_logins_total",
                "Players that reached a backend.",
                &self.logins,
            ),
            (
                "rspigot_proxy_logins_refused_total",
                "Logins refused by the proxy, a plugin or a backend.",
                &self.logins_refused,
            ),
            (
                "rspigot_proxy_switches_total",
                "Players moved to another backend without a disconnect.",
                &self.switches,
            ),
            (
                "rspigot_proxy_switch_failures_total",
                "Moves that found no reachable backend.",
                &self.switch_failures,
            ),
            (
                "rspigot_proxy_kicks_total",
                "Kicks received from backends.",
                &self.kicks,
            ),
            (
                "rspigot_proxy_backends_lost_total",
                "Backend connections that dropped while a player was on them.",
                &self.backends_lost,
            ),
            (
                "rspigot_proxy_backend_unreachable_total",
                "Backend logins that failed or timed out.",
                &self.backend_unreachable,
            ),
            (
                "rspigot_proxy_commands_total",
                "Proxy commands run by players.",
                &self.proxy_commands,
            ),
            (
                "rspigot_proxy_packets_to_client_total",
                "Packets relayed from backends to clients.",
                &self.packets_to_client,
            ),
            (
                "rspigot_proxy_packets_to_backend_total",
                "Packets relayed from clients to backends.",
                &self.packets_to_backend,
            ),
        ];
        for (name, help, counter) in counters {
            let _ = writeln!(out, "# HELP {name} {help}");
            let _ = writeln!(out, "# TYPE {name} counter");
            let _ = writeln!(out, "{name} {}", counter.load(Ordering::Relaxed));
        }
        let _ = writeln!(
            out,
            "# HELP rspigot_proxy_players Players connected to the proxy."
        );
        let _ = writeln!(out, "# TYPE rspigot_proxy_players gauge");
        let _ = writeln!(out, "rspigot_proxy_players {}", gauges.online);
        let _ = writeln!(
            out,
            "# HELP rspigot_proxy_server_players Players on each backend."
        );
        let _ = writeln!(out, "# TYPE rspigot_proxy_server_players gauge");
        for (server, players) in gauges.per_server {
            let _ = writeln!(
                out,
                "rspigot_proxy_server_players{{server=\"{}\"}} {players}",
                escape(server)
            );
        }
        let _ = writeln!(
            out,
            "# HELP rspigot_proxy_shutting_down 1 while the proxy drains its players."
        );
        let _ = writeln!(out, "# TYPE rspigot_proxy_shutting_down gauge");
        let _ = writeln!(
            out,
            "rspigot_proxy_shutting_down {}",
            u8::from(gauges.shutting_down)
        );
        out
    }
}

fn escape(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exposition_lists_counters_and_per_server_gauges() {
        let metrics = Metrics::default();
        bump(&metrics.logins);
        add(&metrics.packets_to_client, 41);
        bump(&metrics.packets_to_client);
        let per_server = vec![("lobby".to_owned(), 3), ("we\"ird".to_owned(), 0)];
        let text = metrics.render(&Gauges {
            online: 3,
            per_server: &per_server,
            shutting_down: false,
        });
        assert!(text.contains("rspigot_proxy_logins_total 1\n"));
        assert!(text.contains("rspigot_proxy_packets_to_client_total 42\n"));
        assert!(text.contains("rspigot_proxy_players 3\n"));
        assert!(text.contains("rspigot_proxy_server_players{server=\"lobby\"} 3\n"));
        assert!(text.contains("rspigot_proxy_server_players{server=\"we\\\"ird\"} 0\n"));
        assert!(text.contains("# TYPE rspigot_proxy_switches_total counter\n"));
        assert!(text.contains("rspigot_proxy_shutting_down 0\n"));
    }
}
