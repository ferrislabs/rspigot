use rand::Rng;
use rspigot_data::variants::VariantFamily;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::ageable;
use rspigot_domain::entity::fire;
use rspigot_domain::entity::living::{self, EQUIPMENT_SLOTS, slot_index};
use rspigot_domain::entity::reward::{damage_value, max_damage, set_damage_value};
use rspigot_domain::entity::species::{self, ConversionStep, Species};
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::GameMode;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::{SpawnLevel, SpawnReason, placement, rules};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource, VoicePitch};
use rspigot_items::EquipmentSlot;
use rspigot_nbt::Compound;

use crate::commands::Target;
use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::loot::{self, LootContext};
use crate::plugins::events::{CreatureSpawnReason, EntityPotionEffectCause, EntityRemoveCause};
use crate::plugins::events::{EntityTransform, EntityTransformReason};
use crate::plugins::steps::TransformStep;
use crate::plugins::{Birth, Combuster, Continuation, EventPayload, Follow, Holder, Spawned};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ZOMBIE_IGNITE_CHANCE: f32 = 0.3;
const ZOMBIE_IGNITE_SECONDS: i32 = 2;
const HUSK_HUNGER_TICKS: i32 = 140;
const CREEPER_PRIMED: &str = "minecraft:entity.creeper.primed";
const CHICKEN_EGG: &str = "minecraft:entity.chicken.egg";
const CHICKEN_LAY: &str = "minecraft:gameplay/chicken_lay";
const AMBIENT_CHANCE: i32 = 1000;
const ANIMAL_AMBIENT_INTERVAL: i32 = 120;
const MOB_AMBIENT_INTERVAL: i32 = 80;
const OCELOT_AMBIENT_INTERVAL: i32 = 900;
const SPAWN_ITEM_DELAY: u16 = 10;
const BABY_PITCH: f32 = 1.5;
const BABY_ZOMBIE_VILLAGER_PITCH: f32 = 2.0;
const ITEM_SPREAD: f64 = 0.2;
const ITEM_LIFT: f64 = 0.2;
const BURN_IN_DAYLIGHT: &str = "minecraft:burn_in_daylight";
const BURN_LIGHT: f32 = 0.5;
const BURN_ROLL: f32 = 30.0;
const BURN_OFFSET: f32 = 0.4;
const ZOMBIFIED_NAUSEA_TICKS: u32 = 200;

