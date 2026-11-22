use rspigot_data::map_colors::decoration_type_id;
use rspigot_domain::Identifier;
use rspigot_domain::cartography;
use rspigot_domain::item::menu_slot;
use rspigot_domain::map_data::{LevelRef, MAP_COLORS, MAP_SIZE, MAX_SCALE, Spot};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_nbt::Compound;

use crate::plugins::{ActionError, ActionErrorKind};
use crate::ports::ClientLink;
use crate::server::Server;

const LIVE_MARKER: &str = "plugin:";
const MAP_DECORATIONS: &str = "minecraft:map_decorations";
const SPIGOT_SLOTS: usize = 41;
const MAX_STACK_MARKERS: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum MapEdit {
    SetCenter {
        x: i32,
        z: i32,
    },
    SetScale(u8),
    SetLocked(bool),
    SetTrackingPosition(bool),
    SetUnlimitedTracking(bool),
    SetPixel {
        x: i32,
        y: i32,
        color: u8,
    },
    SetCanvas(Vec<u8>),
    AddMarker {
        key: String,
        kind: String,
        x: f64,
        z: f64,
        rotation: f64,
        name: Option<String>,
    },
    RemoveMarker(String),
    ClearMarkers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapDecorationView {
    pub kind: u8,
    pub x: i8,
    pub y: i8,
    pub rotation: i8,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapView {
    pub id: i32,
    pub dimension: String,
    pub center_x: i32,
    pub center_z: i32,
    pub scale: u8,
    pub locked: bool,
    pub tracking_position: bool,
    pub unlimited_tracking: bool,
    pub colors: Vec<u8>,
    pub decorations: Vec<MapDecorationView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapMarker {
    pub kind: String,
    pub x: f64,
    pub z: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapMarkerChange {
    Set(MapMarker),
    Remove,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapDecorationEdit {
    pub slot: i32,
    pub key: String,
    pub change: MapMarkerChange,
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn not_found(id: i32) -> ActionError {
    ActionError::new(ActionErrorKind::NotFound, format!("there is no map {id}"))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn edit_map(&mut self, id: i32, edit: MapEdit) -> Result<(), ActionError> {
        let nether = self.world.dimension.key == Identifier::minecraft("the_nether");
        let game_time = self.world.level.game_time;
        let data = self.map_data_mut(id).ok_or_else(|| not_found(id))?;
        match edit {
            MapEdit::SetCenter { x, z } => {
                data.center_x = x;
                data.center_z = z;
                data.mark_dirty();
            }
            MapEdit::SetScale(scale) => {
                if scale > MAX_SCALE {
                    return Err(invalid(format!("scale {scale} is above {MAX_SCALE}")));
                }
                data.scale = scale;
                data.mark_dirty();
            }
            MapEdit::SetLocked(locked) => {
                data.locked = locked;
                data.mark_dirty();
            }
            MapEdit::SetTrackingPosition(tracking) => {
                data.tracking_position = tracking;
                data.mark_dirty();
            }
            MapEdit::SetUnlimitedTracking(unlimited) => {
                data.unlimited_tracking = unlimited;
                data.mark_dirty();
            }
            MapEdit::SetPixel { x, y, color } => {
                let size = MAP_SIZE as i32;
                if !(0..size).contains(&x) || !(0..size).contains(&y) {
                    return Err(invalid(format!(
                        "pixel {x}, {y} is outside the 128 by 128 canvas"
                    )));
                }
                data.update_color(x, y, color);
            }
            MapEdit::AddMarker {
                key,
                kind,
                x,
                z,
                rotation,
                name,
            } => {
                if key.is_empty() {
                    return Err(invalid("a marker needs a key"));
                }
                let kind = decoration_type_id(&kind)
                    .ok_or_else(|| invalid(format!("unknown marker type {kind}")))?;
                if !x.is_finite() || !z.is_finite() || !rotation.is_finite() {
                    return Err(invalid("a marker position is not finite"));
                }
                let level = LevelRef { nether, game_time };
                data.add_decoration(
                    kind,
                    &level,
                    &format!("{LIVE_MARKER}{key}"),
                    Spot { x, z, rotation },
                    name.as_deref().map(Component::from_section_codes),
                );
                data.mark_dirty();
            }
            MapEdit::RemoveMarker(key) => {
                data.remove_decoration(&format!("{LIVE_MARKER}{key}"));
                data.mark_dirty();
            }
            MapEdit::ClearMarkers => {
                data.remove_decorations_with_prefix(LIVE_MARKER);
                data.mark_dirty();
            }
            MapEdit::SetCanvas(colors) => {
                if colors.len() != MAP_COLORS {
                    return Err(invalid(format!(
                        "a canvas holds {MAP_COLORS} colors, not {}",
                        colors.len()
                    )));
                }
                let size = MAP_SIZE as i32;
                for (index, color) in colors.into_iter().enumerate() {
                    let index = index as i32;
                    data.update_color(index % size, index / size, color);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn map_view(&mut self, id: i32) -> Result<MapView, ActionError> {
        let data = self.map_data_mut(id).ok_or_else(|| not_found(id))?;
        Ok(MapView {
            id,
            dimension: data.dimension.clone(),
            center_x: data.center_x,
            center_z: data.center_z,
            scale: data.scale,
            locked: data.locked,
            tracking_position: data.tracking_position,
            unlimited_tracking: data.unlimited_tracking,
            colors: data.colors().to_vec(),
            decorations: data
                .decorations()
                .map(|decoration| MapDecorationView {
                    kind: decoration.kind,
                    x: decoration.x,
                    y: decoration.y,
                    rotation: decoration.rot,
                    name: decoration.name.as_ref().map(|name| name.to_legacy()),
                })
                .collect(),
        })
    }

    pub(crate) fn edit_map_decoration(
        &mut self,
        id: PlayerId,
        edit: MapDecorationEdit,
    ) -> Result<(), ActionError> {
        let slot = usize::try_from(edit.slot)
            .ok()
            .filter(|slot| *slot < SPIGOT_SLOTS)
            .and_then(menu_slot)
            .ok_or_else(|| invalid(format!("slot {} is outside 0..=40", edit.slot)))?;
        if edit.key.is_empty() {
            return Err(invalid("a marker needs a key"));
        }
        let marker = match &edit.change {
            MapMarkerChange::Set(marker) => {
                let kind = decoration_type_id(&marker.kind)
                    .ok_or_else(|| invalid(format!("unknown marker type {}", marker.kind)))?;
                if !marker.x.is_finite() || !marker.z.is_finite() || !marker.rotation.is_finite() {
                    return Err(invalid("a marker position is not finite"));
                }
                Some((marker, kind))
            }
            MapMarkerChange::Remove => None,
        };
        let player = self
            .players
            .get_mut(&id)
            .ok_or_else(ActionError::player_gone)?;
        let mut stack = player
            .inventory
            .get(slot)
            .cloned()
            .ok_or_else(|| invalid(format!("slot {} holds no item", edit.slot)))?;
        if !cartography::is_map_item(&stack) {
            return Err(invalid("the item is not a map"));
        }
        let mut markers = stack
            .components
            .get_compound(MAP_DECORATIONS)
            .cloned()
            .unwrap_or_default();
        match marker {
            Some((marker, _)) => {
                if !markers.contains_key(&edit.key) && markers.len() >= MAX_STACK_MARKERS {
                    return Err(ActionError::new(
                        ActionErrorKind::LimitReached,
                        format!("a map takes at most {MAX_STACK_MARKERS} markers"),
                    ));
                }
                markers.insert(
                    edit.key.clone(),
                    Compound::new()
                        .with("type", marker.kind.as_str())
                        .with("x", marker.x)
                        .with("z", marker.z)
                        .with("rotation", marker.rotation),
                );
            }
            None => {
                markers.remove(&edit.key);
            }
        }
        if markers.is_empty() {
            stack.components.remove(MAP_DECORATIONS);
        } else {
            stack.components.insert(MAP_DECORATIONS, markers);
        }
        let before = Self::equipment_of(player);
        player.inventory.set(slot, Some(stack));
        self.sync_inventory(id);
        self.broadcast_equipment_changes(id, before);
        Ok(())
    }
}
