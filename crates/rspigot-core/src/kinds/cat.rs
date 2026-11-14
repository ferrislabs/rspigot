use rand::Rng;
use rspigot_ai::{CatAction, MobView};
use rspigot_data::variants::VariantFamily;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::species::{self, cat};
use rspigot_domain::entity::{Aabb, Entity, EntityState, tamable};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::rules;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::Tag;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, dye_color, item_tag};
use crate::mobs::ServerRandom;
use crate::plugins::events::EntityRegainHealthRegainReason;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::spawning::LEASH;

const CAT_FOOD: &str = "minecraft:cat_food";
const CAT_COLLAR_DYES: &str = "minecraft:cat_collar_dyes";
const CAT_DOES_NOT_TELEPORT_TO: &str = "minecraft:cat_does_not_teleport_to";
const ENTITIES_CAN_TELEPORT_TO: &str = "minecraft:entities_can_teleport_to";
const MORNING_GIFT: &str = "minecraft:gameplay/cat_morning_gift";
const BEG_INTERVAL: i32 = 100;
const PURR_INTERVAL: i32 = 5;
const TAME_ODDS: i32 = 3;
const GIFT_SPREAD: i32 = 11;
const GIFT_VERTICAL: i32 = 5;

impl<L: ClientLink> Server<L> {
    pub(crate) fn cat_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.cat.lying = cat::is_lying(&entity.base);
    }

    pub(crate) fn cat_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn cat_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        let tame = tamable::is_tame(base);
        let tick = base.tick_count;
        let baby = entity.is_baby();
        let lying = cat::is_lying(base) || cat::is_relax_state_one(base);
        let tempting = self
            .entities
            .mobs
            .entries
            .get(&id)
            .is_some_and(|entry| entry.ai.is_tempting());
        let set = species::sound_set(base, VariantFamily::Cat, baby);
        if tempting
            && !tame
            && tick % BEG_INTERVAL == 0
            && let Some(sound) = set.and_then(|set| set.beg_for_food)
        {
            self.entity_sound(id, sound, 1.0, 1.0);
        }
        if lying
            && tick % PURR_INTERVAL == 0
            && let Some(sound) = set.and_then(|set| set.purr)
        {
            let mut random = self.mob_random(id);
            let volume = 0.6 + 0.4 * (random.next_float() - random.next_float());
            self.store_mob_random(id, random);
            self.entity_sound(id, sound, volume, 1.0);
        }
    }

    pub(crate) fn cat_custom_ai_step(&mut self, id: EntityId) {
        let wanted = self.entities.mobs.entries.get(&id).and_then(|entry| {
            let movement = &entry.ai.brain().movement;
            movement.has_wanted().then(|| movement.speed_modifier())
        });
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            cat::apply_move_pose(base, &mut state.attributes, wanted);
        }
    }

    pub(crate) fn cat_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let uuid = self.players.get(&player)?.profile.id.0;
        let entity = self.entities.store.get(target)?;
        let base = &entity.base;
        let tame = tamable::is_tame(base);
        let owned = tamable::owner(base) == Some(uuid);
        let ordered = entity
            .living()
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| mob.tamable)
            .is_some_and(|state| state.ordered_to_sit);
        let (health, max_health) = self
            .mob_view(target)
            .map_or((0.0, 0.0), |view| (view.health, view.max_health));
        let collar = cat::collar_color(base);
        let is_food = held.is_some_and(|stack| item_tag(stack.item, CAT_FOOD));
        if tame {
            if owned {
                if let Some(stack) = held
                    && item_tag(stack.item, CAT_COLLAR_DYES)
                {
                    if let Some(color) = dye_color(stack.item)
                        && color != collar
                    {
                        if let Some(entity) = self.entities.store.get_mut(target) {
                            cat::set_collar_color(&mut entity.base, color);
                        }
                        self.consume_held(player, hand, 1);
                        self.require_persistence(target);
                        return Some(Interaction::Predicted);
                    }
                } else if is_food && health < max_health {
                    self.cat_feed(player, target, hand, held);
                    return Some(Interaction::Predicted);
                }
                let result = self.cat_super_interact(player, target, hand, held);
                if !result.consumes() {
                    self.set_ordered_to_sit(target, !ordered);
                    return Some(Interaction::Predicted);
                }
                return Some(result);
            }
        } else if is_food {
            self.consume_held(player, hand, 1);
            self.cat_try_to_tame(player, target);
            self.require_persistence(target);
            self.cat_eating_sound(target);
            return Some(Interaction::Predicted);
        }
        let result = self.cat_super_interact(player, target, hand, held);
        if result.consumes() {
            self.require_persistence(target);
        }
        Some(result)
    }

    fn cat_super_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Interaction {
        held.map_or(Interaction::Pass, |stack| {
            self.animal_interact(player, target, hand, stack)
        })
    }

    fn require_persistence(&mut self, id: EntityId) {
        if let Some(mob) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .and_then(|state| state.mob.as_mut())
        {
            mob.persistence_required = true;
        }
    }

    fn cat_feed(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) {
        let nutrition = held
            .and_then(|stack| stack.item.kind())
            .and_then(ItemKind::food)
            .map(|food| food.nutrition as f32);
        self.consume_held(player, hand, 1);
        self.heal_mob(
            Some(player),
            target,
            nutrition.unwrap_or(1.0),
            EntityRegainHealthRegainReason::Eating,
        );
        self.cat_eating_sound(target);
    }

    fn cat_try_to_tame(&mut self, player: PlayerId, target: EntityId) {
        let mut random = self.mob_random(target);
        let lucky = random.next_int(TAME_ODDS) == 0;
        self.store_mob_random(target, random);
        if lucky {
            self.ask_tame(player, target);
        } else {
            self.mob_entity_event(target, crate::tamable::TAME_FAILURE_EVENT);
        }
    }

    fn cat_eating_sound(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let sound = species::sound_set(&entity.base, VariantFamily::Cat, entity.is_baby())
            .and_then(|set| set.eat);
        if let Some(sound) = sound {
            self.entity_sound(id, sound, 1.0, 1.0);
        }
    }

    fn make_voice(&mut self, id: EntityId, sound: &'static str) {
        let Some(entity) = self.entities.store.get(id) else {
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
        self.entity_sound(id, sound, volume, spread + if baby { 1.5 } else { 1.0 });
    }

    pub(crate) fn cat_ambient_voice(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let baby = entity.is_baby();
        let in_love = entity
            .living()
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(rspigot_domain::entity::ageable::AgeableState::in_love);
        let sound = cat::ambient_sound(&entity.base, baby, in_love, &mut random);
        self.store_mob_random(id, random);
        if let Some(sound) = sound {
            self.make_voice(id, sound);
        }
    }

    pub(crate) fn cat_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn cat_after_hurt(&mut self, _id: EntityId, _source: &DamageSource, _dead: bool) {}

    pub(crate) fn cat_tamed(&mut self, id: EntityId) {
        if self.entities.store.get(id).map(|e| e.base.kind) != Some(Kind::Cat) {
            return;
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.reassess_cat_tame_goals(true);
        }
    }

    pub(crate) fn cat_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        partner: EntityId,
        random: &mut impl RandomSource,
    ) {
        let (Some(mother), Some(father)) = (
            self.entities.store.get(parent),
            self.entities.store.get(partner),
        ) else {
            return;
        };
        if father.base.kind != Kind::Cat {
            return;
        }
        let mine = species::variant(&mother.base, VariantFamily::Cat);
        let theirs = species::variant(&father.base, VariantFamily::Cat);
        let variant = if random.next_boolean() { mine } else { theirs };
        species::set_variant(&mut child.base, VariantFamily::Cat, variant);
        if tamable::is_tame(&mother.base) {
            let owner = tamable::owner(&mother.base);
            let colors = (
                cat::collar_color(&mother.base),
                cat::collar_color(&father.base),
            );
            tamable::set_owner(&mut child.base, owner);
            tamable::set_tame(&mut child.base, true);
            let mixed = cat::offspring_collar(colors.0, colors.1, &mut ServerRandom(&mut self.rng));
            cat::set_collar_color(&mut child.base, mixed);
        }
    }

    pub(crate) fn cat_action(&mut self, id: EntityId, action: CatAction) {
        match action {
            CatAction::SetLying(lying) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    cat::set_lying(&mut entity.base, lying);
                }
            }
            CatAction::SetRelaxStateOne(relaxed) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    cat::set_relax_state_one(&mut entity.base, relaxed);
                }
            }
            CatAction::MorningGift => self.cat_morning_gift(id),
        }
    }

    fn cat_morning_gift(&mut self, id: EntityId) {
        let chance = cat::waking_up_gift_chance(
            self.world.level.day_time,
            self.world.dimension.has_skylight,
        );
        let roll: f32 = self.rng.random();
        if roll < chance {
            self.give_morning_gift(id);
        }
    }

    fn leash_anchor(&self, entity: &Entity) -> Option<BlockPos> {
        match entity.extra.get(LEASH)? {
            Tag::IntArray(pos) if pos.len() == 3 => Some(BlockPos::new(pos[0], pos[1], pos[2])),
            Tag::Compound(holder) => {
                let uuid = rspigot_domain::entity::save::read_uuid(holder.get("UUID")?)?;
                let found = self.entities.store.by_uuid(uuid)?;
                Some(self.entities.store.get(found)?.base.position.block())
            }
            _ => None,
        }
    }

    fn give_morning_gift(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let origin = self
            .leash_anchor(entity)
            .unwrap_or_else(|| entity.base.position.block());
        let mut random = self.mob_random(id);
        let x = origin.x + random.next_int(GIFT_SPREAD) - 5;
        let y = origin.y + random.next_int(GIFT_VERTICAL) - 2;
        let z = origin.z + random.next_int(GIFT_SPREAD) - 5;
        self.store_mob_random(id, random);
        self.random_teleport(
            id,
            Vec3::new(f64::from(x), f64::from(y), f64::from(z)),
            CAT_DOES_NOT_TELEPORT_TO,
        );
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let at = entity.base.position.block();
        let body = entity.living().map_or(0.0, |state| state.body_yaw);
        let drops = rspigot_data::loot::table(MORNING_GIFT).map_or_else(Vec::new, |table| {
            let context = crate::loot::LootContext {
                this: Some(&facts),
                ..crate::loot::LootContext::default()
            };
            crate::loot::table_drops(table, &context, &mut self.rng)
        });
        let radians = body * (std::f64::consts::PI / 180.0) as f32;
        let position = Vec3::new(
            f64::from(at.x) - f64::from(mth::sin(f64::from(radians))),
            f64::from(at.y),
            f64::from(at.z) + f64::from(mth::cos(f64::from(radians))),
        );
        for stack in drops {
            let Some(view) = self.subject_view(id) else {
                return;
            };
            let item = self.item_at_location(position, 0.0, stack);
            self.drop_from(view, item);
        }
    }

    fn random_teleport(&mut self, id: EntityId, target: Vec3, avoid: &str) {
        let min_y = self.world.dimension.height.min_y;
        let mut pos = target.block();
        if !self.world.chunks.is_loaded(pos.chunk()) {
            return;
        }
        let mut y = target.y;
        while pos.y > min_y {
            pos = pos.offset(0, -1, 0);
            let state = self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR);
            if has_tag(state, ENTITIES_CAN_TELEPORT_TO) {
                self.teleport_if_free(id, Vec3::new(target.x, y, target.z), avoid, state);
                return;
            }
            y -= 1.0;
        }
    }

    fn teleport_if_free(&mut self, id: EntityId, at: Vec3, avoid: &str, ground: BlockStateId) {
        if has_tag(ground, avoid) {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let size = entity.base.dimensions;
        let area = Aabb::of_size(at, size.width, size.height);
        if !rules::no_collision(self, &area) || rules::contains_any_liquid(self, &area) {
            return;
        }
        let blocked = (area.min_x.floor() as i32..=area.max_x.floor() as i32).any(|x| {
            (area.min_y.floor() as i32..=area.max_y.floor() as i32).any(|y| {
                (area.min_z.floor() as i32..=area.max_z.floor() as i32).any(|z| {
                    self.world
                        .chunks
                        .block(BlockPos::new(x, y, z))
                        .is_some_and(|state| has_tag(state, avoid))
                })
            })
        });
        if blocked {
            return;
        }
        self.relocate(id, at, false);
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().navigation.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::{Block, Stone};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{ChunkPos, Identifier};
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

    fn server_at(day_time: i64) -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let level = LevelData {
            day_time,
            ..LevelData::default()
        };
        let mut server = Server::new(
            config,
            level,
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(7);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn cat_in(server: &mut Server<Quiet>) -> EntityId {
        let mut base = server.new_entity_base(Kind::Cat);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        let cat = crate::mob_actions::new_mob(base);
        server.place_entity(cat).expect("the cat is placed")
    }

    fn gifts(server: &Server<Quiet>) -> Vec<Vec3> {
        server
            .entities_of(Kind::Item)
            .iter()
            .map(|item| item.position)
            .collect()
    }

    #[test]
    fn a_morning_gift_drops_an_item_next_to_the_cat() {
        let mut server = server_at(0);
        let cat = cat_in(&mut server);
        let mut rolls = 0;
        while gifts(&server).is_empty() && rolls < 40 {
            server.cat_action(cat, CatAction::MorningGift);
            server.tick();
            rolls += 1;
        }
        let dropped = gifts(&server);
        assert!(!dropped.is_empty(), "no gift after {rolls} rolls");
        let at = server
            .entities
            .store
            .get(cat)
            .expect("the cat stays")
            .base
            .position;
        for item in dropped {
            assert!((item.x - at.x).abs() <= 1.5 && (item.z - at.z).abs() <= 1.5);
            assert!(item.y >= at.y.floor() && item.y <= at.y.floor() + 1.0);
        }
    }

    #[test]
    fn the_gift_table_only_holds_the_vanilla_morning_gifts() {
        let mut server = server_at(0);
        let cat = cat_in(&mut server);
        for _ in 0..40 {
            server.give_morning_gift(cat);
        }
        server.tick();
        let allowed = [
            "minecraft:rabbit_hide",
            "minecraft:rabbit_foot",
            "minecraft:chicken",
            "minecraft:feather",
            "minecraft:rotten_flesh",
            "minecraft:string",
            "minecraft:phantom_membrane",
        ];
        let seen: Vec<&str> = server
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::Item)
            .filter_map(|entity| rspigot_domain::entity::item::stack(&entity.base))
            .filter_map(|stack| stack.item.kind().map(ItemKind::name))
            .collect();
        assert!(!seen.is_empty());
        assert!(seen.iter().all(|name| allowed.contains(name)), "{seen:?}");
    }

    #[test]
    fn no_gift_falls_in_the_middle_of_the_day() {
        let mut server = server_at(6000);
        let cat = cat_in(&mut server);
        for _ in 0..40 {
            server.cat_action(cat, CatAction::MorningGift);
            server.tick();
        }
        assert!(gifts(&server).is_empty());
    }
}
