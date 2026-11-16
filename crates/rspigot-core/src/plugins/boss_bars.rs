use rspigot_domain::Identifier;
use rspigot_domain::boss_event::{BossColor, BossFlags, BossOverlay};
use rspigot_domain::custom_boss::CustomBossBar;
use rspigot_domain::player::PlayerId;

use crate::ports::ClientLink;
use crate::server::Server;

use super::resolve;

pub const FLAG_DARKEN_SCREEN: &str = "darken_screen";
pub const FLAG_PLAY_BOSS_MUSIC: &str = "play_boss_music";
pub const FLAG_CREATE_WORLD_FOG: &str = "create_world_fog";

#[derive(Debug, Clone, PartialEq)]
pub enum BossBarOrder {
    SetTitle(String),
    SetColor(String),
    SetStyle(String),
    SetProgress(f64),
    SetFlag { flag: String, enabled: bool },
    AddPlayer(String),
    RemovePlayer(String),
    RemoveAllPlayers,
    SetVisible(bool),
    Remove,
}

impl BossBarOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetTitle(_) => "boss_bar.set_title",
            Self::SetColor(_) => "boss_bar.set_color",
            Self::SetStyle(_) => "boss_bar.set_style",
            Self::SetProgress(_) => "boss_bar.set_progress",
            Self::SetFlag { .. } => "boss_bar.set_flag",
            Self::AddPlayer(_) => "boss_bar.add_player",
            Self::RemovePlayer(_) => "boss_bar.remove_player",
            Self::RemoveAllPlayers => "boss_bar.remove_all_players",
            Self::SetVisible(_) => "boss_bar.set_visible",
            Self::Remove => "boss_bar.remove",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossBarSpec {
    pub title: String,
    pub color: String,
    pub style: String,
    pub flags: Vec<String>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossBarView {
    pub bar: u64,
    pub key: Option<String>,
    pub title: String,
    pub color: String,
    pub style: String,
    pub progress: f64,
    pub flags: Vec<String>,
    pub visible: bool,
    pub players: Vec<String>,
}

fn color_of(name: &str) -> Result<BossColor, String> {
    BossColor::from_name(name).ok_or_else(|| format!("{name} is not a boss bar color"))
}

fn overlay_of(name: &str) -> Result<BossOverlay, String> {
    BossOverlay::from_name(name).ok_or_else(|| format!("{name} is not a boss bar style"))
}

fn flag_in(flags: &mut BossFlags, name: &str, enabled: bool) -> Result<(), String> {
    match name {
        FLAG_DARKEN_SCREEN => flags.darken_screen = enabled,
        FLAG_PLAY_BOSS_MUSIC => flags.play_music = enabled,
        FLAG_CREATE_WORLD_FOG => flags.create_world_fog = enabled,
        _ => return Err(format!("{name} is not a boss bar flag")),
    }
    Ok(())
}

fn flag_names(flags: BossFlags) -> Vec<String> {
    [
        (flags.darken_screen, FLAG_DARKEN_SCREEN),
        (flags.play_music, FLAG_PLAY_BOSS_MUSIC),
        (flags.create_world_fog, FLAG_CREATE_WORLD_FOG),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_owned())
    .collect()
}

fn view_of(handle: u64, bar: &CustomBossBar) -> BossBarView {
    let event = bar.event();
    BossBarView {
        bar: handle,
        key: bar.key().map(ToString::to_string),
        title: event.name().to_legacy(),
        color: event.color().name().to_owned(),
        style: event.overlay().name().to_owned(),
        progress: f64::from(event.progress()),
        flags: flag_names(event.flags()),
        visible: event.is_visible(),
        players: event.players().map(|player| player.0.to_string()).collect(),
    }
}

fn parse_player(raw: &str) -> Result<PlayerId, String> {
    uuid::Uuid::parse_str(raw)
        .map(PlayerId)
        .map_err(|_| format!("{raw} is not a player id"))
}

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_boss_bar_order(&mut self, connection: u64, bar: u64, order: BossBarOrder) {
        let name = order.name();
        if let Err(reason) = self.boss_bar_order(bar, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn boss_bar_order(&mut self, handle: u64, order: BossBarOrder) -> Result<(), String> {
        if self.custom_bars.get(handle).is_none() {
            return Err(format!("there is no boss bar {handle}"));
        }
        let missing = || format!("there is no boss bar {handle}");
        match order {
            BossBarOrder::Remove => {
                self.remove_custom_boss_bar(handle);
                Ok(())
            }
            BossBarOrder::SetTitle(title) => {
                let title = resolve::text(&title);
                self.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().set_name(title)))
                    .ok_or_else(missing)
            }
            BossBarOrder::SetColor(name) => {
                let color = color_of(&name)?;
                self.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().set_color(color)))
                    .ok_or_else(missing)
            }
            BossBarOrder::SetStyle(name) => {
                let overlay = overlay_of(&name)?;
                self.change_custom_boss_bar(handle, |bar| {
                    ((), bar.event_mut().set_overlay(overlay))
                })
                .ok_or_else(missing)
            }
            BossBarOrder::SetProgress(progress) => {
                if !(0.0..=1.0).contains(&progress) {
                    return Err(format!("progress {progress} is outside 0..=1"));
                }
                self.change_custom_boss_bar(handle, |bar| ((), bar.set_progress(progress as f32)))
                    .ok_or_else(missing)
            }
            BossBarOrder::SetFlag { flag, enabled } => {
                let mut probe = BossFlags::default();
                flag_in(&mut probe, &flag, enabled)?;
                self.change_custom_boss_bar(handle, |bar| {
                    let event = bar.event_mut();
                    let messages = match flag.as_str() {
                        FLAG_DARKEN_SCREEN => event.set_darken_screen(enabled),
                        FLAG_PLAY_BOSS_MUSIC => event.set_play_music(enabled),
                        _ => event.set_create_world_fog(enabled),
                    };
                    ((), messages)
                })
                .ok_or_else(missing)
            }
            BossBarOrder::AddPlayer(raw) => {
                let player = parse_player(&raw)?;
                if !self.players.contains_key(&player) {
                    return Err(format!("{raw} is not online"));
                }
                self.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(player)))
                    .ok_or_else(missing)
            }
            BossBarOrder::RemovePlayer(raw) => {
                let player = parse_player(&raw)?;
                self.change_custom_boss_bar(handle, |bar| {
                    ((), bar.event_mut().remove_player(player))
                })
                .ok_or_else(missing)
            }
            BossBarOrder::RemoveAllPlayers => self
                .change_custom_boss_bar(handle, |bar| ((), bar.event_mut().remove_all_players()))
                .ok_or_else(missing),
            BossBarOrder::SetVisible(visible) => self
                .change_custom_boss_bar(handle, |bar| ((), bar.event_mut().set_visible(visible)))
                .ok_or_else(missing),
        }
    }

    pub(super) fn create_boss_bar_for_plugin(&mut self, spec: BossBarSpec) -> Result<u64, String> {
        let color = color_of(&spec.color)?;
        let overlay = overlay_of(&spec.style)?;
        let mut flags = BossFlags::default();
        for flag in &spec.flags {
            flag_in(&mut flags, flag, true)?;
        }
        let key = match &spec.key {
            Some(raw) => {
                Some(Identifier::parse(raw).map_err(|_| format!("{raw} is not a valid key"))?)
            }
            None => None,
        };
        let handle = self
            .create_custom_boss_bar(key, resolve::text(&spec.title))
            .map_err(|error| error.to_string())?;
        self.change_custom_boss_bar(handle, |bar| {
            let event = bar.event_mut();
            event.set_color(color);
            event.set_overlay(overlay);
            event.set_darken_screen(flags.darken_screen);
            event.set_play_music(flags.play_music);
            event.set_create_world_fog(flags.create_world_fog);
            bar.set_progress(1.0);
            ((), Vec::new())
        });
        Ok(handle)
    }

    pub(super) fn boss_bar_state(&self, handle: u64) -> Option<BossBarView> {
        self.custom_bars.get(handle).map(|bar| view_of(handle, bar))
    }

    pub(super) fn boss_bar_states(&self) -> Vec<BossBarView> {
        self.custom_bars
            .keyed()
            .into_iter()
            .filter_map(|(_, handle)| self.boss_bar_state(handle))
            .collect()
    }
}
