use std::num::NonZeroU8;

use rand::Rng;
use rspigot_ai::{AiWorld, EndermanAction, MobView, TELEPORT_EVENT, TargetReason, find_teleport};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species::{self, Species};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::loot::{self, LootContext};
use crate::mobs::sky_darken;
use crate::plugins::steps::Grief;
use crate::ports::ClientLink;
use crate::server::Server;

const ENDERMAN_TELEPORT: &str = "minecraft:entity.enderman.teleport";
const TELEPORT_LEVEL_EVENT: i32 = 2018;
const PACK_RADIUS: i32 = 127;
const REPEATED_TELEPORT_ATTEMPTS: i32 = 64;
const DAYLIGHT_DELAY: i32 = 600;
const DAYLIGHT_LIGHT: f32 = 0.5;
const DAYLIGHT_OFFSET: f32 = 0.4;
const DAYLIGHT_ROLL: f32 = 30.0;
const NON_LIVING_TELEPORT_ODDS: i32 = 10;
const PROJECTILES: &str = "minecraft:is_projectile";
const SILK_TOUCH: &str = "minecraft:silk_touch";
const ENCHANTMENTS: &str = "minecraft:enchantments";

pub(crate) fn carried_block_drops<R: Rng + ?Sized>(
    carried: BlockStateId,
    rng: &mut R,
) -> Vec<ItemStack> {
    let mut axe = ItemStack::new(ItemKind::DiamondAxe.id(), NonZeroU8::MIN);
    axe.components
        .insert(ENCHANTMENTS, Compound::new().with(SILK_TOUCH, 1));
    let context = LootContext::block(carried, Some(&axe), None);
    loot::block_drops(&context, rng)
}

