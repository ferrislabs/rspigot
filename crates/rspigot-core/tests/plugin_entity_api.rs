mod common;

use common::*;
use rspigot_core::plugins::{
    Answer, EntityDetails, EntityEdit, EntityOrder, EventKind, EventPayload, ItemSpec, Order,
    Question,
};
use rspigot_domain::{EntityId, Vec3};

const AT: Vec3 = Vec3::new(4.5, 64.0, 4.5);

fn arena() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, AT.block());
    s
}

fn spawn(s: &mut Scene, kind: &str) -> EntityId {
    spawn_at(s, kind, AT)
}

fn spawn_at(s: &mut Scene, kind: &str, position: Vec3) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn edit(s: &mut Scene, entity: EntityId, edit: EntityEdit) {
    s.act(Order::Entity {
        entity,
        order: EntityOrder::Edit(edit),
    });
}

fn details(s: &mut Scene, id: EntityId) -> EntityDetails {
    let Answer::Entity(details) = s.ask(Question::Entity(id)) else {
        panic!("expected the entity {id:?}");
    };
    details
}

fn saved(s: &Scene, entity: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(entity).expect("the entity is saved")
}

fn warned(s: &Scene, order: &str) -> bool {
    s.warnings().iter().any(|warning| warning.contains(order))
}

#[test]
fn a_name_and_the_common_flags_are_set_and_read_back() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::SetCustomName("§6Bob".into()));
    edit(&mut s, zombie, EntityEdit::SetCustomNameVisible(true));
    edit(&mut s, zombie, EntityEdit::SetGlowing(true));
    edit(&mut s, zombie, EntityEdit::SetInvisible(true));
    edit(&mut s, zombie, EntityEdit::SetSilent(true));
    edit(&mut s, zombie, EntityEdit::SetGravity(false));
    edit(&mut s, zombie, EntityEdit::SetInvulnerable(true));
    edit(&mut s, zombie, EntityEdit::SetPersistent(true));

    let read = details(&mut s, zombie);
    assert_eq!(read.custom_name.as_deref(), Some("§6Bob"));
    assert!(read.flags.custom_name_visible && read.flags.glowing && read.flags.invisible);
    assert!(read.flags.silent && !read.flags.gravity && read.flags.invulnerable);
    assert!(read.flags.persistent);
    let nbt = saved(&s, zombie);
    assert_eq!(nbt.get_bool("CustomNameVisible"), Some(true));
    assert_eq!(nbt.get_bool("Glowing"), Some(true));
    assert_eq!(nbt.get_bool("Silent"), Some(true));
    assert_eq!(nbt.get_bool("NoGravity"), Some(true));
    assert_eq!(nbt.get_bool("Invulnerable"), Some(true));
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));

    edit(&mut s, zombie, EntityEdit::ClearCustomName);
    assert_eq!(details(&mut s, zombie).custom_name, None);
}

#[test]
fn fire_freeze_air_fall_rotation_and_tags_are_set() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::SetFireTicks(60));
    edit(&mut s, zombie, EntityEdit::SetFreezeTicks(30));
    edit(&mut s, zombie, EntityEdit::SetAir(123));
    edit(&mut s, zombie, EntityEdit::SetFallDistance(4.5));
    edit(
        &mut s,
        zombie,
        EntityEdit::SetRotation {
            yaw: 90.0,
            pitch: 20.0,
        },
    );
    edit(&mut s, zombie, EntityEdit::AddTag("boss".into()));
    edit(&mut s, zombie, EntityEdit::AddTag("boss".into()));
    edit(&mut s, zombie, EntityEdit::AddTag("zoo".into()));
    edit(&mut s, zombie, EntityEdit::RemoveTag("zoo".into()));

    let read = details(&mut s, zombie);
    assert_eq!(read.fire_ticks, 60);
    assert_eq!(read.freeze_ticks, 30);
    assert_eq!(read.air, 123);
    assert_eq!(read.fall_distance, 4.5);
    assert_eq!((read.rotation.yaw, read.rotation.pitch), (90.0, 20.0));
    assert_eq!(read.tags, vec!["boss".to_owned()]);

    edit(
        &mut s,
        zombie,
        EntityEdit::LookAt(Vec3::new(4.5, 64.0, 14.5)),
    );
    let facing = details(&mut s, zombie).rotation;
    assert!(
        facing.yaw.abs() < 0.01,
        "south is yaw 0, got {}",
        facing.yaw
    );
    edit(&mut s, zombie, EntityEdit::SetTicksLived(0));
    assert!(warned(&s, "entity.set_ticks_lived"));
    edit(&mut s, zombie, EntityEdit::SetTicksLived(500));
    assert_eq!(details(&mut s, zombie).ticks_lived, 500);
}

