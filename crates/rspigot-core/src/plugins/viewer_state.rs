use std::collections::BTreeSet;

use rspigot_domain::EntityId;
use rspigot_domain::player::PlayerId;
use rspigot_domain::world_border::{MAX_CENTER, MAX_SIZE, WorldBorder};
use rustc_hash::FxHashMap;

use crate::commands::CommandContext;
use crate::events::ClientEvent;
use crate::plugins::BorderOrder;
use crate::plugins::resolve::{finite_not_negative, not_negative};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug, Default)]
pub(crate) struct ViewerState {
    hidden: FxHashMap<(PlayerId, EntityId), BTreeSet<String>>,
    borders: FxHashMap<PlayerId, WorldBorder>,
}

impl ViewerState {
    pub(crate) fn hides_entity(&self, viewer: PlayerId, entity: EntityId) -> bool {
        self.hidden.contains_key(&(viewer, entity))
    }

    fn hide(&mut self, plugin: &str, viewer: PlayerId, entity: EntityId) -> bool {
        let plugins = self.hidden.entry((viewer, entity)).or_default();
        let first = plugins.is_empty();
        plugins.insert(plugin.to_owned());
        first
    }

    fn show(&mut self, plugin: &str, viewer: PlayerId, entity: EntityId) -> bool {
        let Some(plugins) = self.hidden.get_mut(&(viewer, entity)) else {
            return false;
        };
        plugins.remove(plugin);
        if !plugins.is_empty() {
            return false;
        }
        self.hidden.remove(&(viewer, entity));
        true
    }

    fn release(&mut self, plugin: &str) -> Vec<(PlayerId, EntityId)> {
        let mut freed = Vec::new();
        self.hidden.retain(|key, plugins| {
            plugins.remove(plugin);
            let keep = !plugins.is_empty();
            if !keep {
                freed.push(*key);
            }
            keep
        });
        freed
    }

    pub(crate) fn retain(
        &mut self,
        player_online: impl Fn(PlayerId) -> bool,
        entity_alive: impl Fn(EntityId) -> bool,
    ) {
        self.hidden
            .retain(|(viewer, entity), _| player_online(*viewer) && entity_alive(*entity));
        self.borders.retain(|viewer, _| player_online(*viewer));
    }
}

fn apply_border(border: &mut WorldBorder, now: i64, order: BorderOrder) -> Result<(), String> {
    match order {
        BorderOrder::Resize { size, ticks } => {
            if !size.is_finite() || !(1.0..=MAX_SIZE).contains(&size) {
                return Err(format!("border size {size} is outside 1..={MAX_SIZE}"));
            }
            if ticks < 0 {
                return Err(format!("border resize time {ticks} is negative"));
            }
            if ticks > 0 {
                let current = border.size(now);
                border.lerp_size(current, size, now, ticks);
            } else {
                border.set_size(size);
            }
        }
        BorderOrder::SetCenter { x, z } => {
            if !x.is_finite() || !z.is_finite() || x.abs() > MAX_CENTER || z.abs() > MAX_CENTER {
                return Err(format!("border center {x}, {z} is outside ±{MAX_CENTER}"));
            }
            border.set_center(x, z);
        }
        BorderOrder::SetDamageAmount(amount) => {
            finite_not_negative("border damage amount", amount)?;
            border.set_damage_per_block(amount);
        }
        BorderOrder::SetDamageBuffer(distance) => {
            finite_not_negative("border damage buffer", distance)?;
            border.set_safe_zone(distance);
        }
        BorderOrder::SetWarningDistance(blocks) => {
            not_negative("border warning distance", blocks)?;
            border.set_warning_blocks(blocks);
        }
        BorderOrder::SetWarningTime(ticks) => {
            not_negative("border warning time", ticks)?;
            border.set_warning_time(ticks);
        }
    }
    Ok(())
}

impl<L: ClientLink> Server<L> {
    pub(super) fn hide_entity_for(
        &mut self,
        connection: u64,
        viewer: PlayerId,
        entity: EntityId,
    ) -> Result<(), String> {
        if let Some(target) = self.player_of_entity(entity) {
            return self.hide_player(connection, viewer, target);
        }
        if self.entities.store.get(entity).is_none() {
            return Err("the entity is gone".to_owned());
        }
        let plugin = self
            .plugins
            .name(connection)
            .ok_or("the plugin is no longer connected")?
            .to_owned();
        if self.plugins.viewer_state.hide(&plugin, viewer, entity) {
            self.update_viewer(entity, viewer);
        }
        Ok(())
    }

    pub(super) fn show_entity_for(
        &mut self,
        connection: u64,
        viewer: PlayerId,
        entity: EntityId,
    ) -> Result<(), String> {
        if let Some(target) = self.player_of_entity(entity) {
            return self.show_player(connection, viewer, target);
        }
        let plugin = self
            .plugins
            .name(connection)
            .ok_or("the plugin is no longer connected")?
            .to_owned();
        if self.plugins.viewer_state.show(&plugin, viewer, entity) {
            self.update_viewer(entity, viewer);
        }
        Ok(())
    }

    pub(crate) fn release_hidden_entities(&mut self, plugin: &str) {
        for (viewer, entity) in self.plugins.viewer_state.release(plugin) {
            self.update_viewer(entity, viewer);
        }
    }

    pub(crate) fn prune_viewer_state(&mut self) {
        let players = &self.players;
        let entities = &self.entities.store;
        self.plugins.viewer_state.retain(
            |viewer| players.contains_key(&viewer),
            |entity| entities.get(entity).is_some(),
        );
    }

    pub(super) fn set_player_border(
        &mut self,
        id: PlayerId,
        order: BorderOrder,
    ) -> Result<(), String> {
        let (world, now) =
            CommandContext::change_world_border(self, |border, now| (border.clone(), now));
        let mut border = self
            .plugins
            .viewer_state
            .borders
            .get(&id)
            .cloned()
            .unwrap_or(world);
        apply_border(&mut border, now, order)?;
        let view = border.view(now);
        self.plugins.viewer_state.borders.insert(id, border);
        self.push(Audience::One(id), ClientEvent::Border(view));
        Ok(())
    }

    pub(super) fn reset_player_border(&mut self, id: PlayerId) {
        if self.plugins.viewer_state.borders.remove(&id).is_none() {
            return;
        }
        let view = CommandContext::change_world_border(self, |border, now| border.view(now));
        self.push(Audience::One(id), ClientEvent::Border(view));
    }
}
