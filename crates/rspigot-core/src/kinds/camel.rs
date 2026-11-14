use std::num::NonZeroU8;

use rspigot_ai::{CamelAction, CamelFacts, MobView};
use rspigot_domain::entity::species::camel::{self, Anchor, CamelState};
use rspigot_domain::entity::species::{Species, equine};
use rspigot_domain::entity::{Aabb, Entity, EntityState, ageable, fluid, living};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;
use rspigot_entities::SoundSource;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::interact::{Interaction, item_tag};
use crate::mob_actions::new_mob;
use crate::plugins::events::EntityRegainHealthRegainReason;
use crate::ports::ClientLink;
use crate::server::Server;

const COLLISION_MARGIN: f64 = 1.0e-7;

fn sounds(kind: Kind) -> CamelSounds {
    if camel::is_husk(kind) {
        CamelSounds {
            ambient: "minecraft:entity.camel_husk.ambient",
            sit: "minecraft:entity.camel_husk.sit",
            stand: "minecraft:entity.camel_husk.stand",
            dash: "minecraft:entity.camel_husk.dash",
            dash_ready: "minecraft:entity.camel_husk.dash_ready",
            eat: "minecraft:entity.camel_husk.eat",
            saddle: "minecraft:entity.camel_husk.saddle",
        }
    } else {
        CamelSounds {
            ambient: "minecraft:entity.camel.ambient",
            sit: "minecraft:entity.camel.sit",
            stand: "minecraft:entity.camel.stand",
            dash: "minecraft:entity.camel.dash",
            dash_ready: "minecraft:entity.camel.dash_ready",
            eat: "minecraft:entity.camel.eat",
            saddle: "minecraft:entity.camel.saddle",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CamelSounds {
    ambient: &'static str,
    sit: &'static str,
    stand: &'static str,
    dash: &'static str,
    dash_ready: &'static str,
    eat: &'static str,
    saddle: &'static str,
}

pub(crate) fn ambient_sound(kind: Kind) -> &'static str {
    sounds(kind).ambient
}

pub(crate) fn eating_sound(kind: Kind) -> &'static str {
    sounds(kind).eat
}

pub(crate) fn camel_state(entity: &Entity) -> Option<&CamelState> {
    match &entity.living()?.mob.as_ref()?.species {
        Species::Camel(state) => Some(state),
        _ => None,
    }
}

fn camel_state_mut(entity: &mut Entity) -> Option<&mut CamelState> {
    match &mut entity.living_mut()?.mob.as_mut()?.species {
        Species::Camel(state) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    fn game_tick(&self) -> i64 {
        self.world.level.game_time
    }

    fn camel_can_change_pose(&self, entity: &Entity) -> bool {
        let base = &entity.base;
        let baby = entity.is_baby();
        let target = if camel::is_sitting(base) {
            rspigot_domain::entity::Pose::Standing
        } else {
            rspigot_domain::entity::Pose::Sitting
        };
        let dimensions = ageable::dimensions_in_pose(base.kind, baby, target);
        let area = Aabb::of_size(base.position, dimensions.width, dimensions.height)
            .deflate(COLLISION_MARGIN);
        fluid::is_free_of_collision(&self.world.chunks, &area)
    }

    pub(crate) fn camel_mob_controlled(&self, id: EntityId) -> bool {
        camel::is_husk(self.kind_of(id).unwrap_or(Kind::Camel))
            && self
                .entities
                .rides
                .first_passenger(id)
                .is_some_and(|passenger| {
                    self.entities
                        .store
                        .get(passenger)
                        .is_some_and(|rider| rider.living().is_some_and(|l| l.is_mob()))
                })
    }

    pub(crate) fn camel_view(&self, entity: &Entity, view: &mut MobView) {
        self.equine_view(entity, view);
        let base = &entity.base;
        let now = self.game_tick();
        let id = base.id;
        view.species.camel = CamelFacts {
            sitting: camel::is_sitting(base),
            in_transition: camel::is_in_pose_transition(base, now),
            pose_time: camel::pose_time(base, now),
            mob_controlled: self.camel_mob_controlled(id),
            controlled_by_player: self.controlling_player(id).is_some(),
            can_change_pose: self.camel_can_change_pose(entity),
        };
    }

    pub(crate) fn camel_pre_tick(&mut self, id: EntityId) -> bool {
        let now = self.game_tick();
        if let Some(entity) = self.entities.store.get_mut(id) {
            let fresh = camel_state_mut(entity).is_some_and(|state| {
                let fresh = state.fresh;
                state.fresh = false;
                fresh
            });
            if fresh {
                camel::reset_to_full_stand(&mut entity.base, now);
            }
        }
        self.equine_pre_tick(id)
    }

    pub(crate) fn camel_post_tick(&mut self, id: EntityId) {
        let now = self.game_tick();
        let passenger = self.entities.rides.is_passenger(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let kind = entity.base.kind;
        let on_ground = entity.base.on_ground;
        let in_liquid = entity.base.fluid.in_liquid();
        let in_water = entity.base.fluid.in_water;
        let dashing = camel::is_dashing(&entity.base);
        let cooldown = camel_state(entity).map_or(0, |state| state.dash_cooldown);
        let stop_dash = dashing
            && cooldown < camel::DASH_READY_THRESHOLD
            && (on_ground || in_liquid || passenger);
        if stop_dash {
            self.camel_set_dashing(id, false);
        }
        let mut ready = false;
        if let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(camel_state_mut)
            .filter(|state| state.dash_cooldown > 0)
        {
            state.dash_cooldown -= 1;
            ready = state.dash_cooldown == 0;
        }
        if ready && let Some(at) = self.entities.store.get(id).map(|e| e.base.position) {
            self.play_sound(
                None,
                at,
                sounds(kind).dash_ready,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let refuse = camel::refuse_to_move(&entity.base, now);
        if refuse {
            clamp_head_to_body(entity);
        }
        if camel::is_sitting(&entity.base) && in_water {
            self.camel_stand_up_instantly(id);
        }
    }

    pub(crate) fn camel_custom_ai_step(&mut self, id: EntityId) {
        let now = self.game_tick();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if camel::refuse_to_move(&entity.base, now) && entity.base.on_ground {
            entity.base.velocity = Vec3::new(0.0, entity.base.velocity.y, 0.0);
            if let Some(state) = entity.living_mut() {
                state.xxa = 0.0;
                state.zza = 0.0;
            }
        }
    }

    pub(crate) fn camel_set_dashing(&mut self, id: EntityId, dashing: bool) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if camel::is_dashing(&entity.base) == dashing {
            return;
        }
        camel::set_dashing(&mut entity.base, dashing);
        if let Some(state) = camel_state_mut(entity)
            && state.dash_cooldown == 0
        {
            state.dash_cooldown = camel::DASH_COOLDOWN;
        }
    }

    pub(crate) fn camel_action(&mut self, id: EntityId, action: CamelAction) {
        match action {
            CamelAction::SitDown => self.camel_sit_down(id),
            CamelAction::StandUp => self.camel_stand_up(id),
            CamelAction::StandUpInstantly => self.camel_stand_up_instantly(id),
        }
    }

    fn camel_pose_event(&mut self, id: EntityId, sound: &'static str) {
        self.entity_sound(id, sound, 1.0, 1.0);
        if let Some(at) = self.entities.store.get(id).map(|e| e.base.position) {
            self.game_event(GameEvent::EntityAction, at, GameEventContext::of(Some(id)));
        }
    }

    fn camel_sit_down(&mut self, id: EntityId) {
        let now = self.game_tick();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        let kind = entity.base.kind;
        if !camel::sit_down(&mut entity.base, baby, now) {
            return;
        }
        self.camel_pose_event(id, sounds(kind).sit);
    }

    fn camel_stand_up(&mut self, id: EntityId) {
        let now = self.game_tick();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        let kind = entity.base.kind;
        if !camel::stand_up(&mut entity.base, baby, now) {
            return;
        }
        self.camel_pose_event(id, sounds(kind).stand);
    }

    pub(crate) fn camel_stand_up_instantly(&mut self, id: EntityId) {
        let now = self.game_tick();
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        camel::stand_up_instantly(&mut entity.base, baby, now);
        if let Some(at) = self.entities.store.get(id).map(|e| e.base.position) {
            self.game_event(GameEvent::EntityAction, at, GameEventContext::of(Some(id)));
        }
    }

    pub(crate) fn camel_on_leash_pull(&mut self, id: EntityId) {
        let now = self.game_tick();
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if camel::is_sitting(&entity.base)
            && !camel::is_in_pose_transition(&entity.base, now)
            && self.camel_can_change_pose(entity)
        {
            self.camel_stand_up(id);
        }
    }

    pub(crate) fn camel_can_jump(&self, id: EntityId) -> bool {
        let now = self.game_tick();
        self.entities
            .store
            .get(id)
            .is_some_and(|entity| !camel::refuse_to_move(&entity.base, now))
    }

    pub(crate) fn camel_start_dash(&mut self, id: EntityId) {
        let Some(kind) = self.kind_of(id) else {
            return;
        };
        self.entity_sound(id, sounds(kind).dash, 1.0, 1.0);
        if let Some(at) = self.entities.store.get(id).map(|e| e.base.position) {
            self.game_event(GameEvent::EntityAction, at, GameEventContext::of(Some(id)));
        }
        self.camel_set_dashing(id, true);
    }

    pub(crate) fn camel_hurt(&mut self, id: EntityId) {
        self.camel_stand_up_instantly(id);
    }

    pub(crate) fn camel_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let kind = self.kind_of(target)?;
        if camel::is_husk(kind)
            && let Some(mob) = self
                .entities
                .store
                .get_mut(target)
                .and_then(|entity| entity.living_mut())
                .and_then(|living| living.mob.as_mut())
        {
            mob.persistence_required = true;
        }
        let entity = self.entities.store.get(target)?;
        let baby = entity.is_baby();
        let sneaking = self.players.get(&player).is_some_and(|p| p.sneaking);
        if sneaking && !baby {
            self.open_mount_inventory(player, target);
            return Some(Interaction::Predicted);
        }
        if let Some(stack) = held {
            if stack.item == ItemKind::Saddle.id() && self.camel_can_saddle(target, stack) {
                self.camel_equip_saddle(player, target, hand, stack);
                return Some(Interaction::Predicted);
            }
            if item_tag(stack.item, camel::food_tag(kind)) {
                let fed = self.camel_handle_eating(player, target);
                if fed {
                    self.consume_held(player, hand, 1);
                    return Some(Interaction::ServerOnly);
                }
                return Some(Interaction::Pass);
            }
        }
        let seated = self.entities.rides.passengers_of(target).len();
        if seated < camel::MAX_PLAYER_PASSENGERS && !baby {
            self.camel_player_ride(player, target);
            return Some(Interaction::Predicted);
        }
        if baby
            && self
                .players
                .get(&player)
                .is_some_and(|p| self.holding_item(p, ItemKind::GoldenDandelion))
        {
            return Some(match held {
                Some(stack) => self.animal_interact(player, target, hand, stack),
                None => Interaction::Pass,
            });
        }
        Some(Interaction::Pass)
    }

    fn holding_item(&self, player: &crate::player::Player<L>, item: ItemKind) -> bool {
        [Hand::Main, Hand::Off].into_iter().any(|hand| {
            self.held(player.id, hand)
                .is_some_and(|stack| stack.item == item.id())
        })
    }

    fn camel_can_saddle(&self, id: EntityId, stack: &ItemStack) -> bool {
        self.entities.store.get(id).is_some_and(|entity| {
            super::equine::is_equippable_in(entity, stack, EquipmentSlot::Saddle)
                && entity
                    .living()
                    .is_some_and(|l| l.equipment(EquipmentSlot::Saddle).is_none())
        })
    }

    fn camel_equip_saddle(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) {
        let mut one = stack.clone();
        one.count = NonZeroU8::MIN;
        self.consume_held(player, hand, 1);
        self.mount_equip(target, EquipmentSlot::Saddle, Some(one));
        if let Some(kind) = self.kind_of(target) {
            self.entity_sound(target, sounds(kind).saddle, 1.0, 1.0);
        }
        if let Some(at) = self.entities.store.get(target).map(|e| e.base.position) {
            let rider = self.players.get(&player).map(|p| p.entity_id);
            self.game_event(GameEvent::Equip, at, GameEventContext::of(rider));
        }
    }

    fn camel_player_ride(&mut self, player: PlayerId, target: EntityId) {
        if let Some((base, mount)) = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| mount_state_of(entity))
        {
            equine::set_eating(base, false);
            mount.clear_standing(base);
        }
        let Some((yaw, pitch)) = self
            .entities
            .store
            .get(target)
            .map(|entity| (entity.base.yaw, entity.base.pitch))
        else {
            return;
        };
        let Some(rider) = self.players.get_mut(&player).map(|p| {
            p.rotation.yaw = yaw;
            p.rotation.pitch = pitch;
            p.entity_id
        }) else {
            return;
        };
        self.start_riding(rider, target, false, true);
    }

    fn camel_handle_eating(&mut self, player: PlayerId, target: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(target) else {
            return false;
        };
        let kind = entity.base.kind;
        let Some(living) = entity.living() else {
            return false;
        };
        let mut attributes = living.attributes.clone();
        let max_health = attributes.value(living::MAX_HEALTH, &living.effects) as f32;
        let hurt = living::health(&entity.base) < max_health;
        let (age, in_love) = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .map_or((0, false), |state| (state.age, state.in_love()));
        let love = camel::can_fall_in_love(kind) && age == 0 && !in_love;
        let grows = entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|state| ageable::can_age_up(&entity.base, state));
        if hurt {
            self.heal_mob(
                Some(player),
                target,
                camel::HEAL_AMOUNT,
                EntityRegainHealthRegainReason::Eating,
            );
        }
        if love {
            let uuid = self.players.get(&player).map(|p| p.profile.id.0);
            self.ask_love(player, target, uuid, None);
        }
        if grows
            && let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(target)
            && let Some(age) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::age_up(base, age, camel::AGE_UP_TICKS, false);
        }
        if !hurt && !love && !grows {
            return false;
        }
        let mut random = self.mob_random(target);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.store_mob_random(target, random);
        self.entity_sound(target, sounds(kind).eat, 1.0, pitch);
        if let Some(at) = self.entities.store.get(target).map(|e| e.base.position) {
            self.game_event(GameEvent::Eat, at, GameEventContext::of(Some(target)));
        }
        true
    }

    pub(crate) fn camel_offspring(
        &mut self,
        parent: EntityId,
        partner: EntityId,
    ) -> Option<Entity> {
        let parent_kind = self.kind_of(parent)?;
        let partner_kind = self.kind_of(partner)?;
        if !camel::can_mate(parent_kind, partner_kind) {
            return None;
        }
        Some(new_mob(self.new_entity_base(Kind::Camel)))
    }

    pub(crate) fn camel_passenger_attachment(&self, vehicle: EntityId, index: usize) -> Vec3 {
        let Some(entity) = self.entities.store.get(vehicle) else {
            return Vec3::ZERO;
        };
        let baby = entity.is_baby();
        let count = self.entities.rides.passengers_of(vehicle).len();
        let passenger = self
            .entities
            .rides
            .passengers_of(vehicle)
            .get(index)
            .copied();
        let animal = passenger
            .and_then(|id| self.entities.store.get(id))
            .is_some_and(|rider| rider.base.kind.extends("Animal"));
        let anchor = Anchor {
            scale: camel::age_scale(baby),
            front: index == 0,
            game_time: self.game_tick(),
        };
        camel::passenger_attachment(
            &entity.base,
            &anchor,
            index,
            count,
            animal,
            entity.base.is_removed(),
        )
    }
}

