use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::boss_event::{BossColor, BossMessages, BossOverlay, ServerBossEvent};
use crate::entity::save::{read_uuid, uuid_tag};
use crate::ident::Identifier;
use crate::player::PlayerId;
use crate::scoreboard::persist::{component_from_tag, component_to_tag};
use crate::text::{Component, NamedColor};

pub const DEFAULT_MAX: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CustomBossError {
    #[error("a boss bar with this id already exists")]
    KeyTaken,
}

impl BossColor {
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

    pub const fn formatting(self) -> NamedColor {
        match self {
            Self::Pink => NamedColor::LightPurple,
            Self::Blue => NamedColor::Blue,
            Self::Red => NamedColor::Red,
            Self::Green => NamedColor::Green,
            Self::Yellow => NamedColor::Yellow,
            Self::Purple => NamedColor::DarkPurple,
            Self::White => NamedColor::White,
        }
    }
}

impl BossOverlay {
    pub const ALL: [Self; 5] = [
        Self::Progress,
        Self::Notched6,
        Self::Notched10,
        Self::Notched12,
        Self::Notched20,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Notched6 => "notched_6",
            Self::Notched10 => "notched_10",
            Self::Notched12 => "notched_12",
            Self::Notched20 => "notched_20",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|overlay| overlay.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomBossBar {
    key: Option<Identifier>,
    event: ServerBossEvent,
    value: i32,
    max: i32,
}

impl CustomBossBar {
    pub fn new(key: Option<Identifier>, uuid: Uuid, name: Component) -> Self {
        let mut event = ServerBossEvent::new(uuid, name, BossColor::White, BossOverlay::Progress);
        event.set_progress(0.0);
        Self {
            key,
            event,
            value: 0,
            max: DEFAULT_MAX,
        }
    }

    pub fn key(&self) -> Option<&Identifier> {
        self.key.as_ref()
    }

    pub fn event(&self) -> &ServerBossEvent {
        &self.event
    }

    pub fn event_mut(&mut self) -> &mut ServerBossEvent {
        &mut self.event
    }

    pub const fn value(&self) -> i32 {
        self.value
    }

    pub const fn max(&self) -> i32 {
        self.max
    }

    fn ratio(&self) -> f32 {
        (self.value as f32 / self.max as f32).clamp(0.0, 1.0)
    }

    pub fn set_value(&mut self, value: i32) -> BossMessages {
        self.value = value;
        let progress = self.ratio();
        self.event.set_progress(progress)
    }

    pub fn set_max(&mut self, max: i32) -> BossMessages {
        self.max = max;
        let progress = self.ratio();
        self.event.set_progress(progress)
    }

    pub fn set_progress(&mut self, progress: f32) -> BossMessages {
        let progress = progress.clamp(0.0, 1.0);
        self.value = (progress * self.max as f32).round() as i32;
        self.event.set_progress(progress)
    }

    pub fn display_name(&self) -> Component {
        Component::text("[")
            .append(self.event.name().clone())
            .append(Component::text("]"))
            .color(self.event.color().formatting())
    }

    pub fn pack(&self) -> Compound {
        let flags = self.event.flags();
        let players = self
            .event
            .players()
            .map(|player| uuid_tag(player.0))
            .collect();
        Compound::new()
            .with("Name", component_to_tag(self.event.name()))
            .with("Visible", self.event.is_visible())
            .with("Value", self.value)
            .with("Max", self.max)
            .with("Color", self.event.color().name())
            .with("Overlay", self.event.overlay().name())
            .with("DarkenScreen", flags.darken_screen)
            .with("PlayBossMusic", flags.play_music)
            .with("CreateWorldFog", flags.create_world_fog)
            .with("Players", Tag::List(players))
    }

    pub fn unpack(key: Identifier, uuid: Uuid, data: &Compound) -> Option<Self> {
        let name = component_from_tag(data.get("Name")?)?;
        let mut bar = Self::new(Some(key), uuid, name);
        bar.event
            .set_visible(data.get_bool("Visible").unwrap_or(false));
        bar.set_value(data.get_i32("Value").unwrap_or(0));
        bar.set_max(data.get_i32("Max").unwrap_or(DEFAULT_MAX));
        bar.event.set_color(
            data.get_str("Color")
                .and_then(BossColor::from_name)
                .unwrap_or(BossColor::White),
        );
        bar.event.set_overlay(
            data.get_str("Overlay")
                .and_then(BossOverlay::from_name)
                .unwrap_or(BossOverlay::Progress),
        );
        bar.event
            .set_darken_screen(data.get_bool("DarkenScreen").unwrap_or(false));
        bar.event
            .set_play_music(data.get_bool("PlayBossMusic").unwrap_or(false));
        bar.event
            .set_create_world_fog(data.get_bool("CreateWorldFog").unwrap_or(false));
        for tag in data.get_list("Players").unwrap_or_default() {
            if let Some(player) = read_uuid(tag) {
                bar.event.add_player(PlayerId(player));
            }
        }
        Some(bar)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CustomBossBars {
    bars: BTreeMap<u64, CustomBossBar>,
    next: u64,
}

impl CustomBossBars {
    pub fn create(
        &mut self,
        key: Option<Identifier>,
        uuid: Uuid,
        name: Component,
    ) -> Result<u64, CustomBossError> {
        if key
            .as_ref()
            .is_some_and(|key| self.handle_of(key).is_some())
        {
            return Err(CustomBossError::KeyTaken);
        }
        self.next += 1;
        self.bars
            .insert(self.next, CustomBossBar::new(key, uuid, name));
        Ok(self.next)
    }

    pub fn get(&self, handle: u64) -> Option<&CustomBossBar> {
        self.bars.get(&handle)
    }

    pub fn get_mut(&mut self, handle: u64) -> Option<&mut CustomBossBar> {
        self.bars.get_mut(&handle)
    }

    pub fn handle_of(&self, key: &Identifier) -> Option<u64> {
        self.bars
            .iter()
            .find(|(_, bar)| bar.key() == Some(key))
            .map(|(handle, _)| *handle)
    }

    pub fn remove(&mut self, handle: u64) -> Option<CustomBossBar> {
        self.bars.remove(&handle)
    }

    pub fn handles(&self) -> Vec<u64> {
        self.bars.keys().copied().collect()
    }

    pub fn keyed(&self) -> Vec<(Identifier, u64)> {
        let mut keyed: Vec<(Identifier, u64)> = self
            .bars
            .iter()
            .filter_map(|(handle, bar)| bar.key().map(|key| (key.clone(), *handle)))
            .collect();
        keyed.sort();
        keyed
    }

    pub fn keyed_count(&self) -> usize {
        self.bars.values().filter(|bar| bar.key().is_some()).count()
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        for (key, handle) in self.keyed() {
            if let Some(bar) = self.bars.get(&handle) {
                out.insert(key.as_str(), bar.pack());
            }
        }
        out
    }

    pub fn load_nbt(&mut self, data: &Compound, mut uuid: impl FnMut() -> Uuid) {
        for (name, tag) in data.iter() {
            let (Ok(key), Some(packed)) = (Identifier::parse(name), tag.as_compound()) else {
                continue;
            };
            if self.handle_of(&key).is_some() {
                continue;
            }
            if let Some(bar) = CustomBossBar::unpack(key, uuid(), packed) {
                self.next += 1;
                self.bars.insert(self.next, bar);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boss_event::BossUpdate;

    fn key(path: &str) -> Identifier {
        Identifier::minecraft(path)
    }

    fn player(n: u128) -> PlayerId {
        PlayerId(Uuid::from_u128(n))
    }

    fn bars() -> (CustomBossBars, u64) {
        let mut bars = CustomBossBars::default();
        let handle = bars
            .create(
                Some(key("raid")),
                Uuid::from_u128(9),
                Component::text("Raid"),
            )
            .unwrap();
        (bars, handle)
    }

    #[test]
    fn a_new_bar_is_empty_white_and_hundred_wide() {
        let (bars, handle) = bars();
        let bar = bars.get(handle).unwrap();
        assert_eq!(bar.value(), 0);
        assert_eq!(bar.max(), 100);
        assert_eq!(bar.event().progress(), 0.0);
        assert_eq!(bar.event().color(), BossColor::White);
        assert!(bar.event().is_visible());
    }

    #[test]
    fn a_key_can_only_be_used_once() {
        let (mut bars, _) = bars();
        assert_eq!(
            bars.create(Some(key("raid")), Uuid::from_u128(1), Component::text("x")),
            Err(CustomBossError::KeyTaken)
        );
        assert!(
            bars.create(None, Uuid::from_u128(2), Component::text("x"))
                .is_ok()
        );
    }

    #[test]
    fn value_and_max_drive_the_progress_and_clamp_it() {
        let (mut bars, handle) = bars();
        let bar = bars.get_mut(handle).unwrap();
        bar.event_mut().add_player(player(1));
        let sent = bar.set_value(25);
        assert_eq!(sent, vec![(player(1), BossUpdate::Progress(0.25))]);
        bar.set_max(50);
        assert_eq!(bar.event().progress(), 0.5);
        bar.set_value(500);
        assert_eq!(bar.event().progress(), 1.0);
    }

    #[test]
    fn setting_the_progress_keeps_it_exact_and_derives_the_value() {
        let (mut bars, handle) = bars();
        let bar = bars.get_mut(handle).unwrap();
        bar.set_progress(0.37);
        assert_eq!(bar.event().progress(), 0.37);
        assert_eq!(bar.value(), 37);
        bar.set_progress(2.0);
        assert_eq!(bar.event().progress(), 1.0);
    }

    #[test]
    fn only_keyed_bars_are_persisted_and_they_round_trip() {
        let (mut bars, handle) = bars();
        bars.create(None, Uuid::from_u128(2), Component::text("loose"))
            .unwrap();
        {
            let bar = bars.get_mut(handle).unwrap();
            bar.set_max(10);
            bar.set_value(5);
            bar.event_mut().set_color(BossColor::Red);
            bar.event_mut().set_overlay(BossOverlay::Notched12);
            bar.event_mut().set_play_music(true);
            bar.event_mut().add_player(player(7));
        }
        let nbt = bars.to_nbt();
        assert_eq!(nbt.len(), 1);
        let packed = nbt.get_compound("minecraft:raid").unwrap();
        assert_eq!(packed.get_str("Color"), Some("red"));
        assert_eq!(packed.get_str("Overlay"), Some("notched_12"));
        assert_eq!(packed.get_i32("Max"), Some(10));
        let mut restored = CustomBossBars::default();
        restored.load_nbt(&nbt, || Uuid::from_u128(3));
        let handle = restored.handle_of(&key("raid")).unwrap();
        let bar = restored.get(handle).unwrap();
        assert_eq!(bar.value(), 5);
        assert_eq!(bar.event().progress(), 0.5);
        assert_eq!(bar.event().color(), BossColor::Red);
        assert!(bar.event().flags().play_music);
        assert!(bar.event().has_player(player(7)));
        assert_eq!(restored.to_nbt(), nbt);
    }

    #[test]
    fn missing_fields_fall_back_to_the_vanilla_defaults() {
        let nbt = Compound::new().with("minecraft:x", Compound::new().with("Name", "X"));
        let mut bars = CustomBossBars::default();
        bars.load_nbt(&nbt, || Uuid::from_u128(1));
        let bar = bars.get(bars.handle_of(&key("x")).unwrap()).unwrap();
        assert_eq!(bar.max(), 100);
        assert!(!bar.event().is_visible());
    }

    #[test]
    fn the_display_name_is_bracketed_and_tinted_by_the_color() {
        let (bars, handle) = bars();
        let shown = bars.get(handle).unwrap().display_name();
        assert_eq!(shown.plain_text(), "[Raid]");
    }
}
