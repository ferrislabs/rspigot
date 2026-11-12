use rspigot_blocks::Face;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectInstance, EffectKind};
use rspigot_domain::entity::living;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::raycast::BlockHit;
use rspigot_domain::entity::{Aabb, EntityState, RemovalReason, arrow, fire};
use rspigot_domain::potion::{self, PotionContents};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::SoundSource;

use crate::campfire;
use crate::health::DamageSource;

use crate::plugins::events::{
    EntityPotionEffectCause, EntityRegainHealthRegainReason, EntityRemoveCause, PotionSplash,
    PotionSplashPatch,
};
use crate::plugins::steps::{Grief, SplashHit, SplashStep};
use crate::plugins::{Continuation, EntityView, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const FIRE_TAG: &str = "minecraft:fire";
const CANDLE_TAGS: [&str; 2] = ["minecraft:candles", "minecraft:candle_cakes"];
const LIT: &str = "lit";
const CANDLE_EXTINGUISH_SOUND: &str = "minecraft:block.candle.extinguish";
const CAMPFIRE_DOUSE_EVENT: i32 = 1009;
const PVP_BLOCKED: [&str; 6] = [
    "minecraft:slowness",
    "minecraft:mining_fatigue",
    "minecraft:blindness",
    "minecraft:hunger",
    "minecraft:weakness",
    "minecraft:poison",
];
const MAX_INTENSITY: f64 = 1.0;
const EXTINGUISH_SOUND: &str = "minecraft:entity.generic.extinguish_fire";
const EXTINGUISH_VOLUME: f32 = 0.7;
const EXTINGUISH_PITCH: f32 = 1.6;
const EXTINGUISH_SPREAD: f32 = 0.4;

#[derive(Debug, Clone, Copy)]
struct Affectable {
    id: EntityId,
    bounds: Aabb,
    position: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct SplashFacts {
    hurts_water: bool,
    extinguishes: bool,
}

fn state_is_lit(state: BlockStateId) -> bool {
    state.property(LIT) == Some("true")
}

fn is_lit_candle(state: BlockStateId) -> bool {
    state_is_lit(state) && CANDLE_TAGS.iter().any(|tag| has_tag(state, tag))
}

fn is_lit_campfire(state: BlockStateId) -> bool {
    state_is_lit(state) && campfire::is_campfire(state)
}

fn douse_positions(pos: BlockPos, face: Face) -> [BlockPos; 6] {
    let (dx, dy, dz) = face.offset();
    let front = pos.offset(dx, dy, dz);
    [
        front,
        pos,
        front.offset(0, 0, -1),
        front.offset(1, 0, 0),
        front.offset(0, 0, 1),
        front.offset(-1, 0, 0),
    ]
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn potion_hit_block(&mut self, id: EntityId, hit: BlockHit) {
        let Some(stack) = self.potion_stack(id) else {
            return;
        };
        if PotionContents::of(&stack).is_in(potion::DOUSES_FIRE) {
            for pos in douse_positions(hit.pos, hit.face) {
                self.douse_fire(id, pos);
            }
        }
        self.splash_potion(
            id,
            SplashHit::Block {
                pos: hit.pos,
                face: hit.face,
            },
        );
    }

    pub(crate) fn potion_hit_entity(&mut self, id: EntityId, target: EntityId) {
        self.splash_potion(id, SplashHit::Entity(target));
    }

    fn potion_stack(&self, id: EntityId) -> Option<rspigot_domain::item::ItemStack> {
        let entity = self.entities.store.get(id)?;
        rspigot_domain::entity::thrown::item(&entity.base)
    }

    fn douse_fire(&mut self, id: EntityId, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let to = if has_tag(state, FIRE_TAG) {
            BlockStateId::AIR
        } else if is_lit_candle(state) || is_lit_campfire(state) {
            match state.with_property(LIT, "false") {
                Some(unlit) => unlit,
                None => return,
            }
        } else {
            return;
        };
        let Some(view) = self.entity_view(id) else {
            return;
        };
        self.change_block(
            view,
            pos,
            to,
            Grief::Douse {
                potion: id,
                pos,
                to,
            },
            false,
        );
    }

    pub(crate) fn finish_douse(&mut self, pos: BlockPos, to: BlockStateId) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if has_tag(state, FIRE_TAG) {
            self.destroy_block_silently(pos);
        } else if is_lit_candle(state) {
            self.set_block(pos, to);
            let at = pos.center();
            self.play_sound(
                None,
                at,
                CANDLE_EXTINGUISH_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        } else if is_lit_campfire(state) {
            self.level_event(CAMPFIRE_DOUSE_EVENT, pos, 0);
            self.set_block(pos, to);
        }
    }

    fn affectables_in(&self, area: &Aabb) -> Vec<Affectable> {
        let mut out: Vec<Affectable> = self
            .entities
            .store
            .iter()
            .filter(|entity| !entity.base.is_removed())
            .filter(|entity| {
                entity
                    .living()
                    .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
            })
            .filter(|entity| entity.base.bounding_box().intersects(area))
            .map(|entity| Affectable {
                id: entity.base.id,
                bounds: entity.base.bounding_box(),
                position: entity.base.position,
            })
            .collect();
        out.extend(
            self.players
                .values()
                .filter(|player| !player.vitals.is_dead())
                .filter_map(|player| {
                    let bounds = Aabb::of_size(player.position, 0.6, player.height() as f32);
                    bounds.intersects(area).then_some(Affectable {
                        id: player.entity_id,
                        bounds,
                        position: player.position,
                    })
                }),
        );
        out
    }

    fn entity_on_fire(&self, id: EntityId) -> bool {
        if self.players.values().any(|player| player.entity_id == id) {
            return self.fire_ticks_of(id) > 0;
        }
        self.entities
            .store
            .get(id)
            .is_some_and(|entity| fire::is_on_fire(&entity.base))
    }

    fn extinguish_entity(&mut self, id: EntityId) {
        let was_on_fire = self.entity_on_fire(id);
        if let Some(player) = self
            .players
            .values_mut()
            .find(|player| player.entity_id == id)
        {
            player.vitals.fire_ticks = player.vitals.fire_ticks.min(0);
        } else if let Some(entity) = self.entities.store.get_mut(id) {
            fire::clear_fire(&mut entity.base);
        }
        if was_on_fire && let Some((position, _)) = self.entity_eye(id) {
            let spread = self.rng_float() - self.rng_float();
            let pitch = EXTINGUISH_PITCH + spread * EXTINGUISH_SPREAD;
            let source = self.entity_source(id);
            self.play_sound(
                None,
                position,
                EXTINGUISH_SOUND,
                source,
                EXTINGUISH_VOLUME,
                pitch,
            );
        }
    }

    fn rng_float(&mut self) -> f32 {
        use rand::Rng;
        self.rng.random::<f32>()
    }

    fn affect_entities_around(&mut self, id: EntityId, facts: SplashFacts) {
        if !facts.hurts_water && !facts.extinguishes {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let area = entity.base.bounding_box().inflate_by(
            potion::NEARBY_WIDTH,
            potion::NEARBY_HEIGHT,
            potion::NEARBY_WIDTH,
        );
        let owner = self.potion_owner(id);
        let candidates = self.affectables_in(&area);
        for candidate in candidates {
            let sensitive = self
                .entities
                .store
                .get(candidate.id)
                .is_some_and(|entity| living::is_sensitive_to_water(entity.base.kind));
            let burning = self.entity_on_fire(candidate.id);
            if !(sensitive || burning) {
                continue;
            }
            if candidate.position.distance_squared(position) >= potion::SPLASH_REACH_SQR {
                continue;
            }
            if facts.hurts_water && sensitive {
                let source = self.indirect_magic(id, owner);
                self.hurt_entity(candidate.id, &source, potion::WATER_DAMAGE);
            }
            if facts.extinguishes
                && self.entity_on_fire(candidate.id)
                && self.entity_alive(candidate.id)
            {
                self.extinguish_entity(candidate.id);
            }
        }
    }

    fn entity_alive(&self, id: EntityId) -> bool {
        if let Some(player) = self.players.values().find(|player| player.entity_id == id) {
            return !player.vitals.is_dead();
        }
        self.entities.store.get(id).is_some_and(|entity| {
            entity
                .living()
                .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        })
    }

    fn potion_owner(&self, id: EntityId) -> Option<EntityId> {
        match &self.entities.store.get(id)?.state {
            EntityState::Thrown(state) => self.owner_entity(state.owner),
            _ => None,
        }
    }

    fn indirect_magic(&self, potion: EntityId, owner: Option<EntityId>) -> DamageSource {
        DamageSource {
            kind: DamageType::IndirectMagic,
            causing: owner.and_then(|owner| self.source_for(owner)),
            direct: self.mob_source(potion),
            position: None,
            knockback: None,
            direct_block: None,
        }
    }

    fn splash_potion(&mut self, id: EntityId, hit: SplashHit) {
        let Some(stack) = self.potion_stack(id) else {
            return;
        };
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let contents = PotionContents::of(&stack);
        let facts = SplashFacts {
            hurts_water: contents.is_in(potion::HURTS_WATER_SENSITIVE),
            extinguishes: contents.is_in(potion::EXTINGUISHES_ENTITIES),
        };
        let effects = contents.effects();
        let scale = potion::duration_scale(&stack);
        let color = contents.color();
        let instant = contents.has_instant_potion();
        let silent = entity.base.is_silent();
        let margin = f64::from(arrow::hit_margin(entity.base.tick_count));
        let position = entity.base.position;
        let reach = entity.base.bounding_box().inflate_by(
            potion::NEARBY_WIDTH,
            potion::NEARBY_HEIGHT,
            potion::NEARBY_WIDTH,
        );
        let bounds = entity.base.bounding_box();
        let owner = self.potion_owner(id);
        self.affect_entities_around(id, facts);
        let mut affected: Vec<(EntityId, f64)> = Vec::new();
        for candidate in self.affectables_in(&reach) {
            if let Some(intensity) = potion::splash_intensity(&bounds, &candidate.bounds, margin) {
                affected.push((candidate.id, intensity));
            }
        }
        let step = SplashStep {
            potion: id,
            owner,
            hit,
            effects,
            scale,
            affected,
            color,
            instant,
            silent,
            pos: position.block(),
        };
        self.ask_splash(step);
    }

    fn ask_splash(&mut self, step: SplashStep) {
        let lane = self.lane_for(None, step.potion);
        let potion = step.potion;
        let hit = step.hit;
        let affected = step.affected.clone();
        self.fire(lane, Continuation::Splash(Box::new(step)), move |server| {
            let view = server.entity_view(potion)?;
            let mut views = Vec::with_capacity(affected.len());
            let mut intensities = Vec::with_capacity(affected.len());
            for (id, intensity) in &affected {
                let target = server.subject_view(*id)?;
                intensities.push((target.clone(), *intensity));
                views.push(target);
            }
            let (hit_block, hit_block_face, hit_entity) = match hit {
                SplashHit::Block { pos, face } => {
                    (Some(pos), Some(crate::plugins::face_of(face)), None)
                }
                SplashHit::Entity(target) => (None, None, Some(server.subject_view(target)?)),
            };
            Some(EventPayload::PotionSplash(PotionSplash {
                affected_entities: views,
                entity_type: view.kind.clone(),
                entity: view.clone(),
                intensities,
                hit_block,
                hit_block_face,
                hit_entity,
                potion: view,
            }))
        });
    }

    pub(crate) fn finish_splash(
        &mut self,
        step: SplashStep,
        cancelled: bool,
        patch: PotionSplashPatch,
    ) {
        let SplashStep {
            potion: id,
            owner,
            effects,
            scale,
            mut affected,
            color,
            instant,
            silent,
            pos,
            ..
        } = step;
        if !cancelled {
            apply_intensity_patch(&mut affected, &patch.intensities);
            for (target, intensity) in affected {
                if !self.entity_alive(target) {
                    continue;
                }
                for effect in &effects {
                    self.splash_effect(id, owner, target, effect, intensity, scale);
                }
            }
        }
        let (event, sound) = if instant {
            (
                potion::INSTANT_SPLASH_EVENT,
                potion::INSTANT_SPLASH_SOUND_EVENT,
            )
        } else {
            (potion::SPLASH_EVENT, potion::SPLASH_SOUND_EVENT)
        };
        self.level_event(event, pos, color);
        if !silent {
            self.level_event(sound, pos, 0);
        }
        if self.entities.store.get(id).is_some() {
            self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
        }
    }

    fn splash_effect(
        &mut self,
        potion_id: EntityId,
        owner: Option<EntityId>,
        target: EntityId,
        effect: &EffectInstance,
        intensity: f64,
        scale: f32,
    ) {
        let Some(kind) = effect.effect.kind() else {
            return;
        };
        if self.blocked_by_pvp(owner, target, kind) {
            return;
        }
        if kind.instantaneous() {
            self.splash_instant(potion_id, owner, target, kind, effect.amplifier, intensity);
            return;
        }
        let duration = potion::scale_duration(effect.duration, intensity, scale);
        let scaled = EffectInstance {
            duration,
            show_icon: effect.visible,
            ..*effect
        };
        if potion::ends_within(&scaled, potion::MIN_SPLASH_DURATION) {
            return;
        }
        self.add_effect_to(target, scaled, EntityPotionEffectCause::PotionSplash);
    }

    fn blocked_by_pvp(&self, owner: Option<EntityId>, target: EntityId, kind: EffectKind) -> bool {
        if self.config.pvp {
            return false;
        }
        let Some(owner) = owner else {
            return false;
        };
        let is_player = |id: EntityId| self.players.values().any(|player| player.entity_id == id);
        is_player(owner)
            && is_player(target)
            && owner != target
            && (kind == EffectKind::InstantDamage || PVP_BLOCKED.contains(&kind.name()))
    }

    fn splash_instant(
        &mut self,
        potion_id: EntityId,
        owner: Option<EntityId>,
        target: EntityId,
        kind: EffectKind,
        amplifier: u8,
        intensity: f64,
    ) {
        let harm = match kind {
            EffectKind::InstantDamage => true,
            EffectKind::InstantHealth => false,
            _ => return,
        };
        let inverted = self.inverted_heal_and_harm(target);
        if harm == inverted {
            let amount = potion::heal_amount(amplifier, intensity);
            self.heal_entity(target, amount, EntityRegainHealthRegainReason::Magic);
        } else {
            let amount = potion::harm_amount(amplifier, intensity);
            let source = self.indirect_magic(potion_id, owner);
            self.hurt_entity(target, &source, amount);
        }
    }

    pub(crate) fn heal_entity(
        &mut self,
        id: EntityId,
        amount: f32,
        reason: EntityRegainHealthRegainReason,
    ) {
        match self
            .players
            .values()
            .find(|player| player.entity_id == id)
            .map(|player| player.id)
        {
            Some(player) => self.heal_player(player, amount, reason),
            None => self.heal_mob(None, id, amount, reason),
        }
    }

    pub(crate) fn add_effect_to(
        &mut self,
        id: EntityId,
        effect: EffectInstance,
        cause: EntityPotionEffectCause,
    ) {
        match self
            .players
            .values()
            .find(|player| player.entity_id == id)
            .map(|player| player.id)
        {
            Some(player) => {
                self.apply_effect(player, effect, Some(cause));
            }
            None => {
                self.apply_entity_effect(id, effect, cause);
            }
        }
    }
}

pub(crate) fn apply_intensity_patch(
    affected: &mut Vec<(EntityId, f64)>,
    patch: &[(EntityView, f64)],
) {
    for (view, value) in patch {
        let existing = affected.iter().position(|(id, _)| *id == view.id);
        if *value <= 0.0 {
            if let Some(index) = existing {
                affected.remove(index);
            }
        } else {
            let value = value.min(MAX_INTENSITY);
            match existing {
                Some(index) => affected[index].1 = value,
                None => affected.push((view.id, value)),
            }
        }
    }
}
