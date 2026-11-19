use rand::Rng;
use rspigot_domain::entity::thrown::{self, ThrownState};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, ageable};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3, mth, potion};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::cooldown;
use crate::events::Hand;
use crate::plugins::events::{
    CreatureSpawnReason, EntityRemoveCause, ExpBottle, ExpBottlePatch, PlayerEggThrow,
    PlayerEggThrowPatch,
};
use crate::plugins::steps::{Birth, BottleStep, HatchStep, Spawned, SplashHit};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const THROW_VOLUME: f32 = 0.5;
const THROW_PITCH_BASE: f32 = 0.4;
const THROW_PITCH_SPREAD: f32 = 0.4;
const THROW_PITCH_FLOOR: f32 = 0.8;
const UNCERTAINTY: f32 = 1.0;
const EGG_HATCH_ODDS: i32 = 8;
const EGG_QUAD_ODDS: i32 = 32;
const BOTTLE_BASE: i32 = 3;
const BOTTLE_SPREAD: i32 = 5;
const BOTTLE_EFFECT: i32 = 2002;

struct Throwable {
    kind: Kind,
    sound: &'static str,
    rotation_offset: f32,
    power: f32,
}

fn throwable(item: ItemKind) -> Option<Throwable> {
    let (kind, sound, rotation_offset, power) = match item {
        ItemKind::Snowball => (Kind::Snowball, "minecraft:entity.snowball.throw", 0.0, 1.5),
        ItemKind::Egg => (Kind::Egg, "minecraft:entity.egg.throw", 0.0, 1.5),
        ItemKind::EnderPearl => (
            Kind::EnderPearl,
            "minecraft:entity.ender_pearl.throw",
            0.0,
            1.5,
        ),
        ItemKind::ExperienceBottle => (
            Kind::ExperienceBottle,
            "minecraft:entity.experience_bottle.throw",
            -20.0,
            0.7,
        ),
        ItemKind::SplashPotion => (
            Kind::SplashPotion,
            "minecraft:entity.splash_potion.throw",
            -20.0,
            0.5,
        ),
        ItemKind::LingeringPotion => (
            Kind::LingeringPotion,
            "minecraft:entity.lingering_potion.throw",
            -20.0,
            0.5,
        ),
        _ => return None,
    };
    Some(Throwable {
        kind,
        sound,
        rotation_offset,
        power,
    })
}

