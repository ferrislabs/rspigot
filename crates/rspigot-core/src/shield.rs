use rand::Rng;
use rspigot_domain::Vec3;
use rspigot_domain::item::ItemStack;
use rspigot_domain::mth::view_vector;
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::bow::{UsingItem, hand_slot};
use crate::cooldown::SHIELD_DISABLE_TICKS;
use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::{DamageSource, SourceEntity};
use crate::player::Player;
use crate::plugins::Holder;
use crate::plugins::events::EntityKnockbackCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const BLOCK_DELAY_TICKS: u64 = 5;
const DURABILITY_THRESHOLD: f32 = 3.0;
const BLOCK_KNOCKBACK: f64 = 0.5;
const BYPASSES_SHIELD: &str = "minecraft:bypasses_shield";
const IS_PROJECTILE: &str = "minecraft:is_projectile";
const BLOCK_SOUND: &str = "minecraft:item.shield.block";
const BREAK_SOUND: &str = "minecraft:item.shield.break";
const BLOCK_EVENT: i8 = 29;
const BREAK_EVENT: i8 = 30;

pub(crate) fn is_shield(stack: &ItemStack) -> bool {
    stack.item == ItemKind::Shield.id()
}

pub(crate) fn is_axe(item: ItemKind) -> bool {
    matches!(
        item,
        ItemKind::WoodenAxe
            | ItemKind::CopperAxe
            | ItemKind::StoneAxe
            | ItemKind::GoldenAxe
            | ItemKind::IronAxe
            | ItemKind::DiamondAxe
            | ItemKind::NetheriteAxe
    )
}

pub(crate) fn starts_raising<L>(player: &Player<L>, hand: Hand, since: u64) -> Option<UsingItem> {
    let stack = player.inventory.get(hand_slot(player, hand))?;
    is_shield(stack).then_some(UsingItem { hand, since })
}

pub(crate) fn faces_source(yaw: f32, victim: Vec3, source: Vec3) -> bool {
    let look = view_vector(0.0, yaw);
    let dx = source.x - victim.x;
    let dz = source.z - victim.z;
    let length = (dx * dx + dz * dz).sqrt();
    if length == 0.0 {
        return true;
    }
    (dx * look.x + dz * look.z) / length >= 0.0
}

impl<L: ClientLink> Server<L> {
    fn raised_shield(&self, id: PlayerId) -> Option<(Hand, ItemKind)> {
        let player = self.players.get(&id)?;
        let using = player.using_item?;
        let stack = player.inventory.get(hand_slot(player, using.hand))?;
        let raised = self.current_tick().saturating_sub(using.since) >= BLOCK_DELAY_TICKS;
        (is_shield(stack) && raised).then_some((using.hand, ItemKind::Shield))
    }

    fn melee_weapon(&self, direct: &SourceEntity) -> Option<ItemKind> {
        match direct.player {
            Some(player) => self.players.get(&player)?.inventory.held()?.item.kind(),
            None => self
                .entities
                .store
                .get(direct.entity)?
                .living()?
                .equipment(EquipmentSlot::Mainhand)?
                .item
                .kind(),
        }
    }

    pub(crate) fn block_with_shield(
        &mut self,
        id: PlayerId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        if amount <= 0.0 || source.kind.in_tag(BYPASSES_SHIELD) {
            return false;
        }
        let Some((hand, shield)) = self.raised_shield(id) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let (entity, position, yaw) = (player.entity_id, player.position, player.rotation.yaw);
        if let Some(from) = source.source_position()
            && !faces_source(yaw, position, from)
        {
            return false;
        }
        let pitch = 0.8 + self.rng.random::<f32>() * 0.4;
        self.play_sound(
            None,
            position,
            BLOCK_SOUND,
            SoundSource::Players,
            1.0,
            pitch,
        );
        self.shield_event(id, entity, BLOCK_EVENT);
        if amount >= DURABILITY_THRESHOLD {
            self.wear(id, hand, 1 + amount.floor() as i32);
        }
        if !source.kind.in_tag(IS_PROJECTILE)
            && let Some(direct) = source.direct.as_ref().filter(|d| d.kind.living())
        {
            let holder = direct
                .player
                .map_or(Holder::Mob(direct.entity), Holder::Player);
            let away = (
                position.x - direct.position.x,
                position.z - direct.position.z,
            );
            self.knock(
                holder,
                BLOCK_KNOCKBACK,
                away,
                EntityKnockbackCause::EntityAttack,
                Some(entity),
            );
            if self.melee_weapon(direct).is_some_and(is_axe) {
                self.disable_shield(id, shield);
            }
        }
        true
    }

