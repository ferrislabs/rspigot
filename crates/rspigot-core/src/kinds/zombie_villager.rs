use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::ageable;
use rspigot_domain::entity::gossip::{GossipContainer, ReputationEvent};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::villager::{self, Profession};
use rspigot_domain::entity::species::zombie_villager::{self, ZombieVillagerState};
use rspigot_domain::entity::trading::MerchantOffers;
use rspigot_domain::entity::{Entity, EntityBase, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::plugins::Follow;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const CURING_BLOCKS: &str = "minecraft:speeds_up_zombie_villager_curing";
const WEAKNESS: &str = "minecraft:weakness";
const STRENGTH: &str = "minecraft:strength";
const NAUSEA: &str = "minecraft:nausea";
const CURE_START_EVENT: i8 = 16;
const CURED_NAUSEA_TICKS: u32 = 200;
const OFFERS: &str = "Offers";
const GOSSIPS: &str = "Gossips";

fn zombie_villager_mut(entity: &mut Entity) -> Option<(&mut EntityBase, &mut ZombieVillagerState)> {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return None;
    };
    match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(Species::Zombie(zombie)) => zombie.villager.as_mut().map(|state| (base, state)),
        _ => None,
    }
}

fn zombie_villager_of(entity: &Entity) -> Option<&ZombieVillagerState> {
    match entity.living()?.mob.as_ref().map(|mob| &mob.species) {
        Some(Species::Zombie(zombie)) => zombie.villager.as_ref(),
        _ => None,
    }
}

pub(crate) fn carry_villager_into_zombie(
    old_base: &EntityBase,
    old_living: &LivingState,
    base: &mut EntityBase,
    state: &mut LivingState,
    extra: &mut Compound,
) {
    let (kind, profession, level) = villager::villager_data(old_base);
    zombie_villager::set_villager_data(base, kind, profession.index(), level);
    zombie_villager::set_finalized(base, villager::is_finalized(old_base));
    let old_mob = old_living.mob.as_ref();
    let xp = match old_mob.map(|mob| &mob.species) {
        Some(Species::Villager(old)) => old.xp,
        _ => 0,
    };
    if let Some(Species::Zombie(zombie)) = state.mob.as_mut().map(|mob| &mut mob.species)
        && let Some(carried) = zombie.villager.as_mut()
    {
        carried.xp = xp;
    }
    if let Some(merchant) = old_mob.and_then(|mob| mob.merchant.as_deref()) {
        if let Some(offers) = &merchant.offers {
            extra.insert(OFFERS, offers.to_nbt());
        }
        extra.insert(GOSSIPS, merchant.gossips.to_nbt());
    }
}