#[test]
fn equipment_round_trips_with_its_drop_chance() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(
        &mut s,
        zombie,
        EntityEdit::SetEquipment {
            slot: "head".into(),
            item: Some(ItemSpec::of("minecraft:iron_helmet", 1)),
        },
    );
    edit(
        &mut s,
        zombie,
        EntityEdit::SetEquipment {
            slot: "mainhand".into(),
            item: Some(ItemSpec::of("minecraft:iron_sword", 1)),
        },
    );
    edit(
        &mut s,
        zombie,
        EntityEdit::SetDropChance {
            slot: "head".into(),
            chance: 1.0,
        },
    );

    let living = details(&mut s, zombie).living.expect("a living entity");
    let slot = |name: &str| {
        living
            .equipment
            .iter()
            .find(|view| view.slot == name)
            .unwrap()
    };
    assert_eq!(
        slot("head").item.as_ref().map(|item| item.key.as_str()),
        Some("minecraft:iron_helmet")
    );
    assert_eq!(slot("head").drop_chance, 1.0);
    assert_eq!(
        slot("mainhand").item.as_ref().map(|item| item.key.as_str()),
        Some("minecraft:iron_sword")
    );
    assert!(slot("feet").item.is_none());
    let nbt = saved(&s, zombie);
    assert_eq!(
        nbt.get_compound("equipment")
            .unwrap()
            .get_compound("head")
            .unwrap()
            .get_str("id"),
        Some("minecraft:iron_helmet")
    );

    edit(
        &mut s,
        zombie,
        EntityEdit::SetEquipment {
            slot: "head".into(),
            item: None,
        },
    );
    edit(
        &mut s,
        zombie,
        EntityEdit::SetEquipment {
            slot: "tail".into(),
            item: None,
        },
    );
    let living = details(&mut s, zombie).living.unwrap();
    assert!(
        living
            .equipment
            .iter()
            .find(|view| view.slot == "head")
            .unwrap()
            .item
            .is_none()
    );
    assert!(warned(&s, "entity.set_equipment"));
}

#[test]
fn a_mob_with_its_ai_off_does_not_move_and_one_with_ai_does() {
    let mut s = arena();
    let player = s.server.player_entity_id(s.players[0].id).unwrap();
    let still = spawn_at(&mut s, "minecraft:zombie", Vec3::new(8.5, 64.0, 4.5));
    let walker = spawn_at(&mut s, "minecraft:zombie", Vec3::new(0.5, 64.0, 4.5));
    for zombie in [still, walker] {
        edit(&mut s, zombie, EntityEdit::SetTarget(player));
        edit(&mut s, zombie, EntityEdit::SetPersistent(true));
    }
    edit(&mut s, still, EntityEdit::SetAi(false));
    let still_before = details(&mut s, still).entity.position;
    let walker_before = details(&mut s, walker).entity.position;

    s.tick(60);

    assert_eq!(details(&mut s, still).entity.position, still_before);
    let walker_after = details(&mut s, walker).entity.position;
    assert!(walker_after.distance_squared(walker_before) > 0.01);
    assert_eq!(saved(&s, still).get_bool("NoAI"), Some(true));
    assert!(!details(&mut s, still).living.unwrap().ai);
}