pub(crate) fn type_in_tag(kind: Kind, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:entity_type", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub(crate) fn ambient_sound(entity: &Entity, game_time: i64) -> Option<&'static str> {
    let base = &entity.base;
    let mob = entity.living()?.mob.as_ref()?;
    if let Some(farm) = mob.species.variant_family() {
        return species::sound_set(base, farm, entity.is_baby())?.ambient;
    }
    match base.kind {
        Kind::Enderman => Some(if species::enderman::is_creepy(base) {
            "minecraft:entity.enderman.scream"
        } else {
            "minecraft:entity.enderman.ambient"
        }),
        Kind::Endermite => Some("minecraft:entity.endermite.ambient"),
        Kind::Sheep => Some("minecraft:entity.sheep.ambient"),
        Kind::Zombie => Some("minecraft:entity.zombie.ambient"),
        Kind::Husk => Some("minecraft:entity.husk.ambient"),
        Kind::Drowned => Some(crate::kinds::drowned_ambient_sound(entity)),
        Kind::Turtle => (!base.fluid.in_water && base.on_ground && !entity.is_baby())
            .then_some(species::turtle::AMBIENT_LAND_SOUND),
        Kind::Guardian | Kind::ElderGuardian => Some(species::guardian::ambient_sound(
            base.kind,
            base.fluid.in_water,
        )),
        Kind::ZombieVillager => Some("minecraft:entity.zombie_villager.ambient"),
        Kind::PolarBear => Some(if entity.is_baby() {
            "minecraft:entity.polar_bear.ambient_baby"
        } else {
            "minecraft:entity.polar_bear.ambient"
        }),
        Kind::ZombifiedPiglin => Some(
            if mob.neutral.is_some_and(|anger| anger.is_angry(game_time)) {
                "minecraft:entity.zombified_piglin.angry"
            } else {
                "minecraft:entity.zombified_piglin.ambient"
            },
        ),
        Kind::Skeleton => Some("minecraft:entity.skeleton.ambient"),
        Kind::Spider => Some("minecraft:entity.spider.ambient"),
        Kind::Rabbit => Some("minecraft:entity.rabbit.ambient"),
        Kind::Ocelot => Some("minecraft:entity.ocelot.ambient"),
        Kind::Silverfish => Some("minecraft:entity.silverfish.ambient"),
        Kind::SnowGolem => Some("minecraft:entity.snow_golem.ambient"),
        Kind::Blaze => Some("minecraft:entity.blaze.ambient"),
        Kind::Ghast => Some("minecraft:entity.ghast.ambient"),
        Kind::Shulker => {
            (!species::shulker::is_closed(base)).then_some("minecraft:entity.shulker.ambient")
        }
        Kind::Panda => Some(species::panda::ambient_sound(base)),
        Kind::Pillager => Some(species::pillager::AMBIENT_SOUND),
        Kind::Evoker => Some(species::evoker::AMBIENT_SOUND),
        Kind::Illusioner => Some(species::illusioner::AMBIENT_SOUND),
        Kind::Goat => Some(crate::kinds::goat_voice(
            entity,
            crate::kinds::GoatSound::Ambient,
        )),
        Kind::Armadillo => crate::kinds::armadillo_ambient_sound(entity),
        Kind::Sniffer => crate::kinds::sniffer_ambient_sound(entity),
        Kind::Frog => Some("minecraft:entity.frog.ambient"),
        Kind::Axolotl => {
            (!species::axolotl::is_playing_dead(base)).then_some(if base.fluid.in_water {
                "minecraft:entity.axolotl.idle_water"
            } else {
                "minecraft:entity.axolotl.idle_air"
            })
        }
        Kind::Nautilus => Some(match (entity.is_baby(), base.eye_in_water) {
            (true, true) => "minecraft:entity.baby_nautilus.ambient",
            (true, false) => "minecraft:entity.baby_nautilus.ambient_land",
            (false, true) => "minecraft:entity.nautilus.ambient",
            (false, false) => "minecraft:entity.nautilus.ambient_land",
        }),
        Kind::ZombieNautilus => Some(if base.eye_in_water {
            "minecraft:entity.zombie_nautilus.ambient"
        } else {
            "minecraft:entity.zombie_nautilus.ambient_land"
        }),
        Kind::Phantom => Some("minecraft:entity.phantom.ambient"),
        Kind::Horse | Kind::Donkey | Kind::Mule | Kind::SkeletonHorse | Kind::ZombieHorse => {
            crate::kinds::equine_ambient_sound(entity)
        }
        _ => None,
    }
}

pub(crate) fn variant_sound(
    entity: &Entity,
    pick: fn(&rspigot_data::variants::SoundSet) -> Option<&'static str>,
) -> Option<Option<&'static str>> {
    let mob = entity.living()?.mob.as_ref()?;
    let farm = mob.species.variant_family()?;
    let set = species::sound_set(
        &entity.base,
        farm,
        entity.is_baby() && farm != VariantFamily::Cow,
    )?;
    Some(pick(&set))
}

const HORSE_AMBIENT_INTERVAL: i32 = 400;

