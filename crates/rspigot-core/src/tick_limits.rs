use std::time::Duration;

use crate::ports::ClientLink;
use crate::server::Server;

const TICKS_PER_SECOND: u64 = 20;
const TICK_BUDGET_SECONDS: f64 = 0.05;

pub(crate) fn overrun_message(took: Duration, limit_ms: i64) -> Option<String> {
    let limit = u128::try_from(limit_ms).ok().filter(|limit| *limit > 0)?;
    (took.as_millis() > limit).then(|| {
        format!(
            "A single server tick took {:.2} seconds (should be max {TICK_BUDGET_SECONDS:.2})",
            took.as_secs_f64()
        )
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn track_emptiness(&mut self) {
        if self.players.is_empty() {
            self.empty_ticks = self.empty_ticks.saturating_add(1);
        } else {
            self.empty_ticks = 0;
        }
    }

    pub(crate) fn paused_while_empty(&self) -> bool {
        let seconds = u64::from(self.config.pause_when_empty_seconds);
        seconds > 0 && self.empty_ticks > seconds * TICKS_PER_SECOND
    }

    pub(crate) fn report_slow_tick(&mut self, took: Duration) {
        self.timings.record(took);
        if let Some(message) = overrun_message(took, self.config.max_tick_time_ms) {
            tracing::error!("{message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn server(pause_seconds: u32) -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            pause_when_empty_seconds: pause_seconds,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        );
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    #[test]
    fn an_empty_server_stops_simulating_after_the_configured_seconds() {
        let mut server = server(1);
        for _ in 0..100 {
            server.tick();
        }
        assert_eq!(server.world.level.game_time, 20);
        assert!(server.paused_while_empty());
    }

    #[test]
    fn without_the_setting_an_empty_server_keeps_simulating() {
        let mut server = server(0);
        for _ in 0..100 {
            server.tick();
        }
        assert_eq!(server.world.level.game_time, 100);
    }

    #[test]
    fn a_tick_over_the_limit_is_reported_and_a_disabled_limit_is_not() {
        let took = Duration::from_secs(61);
        let message = overrun_message(took, 60_000).expect("a report");
        assert_eq!(
            message,
            "A single server tick took 61.00 seconds (should be max 0.05)"
        );
        assert_eq!(overrun_message(took, -1), None);
        assert_eq!(overrun_message(took, 0), None);
        assert_eq!(overrun_message(Duration::from_secs(1), 60_000), None);
    }
}