#[test]
fn health_is_set_inside_its_bounds_only() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::SetHealth(5.0));
    assert_eq!(details(&mut s, zombie).health, Some(5.0));
    edit(&mut s, zombie, EntityEdit::SetHealth(500.0));
    assert!(warned(&s, "entity.set_health"));
    assert_eq!(details(&mut s, zombie).health, Some(5.0));
    edit(&mut s, zombie, EntityEdit::SetAbsorption(4.0));
    let living = details(&mut s, zombie).living.unwrap();
    assert_eq!(living.absorption, 4.0);
    assert_eq!(living.max_health, 20.0);
}

#[test]
fn mob_flags_loot_and_hand_are_set() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::SetCanPickUpLoot(true));
    edit(&mut s, zombie, EntityEdit::SetLeftHanded(true));

    let living = details(&mut s, zombie).living.unwrap();
    assert!(living.can_pick_up_loot && living.left_handed);
    let nbt = saved(&s, zombie);
    assert_eq!(nbt.get_bool("CanPickUpLoot"), Some(true));
    assert_eq!(nbt.get_bool("LeftHanded"), Some(true));
}

#[test]
fn a_mob_is_leashed_to_a_holder_and_freed() {
    let mut s = arena();
    let cow = spawn(&mut s, "minecraft:cow");
    let holder = spawn_at(&mut s, "minecraft:sheep", Vec3::new(5.5, 64.0, 4.5));

    edit(&mut s, cow, EntityEdit::SetLeashHolder(holder));
    s.tick(2);
    assert_eq!(
        details(&mut s, cow).living.unwrap().leash_holder,
        Some(holder)
    );

    edit(&mut s, cow, EntityEdit::ClearLeash);
    assert_eq!(details(&mut s, cow).living.unwrap().leash_holder, None);
    edit(&mut s, cow, EntityEdit::SetLeashHolder(cow));
    assert!(warned(&s, "entity.set_leash_holder"));
}

#[test]
fn a_rider_mounts_dismounts_and_a_vehicle_ejects() {
    let mut s = arena();
    let pig = spawn(&mut s, "minecraft:pig");
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::Mount(pig));
    s.tick(1);
    let vehicle = details(&mut s, pig);
    let rider = details(&mut s, zombie);
    assert_eq!(vehicle.passengers, vec![zombie]);
    assert_eq!(rider.vehicle, Some(pig));

    edit(&mut s, zombie, EntityEdit::Dismount);
    s.tick(1);
    assert_eq!(details(&mut s, zombie).vehicle, None);

    edit(&mut s, zombie, EntityEdit::Mount(pig));
    s.tick(1);
    edit(&mut s, pig, EntityEdit::EjectPassengers);
    s.tick(1);
    assert!(details(&mut s, pig).passengers.is_empty());
    edit(&mut s, zombie, EntityEdit::Dismount);
    assert!(warned(&s, "entity.dismount"));
}

#[test]
fn a_wolf_is_tamed_owned_and_sits() {
    let mut s = arena();
    let wolf = spawn(&mut s, "minecraft:wolf");
    let owner = s.players[0].id.0;

    edit(&mut s, wolf, EntityEdit::SetOwner(owner));
    edit(&mut s, wolf, EntityEdit::SetSitting(true));

    let living = details(&mut s, wolf).living.unwrap();
    assert_eq!(living.owner, Some(owner));
    assert!(living.tamed && living.sitting);
    assert!(saved(&s, wolf).get("Owner").is_some());

    edit(&mut s, wolf, EntityEdit::SetTamed(false));
    let living = details(&mut s, wolf).living.unwrap();
    assert!(!living.tamed);
    assert_eq!(living.owner, None);

    let zombie = spawn(&mut s, "minecraft:zombie");
    edit(&mut s, zombie, EntityEdit::SetSitting(true));
    assert!(warned(&s, "entity.set_sitting"));
}

