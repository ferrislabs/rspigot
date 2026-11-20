use std::sync::atomic::Ordering;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossbeam_channel::Sender;

use crate::ports::ClientLink;
use crate::server::Server;

pub struct TimingsReport {
    pub file: String,
    pub text: String,
}

pub(crate) struct Timings {
    pub(crate) enabled: bool,
    pub(crate) verbose: bool,
    since: Instant,
    ticks: u64,
    total: Duration,
    worst: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            enabled: false,
            verbose: false,
            since: Instant::now(),
            ticks: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
        }
    }
}

impl Timings {
    pub(crate) fn reset(&mut self) {
        self.since = Instant::now();
        self.ticks = 0;
        self.total = Duration::ZERO;
        self.worst = Duration::ZERO;
    }

    pub(crate) fn record(&mut self, took: Duration) {
        if !self.enabled {
            return;
        }
        self.ticks += 1;
        self.total += took;
        self.worst = self.worst.max(took);
    }

    pub(crate) fn average_millis(&self) -> f64 {
        if self.ticks == 0 {
            return 0.0;
        }
        self.total.as_secs_f64() * 1000.0 / self.ticks as f64
    }

    pub(crate) fn render(&self, tps: [f64; 3], verbose_note: bool) -> String {
        let mut text = String::from("rspigot timings report\n");
        text.push_str(&format!(
            "sampled: {:.1} seconds, {} ticks\n",
            self.since.elapsed().as_secs_f64(),
            self.ticks
        ));
        text.push_str(&format!(
            "average tick: {:.3} ms, worst tick: {:.3} ms\n",
            self.average_millis(),
            self.worst.as_secs_f64() * 1000.0
        ));
        text.push_str(&format!(
            "tps (1m, 5m, 15m): {:.2}, {:.2}, {:.2}\n",
            tps[0], tps[1], tps[2]
        ));
        if verbose_note {
            text.push_str("verbose timings: no per-handler data is collected\n");
        }
        text.push_str("this report is local, nothing is uploaded\n");
        text
    }
}

impl<L: ClientLink> Server<L> {
    pub fn with_timings_sink(mut self, sink: Sender<TimingsReport>) -> Self {
        self.timings_sink = Some(sink);
        self
    }

    pub fn with_restart_signal(mut self, signal: Arc<AtomicBool>) -> Self {
        self.restart_signal = Some(signal);
        self
    }

    pub(crate) fn timings_report(&mut self) -> Option<String> {
        let sink = self.timings_sink.as_ref()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let file = format!("timings-{stamp}.txt");
        let text = self.timings.render(self.tps(), self.timings.verbose);
        sink.send(TimingsReport {
            file: file.clone(),
            text,
        })
        .ok()?;
        Some(file)
    }

    pub(crate) fn request_restart(&mut self) {
        self.restarting = true;
        if let Some(signal) = &self.restart_signal {
            signal.store(true, Ordering::SeqCst);
        }
        self.running = false;
    }

    pub(crate) fn shutdown_reason(&self) -> rspigot_domain::text::Component {
        use rspigot_domain::text::Component;
        let messages = &self.config.messages;
        if self.restarting {
            return Component::text(messages.restart.clone());
        }
        if messages.shutdown == crate::config::ServerMessages::default().shutdown {
            return Component::translatable("multiplayer.disconnect.server_shutdown", Vec::new());
        }
        Component::text(messages.shutdown.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::end_tests::arrive;

    #[test]
    fn timings_only_count_ticks_while_enabled_and_average_them() {
        let mut timings = Timings::default();
        timings.record(Duration::from_millis(40));
        assert_eq!(timings.average_millis(), 0.0);
        timings.enabled = true;
        timings.record(Duration::from_millis(10));
        timings.record(Duration::from_millis(30));
        assert!((timings.average_millis() - 20.0).abs() < 1e-9);
        let text = timings.render([20.0, 19.5, 19.0], false);
        assert!(text.contains("average tick: 20.000 ms, worst tick: 30.000 ms"));
        assert!(text.contains("tps (1m, 5m, 15m): 20.00, 19.50, 19.00"));
        assert!(text.contains("nothing is uploaded"));
        timings.reset();
        assert_eq!(timings.average_millis(), 0.0);
    }

    #[test]
    fn a_report_goes_to_the_sink_with_a_timestamped_file_name() {
        let (sink, reports) = crossbeam_channel::unbounded();
        let (server, _, _) = arrive();
        let mut server = server.with_timings_sink(sink);
        let file = server.timings_report().expect("a file name");
        let report = reports.try_recv().expect("a queued report");
        assert_eq!(report.file, file);
        assert!(file.starts_with("timings-") && file.ends_with(".txt"));
        assert!(report.text.starts_with("rspigot timings report"));
    }

    #[test]
    fn a_restart_stops_the_server_raises_the_signal_and_changes_the_kick_message() {
        let signal = Arc::new(AtomicBool::new(false));
        let (server, _, _) = arrive();
        let mut server = server.with_restart_signal(Arc::clone(&signal));
        assert_eq!(
            server.shutdown_reason().plain_text(),
            "multiplayer.disconnect.server_shutdown"
        );
        server.config.messages.shutdown = "Bye".to_owned();
        assert_eq!(server.shutdown_reason().plain_text(), "Bye");
        server.request_restart();
        assert!(!server.running);
        assert!(signal.load(Ordering::SeqCst));
        assert_eq!(
            server.shutdown_reason().plain_text(),
            "Server is restarting"
        );
    }
}
