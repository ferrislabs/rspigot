mod fake_host;

use std::net::IpAddr;
use std::time::{Duration, UNIX_EPOCH};

use fake_host::*;
use rspigot_plugin::{
    ArmorSlot, BlockType, Color, Effect, EffectType, Enchantment, Error, GameMode, InventorySlot,
    ItemStack, Material, Particle, Position, Rotation, Sound, Title, Uuid, Vector,
};
use rspigot_plugin_protocol::v1::{
    self, answer, from_plugin, inventory_order, particle, player_order, question,
};

fn ordered(order: player_order::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(order),
    })
}

fn stocked(order: inventory_order::Order) -> from_plugin::Body {
    from_plugin::Body::Inventory(v1::InventoryOrder {
        player: STEVE.to_owned(),
        order: Some(order),
    })
}

const fn vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
    v1::Vec3 { x, y, z }
}

fn set_slot(slot: i32, item: Option<v1::Item>) -> from_plugin::Body {
    stocked(inventory_order::Order::Set(v1::SetSlot { slot, item }))
}

fn diamond_sword(components: Vec<u8>) -> v1::Item {
    v1::Item {
        key: "minecraft:diamond_sword".to_owned(),
        count: 1,
        name: None,
        lore: Vec::new(),
        enchantments: Vec::new(),
        components,
        custom_model_data: None,
    }
}

fn wire_effect(key: &str, duration: i32, amplifier: i32) -> v1::Effect {
    v1::Effect {
        key: key.to_owned(),
        duration,
        amplifier,
        ambient: false,
        particles: true,
        icon: true,
    }
}

fn full_state() -> v1::PlayerState {
    v1::PlayerState {
        player: Some(steve()),
        health: 17.5,
        food: 12,
        level: 3,
        max_health: 20.0,
        absorption: 4.0,
        saturation: 2.5,
        exhaustion: 1.25,
        exp: 0.5,
        total_exp: 27,
        op: true,
        allow_flight: true,
        flying: true,
        fly_speed: 0.2,
        walk_speed: 0.3,
        sneaking: true,
        sprinting: true,
        on_ground: true,
        fire_ticks: 40,
        dead: true,
        effects: vec![
            wire_effect("minecraft:speed", 200, 1),
            v1::Effect {
                ambient: true,
                particles: false,
                icon: false,
                ..wire_effect("minecraft:regeneration", -1, 0)
            },
        ],
        ping: 42,
        locale: "fr_fr".to_owned(),
        address: "203.0.113.7".to_owned(),
        hidden: vec![ALEX.to_owned()],
    }
}

fn inventory_answer(slots: Vec<v1::Slot>, held_slot: i32) -> answer::Body {
    answer::Body::Inventory(v1::InventoryState { slots, held_slot })
}

#[tokio::test]
async fn player_orders_go_on_the_wire_in_order() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let pos = Position {
                x: 1.0,
                y: 64.0,
                z: 2.0,
            };
            player.send_message("§6Hi");
            player.send_title(Title::new("A").subtitle("B"));
            player.set_health(5.0);
            player.teleport(pos);
            player.add_effect(Effect::new(EffectType::Speed, 200, 1));
            player.play_sound(Sound::EntityCowHurt, 1.0, 1.0);
            player.spawn_particle(
                Particle::Dust {
                    color: Color::rgb(255, 0, 0),
                    scale: 1.0,
                },
                pos,
                3,
                Vector::default(),
                0.0,
            );
            player.set_fly_speed(0.2);
            player.ban(Some("x".into()), None);
        })
    })
    .await;

    for expected in [
        player_order::Order::SendMessage("§6Hi".to_owned()),
        player_order::Order::SendTitle(v1::Title {
            title: "A".to_owned(),
            subtitle: Some("B".to_owned()),
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        }),
        player_order::Order::SetHealth(5.0),
        player_order::Order::Teleport(vec3(1.0, 64.0, 2.0)),
        player_order::Order::AddEffect(wire_effect("minecraft:speed", 200, 1)),
        player_order::Order::PlaySound(v1::Sound {
            key: "minecraft:entity.cow.hurt".to_owned(),
            volume: 1.0,
            pitch: 1.0,
            position: None,
        }),
        player_order::Order::SpawnParticle(v1::Particle {
            key: "minecraft:dust".to_owned(),
            data: Some(particle::Data::Dust(v1::Dust {
                color: 0xFF0000,
                scale: 1.0,
            })),
            position: Some(vec3(1.0, 64.0, 2.0)),
            count: 3,
            spread: Some(vec3(0.0, 0.0, 0.0)),
            speed: 0.0,
        }),
        player_order::Order::SetFlySpeed(0.2),
        player_order::Order::Ban(v1::BanTerms {
            reason: Some("x".to_owned()),
            expires_ms: None,
        }),
    ] {
        assert_eq!(called.session.hear().await, ordered(expected));
    }
    called.finished().await;
}

