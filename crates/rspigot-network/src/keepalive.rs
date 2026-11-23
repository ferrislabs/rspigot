use std::time::{Duration, Instant};

use rspigot_domain::text::Component;
use rspigot_protocol::common::KeepAlive;
use rspigot_protocol::ids;

use crate::NetworkConfig;
use crate::connection::{ConfigWire, ConnectionError, send_configuration_disconnect};

pub(crate) struct ConfigAlive {
    interval: Duration,
    timeout: Duration,
    sent_at: Instant,
    pending: Option<i64>,
}

impl ConfigAlive {
    pub(crate) fn new(config: &NetworkConfig) -> Self {
        Self {
            interval: config.keep_alive_interval,
            timeout: config.keep_alive_timeout,
            sent_at: Instant::now(),
            pending: None,
        }
    }

    pub(crate) fn wait(&self) -> Duration {
        let limit = if self.pending.is_some() {
            self.timeout
        } else {
            self.interval
        };
        limit.saturating_sub(self.sent_at.elapsed())
    }

    pub(crate) async fn pulse<W: ConfigWire>(
        &mut self,
        wire: &mut W,
    ) -> Result<(), ConnectionError> {
        if self.wait() > Duration::ZERO {
            return Ok(());
        }
        if self.pending.is_some() {
            return timed_out(wire).await;
        }
        let id = millis_since_epoch().max(1);
        wire.queue(
            ids::configuration::clientbound::KEEP_ALIVE,
            &KeepAlive { id },
        )?;
        wire.flush().await?;
        self.pending = Some(id);
        self.sent_at = Instant::now();
        Ok(())
    }

    pub(crate) async fn answer<W: ConfigWire>(
        &mut self,
        wire: &mut W,
        id: i64,
    ) -> Result<(), ConnectionError> {
        match self.pending {
            Some(expected) if expected == id => {
                self.pending = None;
                Ok(())
            }
            Some(_) => timed_out(wire).await,
            None => Ok(()),
        }
    }
}

async fn timed_out<W: ConfigWire>(wire: &mut W) -> Result<(), ConnectionError> {
    send_configuration_disconnect(
        wire,
        &Component::translatable("disconnect.timeout", Vec::new()),
    )
    .await?;
    Err(ConnectionError::TimedOut)
}

fn millis_since_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as i64)
}
