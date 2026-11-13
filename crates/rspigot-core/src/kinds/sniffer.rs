use rspigot_ai::{MobView, SnifferAction, SnifferFacts};
use rspigot_domain::entity::species::sniffer;
use rspigot_domain::entity::{Entity, EntityBase, ageable};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::mth;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use std::num::NonZeroU8;

use crate::events::{ClientEvent, EntityStatus};
use crate::game_events::GameEventContext;
use crate::loot::{self, LootContext};
use crate::mob_actions::new_mob;
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rand::Rng;

const DIGGING_TABLE: &str = "minecraft:gameplay/sniffer_digging";
const IDLE_SOUND: &str = "minecraft:entity.sniffer.idle";
const DROP_SEED_SOUND: &str = "minecraft:entity.sniffer.drop_seed";
const EAT_SOUND: &str = "minecraft:entity.sniffer.eat";
const EGG_PLOP_SOUND: &str = "minecraft:block.sniffer_egg.plop";
const EGG_CRACK_SOUND: &str = "minecraft:block.sniffer_egg.crack";
const EGG_HATCH_SOUND: &str = "minecraft:block.sniffer_egg.hatch";
const EGG_HATCH_BOOST: &str = "minecraft:sniffer_egg_hatch_boost";
const EGG_PARTICLE_EVENT: i32 = 3009;
const BLOCK_BREAK_EVENT: i32 = 2001;
const EGG_PICKUP_DELAY: u16 = 10;
const HATCH_READY: u32 = 2;
const HATCH_DURATION: i32 = 24_000;
const HATCH_BOOST_DURATION: i32 = 12_000;
const HATCH_STAGES: i32 = 3;
const HATCH_JITTER: i32 = 300;
const EAT_PITCH_MIN: f32 = 0.8;
const EAT_PITCH_MAX: f32 = 1.2;
const CRACK_PITCH_BASE: f32 = 0.9;
const CRACK_PITCH_SPREAD: f32 = 0.2;
const CRACK_VOLUME: f32 = 0.7;
const DIGGING_EVENT_PERIOD: i32 = 10;
const EGG_BREED_EXPERIENCE: i32 = 7;

pub(crate) fn ambient_sound(entity: &Entity) -> Option<&'static str> {
    (!sniffer::state(&entity.base).silences_ambient()).then_some(IDLE_SOUND)
}

pub(crate) fn hatch_delay(boosted: bool, jitter: i32) -> u64 {
    let base = if boosted {
        HATCH_BOOST_DURATION
    } else {
        HATCH_DURATION
    };
    (base / HATCH_STAGES + jitter) as u64
}