#[tokio::test]
async fn every_player_order_has_its_wire_body() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let online = player.server().players.fetch().await.unwrap();
            let alex = &online[1];
            let pos = Position {
                x: -4.5,
                y: 70.0,
                z: 8.25,
            };
            let spread = Vector {
                x: 0.5,
                y: 1.0,
                z: 1.5,
            };
            player.send_action_bar("§aBar");
            player.send_title(Title::new("T").times(5, 40, 15));
            player.reset_title();
            player.set_tab_list("Head", "Foot");
            player.teleport_facing(
                pos,
                Rotation {
                    yaw: 45.0,
                    pitch: 10.0,
                },
            );
            player.set_game_mode(GameMode::Spectator);
            player.kick("§cBye");
            player.set_absorption(4.0);
            player.damage(2.5);
            player.kill();
            player.set_food_level(18);
            player.set_saturation(3.5);
            player.set_exhaustion(0.75);
            player.give_exp(30);
            player.give_levels(-2);
            player.set_level(7);
            player.set_exp(0.25);
            player.add_effect(Effect {
                ambient: true,
                particles: false,
                icon: false,
                ..Effect::new(EffectType::Poison, Effect::INFINITE, 3)
            });
            player.remove_effect(EffectType::Poison);
            player.clear_effects();
            player.set_fire_ticks(60);
            player.set_velocity(spread);
            player.set_allow_flight(true);
            player.set_flying(false);
            player.set_walk_speed(0.1);
            player.set_op(true);
            player.respawn();
            player.perform_command("time set day");
            player.chat("/spawn");
            player.spawn_particle(Particle::Flame, pos, 1, spread, 0.5);
            player.spawn_particle(Particle::Item(Material::DiamondSword), pos, 2, spread, 0.0);
            player.spawn_particle(
                Particle::BlockCrumble(BlockType::Stone),
                pos,
                4,
                spread,
                0.0,
            );
            player.hide_player(alex);
            player.show_player(alex);
            player.close_inventory();
            player.update_inventory();
            player.ban(None, Some(UNIX_EPOCH + Duration::from_millis(1_000_000)));
            player.ban(None, Some(UNIX_EPOCH - Duration::from_millis(5_000)));
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Players(v1::Empty {}));
    called.session.say(answer(
        request,
        answer::Body::Players(v1::PlayerList {
            players: vec![steve(), alex()],
        }),
    ));
    let particle = |key: &str, data, count, speed| v1::Particle {
        key: key.to_owned(),
        data,
        position: Some(vec3(-4.5, 70.0, 8.25)),
        count,
        spread: Some(vec3(0.5, 1.0, 1.5)),
        speed,
    };
    for expected in [
        player_order::Order::SendActionBar("§aBar".to_owned()),
        player_order::Order::SendTitle(v1::Title {
            title: "T".to_owned(),
            subtitle: None,
            fade_in: 5,
            stay: 40,
            fade_out: 15,
        }),
        player_order::Order::ResetTitle(v1::Empty {}),
        player_order::Order::SetTabList(v1::TabList {
            header: "Head".to_owned(),
            footer: "Foot".to_owned(),
        }),
        player_order::Order::TeleportFacing(v1::Facing {
            position: Some(vec3(-4.5, 70.0, 8.25)),
            rotation: Some(v1::Rotation {
                yaw: 45.0,
                pitch: 10.0,
            }),
        }),
        player_order::Order::SetGameMode(v1::GameMode::Spectator as i32),
        player_order::Order::Kick("§cBye".to_owned()),
        player_order::Order::SetAbsorption(4.0),
        player_order::Order::Damage(2.5),
        player_order::Order::Kill(v1::Empty {}),
        player_order::Order::SetFoodLevel(18),
        player_order::Order::SetSaturation(3.5),
        player_order::Order::SetExhaustion(0.75),
        player_order::Order::GiveExp(30),
        player_order::Order::GiveLevels(-2),
        player_order::Order::SetLevel(7),
        player_order::Order::SetExp(0.25),
        player_order::Order::AddEffect(v1::Effect {
            key: "minecraft:poison".to_owned(),
            duration: -1,
            amplifier: 3,
            ambient: true,
            particles: false,
            icon: false,
        }),
        player_order::Order::RemoveEffect("minecraft:poison".to_owned()),
        player_order::Order::ClearEffects(v1::Empty {}),
        player_order::Order::SetFireTicks(60),
        player_order::Order::SetVelocity(vec3(0.5, 1.0, 1.5)),
        player_order::Order::SetAllowFlight(true),
        player_order::Order::SetFlying(false),
        player_order::Order::SetWalkSpeed(0.1),
        player_order::Order::SetOp(true),
        player_order::Order::Respawn(v1::Empty {}),
        player_order::Order::PerformCommand("time set day".to_owned()),
        player_order::Order::Chat("/spawn".to_owned()),
        player_order::Order::SpawnParticle(particle("minecraft:flame", None, 1, 0.5)),
        player_order::Order::SpawnParticle(particle(
            "minecraft:item",
            Some(particle::Data::Item("minecraft:diamond_sword".to_owned())),
            2,
            0.0,
        )),
        player_order::Order::SpawnParticle(particle(
            "minecraft:block_crumble",
            Some(particle::Data::Block("minecraft:stone".to_owned())),
            4,
            0.0,
        )),
        player_order::Order::HidePlayer(ALEX.to_owned()),
        player_order::Order::ShowPlayer(ALEX.to_owned()),
        player_order::Order::CloseInventory(v1::Empty {}),
        player_order::Order::UpdateInventory(v1::Empty {}),
        player_order::Order::Ban(v1::BanTerms {
            reason: None,
            expires_ms: Some(1_000_000),
        }),
        player_order::Order::Ban(v1::BanTerms {
            reason: None,
            expires_ms: Some(-5_000),
        }),
    ] {
        assert_eq!(called.session.hear().await, ordered(expected));
    }
    called.finished().await;
}

