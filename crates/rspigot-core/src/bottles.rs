use std::num::NonZeroU8;

use rspigot_blocks::StateFlags;
use rspigot_domain::Vec3;
use rspigot_domain::entity::{Aabb, EntityState, cloud};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Rotation};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::ports::ClientLink;
use crate::server::Server;

const FILL_SOUND: &str = "minecraft:item.bottle.fill";
const FILL_DRAGON_BREATH_SOUND: &str = "minecraft:item.bottle.fill_dragonbreath";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const WATER: &str = "minecraft:water";
const CLOUD_SEARCH_INFLATE: f64 = 2.0;
const CLOUD_RADIUS_STEP: f32 = 0.5;

fn water_bottle() -> ItemStack {
    let mut stack = ItemStack::new(ItemKind::Potion.id(), NonZeroU8::MIN);
    stack
        .components
        .insert(POTION_CONTENTS, Compound::new().with("potion", WATER));
    stack
}

fn dragon_breath() -> ItemStack {
    ItemStack::new(ItemKind::DragonBreath.id(), NonZeroU8::MIN)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_glass_bottle(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
    ) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item != ItemKind::GlassBottle.id() {
            return false;
        }
        if let Some(cloud) = self.dragon_cloud_near(id) {
            self.fill_from_cloud(id, hand, cloud);
            return true;
        }
        let Some(hit) = self.look_source(id, rotation) else {
            return false;
        };
        let in_water = self
            .world
            .chunks
            .block(hit.pos)
            .is_some_and(|state| state.flags().contains(StateFlags::WATER));
        if !in_water || self.is_protected(id, hit.pos) {
            return false;
        }
        if let Some(position) = self.players.get(&id).map(|player| player.position) {
            self.play_sound(
                Some(id),
                position,
                FILL_SOUND,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            self.game_event(
                GameEvent::FluidPickup,
                hit.pos.center(),
                GameEventContext::of(self.players.get(&id).map(|player| player.entity_id)),
            );
        }
        self.finish_bottle_fill(id, hand, &stack, water_bottle());
        true
    }

    fn dragon_cloud_near(&self, id: PlayerId) -> Option<EntityId> {
        let player = self.players.get(&id)?;
        let reach = Aabb::of_size(player.position, 0.6, player.height() as f32)
            .inflate(CLOUD_SEARCH_INFLATE);
        self.entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.kind == Kind::AreaEffectCloud
                    && !entity.base.is_removed()
                    && entity.base.bounding_box().intersects(&reach)
            })
            .find(|entity| {
                let EntityState::Cloud(state) = &entity.state else {
                    return false;
                };
                state
                    .owner
                    .and_then(|owner| self.entities.store.by_uuid(owner))
                    .and_then(|owner| self.entities.store.get(owner))
                    .is_some_and(|owner| owner.base.kind == Kind::EnderDragon)
            })
            .map(|entity| entity.base.id)
    }

    fn fill_from_cloud(&mut self, id: PlayerId, hand: Hand, cloud: EntityId) {
        let Some(stack) = self.held(id, hand) else {
            return;
        };
        if let Some(entity) = self.entities.store.get_mut(cloud) {
            let radius = cloud::radius(&entity.base) - CLOUD_RADIUS_STEP;
            cloud::set_radius(&mut entity.base, radius);
            entity.base.needs_sync = true;
        }
        if let Some(position) = self.players.get(&id).map(|player| player.position) {
            self.play_sound(
                None,
                position,
                FILL_DRAGON_BREATH_SOUND,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            self.game_event(
                GameEvent::FluidPickup,
                position,
                GameEventContext::of(self.players.get(&id).map(|player| player.entity_id)),
            );
        }
        self.finish_bottle_fill(id, hand, &stack, dragon_breath());
    }

    fn finish_bottle_fill(
        &mut self,
        id: PlayerId,
        hand: Hand,
        bottle: &ItemStack,
        filled: ItemStack,
    ) {
        self.count_used(id, bottle);
        let creative = self.infinite_materials(id);
        let Some(entity) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        if creative {
            if let Some(player) = self.players.get_mut(&id) {
                let present =
                    player.inventory.slots().iter().flatten().any(|held| {
                        held.item == filled.item && held.components == filled.components
                    });
                if !present {
                    let mut spare = Some(filled);
                    player.inventory.add(&mut spare, true);
                }
            }
        } else {
            let remaining = NonZeroU8::new(bottle.count.get() - 1).map(|count| ItemStack {
                count,
                ..bottle.clone()
            });
            match remaining {
                None => self.set_held(id, hand, Some(filled)),
                Some(left) => {
                    self.set_held(id, hand, Some(left));
                    let mut spare = Some(filled);
                    if let Some(player) = self.players.get_mut(&id) {
                        player.inventory.add(&mut spare, false);
                    }
                    if let Some(spare) = spare {
                        self.drop_at_location(entity, 1.0, spare, Vec3::ZERO);
                    }
                }
            }
        }
        self.refresh_hands(id);
    }
}
