mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    Effect, EntityType, EquipmentSlot, ItemStack, Material, Position, Rotation, Uuid,
};
use rspigot_plugin_protocol::v1::{
    self, answer, entity_edit::Edit, entity_order, from_plugin, question,
};

const ZOMBIE: i32 = 30;

fn snapshot(id: i32) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: id,
        uuid: Uuid::from_u128(id as u128).to_string(),
        kind: "minecraft:zombie".to_owned(),
        position: Some(v1::Vec3 {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        }),
        custom_name: Some("Bob".to_owned()),
        tags: vec!["boss".to_owned()],
        ..Default::default()
    }
}

fn edit(id: i32, edit: Edit) -> from_plugin::Body {
    from_plugin::Body::Entity(v1::EntityOrder {
        entity: id,
        order: Some(entity_order::Order::Edit(v1::EntityEdit {
            edit: Some(edit),
        })),
    })
}

fn state(id: i32) -> v1::EntityState {
    v1::EntityState {
        entity: Some(snapshot(id)),
        rotation: Some(v1::Rotation {
            yaw: 0.0,
            pitch: 0.0,
        }),
        gravity: true,
        velocity: Some(v1::Vec3::default()),
        ..Default::default()
    }
}

async fn serve_entity(called: &mut Called, id: i32, state: v1::EntityState) {
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Entity(id));
    called
        .session
        .say(answer(request, answer::Body::Entity(state)));
}

async fn serve_list(called: &mut Called) {
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot(ZOMBIE)],
        }),
    ));
}

#[tokio::test]
async fn an_entity_handle_sends_every_edit_as_an_order() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let zombie = player
                .world
                .entities
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            assert_eq!(zombie.custom_name.as_deref(), Some("Bob"));
            assert_eq!(zombie.tags, ["boss"]);
            let other = zombie.clone();
            zombie.name.set("§6Bob");
            zombie.name.clear();
            zombie.name.set_visible(true);
            zombie.flags.set_glowing(true);
            zombie.flags.set_invisible(true);
            zombie.flags.set_silent(true);
            zombie.flags.set_gravity(false);
            zombie.flags.set_invulnerable(true);
            zombie.flags.set_persistent(true);
            zombie.vitals.set_health(4.0);
            zombie.vitals.set_absorption(2.0);
            zombie.vitals.set_fire_ticks(40);
            zombie.vitals.set_freeze_ticks(5);
            zombie.vitals.set_air(100);
            zombie.vitals.set_fall_distance(1.5);
            zombie.scoreboard_tags.add("a");
            zombie.scoreboard_tags.remove("a");
            zombie.set_rotation(Rotation {
                yaw: 90.0,
                pitch: 10.0,
            });
            zombie.look_at(Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            });
            zombie.set_ticks_lived(9);
            zombie.passengers.mount(&other);
            zombie.passengers.add(&other);
            zombie.passengers.dismount();
            zombie.passengers.eject();
            zombie.equipment.set(
                EquipmentSlot::Head,
                &ItemStack::new(Material::IronHelmet, 1),
            );
            zombie.equipment.clear(EquipmentSlot::Hand);
            zombie.equipment.set_drop_chance(EquipmentSlot::Head, 1.0);
            zombie.ai.set(false);
            zombie.ai.set_can_pick_up_loot(true);
            zombie.ai.set_left_handed(true);
            zombie.ai.set_target(&other);
            zombie.ai.clear_target();
            zombie.leash.set_holder(&other);
            zombie.leash.clear();
            zombie.owner.set_uuid(Uuid::from_u128(5));
            zombie.owner.clear();
            zombie.owner.set_tamed(true);
            zombie.owner.set_sitting(true);
            zombie.age.set(-100);
            zombie.age.set_locked(true);
            zombie.age.set_breeding(true);
            zombie.add_effect(Effect::new(rspigot_plugin::EffectType::Speed, 20, 0));
        })
    })
    .await;
    serve_list(&mut called).await;

    let helmet = v1::Item {
        key: "minecraft:iron_helmet".to_owned(),
        count: 1,
        ..Default::default()
    };
    let expected = [
        Edit::SetCustomName("§6Bob".to_owned()),
        Edit::ClearCustomName(v1::Empty {}),
        Edit::SetCustomNameVisible(true),
        Edit::SetGlowing(true),
        Edit::SetInvisible(true),
        Edit::SetSilent(true),
        Edit::SetGravity(false),
        Edit::SetInvulnerable(true),
        Edit::SetPersistent(true),
        Edit::SetHealth(4.0),
        Edit::SetAbsorption(2.0),
        Edit::SetFireTicks(40),
        Edit::SetFreezeTicks(5),
        Edit::SetAir(100),
        Edit::SetFallDistance(1.5),
        Edit::AddTag("a".to_owned()),
        Edit::RemoveTag("a".to_owned()),
        Edit::SetRotation(v1::Rotation {
            yaw: 90.0,
            pitch: 10.0,
        }),
        Edit::LookAt(v1::Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }),
        Edit::SetTicksLived(9),
        Edit::Mount(ZOMBIE),
    ];
    for wanted in expected {
        assert_eq!(called.session.hear().await, edit(ZOMBIE, wanted));
    }
    assert_eq!(
        called.session.hear().await,
        edit(ZOMBIE, Edit::Mount(ZOMBIE))
    );
    let rest = [
        Edit::Dismount(v1::Empty {}),
        Edit::EjectPassengers(v1::Empty {}),
        Edit::SetEquipment(v1::EquipmentEdit {
            slot: "head".to_owned(),
            item: Some(helmet),
        }),
        Edit::SetEquipment(v1::EquipmentEdit {
            slot: "mainhand".to_owned(),
            item: None,
        }),
        Edit::SetDropChance(v1::DropChanceEdit {
            slot: "head".to_owned(),
            chance: 1.0,
        }),
        Edit::SetAi(false),
        Edit::SetCanPickUpLoot(true),
        Edit::SetLeftHanded(true),
        Edit::SetTarget(ZOMBIE),
        Edit::ClearTarget(v1::Empty {}),
        Edit::SetLeashHolder(ZOMBIE),
        Edit::ClearLeash(v1::Empty {}),
        Edit::SetOwner(Uuid::from_u128(5).to_string()),
        Edit::ClearOwner(v1::Empty {}),
        Edit::SetTamed(true),
        Edit::SetSitting(true),
        Edit::SetAge(-100),
        Edit::SetAgeLocked(true),
        Edit::SetBreeding(true),
    ];
    for wanted in rest {
        assert_eq!(called.session.hear().await, edit(ZOMBIE, wanted));
    }
    assert!(matches!(
        called.session.hear().await,
        from_plugin::Body::Entity(v1::EntityOrder {
            order: Some(entity_order::Order::AddEffect(_)),
            ..
        })
    ));
    called.finished().await;
}

