use rspigot_domain::EntityId;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, arrow};
use rspigot_entities::EntityKind as Kind;

use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

const ACTIVATION_HEIGHT: f64 = 256.0;
const ITEM_ALWAYS_TICK_EVERY: i64 = 4;
const NEVER_PICKUP: i32 = 32767;

pub(crate) fn immune(entity: &Entity) -> bool {
    let base = &entity.base;
    if base.fluid.in_water || base.remaining_fire_ticks > 0 {
        return true;
    }
    match &entity.state {
        EntityState::Arrow(_) => !arrow::in_ground(base),
        EntityState::Orb(_) => true,
        _ => !base.on_ground,
    }
}

fn always_active(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Player
            | Kind::Snowball
            | Kind::Egg
            | Kind::EnderPearl
            | Kind::SplashPotion
            | Kind::LingeringPotion
            | Kind::ExperienceBottle
            | Kind::EnderDragon
            | Kind::Wither
            | Kind::Fireball
            | Kind::SmallFireball
            | Kind::DragonFireball
            | Kind::WitherSkull
            | Kind::WindCharge
            | Kind::BreezeWindCharge
            | Kind::LightningBolt
            | Kind::Tnt
            | Kind::EndCrystal
            | Kind::FireworkRocket
            | Kind::Trident
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn passive_active(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return true;
        };
        let base = &entity.base;
        let range = self.config.activation.misc;
        if range <= 0 || always_active(base.kind) {
            return true;
        }
        if matches!(entity.state, EntityState::Item(_))
            && (i64::from(base.tick_count) + i64::from(id.0) + 1) % ITEM_ALWAYS_TICK_EVERY == 0
        {
            return true;
        }
        let bounds = base.bounding_box();
        let ignore = self.config.activation.ignore_spectators;
        let near = self
            .entities
            .mobs
            .players
            .iter()
            .filter(|player| !(ignore && player.spectator))
            .any(|player| {
                rspigot_ai::inflate(
                    &player.bounds,
                    f64::from(range),
                    ACTIVATION_HEIGHT,
                    f64::from(range),
                )
                .intersects(&bounds)
            });
        near || immune(entity)
    }

    pub(crate) fn passive_inactive_tick(&mut self, id: EntityId) {
        let despawn_after = self.config.item_despawn_ticks as i32;
        let mut expired = false;
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.tick_count = entity.base.tick_count.wrapping_add(1);
            match &mut entity.state {
                EntityState::Item(state) => {
                    if state.pickup_delay > 0 && state.pickup_delay != NEVER_PICKUP {
                        state.pickup_delay -= 1;
                    }
                    if state.age != i32::MIN + 1 && state.age != -32768 {
                        state.age += 1;
                    }
                    expired = state.age >= despawn_after;
                }
                EntityState::Arrow(state) if arrow::in_ground(&entity.base) => {
                    state.life += 1;
                }
                _ => {}
            }
        }
        if expired {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::Inbound;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn server(misc: i32) -> Server<Quiet> {
        let mut config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 2,
            view_distance: 4,
            ..GameConfig::default()
        };
        config.activation.misc = misc;
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        );
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn join(server: &mut Server<Quiet>) {
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: PlayerId::offline("Alice"),
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Quiet,
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.refresh_mob_players();
    }

    fn resting_item(server: &mut Server<Quiet>, x: f64) -> EntityId {
        let before: Vec<EntityId> = server.ground_items().iter().map(|(id, _)| *id).collect();
        server.handle(Inbound::Console(format!(
            "summon minecraft:item {x} 64 0.5 {{Item:{{id:\"minecraft:dirt\",count:1}}}}"
        )));
        let mut found = None;
        for _ in 0..400 {
            server.tick();
            std::thread::sleep(Duration::from_millis(2));
            found = server
                .ground_items()
                .iter()
                .map(|(id, _)| *id)
                .find(|id| !before.contains(id))
                .filter(|id| {
                    server
                        .entities
                        .store
                        .get(*id)
                        .is_some_and(|entity| entity.base.on_ground)
                });
            if found.is_some() {
                break;
            }
        }
        let id = found.expect("an item at rest");
        let skipped = (4 - (i64::from(id.0) + 1).rem_euclid(4)).rem_euclid(4) as i32 + 1;
        if let Some(entity) = server.entities.store.get_mut(id) {
            entity.base.tick_count = skipped;
        }
        id
    }

    #[test]
    fn a_resting_item_far_from_every_player_is_inactive() {
        let mut server = server(16);
        join(&mut server);
        let far = resting_item(&mut server, 24.5);
        server.refresh_mob_players();
        assert!(!server.passive_active(far));
    }

    #[test]
    fn a_resting_item_near_a_player_is_active() {
        let mut server = server(16);
        join(&mut server);
        let near = resting_item(&mut server, 2.5);
        server.refresh_mob_players();
        assert!(server.passive_active(near));
    }

    #[test]
    fn an_inactive_item_still_ticks_every_fourth_tick_by_its_id() {
        let mut server = server(16);
        join(&mut server);
        let far = resting_item(&mut server, 24.5);
        let turn = (4 - (i64::from(far.0) + 1).rem_euclid(4)).rem_euclid(4) as i32;
        if let Some(entity) = server.entities.store.get_mut(far) {
            entity.base.tick_count = turn;
        }
        assert!(server.passive_active(far));
    }

    #[test]
    fn a_zero_misc_range_keeps_everything_active() {
        let mut server = server(0);
        join(&mut server);
        let far = resting_item(&mut server, 24.5);
        assert!(server.passive_active(far));
    }

    #[test]
    fn an_inactive_tick_ages_the_item_and_discards_it_at_the_despawn_age() {
        let mut server = server(16);
        join(&mut server);
        let far = resting_item(&mut server, 24.5);
        server.config.item_despawn_ticks = 10;
        if let Some(Entity {
            state: EntityState::Item(state),
            ..
        }) = server.entities.store.get_mut(far)
        {
            state.age = 8;
        }
        for _ in 0..3 {
            server.passive_inactive_tick(far);
        }
        assert!(!server.ground_items().iter().any(|(id, _)| *id == far));
    }
}