#[tokio::test]
async fn inventory_calls_use_spigot_slot_numbers() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let inventory = player.inventory();
            let stone = || Some(ItemStack::new(Material::Stone, 64));
            inventory.set(InventorySlot::Armor(ArmorSlot::Helmet), stone());
            inventory.set(InventorySlot::Armor(ArmorSlot::Boots), stone());
            inventory.set(InventorySlot::Offhand, stone());
            inventory.set(InventorySlot::Hotbar(2), stone());
            inventory.set(InventorySlot::Main(0), stone());
            inventory.set(InventorySlot::Hotbar(9), stone());
            inventory.set(InventorySlot::Main(27), None);
            inventory.set(InventorySlot::Armor(ArmorSlot::Leggings), None);
            inventory.set(InventorySlot::Armor(ArmorSlot::Chestplate), None);
            inventory.add(
                ItemStack::new(Material::DiamondSword, 1)
                    .name("§6X")
                    .lore(["a"])
                    .enchant(Enchantment::Sharpness, 5),
            );
            inventory.remove(Material::Cobblestone, 32);
            inventory.clear();
            inventory.set_held_slot(4);
        })
    })
    .await;

    let stone = || {
        Some(v1::Item {
            key: "minecraft:stone".to_owned(),
            count: 64,
            ..v1::Item::default()
        })
    };
    for expected in [
        set_slot(39, stone()),
        set_slot(36, stone()),
        set_slot(40, stone()),
        set_slot(2, stone()),
        set_slot(9, stone()),
        set_slot(-1, stone()),
        set_slot(-1, None),
        set_slot(37, None),
        set_slot(38, None),
        stocked(inventory_order::Order::Add(v1::Item {
            key: "minecraft:diamond_sword".to_owned(),
            count: 1,
            name: Some("§6X".to_owned()),
            lore: vec!["a".to_owned()],
            enchantments: vec![v1::Enchant {
                key: "minecraft:sharpness".to_owned(),
                level: 5,
            }],
            components: vec![],
            custom_model_data: None,
        })),
        stocked(inventory_order::Order::Remove(v1::RemoveItems {
            key: "minecraft:cobblestone".to_owned(),
            count: 32,
        })),
        stocked(inventory_order::Order::Clear(v1::Empty {})),
        stocked(inventory_order::Order::SetHeldSlot(4)),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }
    called.finished().await;
}