fn ambient_interval(kind: Kind) -> i32 {
    if kind == Kind::Ocelot {
        OCELOT_AMBIENT_INTERVAL
    } else if kind.extends("AbstractHorse") {
        HORSE_AMBIENT_INTERVAL
    } else if kind.extends("Animal") {
        ANIMAL_AMBIENT_INTERVAL
    } else {
        MOB_AMBIENT_INTERVAL
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn mob_random(&mut self, id: EntityId) -> LegacyRandom {
        self.entities.mobs.entries.get(&id).map_or_else(
            || LegacyRandom::new(self.rng.random()),
            |entry| entry.ai.brain().random.clone(),
        )
    }

    pub(crate) fn store_mob_random(&mut self, id: EntityId, random: LegacyRandom) {
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().random = random;
        }
    }

    pub(crate) fn species_pre_tick(&mut self, id: EntityId) -> bool {
        if !self.kind_pre_tick(id) {
            return false;
        }
        let now = self.current_tick() as i64;
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return true;
        };
        let alive = !living::is_dead_or_dying(base, state) && !base.is_removed();
        let no_ai = living::is_no_ai(base);
        let in_powder_snow = base.in_powder_snow;
        let mut explode = false;
        let mut primed = false;
        let mut convert = None;
        match state.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Creeper(creeper)) if alive => {
                let tick = species::creeper_tick(base, creeper);
                primed = tick.primed;
                explode = tick.explode;
            }
            Some(Species::Skeleton(skeleton)) => {
                let step = skeleton.freezing.tick(
                    base,
                    species::SKELETON_DATA_STRAY_CONVERSION,
                    in_powder_snow,
                    alive && !no_ai,
                    (
                        species::FREEZING_AFFLICTION_TICKS,
                        species::FREEZING_CONVERSION_TICKS,
                    ),
                    now,
                );
                if step == ConversionStep::Convert {
                    convert = Some((Kind::Stray, species::STRAY_CONVERSION_EVENT));
                }
            }
            _ => {}
        }
        if primed {
            self.entity_sound(id, CREEPER_PRIMED, 1.0, 0.5);
        }
        if explode {
            return !self.explode_creeper(id);
        }
        if let Some((kind, event)) = convert {
            self.convert_mob(id, kind, Some(event));
            return self.entities.store.get(id).is_some();
        }
        true
    }

    pub(crate) fn species_post_tick(&mut self, id: EntityId) {
        self.kind_post_tick(id);
        self.mob_steps_on_egg(id);
        let now = self.current_tick() as i64;
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if !matches!(base.kind, Kind::Zombie | Kind::Husk) {
            return;
        }
        let (converts_to, event) = if base.kind == Kind::Husk {
            (Kind::Zombie, species::HUSK_CONVERSION_EVENT)
        } else {
            (Kind::Drowned, species::DROWNED_CONVERSION_EVENT)
        };
        let alive = !living::is_dead_or_dying(base, state) && !base.is_removed();
        let no_ai = living::is_no_ai(base);
        let eye_in_water = base.eye_in_water;
        let Some(Species::Zombie(zombie)) = state.mob.as_mut().map(|mob| &mut mob.species) else {
            return;
        };
        let step = zombie.drowning.tick(
            base,
            species::ZOMBIE_DATA_DROWNED_CONVERSION,
            eye_in_water,
            alive && !no_ai,
            (
                species::DROWNING_AFFLICTION_TICKS,
                species::DROWNING_CONVERSION_TICKS,
            ),
            now,
        );
        if step == ConversionStep::Convert {
            self.convert_mob(id, converts_to, Some(event));
        }
    }

    pub(crate) fn mob_ambient_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        let kind = entity.base.kind;
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return;
        };
        if !alive {
            return;
        }
        let roll = entry.ai.brain_mut().random.next_int(AMBIENT_CHANCE);
        let time = entry.ambient_sound_time;
        entry.ambient_sound_time += 1;
        if roll >= time {
            return;
        }
        entry.ambient_sound_time = -ambient_interval(kind);
        self.play_ambient_sound(id);
    }

    pub(crate) fn reset_ambient_sound(&mut self, id: EntityId) {
        let kind = self.entities.store.get(id).map(|e| e.base.kind);
        if let (Some(kind), Some(entry)) = (kind, self.entities.mobs.entries.get_mut(&id)) {
            entry.ambient_sound_time = -ambient_interval(kind);
        }
    }

    pub(crate) fn play_mob_voice(
        &mut self,
        id: EntityId,
        pick: impl Fn(&Entity) -> Option<&'static str>,
    ) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_silent() {
            return;
        }
        let Some(sound) = pick(entity) else {
            return;
        };
        let baby = entity.is_baby();
        let Some(sounds) = entity.base.kind.sounds() else {
            return;
        };
        let volume = sounds.volume.get(baby, 1);
        let mut random = self.mob_random(id);
        let spread = (random.next_float() - random.next_float()) * 0.2;
        self.store_mob_random(id, random);
        let baby_pitch = if sounds.pitch == VoicePitch::ZombieVillager {
            BABY_ZOMBIE_VILLAGER_PITCH
        } else {
            BABY_PITCH
        };
        let pitch = spread + if baby { baby_pitch } else { 1.0 };
        self.entity_sound(id, sound, volume, pitch);
    }

    pub(crate) fn mob_ai_step_after(&mut self, id: EntityId) {
        self.burn_undead(id);
        let mut random = self.mob_random(id);
        let mut particles = Vec::new();
        let mut lay_egg = false;
        let mut grew_up = false;
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            let alive = !living::is_dead_or_dying(base, state);
            let baby = state
                .mob
                .as_ref()
                .and_then(|mob| mob.ageable.as_ref())
                .is_some_and(|ageable| ageable::is_baby(base, ageable));
            if let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
                particles = ageable::ai_step(base, ageable, alive, &mut random).particles;
                ageable::love_ai_step(base, ageable, &mut random);
                grew_up = ageable.take_grew_up();
            }
            if let Some(Species::Chicken(chicken)) = state.mob.as_mut().map(|mob| &mut mob.species)
            {
                species::chicken_flap(base);
                if alive && !baby && !chicken.jockey {
                    let time = chicken.egg_time.unwrap_or(0) - 1;
                    chicken.egg_time = Some(time);
                    lay_egg = time <= 0;
                }
            }
        }
        self.store_mob_random(id, random);
        for particle in particles {
            self.push(Audience::Trackers(id), ClientEvent::Particle(particle));
        }
        if lay_egg {
            self.lay_egg(id);
        }
        if grew_up && self.kind_of(id) == Some(Kind::Turtle) {
            self.turtle_grew_up(id);
        }
    }

    fn lay_egg(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let drops = rspigot_data::loot::table(CHICKEN_LAY).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        let dropped = !drops.is_empty();
        for stack in drops {
            self.drop_at_location(id, 0.0, stack, Vec3::ZERO);
        }
        let mut random = self.mob_random(id);
        if dropped {
            let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
            self.entity_sound(id, CHICKEN_EGG, 1.0, pitch);
        }
        let next = species::next_egg_time(&mut random);
        self.store_mob_random(id, random);
        if let Some(Species::Chicken(chicken)) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|e| e.living_mut())
            .and_then(|l| l.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            chicken.egg_time = Some(next);
        }
    }

    pub(crate) fn spawn_at_location(&mut self, position: Vec3, offset: f64, stack: ItemStack) {
        let item = self.item_at_location(position, offset, stack);
        self.add_entity(item, CreatureSpawnReason::Default);
    }

    pub(crate) fn item_at_location(
        &mut self,
        position: Vec3,
        offset: f64,
        stack: ItemStack,
    ) -> Entity {
        let at = Vec3::new(position.x, position.y + offset, position.z);
        let velocity = Vec3::new(
            self.rng.random::<f64>() * ITEM_SPREAD - ITEM_SPREAD / 2.0,
            ITEM_LIFT,
            self.rng.random::<f64>() * ITEM_SPREAD - ITEM_SPREAD / 2.0,
        );
        self.item_entity(at, velocity, SPAWN_ITEM_DELAY, stack)
    }

    pub(crate) fn drop_at_location(
        &mut self,
        dropper: EntityId,
        offset: f64,
        stack: ItemStack,
        push: Vec3,
    ) {
        let Some(view) = self.subject_view(dropper) else {
            return;
        };
        let mut item = self.item_at_location(view.position, offset, stack);
        item.base.velocity = item.base.velocity + push;
        self.drop_from(view, item);
    }

    fn burn_undead(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !type_in_tag(entity.base.kind, BURN_IN_DAYLIGHT) {
            return;
        }
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        if !alive
            || !rspigot_domain::difficulty::monsters_burn(
                self.world.level.day_time,
                self.world.dimension.has_skylight,
            )
        {
            return;
        }
        let light = self.light_at_eyes(entity);
        let eye = Vec3::new(
            entity.base.position.x,
            entity.base.eye_y(),
            entity.base.position.z,
        )
        .block();
        let base = &entity.base;
        let protected = base.fluid.in_water
            || self.is_in_rain(base)
            || base.in_powder_snow
            || base.was_in_powder_snow;
        if light <= BURN_LIGHT {
            return;
        }
        let protection = if entity.base.kind == Kind::ZombieHorse {
            EquipmentSlot::Body
        } else {
            EquipmentSlot::Head
        };
        let helmet = entity
            .living()
            .and_then(|state| state.equipment(protection).cloned());
        let mut random = self.mob_random(id);
        let roll = random.next_float() * BURN_ROLL;
        let burn = roll < (light - BURN_OFFSET) * 2.0
            && !protected
            && self.chunk_view().can_see_sky_at(eye);
        if !burn {
            self.store_mob_random(id, random);
            return;
        }
        match helmet {
            Some(stack) => {
                if max_damage(&stack).is_some() {
                    let amount = random.next_int(2);
                    self.store_mob_random(id, random);
                    self.damage_mob_equipment(id, protection, amount);
                } else {
                    self.store_mob_random(id, random);
                }
            }
            None => {
                self.store_mob_random(id, random);
                self.combust(Holder::Mob(id), fire::SUN_SECONDS, Combuster::Nothing);
            }
        }
    }

    pub(crate) fn zombie_attack_effects(&mut self, attacker: EntityId, target: Target) {
        let Some(entity) = self.entities.store.get(attacker) else {
            return;
        };
        let kind = entity.base.kind;
        if !matches!(
            kind,
            Kind::Zombie
                | Kind::Husk
                | Kind::Drowned
                | Kind::ZombieVillager
                | Kind::ZombifiedPiglin
        ) {
            return;
        }
        let bare = entity
            .living()
            .is_some_and(|living| living.equipment(EquipmentSlot::Mainhand).is_none());
        if !bare {
            return;
        }
        let effective = self.difficulty_at(entity.base.position.block()).effective;
        let on_fire = fire::is_on_fire(&entity.base);
        if on_fire {
            let mut random = self.mob_random(attacker);
            let ignite = random.next_float() < effective * ZOMBIE_IGNITE_CHANCE;
            self.store_mob_random(attacker, random);
            if ignite && let Some(source) = self.entity_view(attacker) {
                let holder = match target {
                    Target::Player(player) => Holder::Player(player),
                    Target::Entity(entity) => Holder::Mob(entity),
                };
                self.combust(
                    holder,
                    (ZOMBIE_IGNITE_SECONDS * effective as i32) as f32,
                    Combuster::Entity(source),
                );
            }
        }
        let living_target = match target {
            Target::Player(_) => true,
            Target::Entity(entity) => self
                .entities
                .store
                .get(entity)
                .is_some_and(|e| e.living().is_some()),
        };
        if kind == Kind::Husk
            && living_target
            && let Some(hunger) = EffectId::from_name("minecraft:hunger")
        {
            let ticks = (HUSK_HUNGER_TICKS * effective as i32).max(0) as u32;
            let effect = EffectInstance::new(hunger, EffectDuration::Ticks(ticks), 0, true);
            let cause = EntityPotionEffectCause::Attack;
            match target {
                Target::Player(player) => {
                    self.apply_effect(player, effect, Some(cause));
                }
                Target::Entity(entity) => {
                    self.apply_entity_effect(entity, effect, cause);
                }
            }
        }
    }

    pub(crate) fn damage_mob_equipment(&mut self, id: EntityId, slot: EquipmentSlot, amount: i32) {
        if amount == 0 {
            return;
        }
        let Some(living) = self.entities.store.get_mut(id).and_then(|e| e.living_mut()) else {
            return;
        };
        let Some(mut stack) = living.equipment(slot).cloned() else {
            return;
        };
        let Some(max) = max_damage(&stack) else {
            return;
        };
        let damage = damage_value(&stack) + amount;
        set_damage_value(&mut stack, damage);
        let broken = damage >= max;
        living.equipment[slot_index(slot)] = if broken { None } else { Some(stack.clone()) };
        let wire = crate::entities::wire_slot(slot);
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire, if broken { None } else { Some(stack) })],
            },
        );
        if broken {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::EquipmentBreak(wire),
                },
            );
        }
    }

    pub(crate) fn ignite_entity(&mut self, id: EntityId, seconds: f32) {
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            if let Some(entity) = self.entities.store.get_mut(id) {
                fire::ignite_for_ticks(&mut entity.base, fire::seconds_to_ticks(seconds));
            }
            return;
        };
        let burning = state.attribute("minecraft:burning_time");
        let ticks = fire::scaled_ticks(fire::seconds_to_ticks(seconds), burning);
        fire::ignite_for_ticks(base, ticks);
    }

    pub(crate) fn converting(&self, id: EntityId) -> bool {
        self.decisions.anywhere(|then| match then {
            Continuation::Spawn(step) => {
                matches!(step.birth.then, Spawned::Converted { old, .. } if old == id)
            }
            Continuation::Transform(step) => {
                matches!(step.birth.then, Spawned::Converted { old, .. } if old == id)
            }
            _ => false,
        })
    }

    pub(crate) fn convert_mob(&mut self, id: EntityId, kind: Kind, event: Option<i32>) {
        self.convert_with(id, kind, event, Follow::Nothing);
    }

    pub(crate) fn convert_with(
        &mut self,
        id: EntityId,
        kind: Kind,
        event: Option<i32>,
        follow: Follow,
    ) {
        if kind == Kind::ZombieVillager {
            self.ensure_offers(id);
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_removed() || self.converting(id) {
            return;
        }
        let old_base = entity.base.clone();
        let old_extra = entity.extra.clone();
        let baby = entity.is_baby();
        let Some(old_living) = entity.living().cloned() else {
            return;
        };
        let mut base = self.new_entity_base(kind);
        base.position = old_base.position;
        base.old_position = old_base.position;
        base.set_yaw(old_base.yaw);
        base.set_pitch(old_base.pitch);
        base.head_yaw = old_base.head_yaw;
        base.velocity = old_base.velocity;
        base.fall_distance = old_base.fall_distance;
        base.on_ground = old_base.on_ground;
        base.invulnerable = old_base.invulnerable;
        base.invulnerable_time = old_base.invulnerable_time;
        base.portal_cooldown = old_base.portal_cooldown;
        base.tags = old_base.tags.clone();
        base.custom_data = old_base.custom_data.clone();
        base.set_custom_name(old_base.custom_name().cloned());
        base.set_custom_name_visible(old_base.custom_name_visible());
        base.set_no_gravity(old_base.is_no_gravity());
        base.set_silent(old_base.is_silent());
        let mut state = living::LivingState::new(&mut base, old_base.yaw);
        fire::set_on_fire_flag(&mut base, fire::is_on_fire(&old_base));
        for slot in EQUIPMENT_SLOTS
            .into_iter()
            .filter(|_| kind != Kind::Villager)
        {
            if let Some(stack) = old_living.equipment(slot) {
                state.equipment[slot_index(slot)] = Some(stack.clone());
                if let (Some(new_mob), Some(old_mob)) = (&mut state.mob, &old_living.mob) {
                    new_mob.drop_chances[slot_index(slot)] = old_mob.drop_chances[slot_index(slot)];
                }
            }
        }
        state.damage.player_memory = old_living.damage.player_memory;
        state.hurt_time = old_living.hurt_time;
        state.body_yaw = old_living.body_yaw;
        state.absorption = old_living.absorption;
        for effect in old_living.effects.iter() {
            let _ = state.effects.apply(*effect);
        }
        let old_mob = old_living.mob.clone();
        if let (Some(new_mob), Some(old_mob)) = (&mut state.mob, &old_mob) {
            new_mob.can_pick_up_loot = old_mob.can_pick_up_loot;
            new_mob.persistence_required = old_mob.persistence_required;
            if let (Species::Zombie(new_zombie), Species::Zombie(old_zombie)) =
                (&mut new_mob.species, &old_mob.species)
                && old_zombie.can_break_doors
            {
                new_zombie.can_break_doors = true;
            }
        }
        if baby && species::is_zombie(kind) {
            species::set_zombie_baby(&mut base, &mut state.attributes, true);
        }
        if baby {
            species::hoglin::set_zoglin_baby(&mut base, &mut state.attributes, true);
        }
        living::set_left_handed(&mut base, living::is_left_handed(&old_base));
        living::set_no_ai(&mut base, living::is_no_ai(&old_base));
        if species::is_zombie(kind) {
            let difficulty = self.difficulty_at(old_base.position.block());
            let mut own = LegacyRandom::new(self.rng.random());
            rspigot_domain::entity::spawn::zombie_handle_attributes(
                &mut base,
                &mut state,
                difficulty.special_multiplier(),
                SpawnReason::Conversion,
                &mut own,
            );
        }
        let (reason, transform) = match (old_base.kind, kind) {
            (_, Kind::Stray) => (CreatureSpawnReason::Frozen, EntityTransformReason::Frozen),
            (_, Kind::Zoglin | Kind::ZombifiedPiglin) => {
                if let Some(nausea) = EffectId::from_name("minecraft:nausea") {
                    let _ = state.effects.apply(EffectInstance::new(
                        nausea,
                        EffectDuration::Ticks(ZOMBIFIED_NAUSEA_TICKS),
                        0,
                        true,
                    ));
                }
                (
                    CreatureSpawnReason::PiglinZombified,
                    EntityTransformReason::PiglinZombified,
                )
            }
            (Kind::Villager, Kind::ZombieVillager) => (
                CreatureSpawnReason::Infection,
                EntityTransformReason::Infection,
            ),
            (Kind::ZombieVillager, Kind::Villager) => {
                (CreatureSpawnReason::Cured, EntityTransformReason::Cured)
            }
            (Kind::Tadpole, Kind::Frog) => (
                CreatureSpawnReason::Metamorphosis,
                EntityTransformReason::Metamorphosis,
            ),
            _ => (CreatureSpawnReason::Drowned, EntityTransformReason::Drowned),
        };
        let mut extra = Compound::new();
        match (old_base.kind, kind) {
            (Kind::Villager, Kind::ZombieVillager) => {
                crate::kinds::carry_villager_into_zombie(
                    &old_base,
                    &old_living,
                    &mut base,
                    &mut state,
                    &mut extra,
                );
            }
            (Kind::ZombieVillager, Kind::Villager) => {
                crate::kinds::carry_zombie_into_villager(
                    &old_base,
                    &old_living,
                    &old_extra,
                    baby,
                    &mut base,
                    &mut state,
                );
            }
            _ => {}
        }
        let mut converted = Entity::new(base, EntityState::Living(Box::new(state)));
        converted.extra = extra;
        if kind == Kind::Frog && old_base.kind == Kind::Tadpole {
            let environment = self.spawn_environment(old_base.position);
            let mut own = LegacyRandom::new(self.rng.random());
            rspigot_domain::entity::spawn::finalize_spawn(
                &mut converted,
                &environment,
                SpawnReason::Conversion,
                None,
                &mut crate::mobs::ServerRandom(&mut self.rng),
                &mut own,
            );
        }
        let birth = Birth::of(reason).then(Spawned::Converted {
            old: id,
            event,
            follow,
        });
        self.ask_transform(id, converted, transform, birth);
    }

    fn ask_transform(
        &mut self,
        old: EntityId,
        converted: Entity,
        reason: EntityTransformReason,
        birth: Birth,
    ) {
        let Some(view) = self.entity_view(old) else {
            self.spawn_entity(converted, birth);
            return;
        };
        let transformed = crate::plugins::view_of_entity(&converted);
        let lane = self.lane_for(None, old);
        let step = TransformStep {
            old,
            converted,
            birth,
        };
        self.fire(lane, Continuation::Transform(Box::new(step)), move |_| {
            Some(EventPayload::EntityTransform(EntityTransform {
                entity_type: view.kind.clone(),
                entity: view,
                transform_reason: reason,
                transformed_entities: vec![transformed.clone()],
                transformed_entity: transformed,
            }))
        });
    }

    pub(crate) fn finish_transform(&mut self, step: TransformStep, cancelled: bool) {
        let TransformStep {
            old,
            converted,
            birth,
        } = step;
        if cancelled {
            if let Spawned::Converted { follow, .. } = birth.then {
                self.conversion_cancelled(old, follow);
            }
            return;
        }
        self.spawn_entity(converted, birth);
    }

    pub(crate) fn finish_conversion(&mut self, old: EntityId, event: Option<i32>, dying: bool) {
        if !dying && !self.living_entity_alive(old) {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(old) else {
            return;
        };
        let silent = entity.base.is_silent();
        let pos = entity.base.position.block();
        if let Some(living) = entity.living_mut() {
            living.equipment = Default::default();
        }
        self.remove_entity(
            old,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Transformation),
        );
        if let Some(event) = event.filter(|_| !silent) {
            self.level_event(event, pos, 0);
        }
    }

    pub(crate) fn level_event(&mut self, event: i32, pos: BlockPos, data: i32) {
        self.push(
            Audience::Nearby {
                position: Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                range: 64.0,
                except: None,
            },
            ClientEvent::LevelEvent { event, pos, data },
        );
    }

    pub(crate) fn hurt_entity(&mut self, id: EntityId, source: &DamageSource, amount: f32) -> bool {
        if let Some(player) = self
            .players
            .values()
            .find(|p| p.entity_id == id)
            .map(|p| p.id)
        {
            return self.hurt(player, source, amount);
        }
        self.hurt_mob(id, source, amount)
    }

    pub(crate) fn burn_damage(&mut self, id: EntityId) {
        self.hurt_entity(
            id,
            &DamageSource::new(DamageType::OnFire),
            fire::ON_FIRE_DAMAGE,
        );
    }

    pub(crate) fn entity_source(&self, id: EntityId) -> SoundSource {
        self.entities
            .store
            .get(id)
            .and_then(|e| e.base.kind.sounds())
            .map_or(SoundSource::Neutral, |s| s.source)
    }
}

