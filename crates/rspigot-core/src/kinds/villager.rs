use std::num::NonZeroU8;

use rand::Rng;
use rspigot_ai::{
    GlobalPos, MemoryModuleType, MemoryValue, MobView, PackedMemories, StackFacts, TradeView,
    VillagerAction, VillagerFacts, golem_detected,
};
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::gossip::{GossipType, ReputationEvent};
use rspigot_domain::entity::living::DEFAULT_DROP_CHANCE;
use rspigot_domain::entity::spawn::set_item_slot;
use rspigot_domain::entity::species::villager::{self, MerchantState, Profession, VillagerState};
use rspigot_domain::entity::{Entity, EntityState, Pose, SyncedValue};
use rspigot_domain::item::ItemStack;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};
use uuid::Uuid;

use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::loot::{self, LootContext};
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityPotionEffectCause};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const DATA_SLEEPING_POS: u8 = villager::DATA_SLEEPING_POS;
const SLEEP_HEIGHT: f64 = 0.6875;
const STAND_UP_LIFT: f64 = 0.1;
const BED_TOP: f64 = 0.5625;
const INVISIBILITY_POTION: &str = "minecraft:invisibility";
const TRADER_DRINK_TICKS: i32 = 32;
const TRADER_DRINK_MILK: &str = "minecraft:entity.wandering_trader.drink_milk";
const TRADER_DRINK_POTION: &str = "minecraft:entity.wandering_trader.drink_potion";
const NO_SOUND: &str = "minecraft:entity.villager.no";
const TRADE_SOUND: &str = "minecraft:entity.villager.trade";
const AMBIENT_SOUND: &str = "minecraft:entity.villager.ambient";
const BELL_SOUND: &str = "minecraft:block.bell.use";
const CROP_PLANTED_SOUND: &str = "minecraft:item.crop.plant";
const ANGRY_EVENT: i8 = 13;
const BELL_RANGE: f64 = 32.0;
const BELL_SCAN: f64 = 48.0;
const GOLEM_SCAN: f64 = 10.0;
const GOLEM_SPAWN_TRIES: i32 = 10;
const GOLEM_HORIZONTAL: i32 = 8;
const GOLEM_VERTICAL: i32 = 6;
const GOLEM_SEARCH_RADIUS: i32 = 10;
const THROW_SPEED: f64 = 0.3;
const THROW_PICKUP_DELAY: u16 = 40;
const EYE_LIFT: f64 = 0.3;
const OVERWORLD_DAY: i64 = 24_000;
const BREAD_LIMIT: i32 = 36;
const BREAD_BATCH: i32 = 3;
const BREAD_MAX: i32 = 3;
const KILL_REPUTATION: i32 = 25;
const HURT_REPUTATION: i32 = 25;

fn villager_state(entity: &Entity) -> Option<&VillagerState> {
    match entity.living()?.mob.as_ref().map(|mob| &mob.species) {
        Some(rspigot_domain::entity::species::Species::Villager(state)) => Some(state),
        _ => None,
    }
}

fn merchant_of(entity: &Entity) -> Option<&MerchantState> {
    entity.living()?.mob.as_ref()?.merchant.as_deref()
}

pub(crate) fn with_villager<R>(
    entity: &mut Entity,
    apply: impl FnOnce(
        &mut rspigot_domain::entity::EntityBase,
        &mut VillagerState,
        &mut MerchantState,
    ) -> R,
) -> Option<R> {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return None;
    };
    let mob = living.mob.as_mut()?;
    let rspigot_domain::entity::species::Species::Villager(state) = &mut mob.species else {
        return None;
    };
    let merchant = mob.merchant.as_deref_mut()?;
    Some(apply(base, state, merchant))
}

pub(crate) fn villager_facts(entity: &Entity) -> VillagerFacts {
    let (Some(state), Some(merchant)) = (villager_state(entity), merchant_of(entity)) else {
        return VillagerFacts::default();
    };
    let base = &entity.base;
    let sleeping = base.pose() == Pose::Sleeping;
    let age = entity
        .living()
        .and_then(|living| living.mob.as_ref())
        .and_then(|mob| mob.ageable.as_ref())
        .map_or(0, |ageable| ageable.age);
    let mut inventory = [None; rspigot_ai::INVENTORY_SLOTS];
    for (slot, stack) in merchant.inventory.iter().enumerate().take(inventory.len()) {
        inventory[slot] = stack.as_ref().map(|stack| StackFacts {
            item: stack.item,
            count: stack.count.get(),
        });
    }
    let (_, profession, level) = villager::villager_data(base);
    VillagerFacts {
        profession,
        level,
        xp: state.xp,
        food_level: state.food_level,
        food_points: merchant.food_points(),
        wheat: merchant.count_item(ItemKind::Wheat),
        has_farm_seeds: merchant.has_farm_seeds(),
        can_breed: state.can_breed(merchant, sleeping, age),
        sleeping,
        trading_player: merchant.trading_player,
        gossip_ready: true,
        hurt_recently: entity.living().is_some_and(|living| living.hurt_time > 0),
        inventory,
        trade_count: merchant
            .offers
            .as_ref()
            .map_or(0, rspigot_domain::entity::trading::MerchantOffers::len),
        job_site: None,
        has_potential_job_site: false,
        interaction_target: None,
        has_job_site_poi: false,
    }
}