#[tokio::test]
async fn an_item_keeps_one_level_per_enchantment_and_its_last_name_and_lore() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.inventory().add(
                ItemStack::new(Material::DiamondSword, 1)
                    .name("first")
                    .name("second")
                    .lore(["old"])
                    .lore(vec![String::from("one"), String::from("two")])
                    .enchant(Enchantment::Sharpness, 1)
                    .enchant(Enchantment::Unbreaking, 3)
                    .enchant(Enchantment::Sharpness, 4),
            );
        })
    })
    .await;

    assert_eq!(
        called.session.hear().await,
        stocked(inventory_order::Order::Add(v1::Item {
            key: "minecraft:diamond_sword".to_owned(),
            count: 1,
            name: Some("second".to_owned()),
            lore: vec!["one".to_owned(), "two".to_owned()],
            enchantments: vec![
                v1::Enchant {
                    key: "minecraft:sharpness".to_owned(),
                    level: 4,
                },
                v1::Enchant {
                    key: "minecraft:unbreaking".to_owned(),
                    level: 3,
                },
            ],
            components: vec![],
            custom_model_data: None,
        }))
    );
    called.finished().await;
}

#[tokio::test]
async fn resolve_reads_the_full_player_state() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let state = player.resolve().await.unwrap();
            assert_eq!(state.player.uuid.to_string(), STEVE);
            assert_eq!(state.player.name, "Steve");
            assert_eq!(state.health, 17.5);
            assert_eq!(state.max_health, 20.0);
            assert_eq!(state.absorption, 4.0);
            assert_eq!(state.food_level, 12);
            assert_eq!(state.saturation, 2.5);
            assert_eq!(state.exhaustion, 1.25);
            assert_eq!(state.level, 3);
            assert_eq!(state.exp, 0.5);
            assert_eq!(state.total_experience, 27);
            assert!(state.op);
            assert!(state.allow_flight);
            assert!(state.flying);
            assert_eq!(state.fly_speed, 0.2);
            assert_eq!(state.walk_speed, 0.3);
            assert!(state.sneaking);
            assert!(state.sprinting);
            assert!(state.on_ground);
            assert_eq!(state.fire_ticks, 40);
            assert!(state.dead);
            assert_eq!(
                state.effects,
                [
                    Effect::new(EffectType::Speed, 200, 1),
                    Effect {
                        kind: EffectType::Regeneration,
                        duration: Effect::INFINITE,
                        amplifier: 0,
                        ambient: true,
                        particles: false,
                        icon: false,
                    },
                ]
            );
            assert_eq!(state.ping, 42);
            assert_eq!(state.locale, "fr_fr");
            assert_eq!(state.address, "203.0.113.7".parse::<IpAddr>().unwrap());
            assert_eq!(state.hidden, [Uuid::parse_str(ALEX).unwrap()]);
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Player(STEVE.to_owned()));
    called
        .session
        .say(answer(request, answer::Body::Player(full_state())));
    called.finished().await;
}

#[tokio::test]
async fn inventory_resolve_reads_every_slot_and_the_held_slot() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let state = player.inventory().resolve().await.unwrap();
            assert_eq!(state.held_slot, 3);
            let slots: Vec<InventorySlot> = state.slots.iter().map(|(slot, _)| *slot).collect();
            let mut expected: Vec<InventorySlot> = (0..9).map(InventorySlot::Hotbar).collect();
            expected.extend((0..27).map(InventorySlot::Main));
            expected.extend(
                [
                    ArmorSlot::Boots,
                    ArmorSlot::Leggings,
                    ArmorSlot::Chestplate,
                    ArmorSlot::Helmet,
                ]
                .map(InventorySlot::Armor),
            );
            expected.push(InventorySlot::Offhand);
            assert_eq!(slots, expected);
            assert_eq!(
                state.slots[0].1,
                Some(
                    ItemStack::new(Material::DiamondSword, 1)
                        .name("§6X")
                        .lore(["a", "b"])
                        .enchant(Enchantment::Sharpness, 5)
                )
            );
            assert_eq!(
                state.slots[39].1,
                Some(ItemStack::new(Material::Cobblestone, 7))
            );
            assert!(
                state
                    .slots
                    .iter()
                    .enumerate()
                    .all(|(index, (_, item))| item.is_some() == (index == 0 || index == 39))
            );
        })
    })
    .await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Inventory(STEVE.to_owned()));
    let slots = (0..=40)
        .map(|slot| v1::Slot {
            slot,
            item: match slot {
                0 => Some(v1::Item {
                    name: Some("§6X".to_owned()),
                    lore: vec!["a".to_owned(), "b".to_owned()],
                    enchantments: vec![v1::Enchant {
                        key: "minecraft:sharpness".to_owned(),
                        level: 5,
                    }],
                    ..diamond_sword(Vec::new())
                }),
                39 => Some(v1::Item {
                    key: "minecraft:cobblestone".to_owned(),
                    count: 7,
                    ..v1::Item::default()
                }),
                _ => None,
            },
        })
        .collect();
    called
        .session
        .say(answer(request, inventory_answer(slots, 3)));
    called.finished().await;
}

