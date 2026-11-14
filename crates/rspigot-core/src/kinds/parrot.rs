use rspigot_ai::{MobView, parrot_imitated_sound};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::{Aabb, Entity, tamable};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::mobs::ServerRandom;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::entity::species::parrot;

const PARROT_FOOD: &str = "minecraft:parrot_food";
const PARROT_POISONOUS_FOOD: &str = "minecraft:parrot_poisonous_food";
const POISON: &str = "minecraft:poison";
const KILL_DAMAGE: f32 = f32::MAX;

impl<L: ClientLink> Server<L> {
    pub(crate) fn parrot_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn parrot_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let uuid = self.players.get(&player)?.profile.id.0;
        let entity = self.entities.store.get(target)?;
        let tame = tamable::is_tame(&entity.base);
        let owned = tamable::owner(&entity.base) == Some(uuid);
        let flying = !entity.base.on_ground;
        let invulnerable = entity.base.invulnerable;
        let ordered = entity
            .living()
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| mob.tamable)
            .is_some_and(|state| state.ordered_to_sit);
        let item = held.map(|stack| stack.item);
        if !tame && item.is_some_and(|item| item_tag(item, PARROT_FOOD)) {
            self.consume_held(player, hand, 1);
            self.parrot_eat_sound(target);
            let mut random = ServerRandom(&mut self.rng);
            let lucky = random.next_int(parrot::TAME_ODDS) == 0;
            if lucky {
                self.ask_tame(player, target);
            } else {
                self.mob_entity_event(target, crate::tamable::TAME_FAILURE_EVENT);
            }
            return Some(Interaction::Predicted);
        }
        if item.is_some_and(|item| item_tag(item, PARROT_POISONOUS_FOOD)) {
            self.consume_held(player, hand, 1);
            if let Some(poison) = EffectId::from_name(POISON) {
                let effect = EffectInstance::new(
                    poison,
                    EffectDuration::Ticks(parrot::POISON_TICKS as u32),
                    0,
                    true,
                );
                self.add_effect_to(target, effect, EntityPotionEffectCause::Food);
            }
            let creative = self.player_is_creative(player);
            if (creative || !invulnerable)
                && let Some(source) = self.source_of(player)
            {
                let source = DamageSource::by(DamageType::PlayerAttack, source);
                self.hurt_mob(target, &source, KILL_DAMAGE);
            }
            return Some(Interaction::Predicted);
        }
        if !flying && tame && owned {
            self.set_ordered_to_sit(target, !ordered);
            return Some(Interaction::Predicted);
        }
        None
    }

    fn player_is_creative(&self, player: PlayerId) -> bool {
        self.players
            .get(&player)
            .is_some_and(|player| player.game_mode == rspigot_domain::player::GameMode::Creative)
    }

    fn parrot_eat_sound(&mut self, id: EntityId) {
        let mut random = ServerRandom(&mut self.rng);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.entity_sound(id, parrot::EAT_SOUND, 1.0, pitch);
    }

    pub(crate) fn parrot_ai_step(&mut self, id: EntityId) {
        let mut random = ServerRandom(&mut self.rng);
        if random.next_int(parrot::IMITATE_ODDS) == 0 {
            self.parrot_imitate_nearby_mobs(id);
        }
    }

    pub(crate) fn parrot_imitate_nearby_mobs(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !self.living_entity_alive(id) || entity.base.is_silent() {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        if random.next_int(2) != 0 {
            return;
        }
        let area = {
            let bounds = entity.base.bounding_box();
            Aabb::new(
                bounds.min_x - parrot::IMITATE_AREA,
                bounds.min_y - parrot::IMITATE_AREA,
                bounds.min_z - parrot::IMITATE_AREA,
                bounds.max_x + parrot::IMITATE_AREA,
                bounds.max_y + parrot::IMITATE_AREA,
                bounds.max_z + parrot::IMITATE_AREA,
            )
        };
        let candidates: Vec<(Kind, bool)> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.id != id
                    && other.base.kind.extends("Mob")
                    && other.base.kind != Kind::Parrot
                    && rspigot_ai::PARROT_IMITATIONS
                        .iter()
                        .any(|(kind, _)| *kind == other.base.kind)
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| (other.base.kind, other.base.is_silent()))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        let (kind, silent) = candidates[random.next_int(candidates.len() as i32) as usize];
        if silent {
            return;
        }
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        let position = self
            .entities
            .store
            .get(id)
            .map_or(Vec3::ZERO, |entity| entity.base.position);
        let source = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.base.kind.sounds())
            .map_or(rspigot_entities::SoundSource::Neutral, |sounds| {
                sounds.source
            });
        self.play_sound(
            None,
            position,
            parrot_imitated_sound(kind),
            source,
            parrot::IMITATE_VOLUME,
            pitch,
        );
    }

    pub(crate) fn parrot_post_tick(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id)
            && !entity.base.on_ground
            && entity.base.velocity.y < 0.0
        {
            entity.base.velocity = entity.base.velocity.multiply(1.0, parrot::GLIDE_FALL, 1.0);
        }
    }

    pub(crate) fn parrot_after_hurt(&mut self, id: EntityId, dead: bool) {
        if !dead {
            self.set_ordered_to_sit(id, false);
        }
    }

    pub(crate) fn parrot_ambient_voice(&mut self, id: EntityId) {
        let peaceful = self.world.difficulty == rspigot_domain::world::Difficulty::Peaceful;
        let mut random = ServerRandom(&mut self.rng);
        let sound = if !peaceful && random.next_int(parrot::AMBIENT_IMITATE_ODDS) == 0 {
            let imitations = rspigot_ai::PARROT_IMITATIONS;
            let pick = random.next_int(imitations.len() as i32) as usize;
            imitations[pick].1
        } else {
            rspigot_ai::PARROT_AMBIENT_SOUND
        };
        self.play_mob_voice(id, |_| Some(sound));
    }
}