pub(crate) fn with_peer_memory_facts(
    facts: &mut rspigot_ai::VillagerPeerFacts,
    memories: &rspigot_ai::Memories,
) {
    let mut full = VillagerFacts::default();
    with_memory_facts(&mut full, memories);
    facts.job_site = full.job_site;
    facts.has_potential_job_site = full.has_potential_job_site;
    facts.interaction_target = full.interaction_target;
}

pub(crate) fn with_memory_facts(facts: &mut VillagerFacts, memories: &rspigot_ai::Memories) {
    facts.job_site = memories
        .get(MemoryModuleType::JobSite)
        .and_then(MemoryValue::as_global_pos)
        .map(|global| global.pos);
    facts.has_potential_job_site = memories.has_value(MemoryModuleType::PotentialJobSite);
    facts.interaction_target = memories
        .get(MemoryModuleType::InteractionTarget)
        .and_then(MemoryValue::as_entity);
}

fn gift_table(profession: Profession, baby: bool) -> String {
    let name = if baby {
        "baby".to_owned()
    } else if profession == Profession::None || profession == Profession::Nitwit {
        "unemployed".to_owned()
    } else {
        profession.name().to_owned()
    };
    format!("minecraft:gameplay/hero_of_the_village/{name}_gift")
}

fn seed_block(item: ItemKind) -> Option<BlockKind> {
    Some(match item {
        ItemKind::WheatSeeds => BlockKind::Wheat,
        ItemKind::BeetrootSeeds => BlockKind::Beetroots,
        ItemKind::Carrot => BlockKind::Carrots,
        ItemKind::Potato => BlockKind::Potatoes,
        ItemKind::TorchflowerSeeds => BlockKind::TorchflowerCrop,
        ItemKind::PitcherPod => BlockKind::PitcherCrop,
        _ => return None,
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn villager_view(&self, entity: &Entity, view: &mut MobView) {
        let mut facts = villager_facts(entity);
        if let Some(brain) = self
            .entities
            .mobs
            .entries
            .get(&entity.base.id)
            .and_then(|entry| entry.ai.behavior_brain())
        {
            with_memory_facts(&mut facts, brain.memories());
        }
        view.species.villager = facts;
    }

    pub(crate) fn villager_registered(&mut self, id: EntityId, ai: &mut rspigot_ai::MobAi) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let (_, profession, _) = villager::villager_data(&entity.base);
        let baby = entity.is_baby();
        ai.rebuild_villager_brain(profession, baby);
        ai.update_villager_schedule(self.world.level.day_time, self.world.level.game_time);
    }

    pub(crate) fn refresh_villager_brain(&mut self, id: EntityId) {
        self.stop_mob_brain(id);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let (_, profession, _) = villager::villager_data(&entity.base);
        let baby = entity.is_baby();
        let (clock, now) = (self.world.level.day_time, self.world.level.game_time);
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.rebuild_villager_brain(profession, baby);
            entry.ai.update_villager_schedule(clock, now);
        }
    }

    pub(crate) fn villager_custom_ai_step(&mut self, id: EntityId) {
        self.merchant_happy_tick(id);
        if self.merchant_offers_empty(id) && self.trading_player_of(id).is_some() {
            self.merchant_stop_trading(id);
        }
        let now = self.world.level.game_time;
        let baby_now = self.entities.store.get(id).is_some_and(Entity::is_baby);
        let brain_baby = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .is_some_and(|brain| {
                brain.state().schedule() == Some(rspigot_ai::Schedule::BABY_VILLAGER)
            });
        if let Some(entity) = self.entities.store.get_mut(id) {
            let unhappy = villager::unhappy_counter(&entity.base);
            if unhappy > 0 {
                villager::set_unhappy_counter(&mut entity.base, unhappy - 1);
            }
            with_villager(entity, |_, state, merchant| {
                state.maybe_decay_gossip(&mut merchant.gossips, now);
            });
        }
        if baby_now != brain_baby {
            self.refresh_villager_brain(id);
        }
    }

    pub(crate) fn villager_ambient_voice(&mut self, id: EntityId) {
        self.play_mob_voice(id, |entity| {
            if entity.base.pose() == Pose::Sleeping {
                None
            } else if merchant_of(entity).is_some_and(|merchant| merchant.trading_player.is_some())
            {
                Some(TRADE_SOUND)
            } else {
                Some(AMBIENT_SOUND)
            }
        });
    }

    pub(crate) fn villager_interact(
        &mut self,
        player: rspigot_domain::player::PlayerId,
        target: EntityId,
        hand: crate::events::Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if held.is_some_and(|stack| stack.item == ItemKind::VillagerSpawnEgg.id()) {
            return None;
        }
        let entity = self.entities.store.get(target)?;
        let alive = entity.living().is_some_and(|living| !living.dead);
        let sleeping = entity.base.pose() == Pose::Sleeping;
        let trading = merchant_of(entity).is_some_and(|merchant| merchant.trading_player.is_some());
        if !alive || trading || sleeping {
            return None;
        }
        if entity.is_baby() {
            self.villager_unhappy(target);
            return Some(Interaction::Predicted);
        }
        let empty = self.merchant_offers_empty_after_generation(target);
        if empty && hand == crate::events::Hand::Main {
            self.villager_unhappy(target);
        }
        if empty {
            return Some(Interaction::Consume);
        }
        self.begin_trading(player, target);
        Some(Interaction::Predicted)
    }

    pub(crate) fn villager_unhappy(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            villager::set_unhappy_counter(&mut entity.base, villager::UNHAPPY_TICKS);
        }
        self.play_mob_voice(id, |_| Some(NO_SOUND));
    }

    pub(crate) fn villager_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        let attacker = source
            .causing
            .as_ref()
            .filter(|cause| cause.kind.living())
            .map(|cause| cause.entity);
        if let Some(attacker) = attacker {
            self.villager_reputation_event(id, ReputationEvent::VillagerHurt, attacker);
            let alive = self
                .entities
                .store
                .get(id)
                .and_then(Entity::living)
                .is_some_and(|living| !living.dead);
            if alive
                && self
                    .players
                    .values()
                    .any(|player| player.entity_id == attacker)
            {
                self.push(
                    Audience::Trackers(id),
                    ClientEvent::EntityStatus {
                        entity: id,
                        status: EntityStatus::Event(ANGRY_EVENT),
                    },
                );
            }
        }
        if dead {
            if let Some(murderer) = attacker {
                self.villager_tell_witnesses(id, murderer);
            }
            self.villager_release_all_pois(id);
            self.villager_stop_trading(id);
        }
    }

    fn villager_stop_trading(&mut self, id: EntityId) {
        self.merchant_stop_trading(id);
    }

    fn villager_tell_witnesses(&mut self, id: EntityId, murderer: EntityId) {
        let witnesses: Vec<EntityId> = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .and_then(|brain| {
                brain
                    .memories()
                    .get(MemoryModuleType::NearestVisibleLivingEntities)
                    .and_then(MemoryValue::as_visible)
                    .map(|visible| visible.nearby_entities().to_vec())
            })
            .unwrap_or_default();
        for witness in witnesses {
            if self.kind_of(witness) == Some(Kind::Villager) {
                self.villager_reputation_event(witness, ReputationEvent::VillagerKilled, murderer);
            }
        }
    }

    fn villager_release_all_pois(&mut self, id: EntityId) {
        let Some(brain) = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
        else {
            return;
        };
        let memories = brain.memories();
        let positions: Vec<BlockPos> = [
            MemoryModuleType::Home,
            MemoryModuleType::JobSite,
            MemoryModuleType::PotentialJobSite,
            MemoryModuleType::MeetingPoint,
        ]
        .into_iter()
        .filter_map(|memory| {
            memories
                .get(memory)
                .and_then(MemoryValue::as_global_pos)
                .map(|global| global.pos)
        })
        .collect();
        for pos in positions {
            self.world.chunks.poi_mut().release(pos);
        }
    }

    pub(crate) fn villager_reputation_event(
        &mut self,
        id: EntityId,
        event: ReputationEvent,
        source: EntityId,
    ) {
        let Some(uuid) = self.entity_uuid(source) else {
            return;
        };
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        with_villager(entity, |_, _, merchant| {
            let gossips = &mut merchant.gossips;
            match event {
                ReputationEvent::ZombieVillagerCured => {
                    gossips.add(uuid, GossipType::MajorPositive, 20);
                    gossips.add(uuid, GossipType::MinorPositive, 25);
                }
                ReputationEvent::Trade => gossips.add(uuid, GossipType::Trading, 2),
                ReputationEvent::VillagerHurt => {
                    gossips.add(uuid, GossipType::MinorNegative, HURT_REPUTATION);
                }
                ReputationEvent::VillagerKilled => {
                    gossips.add(uuid, GossipType::MajorNegative, KILL_REPUTATION);
                }
            }
        });
    }

    fn entity_uuid(&self, id: EntityId) -> Option<Uuid> {
        self.players
            .values()
            .find(|player| player.entity_id == id)
            .map(|player| player.profile.id.0)
            .or_else(|| self.entities.store.get(id).map(|entity| entity.base.uuid))
    }

    pub(crate) fn villager_action(&mut self, id: EntityId, action: VillagerAction) {
        match action {
            VillagerAction::TakePoi(pos) => {
                let poi = self.world.chunks.poi_mut();
                if let Some(kind) = poi.type_at(pos) {
                    poi.take(|candidate| candidate == kind, |_, at| at == pos, pos, 1);
                }
            }
            VillagerAction::ReleasePoi(pos) => {
                self.world.chunks.poi_mut().release(pos);
            }
            VillagerAction::SetProfession(profession) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    with_villager(entity, |base, _, merchant| {
                        let level = villager::level(base);
                        villager::set_profession(base, merchant, profession, level);
                    });
                }
                self.refresh_villager_brain(id);
            }
            VillagerAction::RefreshBrain => self.refresh_villager_brain(id),
            VillagerAction::PlayWorkSound => {
                let sound = self
                    .entities
                    .store
                    .get(id)
                    .and_then(|entity| villager::profession(&entity.base).work_sound());
                if let Some(sound) = sound {
                    self.play_mob_voice(id, move |_| Some(sound));
                }
            }
            VillagerAction::Restock => self.villager_restock(id),
            VillagerAction::WorkAtComposter(pos) => self.villager_work_at_composter(id, pos),
            VillagerAction::SetMainHand(item) => self.villager_set_main_hand(id, item),
            VillagerAction::UseBoneMeal(pos) => self.villager_use_bone_meal(id, pos),
            VillagerAction::ShowTrade(index) => self.villager_show_trade(id, index),
            VillagerAction::ThrowSlot {
                slot,
                amount,
                target,
            } => self.villager_throw_slot(id, slot, amount, target),
            VillagerAction::GiveGift(hero) => self.villager_give_gift(id, hero),
            VillagerAction::Gossip(partner) => self.villager_gossip(id, partner),
            VillagerAction::EatAndDigest => self.villager_eat(id),
            VillagerAction::EatAndDigestPartner(partner) => self.villager_eat(partner),
            VillagerAction::PartnerEvent { partner, event } => self.push(
                Audience::Trackers(partner),
                ClientEvent::EntityStatus {
                    entity: partner,
                    status: EntityStatus::Event(event),
                },
            ),
            VillagerAction::Breed { partner, bed } => self.villager_breed(id, partner, bed),
            VillagerAction::StartSleeping(pos) => self.villager_start_sleeping(id, pos),
            VillagerAction::StopSleeping => self.villager_stop_sleeping(id),
            VillagerAction::RingBell(pos) => self.villager_ring_bell(pos),
            VillagerAction::SpawnGolem {
                timestamp,
                villagers_needed,
            } => self.villager_spawn_golem_if_needed(id, timestamp, villagers_needed),
            VillagerAction::HarvestCrop(pos) => self.destroy_block_dropping(pos, Some(id)),
            VillagerAction::PlantSeed { pos, slot } => self.villager_plant(id, pos, slot),
            VillagerAction::ClearWanderTarget => self.trader_set_wander_target(id, None),
            VillagerAction::StopTrading => self.merchant_stop_trading(id),
            VillagerAction::StartUsing(item) => self.trader_start_using(id, item),
        }
    }

    fn villager_restock(&mut self, id: EntityId) {
        let now = self.world.level.game_time;
        let day = self.world.level.day_time.div_euclid(OVERWORLD_DAY);
        let before = self.offer_uses(id);
        let mut check = None;
        if let Some(entity) = self.entities.store.get_mut(id) {
            with_villager(entity, |_, state, merchant| {
                if let Some(offers) = merchant.offers.as_mut() {
                    check = Some(state.should_restock(offers, now, day));
                }
            });
        }
        let Some(check) = check else {
            return;
        };
        if check.caught_up {
            self.replenish_all(id, &before);
        }
        if check.restock {
            let before = self.offer_uses(id);
            if let Some(entity) = self.entities.store.get_mut(id) {
                with_villager(entity, |_, state, merchant| {
                    if let Some(offers) = merchant.offers.as_mut() {
                        state.restock(offers, now);
                    }
                });
            }
            self.replenish_all(id, &before);
        }
        if check.restock || check.caught_up {
            self.refresh_trading_prices(id);
        }
    }

    fn villager_set_main_hand(&mut self, id: EntityId, item: Option<rspigot_domain::ItemId>) {
        let stack = item.map(|item| ItemStack::new(item, NonZeroU8::MIN));
        if let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            set_item_slot(living, EquipmentSlot::Mainhand, stack);
        }
    }

    pub(crate) fn villager_make_bread(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let rest = with_villager(entity, |_, _, merchant| {
            if merchant.count_item(ItemKind::Bread) > BREAD_LIMIT {
                return None;
            }
            let batches = BREAD_MAX.min(merchant.count_item(ItemKind::Wheat) / BREAD_BATCH);
            if batches == 0 {
                return None;
            }
            let mut to_remove = batches * BREAD_BATCH;
            for slot in 0..merchant.inventory.len() {
                if to_remove == 0 {
                    break;
                }
                let matches = merchant.inventory[slot]
                    .as_ref()
                    .is_some_and(|stack| stack.item == ItemKind::Wheat.id());
                if matches {
                    let removed = merchant.remove_item(slot, to_remove);
                    to_remove -= removed.map_or(0, |stack| i32::from(stack.count.get()));
                }
            }
            let count = u8::try_from(batches).ok().and_then(NonZeroU8::new)?;
            merchant.add_item(ItemStack::new(ItemKind::Bread.id(), count))
        })
        .flatten();
        if let Some(rest) = rest {
            let position = self
                .entities
                .store
                .get(id)
                .map_or(Vec3::new(0.0, 0.0, 0.0), |entity| entity.base.position);
            self.spawn_item_entity(position, Vec3::new(0.0, 0.0, 0.0), 10, rest);
        }
    }

    fn villager_show_trade(&mut self, id: EntityId, index: Option<usize>) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let result = index.and_then(|index| {
            merchant_of(entity)
                .and_then(|merchant| merchant.offers.as_ref())
                .and_then(|offers| offers.get(index))
                .map(|offer| offer.result.clone())
        });
        let shown = result.is_some();
        if let Some(living) = entity.living_mut() {
            set_item_slot(living, EquipmentSlot::Mainhand, result);
            if let Some(mob) = living.mob.as_mut() {
                mob.drop_chances
                    [rspigot_domain::entity::living::slot_index(EquipmentSlot::Mainhand)] =
                    if shown { 0.0 } else { DEFAULT_DROP_CHANCE };
            }
        }
    }

    fn villager_throw_slot(&mut self, id: EntityId, slot: usize, amount: u8, target: Vec3) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let position = entity.base.position;
        let eye = entity.base.eye_height();
        let thrown = with_villager(entity, |_, _, merchant| {
            merchant.remove_item(slot, i32::from(amount))
        })
        .flatten();
        if let Some(stack) = thrown {
            self.villager_throw_item(position, f64::from(eye), stack, target);
        }
    }

    fn villager_throw_item(&mut self, from: Vec3, eye: f64, stack: ItemStack, target: Vec3) {
        let origin = Vec3::new(from.x, from.y + eye - EYE_LIFT, from.z);
        let delta = Vec3::new(target.x - from.x, target.y - from.y, target.z - from.z);
        let length = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
        let velocity = if length > 0.0 {
            Vec3::new(
                delta.x / length * THROW_SPEED,
                delta.y / length * THROW_SPEED,
                delta.z / length * THROW_SPEED,
            )
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        };
        self.spawn_item_entity(origin, velocity, THROW_PICKUP_DELAY, stack);
    }

    fn villager_give_gift(&mut self, id: EntityId, hero: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let profession = villager::profession(&entity.base);
        let baby = entity.is_baby();
        let position = entity.base.position;
        let eye = f64::from(entity.base.eye_height());
        let Some(target) = self
            .players
            .values()
            .find(|player| player.entity_id == hero)
            .map(|player| player.position)
        else {
            return;
        };
        let Some(table) = rspigot_data::loot::table(&gift_table(profession, baby)) else {
            return;
        };
        let drops = loot::table_drops(table, &LootContext::default(), &mut self.rng);
        for stack in drops {
            self.villager_throw_item(position, eye, stack, target);
        }
    }

    fn villager_gossip(&mut self, id: EntityId, partner: EntityId) {
        let now = self.world.level.game_time;
        let ready = |server: &Self, who: EntityId| {
            server
                .entities
                .store
                .get(who)
                .and_then(villager_state)
                .is_some_and(|state| state.gossip_ready(now))
        };
        if !ready(self, id) || !ready(self, partner) {
            return;
        }
        let Some(source) = self
            .entities
            .store
            .get(partner)
            .and_then(merchant_of)
            .map(|merchant| merchant.gossips.clone())
        else {
            return;
        };
        let mut random = ServerRandom(&mut self.rng);
        let changed = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| {
                with_villager(entity, |_, state, merchant| {
                    let count = merchant.gossips.transfer_from(&source, &mut random, 10);
                    state.last_gossip_time = now;
                    count
                })
            })
            .unwrap_or(0);
        if let Some(entity) = self.entities.store.get_mut(partner) {
            with_villager(entity, |_, state, _| state.last_gossip_time = now);
        }
        let _ = changed;
        self.villager_spawn_golem_if_needed(id, now, 5);
    }

    fn villager_eat(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            with_villager(entity, |_, state, merchant| {
                state.eat_and_digest_food(merchant);
            });
        }
    }

    fn villager_breed(&mut self, id: EntityId, partner: EntityId, bed: BlockPos) {
        let (Some(parent), Some(other)) = (
            self.entities.store.get(id),
            self.entities.store.get(partner),
        ) else {
            return;
        };
        let position = parent.base.position;
        let parent_type = villager::villager_data(&parent.base).0;
        let partner_type = villager::villager_data(&other.base).0;
        let roll = self.rng.random::<f64>();
        let biome_type = rspigot_domain::entity::species::zombie_villager::type_for_biome(
            self.biome_at(position.block()),
        );
        let kind = if roll >= 0.5 {
            if roll >= 0.75 {
                partner_type
            } else {
                parent_type
            }
        } else {
            biome_type
        };
        let mut child = crate::mob_actions::new_mob(self.new_entity_base(Kind::Villager));
        villager::set_villager_data(&mut child.base, kind, Profession::None, villager::MIN_LEVEL);
        villager::set_finalized(&mut child.base, true);
        if let EntityState::Living(living) = &mut child.state
            && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            rspigot_domain::entity::ageable::set_age(
                &mut child.base,
                ageable,
                rspigot_domain::entity::ageable::BABY_START_AGE,
            );
        }
        child.base.snap_to(position, 0.0, 0.0);
        let mut packed = PackedMemories::default();
        packed.entries.push((
            MemoryModuleType::Home,
            rspigot_ai::ExpirableValue::of(MemoryValue::GlobalPos(GlobalPos {
                dimension: self.world.dimension.key.as_str().to_owned(),
                pos: bed,
            })),
        ));
        child.extra.insert("Brain", packed.to_nbt());
        for parent in [id, partner] {
            if let Some(entity) = self.entities.store.get_mut(parent)
                && let Some(ageable) = entity
                    .living_mut()
                    .and_then(|living| living.mob.as_mut())
                    .and_then(|mob| mob.ageable.as_mut())
            {
                ageable.age = villager::OFFSPRING_AGE_AFTER_BREEDING;
            }
        }
        let child_id = child.base.id;
        self.add_entity(child, CreatureSpawnReason::Breeding);
        self.push(
            Audience::Trackers(child_id),
            ClientEvent::EntityStatus {
                entity: child_id,
                status: EntityStatus::Event(12),
            },
        );
    }

    fn villager_start_sleeping(&mut self, id: EntityId, pos: BlockPos) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        entity.base.set_pose(Pose::Sleeping);
        entity
            .base
            .data
            .set(DATA_SLEEPING_POS, SyncedValue::OptionalBlockPos(Some(pos)));
        entity.base.position = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + SLEEP_HEIGHT,
            f64::from(pos.z) + 0.5,
        );
        entity.base.velocity = Vec3::new(0.0, 0.0, 0.0);
        self.set_bed_occupied(pos, true);
    }

    fn villager_stop_sleeping(&mut self, id: EntityId) {
        let now = self.world.level.game_time;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let bed = entity
            .base
            .data
            .get(DATA_SLEEPING_POS)
            .and_then(|value| match value {
                SyncedValue::OptionalBlockPos(pos) => *pos,
                _ => None,
            });
        entity.base.set_pose(Pose::Standing);
        entity
            .base
            .data
            .set(DATA_SLEEPING_POS, SyncedValue::OptionalBlockPos(None));
        if let Some(pos) = bed {
            entity.base.position = Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + BED_TOP + STAND_UP_LIFT,
                f64::from(pos.z) + 0.5,
            );
        }
        if let Some(brain) = self
            .entities
            .mobs
            .entries
            .get_mut(&id)
            .and_then(|entry| entry.ai.behavior_brain_mut())
        {
            brain
                .memories_mut()
                .set(MemoryModuleType::LastWoken, MemoryValue::Long(now));
        }
        if let Some(pos) = bed {
            self.set_bed_occupied(pos, false);
        }
    }

    fn set_bed_occupied(&mut self, pos: BlockPos, occupied: bool) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.property("occupied").is_none() {
            return;
        }
        if let Some(next) = state.with_property("occupied", if occupied { "true" } else { "false" })
        {
            self.set_block(pos, next);
        }
    }

    fn villager_ring_bell(&mut self, pos: BlockPos) {
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(
            None,
            center,
            BELL_SOUND,
            rspigot_entities::SoundSource::Blocks,
            2.0,
            1.0,
        );
        let now = self.world.level.game_time;
        let heard: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.living().is_some()
                    && entity.base.position.distance_squared(center) < BELL_SCAN * BELL_SCAN
                    && entity.base.position.distance_squared(center) < BELL_RANGE * BELL_RANGE
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in heard {
            if let Some(brain) = self
                .entities
                .mobs
                .entries
                .get_mut(&id)
                .and_then(|entry| entry.ai.behavior_brain_mut())
            {
                brain
                    .memories_mut()
                    .set(MemoryModuleType::HeardBellTime, MemoryValue::Long(now));
            }
        }
    }

    fn villager_plant(&mut self, id: EntityId, pos: BlockPos, slot: usize) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let seed = with_villager(entity, |_, _, merchant| {
            let stack = merchant.inventory.get(slot)?.as_ref()?;
            let kind = stack.item.kind()?;
            let block = seed_block(kind)?;
            merchant.remove_item(slot, 1)?;
            Some(block)
        })
        .flatten();
        if let Some(block) = seed {
            self.set_block(pos, block.default_state());
            let center = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
            self.play_sound(
                None,
                center,
                CROP_PLANTED_SOUND,
                rspigot_entities::SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }

    fn villager_wants_golem(&self, id: EntityId, now: i64) -> bool {
        let Some(brain) = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
        else {
            return false;
        };
        let slept_recently = brain
            .memories()
            .get(MemoryModuleType::LastSlept)
            .and_then(MemoryValue::as_long)
            .is_some_and(|slept| now - slept < villager::GOLEM_MEMORY_TICKS);
        slept_recently
            && !brain
                .memories()
                .has_value(MemoryModuleType::GolemDetectedRecently)
    }

    fn villager_spawn_golem_if_needed(&mut self, id: EntityId, now: i64, needed: i32) {
        if !self.villager_wants_golem(id, now) {
            return;
        }
        let Some(origin) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let nearby: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::Villager)
            .filter(|entity| {
                let delta = entity.base.position - origin;
                delta.x.abs() <= GOLEM_SCAN
                    && delta.y.abs() <= GOLEM_SCAN
                    && delta.z.abs() <= GOLEM_SCAN
            })
            .map(|entity| entity.base.id)
            .filter(|villager| self.villager_wants_golem(*villager, now))
            .take(5)
            .collect();
        if (nearby.len() as i32) < needed {
            return;
        }
        let Some(position) = self.find_golem_position(origin.block()) else {
            return;
        };
        let mut entity = crate::mob_actions::new_mob(self.new_entity_base(Kind::IronGolem));
        entity.base.snap_to(position, 0.0, 0.0);
        let environment = self.spawn_environment(position);
        let mut own = rspigot_domain::random::LegacyRandom::new(self.rng.random());
        rspigot_domain::entity::spawn::finalize_spawn(
            &mut entity,
            &environment,
            rspigot_domain::spawn::SpawnReason::MobSummoned,
            None,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        self.add_entity(entity, CreatureSpawnReason::VillageDefense);
        for villager in nearby {
            if let Some(brain) = self
                .entities
                .mobs
                .entries
                .get_mut(&villager)
                .and_then(|entry| entry.ai.behavior_brain_mut())
            {
                golem_detected(brain.memories_mut());
            }
        }
    }

    fn find_golem_position(&mut self, center: BlockPos) -> Option<Vec3> {
        for _ in 0..GOLEM_SPAWN_TRIES {
            let dx = self.rng.random_range(-GOLEM_HORIZONTAL..=GOLEM_HORIZONTAL);
            let dz = self.rng.random_range(-GOLEM_HORIZONTAL..=GOLEM_HORIZONTAL);
            for dy in (-GOLEM_VERTICAL..=GOLEM_VERTICAL).rev() {
                let pos = center.offset(dx, dy, dz);
                let below = self.world.chunks.block(pos.offset(0, -1, 0))?;
                let feet = self.world.chunks.block(pos)?;
                let head = self.world.chunks.block(pos.offset(0, 1, 0))?;
                let solid_below = !below.collision_shape().is_empty();
                let clear = feet.collision_shape().is_empty()
                    && head.collision_shape().is_empty()
                    && !feet.flags().contains(rspigot_blocks::StateFlags::WATER);
                if solid_below && clear {
                    let _ = GOLEM_SEARCH_RADIUS;
                    return Some(Vec3::new(
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y),
                        f64::from(pos.z) + 0.5,
                    ));
                }
            }
        }
        None
    }
}

