mod common;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::events::Inbound;
use rspigot_core::permissions::Permissions;
use rspigot_core::reload::ReloadData;
use rspigot_domain::player::PlayerId;

fn rights(permissions: &Permissions, name: &str, level: u8) -> rspigot_core::permissions::Rights {
    permissions.rights(PlayerId::offline(name), name, level)
}

fn shop() -> Permissions {
    Permissions::default()
        .with_group("default", &["chat.use"], &[])
        .with_group("vip", &["kit.use", "chat.color"], &["default"])
        .with_player("Steve", &["vip"])
}

#[test]
fn a_vip_has_the_kit_right() {
    let permissions = shop();
    assert!(rights(&permissions, "Steve", 0).has("kit.use"));
    assert!(rights(&permissions, "Steve", 0).has("chat.color"));
    assert!(!rights(&permissions, "Alex", 0).has("kit.use"));
}

#[test]
fn every_player_belongs_to_the_default_group() {
    let permissions = shop();
    assert!(rights(&permissions, "Alex", 0).has("chat.use"));
}

#[test]
fn a_group_inherits_the_rights_of_its_parents() {
    assert!(rights(&shop(), "Steve", 0).has("chat.use"));
}

#[test]
fn an_inheritance_cycle_does_not_loop() {
    let permissions = Permissions::default()
        .with_group("a", &["x.a"], &["b"])
        .with_group("b", &["x.b"], &["a"])
        .with_player("Steve", &["a"]);
    let rights = rights(&permissions, "Steve", 0);
    assert!(rights.has("x.a") && rights.has("x.b"));
}

#[test]
fn several_groups_add_up() {
    let permissions = Permissions::default()
        .with_group("vip", &["kit.use"], &[])
        .with_group("builder", &["world.edit"], &[])
        .with_player("Steve", &["vip", "builder"]);
    let rights = rights(&permissions, "Steve", 0);
    assert!(rights.has("kit.use") && rights.has("world.edit"));
}

#[test]
fn a_negation_wins_over_a_grant() {
    let permissions = Permissions::default()
        .with_group("default", &["kit.*"], &[])
        .with_group("banned", &["-kit.use"], &[])
        .with_player("Steve", &["banned"]);
    let rights = rights(&permissions, "Steve", 0);
    assert!(!rights.has("kit.use"));
    assert!(rights.has("kit.other"));
}

#[test]
fn a_wildcard_grants_everything_below_it() {
    let permissions = Permissions::default()
        .with_group("admin", &["kit.*"], &[])
        .with_group("root", &["*"], &[])
        .with_player("Steve", &["admin"])
        .with_player("Root", &["root"]);
    assert!(rights(&permissions, "Steve", 0).has("kit.use.deep"));
    assert!(!rights(&permissions, "Steve", 0).has("chat.use"));
    assert!(rights(&permissions, "Root", 0).has("anything.at.all"));
}

#[test]
fn a_player_is_found_by_uuid_or_by_name() {
    let id = PlayerId::offline("Steve");
    let permissions = Permissions::default()
        .with_group("vip", &["kit.use"], &[])
        .with_player(&id.0.hyphenated().to_string(), &["vip"]);
    assert!(permissions.rights(id, "Steve", 0).has("kit.use"));
    let by_name = Permissions::default()
        .with_group("vip", &["kit.use"], &[])
        .with_player("steve", &["vip"]);
    assert!(by_name.rights(id, "Steve", 0).has("kit.use"));
}

#[test]
fn an_op_level_4_has_every_right() {
    let permissions = Permissions::default().with_group("banned", &["-kit.use"], &[]);
    assert!(rights(&permissions, "Owner", 4).has("kit.use"));
    assert!(rights(&permissions, "Owner", 4).has("whatever"));
    assert!(!rights(&permissions, "Mod", 3).has("kit.use"));
}

#[test]
fn a_reload_applies_a_changed_group() {
    let mut server = configured(|config| {
        config.admin = AdminLists {
            operators: vec![ListEntry::named("Owner")],
            ..AdminLists::default()
        };
    })
    .with_permissions(shop());
    let steve = arrive(&mut server, "Steve");
    assert!(server.rights_of(steve.id).expect("online").has("kit.use"));
    let changed = Permissions::default().with_group("vip", &["kit.other"], &[]);
    server.handle(Inbound::Reload(Box::new(ReloadData {
        admin: AdminLists::default(),
        config: None,
        permissions: Some(changed.with_player("Steve", &["vip"])),
    })));
    let rights = server.rights_of(steve.id).expect("online");
    assert!(!rights.has("kit.use"));
    assert!(rights.has("kit.other"));
}
