use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, Identifier};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const DEFAULT_STYLE: &str = "minecraft:default";
const NEAR_BLOCKS: f64 = 128.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaypointIcon {
    pub style: Identifier,
    pub color: Option<u32>,
}

impl Default for WaypointIcon {
    fn default() -> Self {
        Self {
            style: Identifier::parse(DEFAULT_STYLE).unwrap_or_else(|_| unreachable!()),
            color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaypointData {
    Vec3i(i32, i32, i32),
    Azimuth(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaypointOperation {
    Track,
    Untrack,
    Update,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaypointPacket {
    pub operation: WaypointOperation,
    pub id: Uuid,
    pub icon: WaypointIcon,
    pub data: Option<WaypointData>,
}

#[derive(Debug, Default)]
pub struct Waypoints {
    icons: FxHashMap<PlayerId, WaypointIcon>,
    sent: FxHashMap<(PlayerId, PlayerId), (WaypointIcon, WaypointData)>,
}

impl Waypoints {
    pub fn icon(&self, player: PlayerId) -> WaypointIcon {
        self.icons.get(&player).cloned().unwrap_or_default()
    }

    pub fn set_icon(&mut self, player: PlayerId, icon: WaypointIcon) {
        if icon == WaypointIcon::default() {
            self.icons.remove(&player);
        } else {
            self.icons.insert(player, icon);
        }
    }

    pub fn retain_players(&mut self, keep: impl Fn(&PlayerId) -> bool) {
        self.icons.retain(|id, _| keep(id));
    }
}

pub fn team_color_rgb(name: &str) -> Option<u32> {
    Some(match name {
        "black" => 0x000000,
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    })
}

fn data_for(from: rspigot_domain::Vec3, to: rspigot_domain::Vec3) -> WaypointData {
    let dx = to.x - from.x;
    let dz = to.z - from.z;
    if dx.hypot(dz) < NEAR_BLOCKS {
        WaypointData::Vec3i(
            to.x.floor() as i32,
            to.y.floor() as i32,
            to.z.floor() as i32,
        )
    } else {
        WaypointData::Azimuth(dz.atan2(dx) as f32)
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn waypoint_transmitters(
        &self,
        dimension: &Identifier,
    ) -> Vec<(PlayerId, BlockPos)> {
        let mut found: Vec<(PlayerId, BlockPos)> = self
            .players
            .values()
            .filter(|p| p.game_mode != GameMode::Spectator && p.dim == *dimension)
            .map(|p| {
                (
                    p.id,
                    BlockPos::new(
                        p.position.x.floor() as i32,
                        p.position.y.floor() as i32,
                        p.position.z.floor() as i32,
                    ),
                )
            })
            .collect();
        found.sort_by_key(|(id, _)| {
            self.players
                .get(id)
                .map(|p| p.profile.name.to_ascii_lowercase())
        });
        found
    }

    pub(crate) fn sync_waypoints(&mut self) {
        let enabled = self.config.rules.locator_bar;
        let sources: Vec<(PlayerId, Identifier, rspigot_domain::Vec3)> = self
            .players
            .values()
            .filter(|p| enabled && p.game_mode != GameMode::Spectator)
            .map(|p| (p.id, p.dim.clone(), p.position))
            .collect();
        let receivers: Vec<(PlayerId, Identifier, rspigot_domain::Vec3)> = self
            .players
            .values()
            .map(|p| (p.id, p.dim.clone(), p.position))
            .collect();
        let mut current: FxHashMap<(PlayerId, PlayerId), (WaypointIcon, WaypointData)> =
            FxHashMap::default();
        let mut packets: Vec<(PlayerId, WaypointPacket)> = Vec::new();
        for (receiver, dim, at) in &receivers {
            for (source, source_dim, position) in &sources {
                if source == receiver || source_dim != dim {
                    continue;
                }
                let icon = self.surface.waypoints.icon(*source);
                let data = data_for(*at, *position);
                let operation = match self.surface.waypoints.sent.get(&(*receiver, *source)) {
                    None => Some(WaypointOperation::Track),
                    Some(previous) if *previous != (icon.clone(), data) => {
                        Some(WaypointOperation::Update)
                    }
                    Some(_) => None,
                };
                if let Some(operation) = operation {
                    packets.push((
                        *receiver,
                        WaypointPacket {
                            operation,
                            id: source.0,
                            icon: icon.clone(),
                            data: Some(data),
                        },
                    ));
                }
                current.insert((*receiver, *source), (icon, data));
            }
        }
        let gone: Vec<(PlayerId, PlayerId)> = self
            .surface
            .waypoints
            .sent
            .keys()
            .filter(|key| !current.contains_key(key))
            .copied()
            .collect();
        for (receiver, source) in gone {
            let icon = self.surface.waypoints.icon(source);
            if self.players.contains_key(&receiver) {
                packets.push((
                    receiver,
                    WaypointPacket {
                        operation: WaypointOperation::Untrack,
                        id: source.0,
                        icon,
                        data: None,
                    },
                ));
            }
        }
        self.surface.waypoints.sent = current;
        for (receiver, packet) in packets {
            self.push(
                Audience::One(receiver),
                ClientEvent::TrackedWaypoint(Box::new(packet)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_icon_is_not_stored() {
        let mut waypoints = Waypoints::default();
        let player = PlayerId::offline("Alice");
        waypoints.set_icon(
            player,
            WaypointIcon {
                style: Identifier::parse("minecraft:bowtie").unwrap(),
                color: Some(1),
            },
        );
        assert_eq!(waypoints.icon(player).color, Some(1));
        waypoints.set_icon(player, WaypointIcon::default());
        assert_eq!(waypoints.icon(player), WaypointIcon::default());
    }

    #[test]
    fn team_colors_map_to_their_rgb() {
        assert_eq!(team_color_rgb("red"), Some(0xFF5555));
        assert_eq!(team_color_rgb("reset"), None);
    }
}