#[tokio::test]
async fn an_item_read_keeps_its_hidden_components_when_written_back() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let inventory = player.inventory();
            let state = inventory.resolve().await.unwrap();
            let (slot, item) = state.slots.into_iter().next().unwrap();
            assert_eq!(slot, InventorySlot::Hotbar(0));
            inventory.set(InventorySlot::Main(5), item);
        })
    })
    .await;

    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        inventory_answer(
            vec![v1::Slot {
                slot: 0,
                item: Some(diamond_sword(vec![1, 2, 3])),
            }],
            0,
        ),
    ));
    assert_eq!(
        called.session.hear().await,
        set_slot(14, Some(diamond_sword(vec![1, 2, 3])))
    );
    called.finished().await;
}

#[tokio::test]
async fn a_player_answer_the_sdk_cannot_read_is_malformed() {
    let mut called = calling(|player| {
        Box::pin(async move {
            for _ in 0..5 {
                let resolved = player.resolve().await;
                assert!(matches!(resolved, Err(Error::Malformed(_))), "{resolved:?}");
            }
            assert!(matches!(player.resolve().await, Err(Error::NotFound)));
        })
    })
    .await;

    let unknown_effect = v1::PlayerState {
        effects: vec![wire_effect("minecraft:levitation_plus", 20, 0)],
        ..full_state()
    };
    let huge_amplifier = v1::PlayerState {
        effects: vec![wire_effect("minecraft:speed", 20, 256)],
        ..full_state()
    };
    let bad_address = v1::PlayerState {
        address: "not an address".to_owned(),
        ..full_state()
    };
    let bad_hidden = v1::PlayerState {
        hidden: vec!["alex".to_owned()],
        ..full_state()
    };
    for body in [
        answer::Body::Player(unknown_effect),
        answer::Body::Player(huge_amplifier),
        answer::Body::Player(bad_address),
        answer::Body::Player(bad_hidden),
        inventory_answer(Vec::new(), 0),
        answer::Body::NotFound(v1::NotFound {}),
    ] {
        let (request, _) = called.session.asked().await;
        called.session.say(answer(request, body));
    }
    called.finished().await;
}

#[tokio::test]
async fn an_inventory_answer_the_sdk_cannot_read_is_malformed() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let inventory = player.inventory();
            for _ in 0..7 {
                let resolved = inventory.resolve().await;
                assert!(matches!(resolved, Err(Error::Malformed(_))), "{resolved:?}");
            }
            assert!(matches!(inventory.resolve().await, Err(Error::NotFound)));
        })
    })
    .await;

    let holding = |slot, item| {
        inventory_answer(
            vec![v1::Slot {
                slot,
                item: Some(item),
            }],
            0,
        )
    };
    for body in [
        holding(
            0,
            v1::Item {
                key: "minecraft:diamond_swordfish".to_owned(),
                ..diamond_sword(Vec::new())
            },
        ),
        holding(
            0,
            v1::Item {
                enchantments: vec![v1::Enchant {
                    key: "minecraft:sharpest".to_owned(),
                    level: 1,
                }],
                ..diamond_sword(Vec::new())
            },
        ),
        holding(
            0,
            v1::Item {
                enchantments: vec![v1::Enchant {
                    key: "minecraft:sharpness".to_owned(),
                    level: -1,
                }],
                ..diamond_sword(Vec::new())
            },
        ),
        holding(
            0,
            v1::Item {
                count: 256,
                ..diamond_sword(Vec::new())
            },
        ),
        holding(41, diamond_sword(Vec::new())),
        inventory_answer(Vec::new(), 9),
        answer::Body::Players(v1::PlayerList::default()),
        answer::Body::NotFound(v1::NotFound {}),
    ] {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, question::Question::Inventory(STEVE.to_owned()));
        called.session.say(answer(request, body));
    }
    called.finished().await;
}