    fn disable_shield(&mut self, id: PlayerId, shield: ItemKind) {
        self.start_cooldown(id, shield.id(), SHIELD_DISABLE_TICKS);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.using_item = None;
        let (entity, position) = (player.entity_id, player.position);
        self.play_sound(None, position, BREAK_SOUND, SoundSource::Players, 0.8, 0.8);
        self.shield_event(id, entity, BREAK_EVENT);
    }

    fn shield_event(&mut self, id: PlayerId, entity: rspigot_domain::EntityId, status: i8) {
        let event = ClientEvent::EntityStatus {
            entity,
            status: EntityStatus::Event(status),
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rspigot_domain::EntityId;
    use rspigot_domain::damage::DamageType;
    use rspigot_domain::entity::reward::damage_value;
    use rspigot_domain::item::HOTBAR_START;
    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Difficulty;
    use rspigot_entities::EntityKind as Kind;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::{ClientEvent, EntityStatus, Inbound};
    use crate::health::SourceEntity;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    #[derive(Clone, Default)]
    struct Record(Arc<Mutex<Vec<ClientEvent>>>);

    impl ClientLink for Record {
        fn send(&self, event: ClientEvent) {
            self.0.lock().unwrap().push(event);
        }

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn arrive() -> (Server<Record>, PlayerId, Record) {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        let id = PlayerId::offline("Alice");
        let link = Record::default();
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: link.clone(),
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        server.world.difficulty = Difficulty::Normal;
        let player = server.players.get_mut(&id).unwrap();
        player.position = Vec3::new(0.5, 64.0, 0.5);
        player.rotation.yaw = 0.0;
        player.vitals.damage_cooldown = 0;
        player.vitals.load_timeout = 0;
        (server, id, link)
    }

    fn shield_stack() -> ItemStack {
        ItemStack::new(ItemKind::Shield.id(), NonZeroU8::MIN)
    }

    fn give_shield(server: &mut Server<Record>, id: PlayerId) {
        let player = server.players.get_mut(&id).unwrap();
        let slot = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        player.inventory.set(slot, Some(shield_stack()));
    }

    fn raise(server: &mut Server<Record>, id: PlayerId, ticks: usize) {
        give_shield(server, id);
        server.start_using_item(id, Hand::Main);
        for _ in 0..ticks {
            server.tick();
        }
        let player = server.players.get_mut(&id).unwrap();
        player.vitals.damage_cooldown = 0;
        player.position = Vec3::new(0.5, 64.0, 0.5);
    }

    fn mob_source(kind: Kind, kind_damage: DamageType, at: Vec3) -> DamageSource {
        DamageSource::by(
            kind_damage,
            SourceEntity {
                entity: EntityId(900),
                player: None,
                kind,
                creative: false,
                name: Component::text("Zombie"),
                position: at,
            },
        )
    }

    fn zombie(z: f64) -> DamageSource {
        mob_source(Kind::Zombie, DamageType::MobAttack, Vec3::new(0.5, 64.0, z))
    }

    fn shield_damage(server: &Server<Record>, id: PlayerId) -> i32 {
        let player = server.players.get(&id).unwrap();
        let slot = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        player.inventory.get(slot).map_or(-1, damage_value)
    }

    fn events(link: &Record) -> Vec<ClientEvent> {
        link.0.lock().unwrap().clone()
    }

    #[test]
    fn a_raised_shield_blocks_a_melee_hit_from_the_front() {
        let (mut server, id, link) = arrive();
        raise(&mut server, id, 5);
        let before = server.player_health(id).unwrap();
        assert!(!server.hurt(id, &zombie(2.5), 4.0));
        assert_eq!(server.player_health(id), Some(before));
        server.deliver();
        let log = events(&link);
        assert!(log.iter().any(|e| matches!(
            e,
            ClientEvent::Sound { sound, .. } if *sound == "minecraft:item.shield.block"
        )));
        assert!(log.iter().any(|e| matches!(
            e,
            ClientEvent::EntityStatus {
                status: EntityStatus::Event(29),
                ..
            }
        )));
    }

    #[test]
    fn the_same_hit_from_behind_hurts() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 5);
        assert!(server.hurt(id, &zombie(-1.5), 4.0));
        assert_eq!(server.player_health(id), Some(16.0));
    }

    #[test]
    fn a_shield_raised_for_fewer_than_five_ticks_does_not_block() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 4);
        assert!(server.hurt(id, &zombie(2.5), 4.0));
        assert_eq!(server.player_health(id), Some(16.0));
    }

    #[test]
    fn a_lowered_shield_does_not_block() {
        let (mut server, id, _) = arrive();
        give_shield(&mut server, id);
        for _ in 0..8 {
            server.tick();
        }
        assert!(server.hurt(id, &zombie(2.5), 4.0));
    }

    #[test]
    fn fall_and_drowning_bypass_the_shield() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 5);
        assert!(server.hurt(id, &DamageSource::new(DamageType::Fall), 2.0));
        assert_eq!(server.player_health(id), Some(18.0));
        server.players.get_mut(&id).unwrap().vitals.damage_cooldown = 0;
        assert!(server.hurt(id, &DamageSource::new(DamageType::Drown), 2.0));
        assert_eq!(server.player_health(id), Some(16.0));
    }

    #[test]
    fn an_axe_hit_disables_the_shield_for_a_hundred_ticks() {
        let (mut server, id, link) = arrive();
        raise(&mut server, id, 5);
        let bob = PlayerId::offline("Bob");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: bob,
                name: "Bob".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Record::default(),
            address: [127, 0, 0, 1].into(),
            session: 2,
        });
        server.deliver();
        let player = server.players.get_mut(&bob).unwrap();
        player.position = Vec3::new(0.5, 64.0, 2.5);
        player.vitals.load_timeout = 0;
        let slot = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        player.inventory.set(
            slot,
            Some(ItemStack::new(ItemKind::IronAxe.id(), NonZeroU8::MIN)),
        );
        let source = DamageSource::by(DamageType::PlayerAttack, server.source_of(bob).unwrap());
        assert!(!server.hurt(id, &source, 5.0));
        assert!(server.is_on_cooldown(id, ItemKind::Shield.id()));
        assert!(server.players.get(&id).unwrap().using_item.is_none());
        server.deliver();
        let log = events(&link);
        assert!(log.iter().any(|e| matches!(
            e,
            ClientEvent::Cooldown { group, ticks: 100 } if group == "minecraft:shield"
        )));
        assert!(log.iter().any(|e| matches!(
            e,
            ClientEvent::Sound { sound, .. } if *sound == "minecraft:item.shield.break"
        )));
        assert!(log.iter().any(|e| matches!(
            e,
            ClientEvent::EntityStatus {
                status: EntityStatus::Event(30),
                ..
            }
        )));
        for _ in 0..99 {
            server.tick();
        }
        assert!(server.is_on_cooldown(id, ItemKind::Shield.id()));
        server.tick();
        assert!(!server.is_on_cooldown(id, ItemKind::Shield.id()));
    }

    #[test]
    fn a_disabled_shield_cannot_be_raised() {
        let (mut server, id, _) = arrive();
        give_shield(&mut server, id);
        server.start_cooldown(id, ItemKind::Shield.id(), 100);
        assert!(server.held_on_cooldown(id, Hand::Main));
    }

    #[test]
    fn six_blocked_damage_costs_seven_durability() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 5);
        assert!(!server.hurt(id, &zombie(2.5), 6.0));
        assert_eq!(shield_damage(&server, id), 7);
    }

    #[test]
    fn blocked_damage_under_three_costs_nothing() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 5);
        assert!(!server.hurt(id, &zombie(2.5), 2.5));
        assert_eq!(shield_damage(&server, id), 0);
    }

    #[test]
    fn an_arrow_from_the_front_is_blocked() {
        let (mut server, id, _) = arrive();
        raise(&mut server, id, 5);
        let mut source = mob_source(Kind::Arrow, DamageType::Arrow, Vec3::new(0.5, 65.0, 2.0));
        source.causing = None;
        assert!(!server.hurt(id, &source, 4.0));
        assert_eq!(server.player_health(id), Some(20.0));
    }

    #[test]
    fn an_off_hand_shield_blocks_too() {
        let (mut server, id, _) = arrive();
        let player = server.players.get_mut(&id).unwrap();
        player
            .inventory
            .set(rspigot_domain::item::OFFHAND_SLOT, Some(shield_stack()));
        server.start_using_item(id, Hand::Off);
        for _ in 0..5 {
            server.tick();
        }
        server.players.get_mut(&id).unwrap().position = Vec3::new(0.5, 64.0, 0.5);
        assert!(!server.hurt(id, &zombie(2.5), 4.0));
    }
}