pub(crate) fn hatch_level(state: BlockStateId) -> u32 {
    state
        .property("hatch")
        .and_then(|hatch| hatch.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn is_ready_to_hatch(state: BlockStateId) -> bool {
    hatch_level(state) == HATCH_READY
}

fn head_block_of(base: &EntityBase) -> BlockPos {
    sniffer::head_block(base.position, mth::view_vector(base.pitch, base.yaw))
}

fn block_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn sniffer_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.sniffer = SnifferFacts {
            state: sniffer::state(&entity.base),
        };
    }

    pub(crate) fn sniffer_registered(&mut self, id: EntityId) {
        if let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut) {
            living.jump_tuning.hop = Some(0.0);
        }
    }

    pub(crate) fn sniffer_action(&mut self, id: EntityId, action: SnifferAction) {
        match action {
            SnifferAction::StartDigging => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    let at = entity.base.tick_count + sniffer::DIG_DROP_DELAY;
                    sniffer::set_drop_seed_at_tick(&mut entity.base, at);
                }
                self.push(
                    Audience::Trackers(id),
                    ClientEvent::EntityStatus {
                        entity: id,
                        status: EntityStatus::Event(sniffer::DIG_START_EVENT),
                    },
                );
            }
        }
    }

    pub(crate) fn sniffer_pre_tick(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return true;
        };
        if !sniffer::is_digging(&entity.base) {
            return true;
        }
        let tick = entity.base.tick_count;
        let due = sniffer::drop_seed_at_tick(&entity.base) == tick;
        let head = head_block_of(&entity.base);
        if tick % DIGGING_EVENT_PERIOD == 0 {
            self.game_event(
                GameEvent::EntityAction,
                block_center(head),
                GameEventContext::of(Some(id)),
            );
        }
        if due {
            self.sniffer_drop_seed(id, head);
        }
        true
    }

    fn sniffer_drop_seed(&mut self, id: EntityId, head: BlockPos) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let drops = rspigot_data::loot::table(DIGGING_TABLE).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        let corner = Vec3::new(f64::from(head.x), f64::from(head.y), f64::from(head.z));
        for stack in drops {
            let item = self.item_at_location(corner, 0.0, stack);
            if let Some(view) = self.subject_view(id) {
                self.drop_from(view, item);
            }
        }
        self.entity_sound(id, DROP_SEED_SOUND, 1.0, 1.0);
    }

    pub(crate) fn sniffer_died(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        sniffer::set_state(&mut entity.base, baby, sniffer::State::Idling);
    }

    pub(crate) fn sniffer_eat_sound(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let pitch = EAT_PITCH_MIN + random.next_float() * (EAT_PITCH_MAX - EAT_PITCH_MIN);
        self.store_mob_random(id, random);
        self.entity_sound(id, EAT_SOUND, 1.0, pitch);
    }

    pub(crate) fn sniffer_lay_egg(&mut self, parent: EntityId, partner: EntityId) {
        let Some(position) = self
            .entities
            .store
            .get(parent)
            .map(|entity| entity.base.position)
        else {
            return;
        };
        let egg = ItemStack::new(ItemKind::SnifferEgg.id(), NonZeroU8::MIN);
        let velocity = Vec3::new(
            self.rng.random::<f64>() * 0.2 - 0.1,
            0.2,
            self.rng.random::<f64>() * 0.2 - 0.1,
        );
        let item = self.item_entity(position, velocity, EGG_PICKUP_DELAY, egg);
        let mut random = self.mob_random(parent);
        let experience = random.next_int(EGG_BREED_EXPERIENCE) + 1;
        let pitch = sniffer::egg_plop_pitch(random.next_float(), random.next_float());
        self.store_mob_random(parent, random);
        self.finish_childless_breeding(parent, partner, position, experience);
        self.entity_sound(parent, EGG_PLOP_SOUND, 1.0, pitch);
        self.add_entity(item, CreatureSpawnReason::Default);
    }

    pub(crate) fn sniffer_egg_placed(&mut self, pos: BlockPos, state: BlockStateId) -> Option<u64> {
        let boosted = self.sniffer_egg_boosted(pos);
        if boosted {
            self.level_event(EGG_PARTICLE_EVENT, pos, 0);
        }
        let jitter = self.rng.random_range(0..HATCH_JITTER);
        self.game_event(
            GameEvent::BlockPlace,
            block_center(pos),
            GameEventContext::block(state),
        );
        Some(hatch_delay(boosted, jitter))
    }

    fn sniffer_egg_boosted(&self, pos: BlockPos) -> bool {
        let below = pos.offset(0, -1, 0);
        self.world
            .chunks
            .block(below)
            .and_then(BlockStateId::kind)
            .is_some_and(|kind| {
                rspigot_data::tags::tag("minecraft:block", EGG_HATCH_BOOST)
                    .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
            })
    }

    pub(crate) fn sniffer_egg_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let center = block_center(pos);
        let pitch = CRACK_PITCH_BASE + self.rng.random::<f32>() * CRACK_PITCH_SPREAD;
        if !is_ready_to_hatch(state) {
            self.play_sound(
                None,
                center,
                EGG_CRACK_SOUND,
                SoundSource::Blocks,
                CRACK_VOLUME,
                pitch,
            );
            if let Some(next) = state.with_property("hatch", &(hatch_level(state) + 1).to_string())
            {
                self.set_block(pos, next);
            }
            return;
        }
        self.play_sound(
            None,
            center,
            EGG_HATCH_SOUND,
            SoundSource::Blocks,
            CRACK_VOLUME,
            pitch,
        );
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
        self.set_block(pos, BlockStateId::AIR);
        let mut child = new_mob(self.new_entity_base(Kind::Sniffer));
        if let rspigot_domain::entity::EntityState::Living(living) = &mut child.state
            && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::set_baby(&mut child.base, ageable, true);
        }
        let yaw = mth::wrap_degrees(self.rng.random::<f32>() * 360.0);
        child.base.snap_to(center, yaw, 0.0);
        let own = LegacyRandom::new(self.rng.random());
        self.spawn_entity(
            child,
            Birth::of(CreatureSpawnReason::Breeding).with_random(own),
        );
    }
}