fn mount_state_of(
    entity: &mut Entity,
) -> Option<(
    &mut rspigot_domain::entity::EntityBase,
    &mut equine::MountState,
)> {
    let Entity {
        base,
        state: EntityState::Living(state),
        ..
    } = entity
    else {
        return None;
    };
    let mount = state.mob.as_mut()?.mount.as_deref_mut()?;
    Some((base, mount))
}

fn clamp_head_to_body(entity: &mut Entity) {
    let Some(state) = entity.living() else {
        return;
    };
    let body = state.body_yaw;
    let head = entity.base.head_yaw;
    let max = camel::MAX_HEAD_Y_ROT as f32;
    let diff = mth::wrap_degrees(body - head);
    let clamped = mth::clamp(diff, -max, max);
    entity.base.head_yaw = head + diff - clamped;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::{Block, Stone};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::entity::spawn::Jockey;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos, Identifier};
    use rspigot_nbt::Compound;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::ClientEvent;
    use crate::ports::{ChunkGenerator, ChunkStore, EntityStore, StoreError};
    use crate::status::StatusBoard;

    struct Quiet;

    impl ClientLink for Quiet {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    impl ChunkStore for Quiet {
        fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(None)
        }

        fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl EntityStore for Quiet {
        fn load_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
        ) -> Result<Option<Vec<Compound>>, StoreError> {
            Ok(None)
        }

        fn save_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
            _: &[Compound],
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl ChunkGenerator for Quiet {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            let mut chunk = ChunkColumn::empty(pos, dimension.height, rspigot_domain::BiomeId(0));
            for x in 0..16 {
                for z in 0..16 {
                    for y in 60..=63 {
                        let at = BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                        chunk.set_block(at, Stone::DEFAULT_STATE);
                    }
                }
            }
            chunk
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::new(0.5, 64.0, 0.5)
        }
    }

    fn server() -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(11);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn husk_with(server: &mut Server<Quiet>, jockey: Option<Jockey>, at: Vec3) -> EntityId {
        let mut base = server.new_entity_base(Kind::Husk);
        base.position = at;
        let mut husk = new_mob(base);
        if let Some(mob) = husk.living_mut().and_then(|l| l.mob.as_mut()) {
            mob.jockey = jockey;
        }
        server.place_entity(husk).expect("the husk is placed")
    }

    #[test]
    fn a_natural_husk_sometimes_rides_a_camel_husk_with_a_parched_behind_it() {
        let mut server = server();
        let mut husks = Vec::new();
        for index in 0..200 {
            let at = Vec3::new(0.5 + f64::from(index % 10) * 3.0, 64.0, 0.5);
            husks.push(husk_with(&mut server, Some(Jockey::CamelHuskRider), at));
            if !server.entities_of(Kind::CamelHusk).is_empty() {
                break;
            }
        }
        server.tick();
        let camels = server.entities_of(Kind::CamelHusk);
        assert!(!camels.is_empty(), "a camel husk appears within the rolls");
        let camel = camels[0].id;
        let passengers = server.entities.rides.passengers_of(camel).to_vec();
        assert_eq!(passengers.len(), 2);
        assert!(husks.contains(&passengers[0]));
        assert_eq!(
            server.kind_of(passengers[1]),
            Some(Kind::Parched),
            "the second seat holds a parched"
        );
        let spear = server
            .entities
            .store
            .get(passengers[0])
            .and_then(|e| e.living())
            .and_then(|l| l.equipment(EquipmentSlot::Mainhand))
            .map(|stack| stack.item);
        assert_eq!(spear, Some(ItemKind::IronSpear.id()));
    }

    #[test]
    fn a_husk_without_the_plan_never_brings_a_camel() {
        let mut server = server();
        for index in 0..30 {
            let at = Vec3::new(0.5 + f64::from(index) * 2.0, 64.0, 8.5);
            husk_with(&mut server, None, at);
        }
        server.tick();
        assert!(server.entities_of(Kind::CamelHusk).is_empty());
    }

    fn warmed() -> Server<Quiet> {
        let mut server = server();
        for _ in 0..70 {
            server.tick();
        }
        server
    }

    fn camel_in(server: &mut Server<Quiet>, kind: Kind, at: Vec3) -> EntityId {
        let mut base = server.new_entity_base(kind);
        base.position = at;
        let camel = new_mob(base);
        server.place_entity(camel).expect("the camel is placed")
    }

    #[test]
    fn sitting_and_standing_follow_the_vanilla_pose_clock() {
        let mut server = warmed();
        let id = camel_in(&mut server, Kind::Camel, Vec3::new(0.5, 64.0, 0.5));
        let standing = server
            .entities
            .store
            .get(id)
            .unwrap()
            .base
            .dimensions
            .height;
        server.camel_action(id, CamelAction::SitDown);
        let base = &server.entities.store.get(id).unwrap().base;
        let now = server.world.level.game_time;
        assert!(camel::is_sitting(base));
        assert!((standing - base.dimensions.height - 1.43).abs() < 1e-5);
        assert!(camel::refuse_to_move(base, now));
        server.camel_action(id, CamelAction::StandUp);
        assert!(!camel::is_sitting(
            &server.entities.store.get(id).unwrap().base
        ));
        server.camel_action(id, CamelAction::SitDown);
        server.camel_action(id, CamelAction::StandUpInstantly);
        let base = &server.entities.store.get(id).unwrap().base;
        assert!(!camel::is_sitting(base));
        assert!(!camel::refuse_to_move(base, now + 1));
        assert_eq!(base.dimensions.height, standing);
    }

    #[test]
    fn the_front_rider_sits_two_blocks_up_and_half_a_block_forward() {
        let mut server = warmed();
        let id = camel_in(&mut server, Kind::Camel, Vec3::new(0.5, 64.0, 0.5));
        let front = server.camel_passenger_attachment(id, 0);
        assert!((front.y - 2.0).abs() < 1e-5);
        assert!((front.z - 0.5).abs() < 1e-5);
        assert_eq!(camel::MAX_PASSENGERS, 3);
    }

    #[test]
    fn a_dash_arms_the_cooldown_and_a_hurt_camel_stands_up_instantly() {
        let mut server = warmed();
        let id = camel_in(&mut server, Kind::Camel, Vec3::new(0.5, 64.0, 0.5));
        server.camel_start_dash(id);
        assert!(camel::is_dashing(
            &server.entities.store.get(id).unwrap().base
        ));
        let cooldown = server
            .entities
            .store
            .get(id)
            .and_then(camel_state)
            .map(|state| state.dash_cooldown);
        assert_eq!(cooldown, Some(camel::DASH_COOLDOWN));
        server.camel_action(id, CamelAction::SitDown);
        server.camel_hurt(id);
        assert!(!camel::is_sitting(
            &server.entities.store.get(id).unwrap().base
        ));
    }
}
