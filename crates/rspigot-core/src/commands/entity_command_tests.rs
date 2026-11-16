use rspigot_domain::player::PlayerId;

use super::{CommandContext, CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    server.deliver();
    link.take();
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn max_health(&self) -> f32 {
        crate::health::max_health(self.server.players.get(&self.id).expect("player"))
    }

    fn held_levels(&self) -> Vec<(String, u8)> {
        let stack = self.server.main_hand(super::Target::Player(self.id));
        stack
            .map(|s| rspigot_domain::enchant::levels(&s))
            .unwrap_or_default()
    }
}

#[test]
fn attribute_reads_the_value_and_the_base_of_a_player() {
    let mut rig = rig();
    assert_eq!(
        rig.run("attribute @s minecraft:max_health get"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.attribute.value.get.success attribute.name.max_health Alice 20"]
    );
    assert_eq!(
        rig.run("attribute @s max_health base get 10"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.attribute.base_value.get.success attribute.name.max_health Alice 20"]
    );
}

#[test]
fn attribute_base_set_and_reset_change_the_player_max_health() {
    let mut rig = rig();
    assert_eq!(
        rig.run("attribute @s max_health base set 40"),
        CommandOutcome::Success
    );
    assert_eq!(rig.max_health(), 40.0);
    assert_eq!(
        rig.texts(),
        ["commands.attribute.base_value.set.success attribute.name.max_health Alice 40"]
    );
    assert_eq!(
        rig.run("attribute @s max_health base reset"),
        CommandOutcome::Success
    );
    assert_eq!(rig.max_health(), 20.0);
    assert_eq!(
        rig.texts(),
        ["commands.attribute.base_value.reset.success attribute.name.max_health Alice 20"]
    );
}

#[test]
fn attribute_modifiers_add_once_apply_and_remove() {
    let mut rig = rig();
    assert_eq!(
        rig.run("attribute @s max_health modifier add test:more 10 add_value"),
        CommandOutcome::Success
    );
    assert_eq!(rig.max_health(), 30.0);
    assert_eq!(
        rig.run("attribute @s max_health modifier add test:more 10 add_value"),
        CommandOutcome::Failure
    );
    rig.texts();
    assert_eq!(
        rig.run("attribute @s max_health modifier value get test:more"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.attribute.modifier.value.get.success test:more attribute.name.max_health Alice 10"
        ]
    );
    assert_eq!(
        rig.run("attribute @s max_health modifier add test:half 0.5 add_multiplied_total"),
        CommandOutcome::Success
    );
    assert_eq!(rig.max_health(), 45.0);
    assert_eq!(
        rig.run("attribute @s max_health modifier remove test:more"),
        CommandOutcome::Success
    );
    assert_eq!(rig.max_health(), 30.0);
    assert_eq!(
        rig.run("attribute @s max_health modifier remove test:more"),
        CommandOutcome::Failure
    );
}

#[test]
fn attribute_rejects_unknown_attributes_and_attributes_a_target_lacks() {
    let mut rig = rig();
    assert_eq!(rig.run("attribute @s nothing get"), CommandOutcome::Failure);
    rig.run("summon minecraft:cow 5 70 5");
    rig.texts();
    assert_eq!(
        rig.run("attribute @e[type=cow,limit=1] minecraft:attack_damage get"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.attribute.failed.no_attribute entity.minecraft.cow attribute.name.attack_damage"
        ]
    );
}

#[test]
fn attribute_changes_a_mob_and_caps_its_health() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.texts();
    assert_eq!(
        rig.run("attribute @e[type=cow,limit=1] max_health base set 4"),
        CommandOutcome::Success
    );
    let cow = rig
        .server
        .select_entities(CommandSender::Player(rig.id), "@e[type=cow]")
        .expect("parses")[0];
    let super::Target::Entity(id) = cow else {
        panic!("a mob");
    };
    let entity = rig.server.entities.store.get(id).expect("cow");
    assert_eq!(rspigot_domain::entity::living::health(&entity.base), 4.0);
}

#[test]
fn damage_hurts_a_mob_by_the_given_amount() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.texts();
    let health = |rig: &Rig| {
        let id = match rig
            .server
            .select_entities(CommandSender::Player(rig.id), "@e[type=cow]")
            .expect("parses")[0]
        {
            super::Target::Entity(id) => id,
            super::Target::Player(_) => panic!("a mob"),
        };
        rspigot_domain::entity::living::health(
            &rig.server.entities.store.get(id).expect("cow").base,
        )
    };
    let before = health(&rig);
    assert_eq!(
        rig.run("damage @e[type=cow,limit=1] 3 minecraft:generic"),
        CommandOutcome::Success
    );
    assert_eq!(health(&rig), before - 3.0);
    assert_eq!(
        rig.texts(),
        ["commands.damage.success 3 entity.minecraft.cow"]
    );
}

#[test]
fn damage_refuses_an_unknown_type_and_a_negative_amount() {
    let mut rig = rig();
    assert_eq!(
        rig.run("damage @s 3 minecraft:nothing"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.run("damage @s -1"), CommandOutcome::Failure);
    assert_eq!(rig.run("damage"), CommandOutcome::Failure);
}

#[test]
fn damage_by_an_entity_and_at_a_position_respects_the_hurt_window() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.run("summon minecraft:pig 6 70 5");
    rig.texts();
    assert_eq!(
        rig.run("damage @e[type=cow,limit=1] 2 minecraft:mob_attack by @e[type=pig,limit=1]"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("damage @e[type=cow,limit=1] 2 minecraft:generic at 1 70 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("damage @e[type=cow,limit=1] 5 minecraft:generic at 1 70 1"),
        CommandOutcome::Success
    );
}

#[test]
fn enchant_adds_a_level_to_the_held_item() {
    let mut rig = rig();
    rig.run("give @s minecraft:diamond_sword");
    rig.texts();
    assert_eq!(
        rig.run("enchant @s minecraft:sharpness 3"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.held_levels(),
        vec![("minecraft:sharpness".to_owned(), 3)]
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.enchant.success.single enchantment.minecraft.sharpness enchantment.level.3 Alice"
        ]
    );
    assert_eq!(rig.run("enchant @s unbreaking"), CommandOutcome::Success);
    assert_eq!(rig.held_levels().len(), 2);
}

#[test]
fn enchant_refuses_levels_incompatible_items_and_empty_hands() {
    let mut rig = rig();
    assert_eq!(
        rig.run("enchant @s minecraft:sharpness"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.enchant.failed.itemless Alice"]);
    rig.run("give @s minecraft:diamond_sword");
    rig.texts();
    assert_eq!(
        rig.run("enchant @s minecraft:sharpness 6"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.enchant.failed.level 6 5"]);
    assert_eq!(rig.run("enchant @s sharpness 5"), CommandOutcome::Success);
    assert_eq!(rig.run("enchant @s smite"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("enchant @s minecraft:protection"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.run("enchant @s nothing"), CommandOutcome::Failure);
}
