use rspigot_ai::{MobGoal, MobView, SilverfishAction, SilverfishGoal};
use rspigot_blocks::infested::{infested_state_by_host, is_compatible_host_block};
use rspigot_domain::entity::Entity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};

use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::plugins::events::EntityRemoveCause;
use crate::plugins::steps::Grief;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::entity::RemovalReason;

const ALWAYS_TRIGGERS_SILVERFISH: &str = "minecraft:always_triggers_silverfish";
pub(crate) const SPAWN_ANIM_EVENT: i8 = 20;

fn wakes_friends(source: &DamageSource) -> bool {
    source.causing.is_some() || source.kind.in_tag(ALWAYS_TRIGGERS_SILVERFISH)
}

fn align_body(entity: &mut Entity) {
    let yaw = entity.base.yaw;
    if let Some(living) = entity.living_mut() {
        living.body_yaw = yaw;
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn silverfish_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn silverfish_pre_tick(&mut self, id: EntityId) -> bool {
        if let Some(entity) = self.entities.store.get_mut(id) {
            align_body(entity);
        }
        true
    }

    pub(crate) fn silverfish_post_tick(&mut self, _id: EntityId) {}

    pub(crate) fn silverfish_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn silverfish_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn silverfish_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        if wakes_friends(source) {
            self.notify_friends(id);
        }
        Some(amount)
    }

    pub(crate) fn silverfish_after_hurt(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
    }

    pub(crate) fn silverfish_action(&mut self, id: EntityId, action: SilverfishAction) {
        match action {
            SilverfishAction::WakeUpFriends(origin) => self.scan_friends(id, origin, 0),
            SilverfishAction::Merge(pos) => self.merge_with_stone(id, pos),
        }
    }

    fn notify_friends(&mut self, id: EntityId) {
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return;
        };
        for goal in entry.ai.goals_mut().goals_mut() {
            if let MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(friends)) = goal {
                friends.notify_hurt();
            }
        }
    }

    fn merge_with_stone(&mut self, id: EntityId, pos: BlockPos) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let Some(to) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| is_compatible_host_block(*state))
            .and_then(infested_state_by_host)
        else {
            return;
        };
        self.change_block(
            view,
            pos,
            to,
            Grief::Merge {
                silverfish: id,
                pos,
            },
            false,
        );
    }

    pub(crate) fn finish_merge(&mut self, id: EntityId, pos: BlockPos, cancelled: bool) {
        if cancelled || !self.living_entity_alive(id) {
            return;
        }
        let Some(to) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| is_compatible_host_block(*state))
            .and_then(infested_state_by_host)
        else {
            return;
        };
        self.set_block(pos, to);
        self.spawn_anim(id);
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::EnterBlock),
        );
    }

    pub(crate) fn spawn_anim(&mut self, id: EntityId) {
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(SPAWN_ANIM_EVENT),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::damage::DamageType;
    use rspigot_domain::entity::save::load_entity;
    use rspigot_domain::text::Component;
    use rspigot_domain::{EntityId, Vec3};
    use rspigot_entities::EntityKind as Kind;
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::health::SourceEntity;

    fn attacker() -> SourceEntity {
        SourceEntity {
            entity: EntityId(9),
            player: None,
            kind: Kind::Zombie,
            creative: false,
            name: Component::text("zombie"),
            position: Vec3::new(0.0, 64.0, 0.0),
        }
    }

    #[test]
    fn damage_from_an_entity_wakes_the_friends() {
        let source = DamageSource::by(DamageType::MobAttack, attacker());
        assert!(wakes_friends(&source));
    }

    #[test]
    fn magic_wakes_the_friends_without_an_attacker() {
        assert!(wakes_friends(&DamageSource::new(DamageType::Magic)));
    }

    #[test]
    fn other_environmental_damage_does_not() {
        for kind in [
            DamageType::Generic,
            DamageType::Fall,
            DamageType::OutOfWorld,
        ] {
            assert!(!wakes_friends(&DamageSource::new(kind)), "{kind:?}");
        }
    }

    #[test]
    fn the_body_follows_the_yaw_before_each_tick() {
        let compound = Compound::new().with("id", "minecraft:silverfish").with(
            "Rotation",
            rspigot_nbt::Tag::List(vec![
                rspigot_nbt::Tag::Float(135.0),
                rspigot_nbt::Tag::Float(0.0),
            ]),
        );
        let mut entity = load_entity(&compound, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(entity.living().unwrap().body_yaw, 0.0);
        align_body(&mut entity);
        assert_eq!(entity.base.yaw, 135.0);
        assert_eq!(entity.living().unwrap().body_yaw, 135.0);
    }
}