#[tokio::test]
async fn the_entity_reads_come_from_its_state() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let zombie = player
                .world
                .entities
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            assert_eq!(zombie.name.get().await.unwrap().as_deref(), Some("§aBob"));
            assert!(zombie.name.get_visible().await.unwrap());
            assert!(zombie.flags.get_glowing().await.unwrap());
            assert_eq!(zombie.vitals.get_fire_ticks().await.unwrap(), 20);
            assert_eq!(zombie.scoreboard_tags.get_all().await.unwrap(), ["t"]);
            let head = zombie.equipment.get(EquipmentSlot::Head).await.unwrap();
            assert_eq!(head.map(|item| item.material), Some(Material::IronHelmet));
            assert_eq!(
                zombie
                    .equipment
                    .get_drop_chance(EquipmentSlot::Head)
                    .await
                    .unwrap(),
                Some(0.5)
            );
            assert_eq!(
                zombie.owner.get_owner().await.unwrap(),
                Some(Uuid::from_u128(9))
            );
            assert!(zombie.owner.is_tamed().await.unwrap());
            let target = zombie.ai.get_target().await.unwrap().unwrap();
            assert_eq!(target.entity_id, 31);
            let holder = zombie.leash.get_holder().await.unwrap().unwrap();
            assert_eq!(holder.entity_id, 32);
            let passengers = zombie.passengers.get_passengers().await.unwrap();
            assert_eq!(
                passengers
                    .iter()
                    .map(|entity| entity.entity_id)
                    .collect::<Vec<_>>(),
                [33, 34]
            );
            let vehicle = zombie.passengers.get_vehicle().await.unwrap().unwrap();
            assert_eq!(vehicle.entity_id, 35);
            assert_eq!(zombie.age.get().await.unwrap(), Some(-5));
            let full = zombie.get_state().await.unwrap();
            assert_eq!(full.entity.kind, EntityType::Zombie);
            assert_eq!(full.living.map(|living| living.max_health), Some(20.0));
        })
    })
    .await;
    serve_list(&mut called).await;

    let living = v1::LivingExtra {
        max_health: 20.0,
        ai: true,
        equipment: vec![v1::EquipmentState {
            slot: "head".to_owned(),
            item: Some(v1::Item {
                key: "minecraft:iron_helmet".to_owned(),
                count: 1,
                ..Default::default()
            }),
            drop_chance: 0.5,
        }],
        leash_holder: Some(32),
        owner: Some(Uuid::from_u128(9).to_string()),
        tamed: true,
        target: Some(31),
        age: -5,
        ..Default::default()
    };
    let full = v1::EntityState {
        custom_name: Some("§aBob".to_owned()),
        custom_name_visible: true,
        glowing: true,
        fire_ticks: 20,
        tags: vec!["t".to_owned()],
        passengers: vec![33, 34],
        vehicle: Some(35),
        living: Some(living),
        ..state(ZOMBIE)
    };
    for _ in 0..4 {
        serve_entity(&mut called, ZOMBIE, full.clone()).await;
    }
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, 31, state(31)).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, 32, state(32)).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, 33, state(33)).await;
    serve_entity(&mut called, 34, state(34)).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, 35, state(35)).await;
    serve_entity(&mut called, ZOMBIE, full.clone()).await;
    serve_entity(&mut called, ZOMBIE, full).await;
    called.finished().await;
}