fn aim(yaw: f32, pitch: f32, offset: f32) -> Vec3 {
    let rad = |degrees: f32| f64::from(degrees * mth::DEG_TO_RAD);
    Vec3::new(
        f64::from(-mth::sin(rad(yaw)) * mth::cos(rad(pitch))),
        f64::from(-mth::sin(rad(pitch + offset))),
        f64::from(mth::cos(rad(yaw)) * mth::cos(rad(pitch))),
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_throwable(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item.kind() == Some(ItemKind::EnderEye) {
            return self.use_eye_of_ender(id, hand);
        }
        let Some(throwable) = stack.item.kind().and_then(throwable) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let (position, rotation, movement, on_ground, owner) = (
            player.position,
            player.rotation,
            player.known_movement,
            player.on_ground,
            player.profile.id.0,
        );
        let eye = player.eye_position().y - thrown::OWNER_EYE_OFFSET;
        let pitch =
            THROW_PITCH_BASE / (self.rng.random::<f32>() * THROW_PITCH_SPREAD + THROW_PITCH_FLOOR);
        self.play_sound(
            Some(id),
            position,
            throwable.sound,
            SoundSource::Neutral,
            THROW_VOLUME,
            pitch,
        );
        let mut base = self.new_entity_base(throwable.kind);
        let origin = Vec3::new(position.x, eye, position.z);
        base.position = origin;
        base.old_position = origin;
        let seed = self.rng.random::<i64>();
        let mut state = ThrownState::new(&mut base, seed);
        thrown::set_item(&mut base, &stack);
        state.owner = Some(owner);
        let direction = aim(rotation.yaw, rotation.pitch, throwable.rotation_offset);
        thrown::shoot(
            &mut base,
            &mut state,
            direction,
            throwable.power,
            UNCERTAINTY,
        );
        base.velocity = base.velocity
            + Vec3::new(
                movement.x,
                if on_ground { 0.0 } else { movement.y },
                movement.z,
            );
        let projectile = Entity::new(base, EntityState::Thrown(Box::new(state)));
        self.spawn_entity(
            projectile,
            Birth::of(CreatureSpawnReason::Default)
                .by(id)
                .then(Spawned::Thrown {
                    player: id,
                    hand,
                    stack,
                }),
        );
        true
    }

    pub(crate) fn finish_throw(
        &mut self,
        player: PlayerId,
        hand: Hand,
        stack: &ItemStack,
        placed: Option<EntityId>,
    ) {
        if placed.is_none() {
            self.send_inventory(player);
            return;
        }
        if self.held(player, hand).as_ref() == Some(stack) {
            self.consume_held(player, hand, 1);
        }
        if let Some(ticks) = stack.item.kind().and_then(cooldown::use_ticks) {
            self.start_cooldown(player, stack.item, ticks);
        }
        self.player_swing(player, hand, false);
        self.refresh_hands(player);
    }

    pub(crate) fn hatch_babies(
        &mut self,
        player: Option<PlayerId>,
        kind: Kind,
        count: i32,
        position: Vec3,
        yaw: f32,
    ) {
        for _ in 0..count {
            let Ok((mut entity, random)) = self.summon_entity(
                kind,
                position,
                None,
                rspigot_domain::spawn::SpawnReason::Triggered,
            ) else {
                continue;
            };
            if let EntityState::Living(state) = &mut entity.state
                && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                ageable::set_baby(&mut entity.base, ageable, true);
            }
            entity.base.snap_to(position, yaw, 0.0);
            let birth = Birth::of(CreatureSpawnReason::Egg).with_random(random);
            self.spawn_entity(
                entity,
                match player {
                    Some(player) => birth.by(player),
                    None => birth,
                },
            );
        }
    }

    pub(crate) fn thrown_owner_player(&self, id: EntityId) -> Option<PlayerId> {
        let EntityState::Thrown(state) = &self.entities.store.get(id)?.state else {
            return None;
        };
        let owner = state.owner?;
        self.players
            .values()
            .find(|player| player.profile.id.0 == owner)
            .map(|player| player.id)
    }

    pub(crate) fn egg_hit(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let (position, yaw) = (entity.base.position, entity.base.yaw);
        let hatching = self.rng.random_range(0..EGG_HATCH_ODDS) == 0;
        let count = if self.rng.random_range(0..EGG_QUAD_ODDS) == 0 {
            4
        } else {
            1
        };
        let player = self.thrown_owner_player(id);
        let step = HatchStep {
            player,
            position,
            yaw,
            hatching,
            kind: Kind::Chicken,
            count,
        };
        let Some(player) = player else {
            self.finish_hatch(step, PlayerEggThrowPatch::default());
            return;
        };
        self.fire(
            self.lane_for(Some(player), id),
            Continuation::Hatch(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerEggThrow(PlayerEggThrow {
                    egg: server.entity_view(id)?,
                    hatching,
                    hatching_type: Kind::Chicken.name().to_owned(),
                    num_hatches: count,
                    player: server.snapshot_of(player)?,
                }))
            },
        );
    }

    pub(crate) fn finish_hatch(&mut self, step: HatchStep, patch: PlayerEggThrowPatch) {
        let hatching = patch.hatching.unwrap_or(step.hatching);
        let kind = patch
            .hatching_type
            .as_deref()
            .and_then(Kind::from_name)
            .unwrap_or(step.kind);
        let count = patch.num_hatches.unwrap_or(step.count);
        if hatching {
            self.hatch_babies(step.player, kind, i32::from(count), step.position, step.yaw);
        }
    }

    pub(crate) fn bottle_hit(&mut self, id: EntityId, hit: Option<SplashHit>) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let experience = BOTTLE_BASE
            + self.rng.random_range(0..BOTTLE_SPREAD)
            + self.rng.random_range(0..BOTTLE_SPREAD);
        let step = BottleStep {
            position,
            experience,
        };
        let owner = self.thrown_owner_player(id);
        let lane = self.lane_for(owner, id);
        self.fire(lane, Continuation::Bottle(step), move |server| {
            let view = server.entity_view(id)?;
            let (hit_block, hit_block_face, hit_entity) = match hit {
                Some(SplashHit::Block { pos, face }) => {
                    (Some(pos), Some(crate::plugins::face_of(face)), None)
                }
                Some(SplashHit::Entity(target)) => (None, None, Some(server.subject_view(target)?)),
                None => (None, None, None),
            };
            Some(EventPayload::ExpBottle(ExpBottle {
                entity_type: view.kind.clone(),
                entity: view,
                experience,
                hit_block,
                hit_block_face,
                hit_entity,
                show_effect: true,
            }))
        });
        self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
    }

    pub(crate) fn finish_bottle(&mut self, step: BottleStep, patch: ExpBottlePatch) {
        let experience = patch.experience.unwrap_or(step.experience);
        if experience <= 0 {
            return;
        }
        if patch.show_effect.unwrap_or(true) {
            self.level_event(BOTTLE_EFFECT, step.position.block(), potion::DEFAULT_COLOR);
        }
        self.award_experience(step.position, experience);
    }
}
