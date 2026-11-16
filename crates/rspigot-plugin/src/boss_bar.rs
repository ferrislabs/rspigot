use rspigot_plugin_protocol::v1::boss_bar_order::Order;
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};

use crate::Error;
use crate::link::Link;
use crate::player::Player;
use crate::wire::parse_uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarColor {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

impl BarColor {
    pub const ALL: [Self; 7] = [
        Self::Pink,
        Self::Blue,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Purple,
        Self::White,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Pink => "pink",
            Self::Blue => "blue",
            Self::Red => "red",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Purple => "purple",
            Self::White => "white",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarStyle {
    Solid,
    Segmented6,
    Segmented10,
    Segmented12,
    Segmented20,
}

impl BarStyle {
    pub const ALL: [Self; 5] = [
        Self::Solid,
        Self::Segmented6,
        Self::Segmented10,
        Self::Segmented12,
        Self::Segmented20,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Solid => "progress",
            Self::Segmented6 => "notched_6",
            Self::Segmented10 => "notched_10",
            Self::Segmented12 => "notched_12",
            Self::Segmented20 => "notched_20",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarFlag {
    DarkenSky,
    PlayBossMusic,
    CreateFog,
}

impl BarFlag {
    pub const ALL: [Self; 3] = [Self::DarkenSky, Self::PlayBossMusic, Self::CreateFog];

    pub const fn name(self) -> &'static str {
        match self {
            Self::DarkenSky => "darken_screen",
            Self::PlayBossMusic => "play_boss_music",
            Self::CreateFog => "create_world_fog",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|flag| flag.name() == name)
    }
}

#[derive(Debug, Clone)]
pub struct BossBars {
    link: Link,
}

impl BossBars {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn create(
        &self,
        title: impl Into<String>,
        color: BarColor,
        style: BarStyle,
        flags: &[BarFlag],
    ) -> Result<BossBar, Error> {
        self.build(None, title.into(), color, style, flags).await
    }

    pub async fn create_keyed(
        &self,
        key: impl Into<String>,
        title: impl Into<String>,
        color: BarColor,
        style: BarStyle,
        flags: &[BarFlag],
    ) -> Result<BossBar, Error> {
        self.build(Some(key.into()), title.into(), color, style, flags)
            .await
    }

    pub async fn all(&self) -> Result<Vec<BossBar>, Error> {
        let list = self
            .link
            .ask(
                question::Question::BossBars(v1::Empty {}),
                "the boss bars",
                |body| match body {
                    answer::Body::BossBars(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        Ok(list
            .bars
            .into_iter()
            .map(|state| BossBar::new(state.bar, state.key, self.link.clone()))
            .collect())
    }

    pub async fn get(&self, key: &str) -> Result<Option<BossBar>, Error> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .find(|bar| bar.key() == Some(key)))
    }

    async fn build(
        &self,
        key: Option<String>,
        title: String,
        color: BarColor,
        style: BarStyle,
        flags: &[BarFlag],
    ) -> Result<BossBar, Error> {
        let bar = self
            .link
            .ask(
                question::Question::CreateBossBar(v1::BossBarCreate {
                    title,
                    color: color.name().to_owned(),
                    style: style.name().to_owned(),
                    flags: flags.iter().map(|flag| flag.name().to_owned()).collect(),
                    key: key.clone(),
                }),
                "a new boss bar",
                |body| match body {
                    answer::Body::BossBarCreated(bar) => Some(bar),
                    _ => None,
                },
            )
            .await?;
        Ok(BossBar::new(bar, key, self.link.clone()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossBarState {
    pub title: String,
    pub color: BarColor,
    pub style: BarStyle,
    pub progress: f64,
    pub flags: Vec<BarFlag>,
    pub visible: bool,
    pub players: Vec<uuid::Uuid>,
}

impl BossBarState {
    fn from_wire(state: v1::BossBarState) -> Result<Self, Error> {
        let color = BarColor::from_name(&state.color)
            .ok_or_else(|| Error::Malformed(format!("the boss bar color {}", state.color)))?;
        let style = BarStyle::from_name(&state.style)
            .ok_or_else(|| Error::Malformed(format!("the boss bar style {}", state.style)))?;
        let mut flags = Vec::new();
        for name in &state.flags {
            flags.push(
                BarFlag::from_name(name)
                    .ok_or_else(|| Error::Malformed(format!("the boss bar flag {name}")))?,
            );
        }
        let mut players = Vec::new();
        for raw in &state.players {
            players.push(parse_uuid(raw, "a boss bar viewer")?);
        }
        Ok(Self {
            title: state.title,
            color,
            style,
            progress: state.progress,
            flags,
            visible: state.visible,
            players,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BossBar {
    bar: u64,
    key: Option<String>,
    link: Link,
}

impl PartialEq for BossBar {
    fn eq(&self, other: &Self) -> bool {
        self.bar == other.bar
    }
}

impl BossBar {
    pub(crate) fn new(bar: u64, key: Option<String>, link: Link) -> Self {
        Self { bar, key, link }
    }

    pub const fn id(&self) -> u64 {
        self.bar
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub async fn state(&self) -> Result<BossBarState, Error> {
        let state = self
            .link
            .ask(
                question::Question::BossBar(self.bar),
                "a boss bar",
                |body| match body {
                    answer::Body::BossBar(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        BossBarState::from_wire(state)
    }

    pub fn set_title(&self, title: impl Into<String>) {
        self.order(Order::SetTitle(title.into()));
    }

    pub fn set_color(&self, color: BarColor) {
        self.order(Order::SetColor(color.name().to_owned()));
    }

    pub fn set_style(&self, style: BarStyle) {
        self.order(Order::SetStyle(style.name().to_owned()));
    }

    pub fn set_progress(&self, progress: f64) {
        self.order(Order::SetProgress(progress));
    }

    pub fn add_flag(&self, flag: BarFlag) {
        self.set_flag(flag, true);
    }

    pub fn remove_flag(&self, flag: BarFlag) {
        self.set_flag(flag, false);
    }

    pub fn set_flag(&self, flag: BarFlag, enabled: bool) {
        self.order(Order::SetFlag(v1::BossFlagOrder {
            flag: flag.name().to_owned(),
            enabled,
        }));
    }

    pub fn add_player(&self, player: &Player) {
        self.order(Order::AddPlayer(player.uuid.to_string()));
    }

    pub fn remove_player(&self, player: &Player) {
        self.order(Order::RemovePlayer(player.uuid.to_string()));
    }

    pub fn remove_all_players(&self) {
        self.order(Order::RemoveAllPlayers(v1::Empty {}));
    }

    pub fn set_visible(&self, visible: bool) {
        self.order(Order::SetVisible(visible));
    }

    pub fn remove(&self) {
        self.order(Order::Remove(v1::Empty {}));
    }

    fn order(&self, order: Order) {
        self.link
            .order(from_plugin::Body::BossBar(v1::BossBarOrder {
                bar: self.bar,
                order: Some(order),
            }));
    }
}
