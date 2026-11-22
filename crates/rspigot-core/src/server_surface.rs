use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rspigot_domain::Identifier;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};
use rustc_hash::FxHashMap;

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::waypoints::Waypoints;

pub const PERF_RUN_TICKS: u64 = 200;
const CHAT_COST: u32 = 20;
const REPORT_DIR: &str = "debug/profiling";

#[derive(Debug)]
pub struct PerfRun {
    pub started: Instant,
    pub tick: u64,
    pub requester: Option<PlayerId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerfReport {
    pub seconds: f64,
    pub ticks: i64,
    pub saved: Result<String, String>,
}

#[derive(Debug, Default)]
pub struct SurfaceState {
    pub waypoints: Waypoints,
    pub spam: FxHashMap<PlayerId, u32>,
    pub command_spam: FxHashMap<PlayerId, u32>,
    pub post_effects: FxHashMap<PlayerId, Vec<Identifier>>,
    pub perf: Option<PerfRun>,
}

fn write_report(seconds: f64, ticks: i64, tps: [f64; 3]) -> Result<String, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let path = format!("{REPORT_DIR}/perf-{stamp}.txt");
    let body = format!(
        "duration_seconds: {seconds:.3}\nticks: {ticks}\ntps_1m: {:.2}\ntps_5m: {:.2}\ntps_15m: {:.2}\n",
        tps[0], tps[1], tps[2]
    );
    std::fs::create_dir_all(REPORT_DIR)
        .and_then(|()| std::fs::write(&path, body))
        .map(|()| path)
        .map_err(|error| error.to_string())
}

pub fn stopped_message(report: &PerfReport) -> Component {
    let rate = if report.seconds > 0.0 {
        report.ticks as f64 / report.seconds
    } else {
        0.0
    };
    Component::translatable(
        "commands.perf.stopped",
        vec![
            Component::text(format!("{:.2}", report.seconds)),
            Component::text(report.ticks.to_string()),
            Component::text(format!("{rate:.2}")),
        ],
    )
}

pub fn saved_message(report: &PerfReport) -> Component {
    match &report.saved {
        Ok(path) => Component::translatable(
            "commands.perf.reportSaved",
            vec![Component::text(path.clone())],
        ),
        Err(_) => {
            Component::translatable("commands.perf.reportFailed", Vec::new()).color(NamedColor::Red)
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn begin_perf_run(&mut self, requester: Option<PlayerId>) -> bool {
        if self.surface.perf.is_some() {
            return false;
        }
        self.surface.perf = Some(PerfRun {
            started: Instant::now(),
            tick: self.tick,
            requester,
        });
        true
    }

    pub(crate) fn end_perf_run(&mut self) -> Option<(PerfReport, Option<PlayerId>)> {
        let run = self.surface.perf.take()?;
        let seconds = run.started.elapsed().as_secs_f64();
        let ticks = i64::try_from(self.tick.saturating_sub(run.tick)).unwrap_or(i64::MAX);
        let saved = write_report(seconds, ticks, self.tps());
        Some((
            PerfReport {
                seconds,
                ticks,
                saved,
            },
            run.requester,
        ))
    }

    pub(crate) fn finish_perf_run(&mut self) {
        let due = self
            .surface
            .perf
            .as_ref()
            .is_some_and(|run| self.tick.saturating_sub(run.tick) >= PERF_RUN_TICKS);
        if !due {
            return;
        }
        let Some((report, requester)) = self.end_perf_run() else {
            return;
        };
        let lines = [stopped_message(&report), saved_message(&report)];
        for line in lines {
            match requester {
                Some(id) if self.players.contains_key(&id) => {
                    self.push(Audience::One(id), ClientEvent::system(line));
                }
                _ => tracing::info!("{}", crate::console::console_text(&line)),
            }
        }
    }

    pub(crate) fn tick_chat_guard(&mut self) {
        let players = &self.players;
        let surface = &mut self.surface;
        surface
            .waypoints
            .retain_players(|id| players.contains_key(id));
        surface.spam.retain(|id, _| players.contains_key(id));
        surface
            .command_spam
            .retain(|id, _| players.contains_key(id));
        surface
            .post_effects
            .retain(|id, _| players.contains_key(id));
        for count in surface
            .spam
            .values_mut()
            .chain(surface.command_spam.values_mut())
        {
            *count = count.saturating_sub(1);
        }
    }

    pub(crate) fn spam_check(&mut self, id: PlayerId, command: bool) -> bool {
        let limit = if command {
            self.config.chat.command_spam_threshold_ticks
        } else {
            self.config.chat.spam_threshold_ticks
        };
        let exempt = self.players.get(&id).is_none_or(|p| p.operator);
        if limit == 0 || exempt {
            return true;
        }
        let map = if command {
            &mut self.surface.command_spam
        } else {
            &mut self.surface.spam
        };
        let count = map.entry(id).or_insert(0);
        *count += CHAT_COST;
        if *count > limit {
            self.disconnect_player(id, Component::translatable("disconnect.spam", Vec::new()));
            return false;
        }
        true
    }
}