fn packed_difference(from: BlockPos, to: BlockPos) -> i32 {
    let diff = |a: i32, b: i32| (b - a).clamp(-PACK_RADIUS, PACK_RADIUS);
    let pack = |value: i32| (value + PACK_RADIUS) & 0xFF;
    (pack(diff(from.x, to.x)) << 16) | (pack(diff(from.y, to.y)) << 8) | pack(diff(from.z, to.z))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn enderman_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.enderman.carried = species::enderman::carried_block(&entity.base);
    }

    pub(crate) fn enderman_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn enderman_ai_step(&mut self, id: EntityId) {
        if self.living_entity_alive(id) {
            self.update_persistent_anger(id, true);
        }
    }

    pub(crate) fn enderman_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let wet = entity.base.fluid.in_water || self.is_in_rain(&entity.base);
        if wet && self.living_entity_alive(id) {
            self.hurt_mob(id, &DamageSource::new(DamageType::Drown), 1.0);
        }
    }

    pub(crate) fn enderman_target_changed(&mut self, id: EntityId, target: Option<EntityId>) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            species::enderman::target_changed(base, state, target.is_some());
        }
    }

    pub(crate) fn enderman_custom_ai_step(&mut self, id: EntityId) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(Species::Enderman(state)) = self
            .entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            return;
        };
        let sky = sky_darken(&self.world.level);
        let (bright, sees_sky) = {
            let world = self.mob_world(sky);
            (
                world.bright_outside(),
                rspigot_ai::BlockView::can_see_sky(&world, view.block_pos()),
            )
        };
        if !bright || view.tick_count < state.target_change_time + DAYLIGHT_DELAY {
            return;
        }
        let light = view.light;
        if light <= DAYLIGHT_LIGHT || !sees_sky {
            return;
        }
        let mut random = self.mob_random(id);
        let roll = random.next_float() * DAYLIGHT_ROLL;
        self.store_mob_random(id, random);
        if roll < (light - DAYLIGHT_OFFSET) * 2.0 {
            self.forget_target(id, TargetReason::Unknown);
            self.enderman_teleport(id);
        }
    }

    pub(crate) fn enderman_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn enderman_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        if source.kind.in_tag(PROJECTILES) {
            self.enderman_repeatedly_teleport(id);
            return None;
        }
        Some(amount)
    }

    pub(crate) fn enderman_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        if source.living_cause() {
            return;
        }
        let mut random = self.mob_random(id);
        let teleports = random.next_int(NON_LIVING_TELEPORT_ODDS) != 0;
        self.store_mob_random(id, random);
        if teleports && !dead {
            self.enderman_teleport(id);
        }
    }

    pub(crate) fn enderman_action(&mut self, id: EntityId, action: EndermanAction) {
        match action {
            EndermanAction::Teleport(to) => self.enderman_move(id, to),
            EndermanAction::BeingStaredAt => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    species::enderman::set_being_stared_at(&mut entity.base);
                }
            }
            EndermanAction::TakeBlock { pos, carried } => {
                let Some(view) = self.entity_view(id) else {
                    return;
                };
                let change = Grief::EndermanTake {
                    enderman: id,
                    pos,
                    carried,
                };
                self.change_block(view, pos, BlockStateId::AIR, change, false);
            }
            EndermanAction::PlaceBlock { pos, state } => {
                let Some(view) = self.entity_view(id) else {
                    return;
                };
                let change = Grief::EndermanPlace {
                    enderman: id,
                    pos,
                    state,
                };
                self.change_block(view, pos, state, change, false);
            }
        }
    }

    pub(crate) fn finish_enderman_take(
        &mut self,
        id: EntityId,
        pos: BlockPos,
        carried: BlockStateId,
    ) {
        let carrying = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| species::enderman::carries_block(&entity.base));
        let taken = self
            .world
            .chunks
            .block(pos)
            .is_some_and(|state| state.kind() == carried.kind() && !state.is_air());
        if carrying || !taken {
            return;
        }
        self.set_block(pos, BlockStateId::AIR);
        if let Some(entity) = self.entities.store.get_mut(id) {
            species::enderman::set_carried_block(&mut entity.base, Some(carried));
        }
    }

    pub(crate) fn finish_enderman_place(
        &mut self,
        id: EntityId,
        pos: BlockPos,
        state: BlockStateId,
    ) {
        let carrying = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| species::enderman::carries_block(&entity.base));
        let free = self
            .world
            .chunks
            .block(pos)
            .is_some_and(BlockStateId::is_air);
        if !carrying || !free {
            return;
        }
        self.set_block(pos, state);
        if let Some(entity) = self.entities.store.get_mut(id) {
            species::enderman::set_carried_block(&mut entity.base, None);
        }
    }

    pub(crate) fn enderman_teleport(&mut self, id: EntityId) -> bool {
        if !self.living_entity_alive(id) {
            return false;
        }
        let Some(view) = self.mob_view(id) else {
            return false;
        };
        let mut random = self.mob_random(id);
        let found = {
            let world = self.mob_world(sky_darken(&self.world.level));
            find_teleport(&world, &view, &mut random)
        };
        self.store_mob_random(id, random);
        match found {
            Some(to) => {
                self.enderman_move(id, to);
                true
            }
            None => false,
        }
    }

    fn enderman_repeatedly_teleport(&mut self, id: EntityId) {
        for _ in 0..REPEATED_TELEPORT_ATTEMPTS {
            if self.enderman_teleport(id) {
                return;
            }
        }
    }

    fn enderman_move(&mut self, id: EntityId, to: Vec3) {
        let Some(from) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        self.relocate_tracked(id, to, false, Some(from));
    }

    pub(crate) fn enderman_teleported(&mut self, id: EntityId, from: Vec3) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let (now, silent) = (entity.base.position, entity.base.is_silent());
        self.mob_entity_event(id, TELEPORT_EVENT);
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().navigation.stop();
        }
        if silent {
            return;
        }
        let data = packed_difference(from.block(), now.block());
        self.level_event(TELEPORT_LEVEL_EVENT, from.block(), data);
        self.play_sound(None, now, ENDERMAN_TELEPORT, SoundSource::Hostile, 1.0, 1.0);
        self.play_sound(None, now, ENDERMAN_TELEPORT, SoundSource::Hostile, 1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;

    use super::*;

    fn names(drops: &[ItemStack]) -> Vec<&'static str> {
        drops
            .iter()
            .map(|stack| stack.item.kind().expect("an item").name())
            .collect()
    }

    #[test]
    fn a_carried_block_is_dropped_as_if_broken_with_silk_touch() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(3);
        let grass = BlockKind::GrassBlock.default_state();
        assert_eq!(
            names(&carried_block_drops(grass, &mut rng)),
            ["minecraft:grass_block"]
        );
        let stone = BlockKind::Stone.default_state();
        assert_eq!(
            names(&carried_block_drops(stone, &mut rng)),
            ["minecraft:stone"]
        );
        let pumpkin = BlockKind::Pumpkin.default_state();
        assert_eq!(
            names(&carried_block_drops(pumpkin, &mut rng)),
            ["minecraft:pumpkin"]
        );
    }

    #[test]
    fn teleport_data_packs_the_position_difference() {
        let from = BlockPos::new(0, 64, 0);
        assert_eq!(
            packed_difference(from, from),
            (127 << 16) | (127 << 8) | 127
        );
        let far = packed_difference(from, BlockPos::new(500, -500, -3));
        assert_eq!(far, 254 << 16 | 124);
    }
}