fn trader_state_mut(
    entity: &mut Entity,
) -> Option<&mut rspigot_domain::entity::species::wandering_trader::WanderingTraderState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(rspigot_domain::entity::species::Species::WanderingTrader(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn wandering_trader_view(&self, entity: &Entity, view: &mut MobView) {
        if let Some(rspigot_domain::entity::species::Species::WanderingTrader(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        {
            view.species.trader.wander_target = state.wander_target;
        }
        view.species.trader.trading_player =
            merchant_of(entity).and_then(|merchant| merchant.trading_player);
        view.species.trader.hurt_recently =
            entity.living().is_some_and(|living| living.hurt_time > 0);
    }

    pub(crate) fn trader_start_using(&mut self, id: EntityId, item: rspigot_ai::TraderItem) {
        let stack = match item {
            rspigot_ai::TraderItem::InvisibilityPotion => {
                rspigot_domain::potion::stack(ItemKind::Potion, INVISIBILITY_POTION)
            }
            rspigot_ai::TraderItem::MilkBucket => {
                ItemStack::new(ItemKind::MilkBucket.id(), NonZeroU8::MIN)
            }
        };
        let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        set_item_slot(living, EquipmentSlot::Mainhand, Some(stack));
        rspigot_domain::entity::living::start_using_item(base, living, EquipmentSlot::Mainhand);
    }

    pub(crate) fn trader_item_use_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.kind != Kind::WanderingTrader {
            return;
        }
        let Some(living) = entity.living() else {
            return;
        };
        let Some((slot, ticks)) = living.using_item else {
            return;
        };
        let Some(stack) = living.equipment(slot).cloned() else {
            return;
        };
        let milk = stack.item == ItemKind::MilkBucket.id();
        if ticks < TRADER_DRINK_TICKS || !(milk || stack.item == ItemKind::Potion.id()) {
            return;
        }
        if milk {
            self.remove_entity_effects(id, None, EntityPotionEffectCause::Milk);
        } else {
            for effect in rspigot_domain::potion::PotionContents::of(&stack).effects() {
                self.apply_entity_effect(id, effect, EntityPotionEffectCause::PotionDrink);
            }
        }
        let sound = if milk {
            TRADER_DRINK_MILK
        } else {
            TRADER_DRINK_POTION
        };
        self.entity_sound(id, sound, 1.0, 1.0);
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        {
            let remainder = if milk {
                ItemKind::Bucket
            } else {
                ItemKind::GlassBottle
            };
            set_item_slot(
                living,
                EquipmentSlot::Mainhand,
                Some(ItemStack::new(remainder.id(), NonZeroU8::MIN)),
            );
            rspigot_domain::entity::living::stop_using_item(base, living);
        }
    }

    pub(crate) fn trader_set_wander_target(&mut self, id: EntityId, target: Option<BlockPos>) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(trader_state_mut) {
            state.wander_target = target;
        }
    }

    pub(crate) fn wandering_trader_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let trading = merchant_of(entity).is_some_and(|merchant| merchant.trading_player.is_some());
        let expired = trader_state_mut(entity).is_some_and(|state| state.tick_despawn(trading));
        if expired {
            self.remove_entity(
                id,
                rspigot_domain::entity::RemovalReason::Discarded,
                Some(crate::plugins::events::EntityRemoveCause::Despawn),
            );
        }
    }
}

pub(crate) fn reputation_of(entity: &Entity, player: Uuid) -> i32 {
    merchant_of(entity).map_or(0, |merchant| merchant.gossips.reputation(player))
}

pub(crate) fn trade_views(entity: &Entity) -> Vec<TradeView> {
    merchant_of(entity)
        .and_then(|merchant| merchant.offers.as_ref())
        .map(|offers| {
            offers
                .iter()
                .map(|offer| TradeView {
                    cost_a: offer.cost_a.item,
                    cost_b: offer.cost_b.as_ref().map(|cost| cost.item),
                    out_of_stock: offer.is_out_of_stock(),
                })
                .collect()
        })
        .unwrap_or_default()
}
