use std::collections::BTreeSet;

use uuid::Uuid;

use crate::player::PlayerId;
use crate::text::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossColor {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossOverlay {
    Progress,
    Notched6,
    Notched10,
    Notched12,
    Notched20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BossFlags {
    pub darken_screen: bool,
    pub play_music: bool,
    pub create_world_fog: bool,
}

impl BossFlags {
    pub const DARKEN_SCREEN: u8 = 1;
    pub const PLAY_MUSIC: u8 = 2;
    pub const CREATE_WORLD_FOG: u8 = 4;

    pub const fn bits(self) -> u8 {
        (self.darken_screen as u8 * Self::DARKEN_SCREEN)
            | (self.play_music as u8 * Self::PLAY_MUSIC)
            | (self.create_world_fog as u8 * Self::CREATE_WORLD_FOG)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BossUpdate {
    Add {
        name: Component,
        progress: f32,
        color: BossColor,
        overlay: BossOverlay,
        flags: BossFlags,
    },
    Remove,
    Progress(f32),
    Name(Component),
    Style {
        color: BossColor,
        overlay: BossOverlay,
    },
    Properties(BossFlags),
}

pub type BossMessages = Vec<(PlayerId, BossUpdate)>;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerBossEvent {
    id: Uuid,
    name: Component,
    progress: f32,
    color: BossColor,
    overlay: BossOverlay,
    flags: BossFlags,
    visible: bool,
    players: BTreeSet<PlayerId>,
}

impl ServerBossEvent {
    pub fn new(id: Uuid, name: Component, color: BossColor, overlay: BossOverlay) -> Self {
        Self {
            id,
            name,
            progress: 1.0,
            color,
            overlay,
            flags: BossFlags::default(),
            visible: true,
            players: BTreeSet::new(),
        }
    }

    pub const fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &Component {
        &self.name
    }

    pub const fn progress(&self) -> f32 {
        self.progress
    }

    pub const fn color(&self) -> BossColor {
        self.color
    }

    pub const fn overlay(&self) -> BossOverlay {
        self.overlay
    }

    pub const fn flags(&self) -> BossFlags {
        self.flags
    }

    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.players.iter().copied()
    }

    pub fn has_player(&self, player: PlayerId) -> bool {
        self.players.contains(&player)
    }

    pub fn add_packet(&self) -> BossUpdate {
        BossUpdate::Add {
            name: self.name.clone(),
            progress: self.progress,
            color: self.color,
            overlay: self.overlay,
            flags: self.flags,
        }
    }

    fn broadcast(&self, update: &BossUpdate) -> BossMessages {
        if !self.visible {
            return Vec::new();
        }
        self.players
            .iter()
            .map(|player| (*player, update.clone()))
            .collect()
    }

    pub fn add_player(&mut self, player: PlayerId) -> BossMessages {
        if self.players.insert(player) && self.visible {
            return vec![(player, self.add_packet())];
        }
        Vec::new()
    }

    pub fn remove_player(&mut self, player: PlayerId) -> BossMessages {
        if self.players.remove(&player) && self.visible {
            return vec![(player, BossUpdate::Remove)];
        }
        Vec::new()
    }

    pub fn remove_all_players(&mut self) -> BossMessages {
        let players: Vec<PlayerId> = self.players.iter().copied().collect();
        players
            .into_iter()
            .flat_map(|player| self.remove_player(player))
            .collect()
    }

    pub fn set_progress(&mut self, progress: f32) -> BossMessages {
        if progress == self.progress {
            return Vec::new();
        }
        self.progress = progress;
        self.broadcast(&BossUpdate::Progress(progress))
    }

    pub fn set_color(&mut self, color: BossColor) -> BossMessages {
        if color == self.color {
            return Vec::new();
        }
        self.color = color;
        self.broadcast(&BossUpdate::Style {
            color,
            overlay: self.overlay,
        })
    }

    pub fn set_overlay(&mut self, overlay: BossOverlay) -> BossMessages {
        if overlay == self.overlay {
            return Vec::new();
        }
        self.overlay = overlay;
        self.broadcast(&BossUpdate::Style {
            color: self.color,
            overlay,
        })
    }

    pub fn set_darken_screen(&mut self, darken: bool) -> BossMessages {
        if darken == self.flags.darken_screen {
            return Vec::new();
        }
        self.flags.darken_screen = darken;
        self.broadcast(&BossUpdate::Properties(self.flags))
    }

    pub fn set_play_music(&mut self, play: bool) -> BossMessages {
        if play == self.flags.play_music {
            return Vec::new();
        }
        self.flags.play_music = play;
        self.broadcast(&BossUpdate::Properties(self.flags))
    }

    pub fn set_create_world_fog(&mut self, fog: bool) -> BossMessages {
        if fog == self.flags.create_world_fog {
            return Vec::new();
        }
        self.flags.create_world_fog = fog;
        self.broadcast(&BossUpdate::Properties(self.flags))
    }

    pub fn set_name(&mut self, name: Component) -> BossMessages {
        if name == self.name {
            return Vec::new();
        }
        self.name = name.clone();
        self.broadcast(&BossUpdate::Name(name))
    }

    pub fn set_visible(&mut self, visible: bool) -> BossMessages {
        if visible == self.visible {
            return Vec::new();
        }
        self.visible = visible;
        let update = if visible {
            self.add_packet()
        } else {
            BossUpdate::Remove
        };
        self.players
            .iter()
            .map(|player| (*player, update.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(n: u128) -> PlayerId {
        PlayerId(Uuid::from_u128(n))
    }

    fn bar() -> ServerBossEvent {
        ServerBossEvent::new(
            Uuid::from_u128(1),
            Component::text("Wither"),
            BossColor::Purple,
            BossOverlay::Progress,
        )
    }

    #[test]
    fn a_new_bar_is_visible_full_and_has_no_flags() {
        let bar = bar();
        assert!(bar.is_visible());
        assert_eq!(bar.progress(), 1.0);
        assert_eq!(bar.flags().bits(), 0);
        assert_eq!(bar.players().count(), 0);
    }

    #[test]
    fn adding_a_player_sends_the_add_packet_once() {
        let mut bar = bar();
        let sent = bar.add_player(player(5));
        assert_eq!(sent, vec![(player(5), bar.add_packet())]);
        assert!(bar.add_player(player(5)).is_empty());
        assert!(bar.has_player(player(5)));
    }

    #[test]
    fn removing_a_player_sends_the_remove_packet_only_when_present() {
        let mut bar = bar();
        assert!(bar.remove_player(player(5)).is_empty());
        bar.add_player(player(5));
        assert_eq!(
            bar.remove_player(player(5)),
            vec![(player(5), BossUpdate::Remove)]
        );
        assert!(!bar.has_player(player(5)));
    }

    #[test]
    fn progress_changes_are_broadcast_to_every_player_and_unchanged_values_are_silent() {
        let mut bar = bar();
        bar.add_player(player(1));
        bar.add_player(player(2));
        assert!(bar.set_progress(1.0).is_empty());
        let sent = bar.set_progress(0.5);
        assert_eq!(sent.len(), 2);
        assert!(
            sent.iter()
                .all(|(_, update)| *update == BossUpdate::Progress(0.5))
        );
        assert_eq!(bar.progress(), 0.5);
    }

    #[test]
    fn a_hidden_bar_sends_nothing_on_changes_and_removes_itself_from_viewers() {
        let mut bar = bar();
        bar.add_player(player(1));
        let hidden = bar.set_visible(false);
        assert_eq!(hidden, vec![(player(1), BossUpdate::Remove)]);
        assert!(bar.set_progress(0.1).is_empty());
        assert!(bar.add_player(player(2)).is_empty());
        let shown = bar.set_visible(true);
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().all(
            |(_, update)| matches!(update, BossUpdate::Add { progress, .. } if *progress == 0.1)
        ));
    }

    #[test]
    fn the_flags_pack_into_the_vanilla_bits_and_darken_screen_updates_the_properties() {
        let mut bar = bar();
        bar.add_player(player(1));
        let sent = bar.set_darken_screen(true);
        assert_eq!(
            sent,
            vec![(
                player(1),
                BossUpdate::Properties(BossFlags {
                    darken_screen: true,
                    ..BossFlags::default()
                })
            )]
        );
        bar.set_play_music(true);
        bar.set_create_world_fog(true);
        assert_eq!(bar.flags().bits(), 7);
        assert!(bar.set_darken_screen(true).is_empty());
    }

    #[test]
    fn a_name_or_style_change_is_broadcast_and_removing_all_players_empties_the_bar() {
        let mut bar = bar();
        bar.add_player(player(1));
        assert_eq!(bar.set_name(Component::text("Wither")).len(), 0);
        assert_eq!(bar.set_name(Component::text("Bob")).len(), 1);
        assert_eq!(
            bar.set_color(BossColor::Red),
            vec![(
                player(1),
                BossUpdate::Style {
                    color: BossColor::Red,
                    overlay: BossOverlay::Progress
                }
            )]
        );
        assert_eq!(bar.set_overlay(BossOverlay::Notched10).len(), 1);
        assert_eq!(bar.remove_all_players().len(), 1);
        assert_eq!(bar.players().count(), 0);
    }
}
