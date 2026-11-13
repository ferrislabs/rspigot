use rand::Rng;
use rspigot_ai::{Hand, MobAction, TargetReason, is_edible_for_sheep};
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::ageable::{self, PARENT_AGE_AFTER_BREEDING, SHEEP_REGROWTH_SECONDS};
use rspigot_domain::entity::living::{self, LivingState};
use rspigot_domain::entity::species::{self, rabbit};
use rspigot_domain::entity::{Entity, EntityBase, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::commands::Target;
use crate::events::{Animation, ClientEvent, EntityStatus};
use crate::plugins::events::{
    CreatureSpawnReason, EntityBreed, EntityBreedPatch, EntityEnterLoveModePatch, EntityInteract,
    SheepRegrowWool,
};
use crate::plugins::steps::{BreedStep, Grief, LoveStep};
use crate::plugins::{Continuation, EventPayload, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const LOVE_EVENT: i8 = 18;
const BLOCK_BREAK_EVENT: i32 = 2001;
const BREED_EXPERIENCE: i32 = 7;
const TURTLE_EGG_BREAK: &str = "minecraft:entity.turtle.egg_break";

pub(crate) fn new_mob(base: EntityBase) -> Entity {
    let mut base = base;
    let living = LivingState::new(&mut base, 0.0);
    Entity::new(base, EntityState::Living(Box::new(living)))
}

fn eaten_carrot(state: BlockStateId) -> BlockStateId {
    let age: u16 = state
        .property("age")
        .and_then(|age| age.parse().ok())
        .unwrap_or(0);
    match age.checked_sub(1) {
        Some(younger) => state
            .with_property("age", &younger.to_string())
            .unwrap_or(BlockStateId::AIR),
        None => BlockStateId::AIR,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn apply_mob_action(&mut self, id: EntityId, action: MobAction) {
        match action {
            MobAction::Swing(hand) => self.mob_swing(id, hand),
            MobAction::Water(action) => self.water_action(id, action),
            MobAction::Attack(target) => {
                let target = match self.players.values().find(|p| p.entity_id == target) {
                    Some(player) => Target::Player(player.id),
                    None => Target::Entity(target),
                };
                self.do_hurt_target(id, target);
            }
            MobAction::RangedAttack { target, power } => {
                self.perform_ranged_attack(id, target, power)
            }
            MobAction::Breed(partner) => self.spawn_child_from_breeding(id, partner),
            MobAction::EntityEvent(event) => self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::Event(event),
                },
            ),
            MobAction::EatGrass => self.eat_grass(id),
            MobAction::RaidCarrot(pos) => self.raid_carrot(id, pos),
            MobAction::Alert { mob, target } => {
                let idle = self
                    .entities
                    .mobs
                    .entries
                    .get(&mob)
                    .is_some_and(|entry| entry.ai.target().is_none());
                if idle && let Some(view) = self.entity_view(mob) {
                    self.request_target(view, target, TargetReason::TargetAttackedNearbyEntity);
                }
            }
            MobAction::Particle(particle) => {
                self.push(Audience::Trackers(id), ClientEvent::Particle(particle));
            }
            MobAction::BlockSound {
                sound,
                source,
                pos,
                volume,
                pitch,
            } => {
                let at = Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                );
                self.play_sound(None, at, sound, source, volume, pitch);
            }
            MobAction::RemoveTurtleEgg(pos) => self.interact_with_egg(id, pos),
            MobAction::LevelEvent { event, pos, data } => self.level_event(event, pos, data),
            MobAction::DestroyProgress { pos, stage } => self.push(
                Audience::ChunkViewers(pos.chunk()),
                ClientEvent::BlockDestruction {
                    entity: id,
                    pos,
                    stage: stage as i8,
                },
            ),
            MobAction::Spell(action) => self.spell_action(id, action),
            MobAction::Wolf(action) => self.wolf_action(id, action),
            MobAction::Cat(action) => self.cat_action(id, action),
            MobAction::Fox(action) => self.fox_action(id, action),
            MobAction::Panda(action) => self.panda_action(id, action),
            MobAction::PolarBear(action) => self.polar_bear_action(id, action),
            MobAction::Enderman(action) => self.enderman_action(id, action),
            MobAction::Silverfish(action) => self.silverfish_action(id, action),
            MobAction::IronGolem(action) => self.iron_golem_action(id, action),
            MobAction::Blaze(action) => self.blaze_action(id, action),
            MobAction::Ghast(action) => self.ghast_action(id, action),
            MobAction::Air(action) => self.air_action(id, action),
            MobAction::Turtle(action) => self.turtle_action(id, action),
            MobAction::Guardian(action) => self.guardian_action(id, action),
            MobAction::Axolotl(action) => self.axolotl_action(id, action),
            MobAction::Nautilus(action) => self.nautilus_action(id, action),
            MobAction::Shulker(action) => self.shulker_action(id, action),
            MobAction::Raider(action) => self.raider_action(id, action),
            MobAction::Crossbow(action) => self.crossbow_action(id, action),
            MobAction::AmbientSound => self.play_ambient_sound(id),
            MobAction::Voice(sound) => self.play_mob_voice(id, move |_| Some(sound)),
            MobAction::Message { to, message } => self.brain_message(to, message),
            MobAction::Sound {
                sound,
                volume,
                pitch,
            } => self.entity_sound(id, sound, volume, pitch),
            MobAction::Goat(action) => self.goat_action(id, action),
            MobAction::Armadillo(action) => self.armadillo_action(id, action),
            MobAction::Sniffer(action) => self.sniffer_action(id, action),
            MobAction::Frog(action) => self.frog_action(id, action),
            MobAction::Breeze(action) => self.breeze_action(id, action),
            MobAction::Creaking(action) => self.creaking_action(id, action),
            MobAction::CopperGolem(action) => self.copper_golem_action(id, action),
            MobAction::Piglin(action) => self.piglin_action(id, action),
            MobAction::Villager(action) => self.villager_action(id, action),
            MobAction::Warden(action) => self.warden_action(id, action),
            MobAction::SetSitting(sitting) => self.set_sitting_pose(id, sitting),
            MobAction::TeleportTo(position) => self.teleport_mob_to(id, position),
            MobAction::ForgetAndRefreshAnger(mob) => self.forget_and_refresh_anger(mob),
            MobAction::SetMemory { mob, memory, value } => {
                if let Some(brain) = self
                    .entities
                    .mobs
                    .entries
                    .get_mut(&mob)
                    .and_then(|entry| entry.ai.behavior_brain_mut())
                {
                    brain.memories_mut().set_option(memory, value);
                }
            }
            MobAction::SetDoorOpen {
                pos,
                open,
                interact,
            } => self.mob_door_action(id, pos, open, interact),
            MobAction::BreakDoor(pos) => {
                self.set_block(pos, BlockStateId::AIR);
                self.level_event(rspigot_ai::ZOMBIE_BREAK_DOOR_EVENT, pos, 0);
                let state = self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR);
                self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
            }
        }
    }

    pub(crate) fn mob_swing(&mut self, id: EntityId, hand: Hand) {
        let started = self
            .entities
            .store
            .get_mut(id)
            .and_then(|e| e.living_mut())
            .is_some_and(living::start_swing);
        if started {
            let animation = match hand {
                Hand::Main => Animation::SwingMainArm,
                Hand::Off => Animation::SwingOffHand,
            };
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityAnimation {
                    entity: id,
                    animation,
                },
            );
        }
    }

    fn interact_with_egg(&mut self, id: EntityId, pos: BlockPos) {
        let Some(mob) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::EggBreak {
                mob: id,
                pos,
                trample: false,
            },
            move |_| {
                Some(EventPayload::EntityInteract(EntityInteract {
                    block: pos,
                    entity_type: mob.kind.clone(),
                    entity: mob,
                }))
            },
        );
    }

    pub(crate) fn break_turtle_egg(&mut self, id: EntityId, pos: BlockPos) {
        let egg = self.world.chunks.block(pos).and_then(BlockStateId::kind);
        if egg != Some(BlockKind::TurtleEgg) || self.entities.store.get(id).is_none() {
            return;
        }
        self.set_block(pos, BlockStateId::AIR);
        let mut random = self.mob_random(id);
        let poof = rspigot_ai::egg_poof(&mut random, pos);
        self.store_mob_random(id, random);
        for particle in poof {
            self.push(Audience::Trackers(id), ClientEvent::Particle(particle));
        }
        let pitch = 0.9 + self.rng.random::<f32>() * 0.2;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(None, at, TURTLE_EGG_BREAK, SoundSource::Blocks, 0.7, pitch);
    }

    fn raid_carrot(&mut self, id: EntityId, pos: BlockPos) {
        let Some(rabbit) = self.entity_view(id) else {
            return;
        };
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| state.kind() == Some(BlockKind::Carrots))
        else {
            return;
        };
        let to = eaten_carrot(state);
        let refused = !self.config.rules.mob_griefing;
        self.change_block(rabbit, pos, to, Grief::Raid { rabbit: id, pos }, refused);
    }

    pub(crate) fn finish_raid(&mut self, pos: BlockPos, rabbit: EntityId) {
        self.rabbit_ate_carrot(rabbit);
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| state.kind() == Some(BlockKind::Carrots))
        else {
            return;
        };
        let to = eaten_carrot(state);
        if !to.is_air() {
            self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
        }
        self.set_block(pos, to);
    }

    fn eat_grass(&mut self, id: EntityId) {
        let Some(sheep) = self.entity_view(id) else {
            return;
        };
        let pos = sheep.position.block();
        let refused = !self.config.rules.mob_griefing;
        let here = self.world.chunks.block(pos);
        let below = pos.offset(0, -1, 0);
        let (target, grazed) = if is_edible_for_sheep(here) {
            (pos, false)
        } else if self.world.chunks.block(below).and_then(BlockStateId::kind)
            == Some(BlockKind::GrassBlock)
        {
            (below, true)
        } else {
            return;
        };
        let change = Grief::Graze {
            sheep: id,
            pos: target,
            below: grazed,
        };
        self.change_block(sheep, target, BlockStateId::AIR, change, refused);
    }

    pub(crate) fn finish_graze(
        &mut self,
        id: EntityId,
        pos: BlockPos,
        below: bool,
        cancelled: bool,
    ) {
        if !cancelled {
            let state = self.world.chunks.block(pos);
            if below {
                if state.and_then(BlockStateId::kind) == Some(BlockKind::GrassBlock) {
                    self.level_event(
                        BLOCK_BREAK_EVENT,
                        pos,
                        i32::from(BlockKind::GrassBlock.default_state().0),
                    );
                    self.set_block(pos, BlockKind::Dirt.default_state());
                }
            } else if is_edible_for_sheep(state) {
                self.destroy_block_silently(pos);
            }
        }
        self.mob_ate(id);
    }

    pub(crate) fn destroy_block_silently(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.is_air() {
            return;
        }
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
        self.set_block(pos, BlockStateId::AIR);
    }

    fn mob_ate(&mut self, id: EntityId) {
        let Some(sheep) = self
            .entity_view(id)
            .filter(|view| view.kind == Kind::Sheep.name())
        else {
            return;
        };
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::Regrow(id), move |_| {
            Some(EventPayload::SheepRegrowWool(SheepRegrowWool {
                entity_type: sheep.kind.clone(),
                entity: sheep,
            }))
        });
    }

    pub(crate) fn regrow_wool(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if base.kind != Kind::Sheep || living::is_dead_or_dying(base, state) {
            return;
        }
        species::set_sheared(base, false);
        if let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            && ageable::can_age_up(base, ageable)
        {
            ageable::age_up(base, ageable, SHEEP_REGROWTH_SECONDS, false);
        }
    }

    pub(crate) fn breed_offspring(
        &mut self,
        parent: EntityId,
        partner: EntityId,
        random: &mut LegacyRandom,
    ) -> Option<Entity> {
        let parent_entity = self.entities.store.get(parent)?;
        let kind = parent_entity.base.kind;
        if !matches!(
            kind,
            Kind::Cow
                | Kind::Pig
                | Kind::Chicken
                | Kind::Sheep
                | Kind::Rabbit
                | Kind::Ocelot
                | Kind::Fox
                | Kind::Wolf
                | Kind::Cat
                | Kind::Panda
                | Kind::Goat
                | Kind::Armadillo
                | Kind::Bee
                | Kind::Axolotl
                | Kind::Nautilus
        ) {
            return None;
        }
        let parent_variant = parent_entity
            .living()?
            .mob
            .as_ref()?
            .species
            .farm()
            .map(|farm| (farm, species::variant(&parent_entity.base, farm)));
        let parent_color = species::wool_color(&parent_entity.base);
        let parent_fox = species::fox::variant(&parent_entity.base);
        let partner_entity = self.entities.store.get(partner)?;
        let partner_variant = partner_entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .and_then(|mob| mob.species.farm())
            .map(|farm| species::variant(&partner_entity.base, farm));
        let partner_color = species::wool_color(&partner_entity.base);
        let partner_fox = species::fox::variant(&partner_entity.base);
        let partner_kind = partner_entity.base.kind;
        let rabbit_parents = (kind == Kind::Rabbit).then(|| {
            (
                rabbit::variant(&parent_entity.base),
                rabbit::variant(&partner_entity.base),
                self.biome_at(parent_entity.base.position.block()),
            )
        });
        let base = self.new_entity_base(kind);
        let mut child = new_mob(base);
        if let (Some((own, other, biome)), EntityState::Living(state)) =
            (rabbit_parents, &mut child.state)
        {
            let fallback = {
                let mut level = crate::mobs::ServerRandom(&mut self.rng);
                rabbit::random_variant(biome, &mut level)
            };
            let chosen = rabbit::offspring_variant(fallback, own, other, random);
            rabbit::set_variant(&mut child.base, &mut state.attributes, chosen);
        }
        if let Some((farm, variant)) = parent_variant
            && partner_kind == kind
        {
            let chosen = if random.next_boolean() {
                variant
            } else {
                partner_variant.unwrap_or(variant)
            };
            species::set_variant(&mut child.base, farm, chosen);
        }
        if kind == Kind::Fox && partner_kind == kind {
            let chosen = if random.next_boolean() {
                parent_fox
            } else {
                partner_fox
            };
            species::fox::set_variant(&mut child.base, chosen);
        }
        if kind == Kind::Wolf {
            self.wolf_offspring(&mut child, parent, partner, random);
        }
        if kind == Kind::Sheep {
            let mut level = crate::mobs::ServerRandom(&mut self.rng);
            let color = species::mixed_wool_color(parent_color, partner_color, &mut level);
            species::set_wool_color(&mut child.base, color);
        }
        if kind == Kind::Cat {
            self.cat_offspring(&mut child, parent, partner, random);
        }
        if kind == Kind::Panda {
            self.panda_offspring(parent, partner, &mut child);
        }
        if kind == Kind::Goat {
            self.goat_offspring(&mut child, parent, partner, random);
        }
        if kind == Kind::Axolotl {
            self.axolotl_offspring(&mut child, parent, partner, random);
        }
        if kind == Kind::Nautilus {
            self.nautilus_offspring(&mut child, parent, random);
        }
        Some(child)
    }

    pub(crate) fn spawn_child_from_breeding(&mut self, parent: EntityId, partner: EntityId) {
        match self.kind_of(parent) {
            Some(Kind::Turtle) => {
                self.turtle_breed(parent, partner);
                return;
            }
            Some(Kind::Sniffer) => {
                self.sniffer_lay_egg(parent, partner);
                return;
            }
            Some(Kind::Frog) => {
                self.frog_breed(parent, partner);
                return;
            }
            _ => {}
        }
        let mut random = self.mob_random(parent);
        let Some(mut child) = self.breed_offspring(parent, partner, &mut random) else {
            self.store_mob_random(parent, random);
            return;
        };
        let Some(position) = self.entities.store.get(parent).map(|e| e.base.position) else {
            return;
        };
        if let EntityState::Living(state) = &mut child.state
            && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::set_baby(&mut child.base, ageable, true);
        }
        self.fox_breeding_trust(parent, partner, &mut child);
        child.base.snap_to(position, 0.0, 0.0);
        let experience = random.next_int(BREED_EXPERIENCE) + 1;
        self.store_mob_random(parent, random);
        let (Some(mother), Some(father)) = (self.entity_view(parent), self.entity_view(partner))
        else {
            return;
        };
        let baby = crate::plugins::view_of_entity(&child);
        let (bred_with, breeder) = self.breeding_facts(parent, partner);
        let lane = self.lane_for(None, parent);
        let step = BreedStep {
            parent,
            partner,
            child,
            experience,
            position,
        };
        self.fire(lane, Continuation::Breed(Box::new(step)), move |server| {
            let breeder = match breeder {
                Some(entity) => Some(server.subject_view(entity)?),
                None => None,
            };
            Some(EventPayload::EntityBreed(EntityBreed {
                bred_with: bred_with.as_ref().map(spec_of),
                breeder,
                entity_type: baby.kind.clone(),
                entity: baby,
                experience,
                father,
                mother,
            }))
        });
    }

    pub(crate) fn finish_childless_breeding(
        &mut self,
        parent: EntityId,
        partner: EntityId,
        position: Vec3,
        experience: i32,
    ) {
        for id in [parent, partner] {
            if let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(id)
                && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                ageable::set_age(base, ageable, PARENT_AGE_AFTER_BREEDING);
                ageable.reset_love();
            }
        }
        self.push(
            Audience::Trackers(parent),
            ClientEvent::EntityStatus {
                entity: parent,
                status: EntityStatus::Event(LOVE_EVENT),
            },
        );
        if self.config.rules.mob_drops && experience > 0 {
            self.spawn_orb(position, experience);
        }
    }

    fn breeding_facts(
        &self,
        parent: EntityId,
        partner: EntityId,
    ) -> (Option<ItemStack>, Option<EntityId>) {
        let love = |id: EntityId| {
            self.entities
                .store
                .get(id)
                .and_then(|e| e.living())
                .and_then(|l| l.mob.as_ref())
                .and_then(|m| m.ageable.as_ref())
                .and_then(|a| a.love.clone())
        };
        let online = |cause: Option<uuid::Uuid>| {
            cause.and_then(|uuid| {
                self.players
                    .values()
                    .find(|player| player.id.0 == uuid)
                    .map(|player| player.entity_id)
            })
        };
        let mother = love(parent);
        let breeder = online(mother.as_ref().and_then(|love| love.cause))
            .or_else(|| online(love(partner).and_then(|love| love.cause)));
        (mother.and_then(|love| love.breed_item), breeder)
    }

    pub(crate) fn finish_breed(&mut self, step: BreedStep, patch: EntityBreedPatch) {
        let BreedStep {
            parent,
            partner,
            child,
            experience,
            position,
        } = step;
        let in_love = |server: &Self, id: EntityId| {
            server.entities.store.get(id).is_some_and(|e| {
                e.living().is_some_and(|l| {
                    !living::is_dead_or_dying(&e.base, l)
                        && l.mob
                            .as_ref()
                            .and_then(|m| m.ageable.as_ref())
                            .is_some_and(ageable::AgeableState::in_love)
                })
            })
        };
        if !in_love(self, parent) || !in_love(self, partner) {
            return;
        }
        for id in [parent, partner] {
            if let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(id)
                && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                ageable::set_age(base, ageable, PARENT_AGE_AFTER_BREEDING);
                ageable.reset_love();
            }
        }
        self.push(
            Audience::Trackers(parent),
            ClientEvent::EntityStatus {
                entity: parent,
                status: EntityStatus::Event(LOVE_EVENT),
            },
        );
        let experience = patch.experience.unwrap_or(experience).max(0);
        if self.config.rules.mob_drops && experience > 0 {
            self.spawn_orb(position, experience);
        }
        self.add_entity(child, CreatureSpawnReason::Breeding);
    }

    pub(crate) fn finish_love(&mut self, step: LoveStep, patch: EntityEnterLoveModePatch) {
        let LoveStep {
            target: id,
            cause,
            item,
        } = step;
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if living::is_dead_or_dying(base, state) {
            return;
        }
        let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) else {
            return;
        };
        if ageable.age != 0
            || !ageable.can_fall_in_love()
            || (base.kind == Kind::Turtle && species::turtle::has_egg(base))
        {
            return;
        }
        let ticks = patch.ticks_in_love.unwrap_or(ageable::LOVE_TICKS);
        ageable.fall_in_love(ticks, cause, item);
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(LOVE_EVENT),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carrots(age: &str) -> BlockStateId {
        BlockKind::Carrots
            .state_with(&[("age", age)])
            .expect("carrot age")
    }

    #[test]
    fn a_nibbled_carrot_loses_one_stage() {
        assert_eq!(eaten_carrot(carrots("7")), carrots("6"));
        assert_eq!(eaten_carrot(carrots("1")), carrots("0"));
    }

    #[test]
    fn a_seedling_carrot_is_eaten_whole() {
        assert_eq!(eaten_carrot(carrots("0")), BlockStateId::AIR);
    }
}