#[test]
fn a_mob_takes_a_target_and_forgets_it() {
    let mut s = arena();
    let player = s.server.player_entity_id(s.players[0].id).unwrap();
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::SetTarget(player));
    assert_eq!(details(&mut s, zombie).living.unwrap().target, Some(player));

    edit(&mut s, zombie, EntityEdit::ClearTarget);
    assert_eq!(details(&mut s, zombie).living.unwrap().target, None);
    edit(&mut s, zombie, EntityEdit::SetTarget(EntityId(987_654)));
    assert!(warned(&s, "entity.set_target"));
}

#[test]
fn an_animal_ages_locks_and_enters_love() {
    let mut s = arena();
    let sheep = spawn(&mut s, "minecraft:sheep");

    edit(&mut s, sheep, EntityEdit::SetAge(-6000));
    edit(&mut s, sheep, EntityEdit::SetAgeLocked(true));
    let living = details(&mut s, sheep).living.unwrap();
    assert!(living.baby && living.age_locked);
    assert_eq!(living.age, -6000);

    edit(&mut s, sheep, EntityEdit::SetAge(0));
    edit(&mut s, sheep, EntityEdit::SetBreeding(true));
    let living = details(&mut s, sheep).living.unwrap();
    assert!(!living.baby && living.in_love);
    edit(&mut s, sheep, EntityEdit::SetBreeding(false));
    assert!(!details(&mut s, sheep).living.unwrap().in_love);

    let zombie = spawn(&mut s, "minecraft:zombie");
    edit(&mut s, zombie, EntityEdit::SetBreeding(true));
    assert!(warned(&s, "entity.set_breeding"));
}

#[test]
fn a_non_living_entity_refuses_the_living_edits() {
    let mut s = arena();
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: "minecraft:tnt".into(),
        position: AT,
    }) else {
        panic!("expected a tnt");
    };

    edit(&mut s, view.id, EntityEdit::SetHealth(1.0));

    assert!(warned(&s, "entity.set_health"));
}

#[test]
fn an_event_carrying_the_entity_reflects_its_new_name_and_tags() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityDamage)]);
    stream_chunk_of(&mut s, AT.block());
    let zombie = spawn(&mut s, "minecraft:zombie");
    edit(&mut s, zombie, EntityEdit::SetCustomName("§6Bob".into()));
    edit(&mut s, zombie, EntityEdit::AddTag("boss".into()));

    s.act(Order::Entity {
        entity: zombie,
        order: EntityOrder::Damage(1.0),
    });

    let events = settled_for(&s.plugin, EventKind::EntityDamage)
        .into_iter()
        .chain(asked_for(&s.plugin, EventKind::EntityDamage))
        .collect::<Vec<_>>();
    let EventPayload::EntityDamage(damage) = &events.first().expect("a damage event").payload
    else {
        panic!("expected an entity damage");
    };
    assert_eq!(damage.entity.marks.custom_name(), Some("§6Bob"));
    assert_eq!(damage.entity.marks.tags(), ["boss".to_owned()]);
}

#[test]
fn mounting_and_targeting_go_through_the_vanilla_events() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityMount),
            observe(2, EventKind::EntityDismount),
            observe(3, EventKind::EntityTargetLivingEntity),
        ],
    );
    stream_chunk_of(&mut s, AT.block());
    let player = s.server.player_entity_id(s.players[0].id).unwrap();
    let pig = spawn(&mut s, "minecraft:pig");
    let zombie = spawn(&mut s, "minecraft:zombie");

    edit(&mut s, zombie, EntityEdit::Mount(pig));
    s.tick(1);
    edit(&mut s, zombie, EntityEdit::Dismount);
    s.tick(1);
    edit(&mut s, zombie, EntityEdit::SetTarget(player));

    let kinds: Vec<EventKind> = fired(&s.plugin)
        .into_iter()
        .map(|event| event.payload.kind())
        .collect();
    for kind in [
        EventKind::EntityMount,
        EventKind::EntityDismount,
        EventKind::EntityTargetLivingEntity,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} fired");
    }
}