pub(crate) fn carry_zombie_into_villager(
    old_base: &EntityBase,
    old_living: &LivingState,
    old_extra: &Compound,
    baby: bool,
    base: &mut EntityBase,
    state: &mut LivingState,
) {
    let (kind, profession, level) = zombie_villager::villager_data(old_base);
    villager::set_villager_data(base, kind, Profession::from_index(profession), level);
    villager::set_finalized(base, zombie_villager::is_finalized(old_base));
    let xp = match old_living.mob.as_ref().map(|mob| &mob.species) {
        Some(Species::Zombie(zombie)) => zombie.villager.map_or(0, |carried| carried.xp),
        _ => 0,
    };
    let Some(mob) = state.mob.as_mut() else {
        return;
    };
    if let Species::Villager(new) = &mut mob.species {
        new.xp = xp;
    }
    if let Some(merchant) = mob.merchant.as_deref_mut() {
        merchant.offers = old_extra.get_compound(OFFERS).map(MerchantOffers::from_nbt);
        merchant.gossips = old_extra
            .get(GOSSIPS)
            .map(GossipContainer::from_nbt)
            .unwrap_or_default();
    }
    if baby && let Some(ageable) = mob.ageable.as_mut() {
        ageable::set_baby(base, ageable, true);
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn zombie_villager_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if held?.item != ItemKind::GoldenApple.id() {
            return None;
        }
        let weakness = EffectId::from_name(WEAKNESS)?;
        let weak = self
            .entities
            .store
            .get(target)?
            .living()?
            .effects
            .get(weakness)
            .is_some();
        if !weak {
            return Some(Interaction::Consume);
        }
        self.consume_held(player, hand, 1);
        let starter = self.players.get(&player).map(|state| state.profile.id.0);
        let mut random = self.mob_random(target);
        let time =
            random.next_int(zombie_villager::CURE_SPREAD_TICKS) + zombie_villager::CURE_BASE_TICKS;
        self.store_mob_random(target, random);
        self.zombie_villager_start_converting(target, starter, time);
        Some(Interaction::ServerOnly)
    }

    pub(crate) fn zombie_villager_start_converting(
        &mut self,
        id: EntityId,
        starter: Option<uuid::Uuid>,
        time: i32,
    ) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some((base, state)) = zombie_villager_mut(entity) else {
            return;
        };
        state.start_converting(base, starter, time);
        self.remove_entity_effects(
            id,
            EffectId::from_name(WEAKNESS),
            EntityPotionEffectCause::Conversion,
        );
        if let Some(strength) = EffectId::from_name(STRENGTH) {
            let amplifier = 0;
            self.apply_entity_effect(
                id,
                EffectInstance::new(
                    strength,
                    EffectDuration::Ticks(time.max(0) as u32),
                    amplifier,
                    true,
                ),
                EntityPotionEffectCause::Conversion,
            );
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(CURE_START_EVENT),
            },
        );
    }

    pub(crate) fn zombie_villager_pre_tick(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return true;
        };
        let alive = !entity.base.is_removed()
            && entity.living().is_some_and(|living| {
                !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
            });
        let converting =
            zombie_villager_of(entity).is_some_and(|state| state.is_converting(&entity.base));
        if !alive || !converting || self.converting(id) {
            return true;
        }
        let position = entity.base.position;
        let mut random = self.mob_random(id);
        let chunks = &self.world.chunks;
        let progress = zombie_villager::conversion_progress(position, &mut random, |pos| {
            chunks
                .block(pos)
                .is_some_and(|state| has_tag(state, CURING_BLOCKS))
        });
        self.store_mob_random(id, random);
        let finished = self
            .entities
            .store
            .get_mut(id)
            .and_then(zombie_villager_mut)
            .is_some_and(|(_, state)| state.advance_conversion(progress));
        if finished {
            self.finish_cure(id);
        }
        true
    }

    fn finish_cure(&mut self, id: EntityId) {
        let starter = self
            .entities
            .store
            .get(id)
            .and_then(zombie_villager_of)
            .and_then(|state| state.conversion_starter);
        self.convert_with(
            id,
            Kind::Villager,
            Some(CURE_EVENT),
            Follow::Cure { starter },
        );
    }

    pub(crate) fn conversion_cancelled(&mut self, old: EntityId, follow: Follow) {
        if matches!(follow, Follow::Infection { .. }) {
            self.die_mob(old, &DamageSource::new(DamageType::Generic));
            return;
        }
        if matches!(follow, Follow::Metamorphosis) {
            self.tadpole_conversion_failed(old);
            return;
        }
        if !matches!(follow, Follow::Cure { .. }) {
            return;
        }
        if let Some((base, state)) = self
            .entities
            .store
            .get_mut(old)
            .and_then(zombie_villager_mut)
        {
            state.stop_converting(base);
        }
    }

    pub(crate) fn follow_conversion(
        &mut self,
        old: EntityId,
        placed: Option<EntityId>,
        follow: Follow,
    ) {
        match follow {
            Follow::Nothing => {}
            Follow::Metamorphosis => self.tadpole_metamorphosed(old, placed),
            Follow::Infection { at } => {
                if let Some(pos) = at {
                    self.level_event(INFECTION_EVENT, pos, 0);
                }
            }
            Follow::Cure { starter } => {
                self.drop_cured_equipment(old);
                let Some(new) = placed else {
                    self.conversion_cancelled(old, follow);
                    return;
                };
                if let Some(nausea) = EffectId::from_name(NAUSEA) {
                    self.apply_entity_effect(
                        new,
                        EffectInstance::new(
                            nausea,
                            EffectDuration::Ticks(CURED_NAUSEA_TICKS),
                            0,
                            true,
                        ),
                        EntityPotionEffectCause::Conversion,
                    );
                }
                let player = starter.and_then(|uuid| {
                    self.players
                        .values()
                        .find(|candidate| candidate.profile.id.0 == uuid)
                        .map(|candidate| candidate.entity_id)
                });
                if let Some(player) = player {
                    self.villager_reputation_event(
                        new,
                        ReputationEvent::ZombieVillagerCured,
                        player,
                    );
                }
            }
        }
    }

    fn drop_cured_equipment(&mut self, old: EntityId) {
        let Some(entity) = self.entities.store.get(old) else {
            return;
        };
        let position = entity.base.position;
        let stacks: Vec<ItemStack> = rspigot_domain::entity::living::EQUIPMENT_SLOTS
            .into_iter()
            .filter_map(|slot| entity.living()?.equipment(slot).cloned())
            .collect();
        for stack in stacks {
            self.spawn_item_entity(position, Vec3::new(0.0, 0.0, 0.0), 10, stack);
        }
    }

    pub(crate) fn zombie_infects_villager(
        &mut self,
        victim: EntityId,
        source: &DamageSource,
    ) -> bool {
        let Some(killer) = source.causing.as_ref().map(|cause| cause.entity) else {
            return false;
        };
        let Some(kind) = self.kind_of(killer) else {
            return false;
        };
        if !(kind == Kind::Zombie || kind.extends("Zombie"))
            || self.kind_of(victim) != Some(Kind::Villager)
        {
            return false;
        }
        let difficulty = self.world.difficulty;
        if difficulty != Difficulty::Normal && difficulty != Difficulty::Hard {
            return false;
        }
        if difficulty != Difficulty::Hard {
            let mut random = self.mob_random(killer);
            let skip = random.next_boolean();
            self.store_mob_random(killer, random);
            if skip {
                return false;
            }
        }
        let (silent, at) = self
            .entities
            .store
            .get(killer)
            .map_or((true, None), |entity| {
                (entity.base.is_silent(), Some(entity.base.position.block()))
            });
        let follow = Follow::Infection {
            at: at.filter(|_| !silent),
        };
        self.convert_with(victim, Kind::ZombieVillager, None, follow);
        self.infection_pending(victim)
    }

    fn infection_pending(&self, victim: EntityId) -> bool {
        self.converting(victim)
            || self
                .entities
                .store
                .get(victim)
                .is_none_or(|entity| entity.base.is_removed())
    }
}

const CURE_EVENT: i32 = 1027;
const INFECTION_EVENT: i32 = 1026;