const REINFORCEMENT_ATTEMPTS: usize = 50;
const REINFORCEMENT_MIN: i32 = 7;
const REINFORCEMENT_MAX: i32 = 40;
const REINFORCEMENT_PLAYER_RANGE: f64 = 7.0;
const CALLER_CHARGE: &str = "minecraft:reinforcement_caller_charge";
const CALLEE_CHARGE: &str = "minecraft:reinforcement_callee_charge";
const CHARGE_STEP: f64 = 0.05;
const CALLEE_CHARGE_AMOUNT: f64 = -0.050_000_000_745_058_06;

impl<L: ClientLink> Server<L> {
    pub(crate) fn zombie_reinforcements(&mut self, id: EntityId, source: &DamageSource) {
        if self.world.difficulty != rspigot_domain::world::Difficulty::Hard {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let kind = entity.base.kind;
        if !matches!(
            kind,
            Kind::Zombie
                | Kind::Husk
                | Kind::Drowned
                | Kind::ZombieVillager
                | Kind::ZombifiedPiglin
        ) {
            return;
        }
        let position = entity.base.position;
        let Some(living) = entity.living_mut() else {
            return;
        };
        let chance = living.attribute(rspigot_domain::entity::spawn::SPAWN_REINFORCEMENTS);
        let target = self.mob_target(id).or_else(|| {
            source
                .causing
                .as_ref()
                .filter(|c| c.kind.living())
                .map(|c| c.entity)
        });
        let Some(target) = target else {
            return;
        };
        let mut random = self.mob_random(id);
        let roll = f64::from(random.next_float());
        if roll >= chance || !self.world.level.spawn_monsters {
            self.store_mob_random(id, random);
            return;
        }
        let x = position.x.floor() as i32;
        let y = position.y.floor() as i32;
        let z = position.z.floor() as i32;
        for _ in 0..REINFORCEMENT_ATTEMPTS {
            let mut offset = || {
                random.next_int_between(REINFORCEMENT_MIN, REINFORCEMENT_MAX)
                    * random.next_int_between(-1, 1)
            };
            let (rx, ry, rz) = (x + offset(), y + offset(), z + offset());
            let pos = BlockPos::new(rx, ry, rz);
            if !self.reinforcement_spot(kind, pos) {
                continue;
            }
            let at = Vec3::new(f64::from(rx), f64::from(ry), f64::from(rz));
            if !self.reinforcement_fits(kind, at) {
                continue;
            }
            self.store_mob_random(id, random);
            self.spawn_reinforcement(kind, id, at, target);
            return;
        }
        self.store_mob_random(id, random);
    }

    fn reinforcement_spot(&mut self, kind: Kind, pos: BlockPos) -> bool {
        placement::is_spawn_position_ok(self, kind, pos)
            && rules::check_spawn_rules(self, kind, SpawnReason::Reinforcement, pos)
    }

    fn reinforcement_fits(&self, kind: Kind, at: Vec3) -> bool {
        let range = REINFORCEMENT_PLAYER_RANGE * REINFORCEMENT_PLAYER_RANGE;
        let near_player = self.players.values().any(|p| {
            !p.vitals.is_dead()
                && p.game_mode != GameMode::Spectator
                && p.position.distance_squared(at) < range
        });
        if near_player {
            return false;
        }
        let dimensions = kind.dimensions();
        let area = Aabb::of_size(at, dimensions.width, dimensions.height);
        self.is_unobstructed(&area)
            && rules::no_collision(self, &area)
            && !rules::contains_any_liquid(self, &area)
    }

    fn spawn_reinforcement(&mut self, kind: Kind, caller: EntityId, at: Vec3, target: EntityId) {
        let base = self.new_entity_base(kind);
        let mut zombie = crate::mob_actions::new_mob(base);
        zombie.base.snap_to(at, 0.0, 0.0);
        let environment = self.spawn_environment(at);
        let mut own = LegacyRandom::new(self.rng.random());
        {
            let mut level = crate::mobs::ServerRandom(&mut self.rng);
            rspigot_domain::entity::spawn::finalize_spawn(
                &mut zombie,
                &environment,
                SpawnReason::Reinforcement,
                None,
                &mut level,
                &mut own,
            );
        }
        if let Some(living) = zombie.living_mut() {
            living.attributes.add_or_replace_modifier(
                rspigot_domain::entity::spawn::SPAWN_REINFORCEMENTS,
                rspigot_domain::effect::AttributeModifier {
                    id: std::borrow::Cow::Borrowed(CALLEE_CHARGE),
                    amount: CALLEE_CHARGE_AMOUNT,
                    operation: rspigot_domain::effect::AttributeOperation::AddValue,
                },
            );
        }
        self.request_target(
            crate::plugins::view_of_entity(&zombie),
            target,
            rspigot_ai::TargetReason::ReinforcementTarget,
        );
        self.spawn_entity(
            zombie,
            Birth::of(CreatureSpawnReason::Reinforcements).with_random(own),
        );
        if let Some(living) = self
            .entities
            .store
            .get_mut(caller)
            .and_then(|e| e.living_mut())
        {
            let existing = living
                .attributes
                .modifier_amount(
                    rspigot_domain::entity::spawn::SPAWN_REINFORCEMENTS,
                    CALLER_CHARGE,
                )
                .unwrap_or(0.0);
            living.attributes.add_or_replace_modifier(
                rspigot_domain::entity::spawn::SPAWN_REINFORCEMENTS,
                rspigot_domain::effect::AttributeModifier {
                    id: std::borrow::Cow::Borrowed(CALLER_CHARGE),
                    amount: existing - CHARGE_STEP,
                    operation: rspigot_domain::effect::AttributeOperation::AddValue,
                },
            );
        }
    }
}
