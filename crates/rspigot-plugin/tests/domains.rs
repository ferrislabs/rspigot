mod fake_host;

use std::net::IpAddr;
use std::time::{Duration, UNIX_EPOCH};

use fake_host::*;
use rspigot_plugin::{
    ActionErrorKind, Ban, BanTerms, BlockPos, Color, Difficulty, Effect, EffectType, Error,
    Explosion, Facing, GameMode, GameRule, GameRuleSetting, InventorySlot, ItemRemoval, ItemStack,
    Location, Material, Particle, ParticleBurst, Position, Rotation, SlotItem, Sound, SoundCue,
    TabList, Title, Vector, Weather,
};
use rspigot_plugin_protocol::v1::{
    self, answer, from_plugin, inventory_order, particle, player_order, server_order, world_order,
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

fn compass(count: i32) -> v1::Item {
    v1::Item {
        key: "minecraft:compass".to_owned(),
        count,
        name: None,
        lore: Vec::new(),
        enchantments: Vec::new(),
        components: Vec::new(),
        custom_model_data: None,
    }
}

async fn heard(called: &mut Called, expected: Vec<from_plugin::Body>) {
    for body in expected {
        assert_eq!(called.session.hear().await, body);
    }
    called.finished().await;
}

#[tokio::test]
async fn player_inventory_close_sends_the_close_order() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let inventory = player.inventory();
            inventory.close();
            inventory.update();
            inventory.add(ItemStack::new(Material::Compass, 2));
            inventory.set(SlotItem::new(InventorySlot::Offhand, None));
            inventory.remove(ItemRemoval::new(Material::Compass, 1));
            inventory.clear();
            inventory.set_held_slot(3);
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::CloseInventory(v1::Empty {})),
            ordered(player_order::Order::UpdateInventory(v1::Empty {})),
            stocked(inventory_order::Order::Add(compass(2))),
            stocked(inventory_order::Order::Set(v1::SetSlot {
                slot: 40,
                item: None,
            })),
            stocked(inventory_order::Order::Remove(v1::RemoveItems {
                key: "minecraft:compass".to_owned(),
                count: 1,
            })),
            stocked(inventory_order::Order::Clear(v1::Empty {})),
            stocked(inventory_order::Order::SetHeldSlot(3)),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_health_sends_the_health_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let health = player.health();
            health.set(5.0);
            health.set_absorption(2.0);
            health.damage(3.5);
            health.set_fire_ticks(40);
            health.kill();
            health.respawn();
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::SetHealth(5.0)),
            ordered(player_order::Order::SetAbsorption(2.0)),
            ordered(player_order::Order::Damage(3.5)),
            ordered(player_order::Order::SetFireTicks(40)),
            ordered(player_order::Order::Kill(v1::Empty {})),
            ordered(player_order::Order::Respawn(v1::Empty {})),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_food_sends_the_food_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let food = player.food();
            food.set_level(18);
            food.set_saturation(2.5);
            food.set_exhaustion(1.0);
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::SetFoodLevel(18)),
            ordered(player_order::Order::SetSaturation(2.5)),
            ordered(player_order::Order::SetExhaustion(1.0)),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_experience_sends_the_experience_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let experience = player.experience();
            experience.give(10);
            experience.give_levels(2);
            experience.set_level(7);
            experience.set_progress(0.5);
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::GiveExp(10)),
            ordered(player_order::Order::GiveLevels(2)),
            ordered(player_order::Order::SetLevel(7)),
            ordered(player_order::Order::SetExp(0.5)),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_effects_sends_the_effect_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let effects = player.effects();
            effects.add(Effect::new(EffectType::Speed, 200, 1));
            effects.remove(EffectType::Speed);
            effects.clear();
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::AddEffect(v1::Effect {
                key: "minecraft:speed".to_owned(),
                duration: 200,
                amplifier: 1,
                ambient: false,
                particles: true,
                icon: true,
            })),
            ordered(player_order::Order::RemoveEffect(
                "minecraft:speed".to_owned(),
            )),
            ordered(player_order::Order::ClearEffects(v1::Empty {})),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_abilities_sends_the_ability_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let abilities = player.abilities();
            abilities.set_allow_flight(true);
            abilities.set_flying(true);
            abilities.set_fly_speed(0.2);
            abilities.set_walk_speed(0.3);
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::SetAllowFlight(true)),
            ordered(player_order::Order::SetFlying(true)),
            ordered(player_order::Order::SetFlySpeed(0.2)),
            ordered(player_order::Order::SetWalkSpeed(0.3)),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_movement_sends_the_velocity_order() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.movement().set_velocity(Vector {
                x: 0.0,
                y: 1.5,
                z: 0.0,
            });
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SetVelocity(vec3(
            0.0, 1.5, 0.0,
        )))],
    )
    .await;
}

#[tokio::test]
async fn player_operator_sends_the_operator_order() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.operator().set(true);
        })
    })
    .await;

    heard(&mut called, vec![ordered(player_order::Order::SetOp(true))]).await;
}

#[tokio::test]
async fn player_connection_sends_the_kick_and_ban_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let connection = player.connection();
            connection.kick("bye");
            connection.ban(BanTerms {
                reason: Some("x".to_owned()),
                expires: Some(UNIX_EPOCH + Duration::from_secs(2)),
            });
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::Kick("bye".to_owned())),
            ordered(player_order::Order::Ban(v1::BanTerms {
                reason: Some("x".to_owned()),
                expires_ms: Some(2000),
            })),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_sound_plays_the_sound_for_the_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player
                .sound()
                .play(SoundCue::new(Sound::EntityCowHurt).volume(0.5).pitch(2.0));
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::PlaySound(v1::Sound {
            key: "minecraft:entity.cow.hurt".to_owned(),
            volume: 0.5,
            pitch: 2.0,
            position: None,
        }))],
    )
    .await;
}

#[tokio::test]
async fn player_particle_spawns_the_particle_for_the_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.particle().spawn(
                ParticleBurst::new(
                    Particle::Dust {
                        color: Color::rgb(255, 0, 0),
                        scale: 1.0,
                    },
                    Position {
                        x: 1.0,
                        y: 64.0,
                        z: 2.0,
                    },
                )
                .count(3),
            );
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SpawnParticle(v1::Particle {
            key: "minecraft:dust".to_owned(),
            data: Some(particle::Data::Dust(v1::Dust {
                color: 0xFF0000,
                scale: 1.0,
            })),
            position: Some(vec3(1.0, 64.0, 2.0)),
            count: 3,
            spread: Some(vec3(0.0, 0.0, 0.0)),
            speed: 0.0,
        }))],
    )
    .await;
}

#[tokio::test]
async fn player_message_sends_the_message_and_the_chat_line() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.message().send("§6Hi");
            player.message().chat("hello");
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::SendMessage("§6Hi".to_owned())),
            ordered(player_order::Order::Chat("hello".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_title_sends_and_resets_the_title() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.title().send(Title::new("A").subtitle("B"));
            player.title().reset();
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::SendTitle(v1::Title {
                title: "A".to_owned(),
                subtitle: Some("B".to_owned()),
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            })),
            ordered(player_order::Order::ResetTitle(v1::Empty {})),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_action_bar_sends_the_text() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.action_bar().send("§aBar");
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SendActionBar(
            "§aBar".to_owned(),
        ))],
    )
    .await;
}

#[tokio::test]
async fn player_tab_list_sets_the_header_and_footer() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.tab_list().set(TabList::new("Head", "Foot"));
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SetTabList(v1::TabList {
            header: "Head".to_owned(),
            footer: "Foot".to_owned(),
        }))],
    )
    .await;
}

#[tokio::test]
async fn player_location_waits_for_the_result_of_each_teleport() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let pos = Position {
                x: -4.5,
                y: 70.0,
                z: 8.25,
            };
            let rotation = Rotation {
                yaw: 45.0,
                pitch: 10.0,
            };
            let location = player.location();
            location.teleport(pos).await.unwrap();
            location
                .teleport_facing(Facing::new(pos, rotation))
                .await
                .unwrap();
            let refused = location
                .teleport_to(&Location {
                    world: "minecraft:the_nether".to_owned(),
                    position: pos,
                    rotation,
                })
                .await;
            let Err(Error::Action(error)) = refused else {
                panic!("expected a refused teleport, got {refused:?}");
            };
            assert_eq!(error.kind, ActionErrorKind::TargetGone);
            assert_eq!(error.reason, "the player is not online");
        })
    })
    .await;

    let pos = vec3(-4.5, 70.0, 8.25);
    let rotation = v1::Rotation {
        yaw: 45.0,
        pitch: 10.0,
    };
    let session = &mut called.session;
    session
        .performed(ordered(player_order::Order::Teleport(pos)))
        .await;
    session
        .performed(ordered(player_order::Order::TeleportFacing(v1::Facing {
            position: Some(pos),
            rotation: Some(rotation),
        })))
        .await;
    let (request, body) = session.acted().await;
    assert_eq!(
        body,
        ordered(player_order::Order::TeleportTo(v1::Location {
            world: "minecraft:the_nether".to_owned(),
            position: Some(pos),
            rotation: Some(rotation),
        }))
    );
    session.say(answer(
        request,
        answer::Body::Failed(v1::ActionError {
            kind: v1::ActionErrorKind::TargetGone as i32,
            reason: "the player is not online".to_owned(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn player_game_mode_sets_the_game_mode() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.game_mode().set(GameMode::Spectator);
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SetGameMode(
            v1::GameMode::Spectator as i32,
        ))],
    )
    .await;
}

#[tokio::test]
async fn player_command_performs_the_command_line() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.command().perform("spawn");
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::PerformCommand(
            "spawn".to_owned(),
        ))],
    )
    .await;
}

#[tokio::test]
async fn player_visibility_hides_and_shows_another_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let online = player.server().get_players().await.unwrap();
            let alex = &online[1];
            player.visibility().hide(alex);
            player.visibility().show(alex);
        })
    })
    .await;
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        v1::answer::Body::Players(v1::PlayerList {
            players: vec![steve(), alex()],
        }),
    ));

    heard(
        &mut called,
        vec![
            ordered(player_order::Order::HidePlayer(ALEX.to_owned())),
            ordered(player_order::Order::ShowPlayer(ALEX.to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn player_scoreboard_shows_the_scoreboard_to_the_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let main = player.server().scoreboard_manager().main_scoreboard();
            player.scoreboard().set(&main);
        })
    })
    .await;

    heard(
        &mut called,
        vec![ordered(player_order::Order::SetScoreboard(0))],
    )
    .await;
}

fn worldly(order: world_order::Order) -> from_plugin::Body {
    from_plugin::Body::World(v1::WorldOrder { order: Some(order) })
}

fn served(order: server_order::Order) -> from_plugin::Body {
    from_plugin::Body::Server(v1::ServerOrder { order: Some(order) })
}

#[tokio::test]
async fn world_time_weather_and_difficulty_set_their_value() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = player.world();
            world.time().set(6000);
            world.weather().set(Weather::Thunder);
            world.difficulty().set(Difficulty::Hard);
            world.pvp().set(false);
            world.spawn_point().set(BlockPos { x: 8, y: 64, z: -8 });
            world
                .game_rule()
                .set(GameRuleSetting::new(GameRule::BlockDrops, true));
            world.storage().save();
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            worldly(world_order::Order::SetTime(6000)),
            worldly(world_order::Order::SetWeather(v1::Weather::Thunder as i32)),
            worldly(world_order::Order::SetDifficulty(
                v1::Difficulty::Hard as i32,
            )),
            worldly(world_order::Order::SetPvp(false)),
            worldly(world_order::Order::SetSpawn(v1::BlockPos {
                x: 8,
                y: 64,
                z: -8,
            })),
            worldly(world_order::Order::SetGameRule(v1::SetGameRule {
                rule: v1::GameRule::BlockDrops as i32,
                value: true,
            })),
            worldly(world_order::Order::Save(v1::Empty {})),
        ],
    )
    .await;
}

#[tokio::test]
async fn world_sound_particle_and_explosion_reach_everyone_nearby() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let world = player.world();
            let here = Position {
                x: 4.5,
                y: 64.0,
                z: -2.5,
            };
            world.sound().play(
                SoundCue::new(Sound::EntityGenericExplode)
                    .volume(4.0)
                    .pitch(0.5)
                    .at(here),
            );
            world.particle().spawn(
                ParticleBurst::new(Particle::Flame, here)
                    .count(8)
                    .spread(Vector {
                        x: 0.5,
                        y: 1.0,
                        z: 1.5,
                    })
                    .speed(0.1),
            );
            world.explosion().create(Explosion::new(here, 4.0, true));
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            worldly(world_order::Order::PlaySound(v1::Sound {
                key: "minecraft:entity.generic.explode".to_owned(),
                volume: 4.0,
                pitch: 0.5,
                position: Some(vec3(4.5, 64.0, -2.5)),
            })),
            worldly(world_order::Order::SpawnParticle(v1::Particle {
                key: "minecraft:flame".to_owned(),
                data: None,
                position: Some(vec3(4.5, 64.0, -2.5)),
                count: 8,
                spread: Some(vec3(0.5, 1.0, 1.5)),
                speed: 0.1,
            })),
            worldly(world_order::Order::Explode(v1::Explode {
                position: Some(vec3(4.5, 64.0, -2.5)),
                power: 4.0,
                breaks_blocks: true,
            })),
        ],
    )
    .await;
}

#[tokio::test]
async fn server_message_command_and_lifecycle_send_their_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            server.message().broadcast("hi");
            server.command().dispatch("time set 0");
            server.lifecycle().save();
            server.lifecycle().shutdown();
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            served(server_order::Order::Broadcast("hi".to_owned())),
            served(server_order::Order::DispatchCommand(
                "time set 0".to_owned(),
            )),
            served(server_order::Order::Save(v1::Empty {})),
            served(server_order::Order::Shutdown(v1::Empty {})),
        ],
    )
    .await;
}

#[tokio::test]
async fn server_name_lists_and_ban_lists_send_their_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            server.whitelist().set_enabled(true);
            server.whitelist().add("Carol");
            server.whitelist().remove("Dave");
            server.operators().add("Alice");
            server.operators().remove("Bob");
            server.bans().add(
                Ban::new("Steve")
                    .reason("grief")
                    .expires(UNIX_EPOCH + Duration::from_secs(1000)),
            );
            server.bans().remove("Steve");
            let address = "203.0.113.7".parse::<IpAddr>().unwrap();
            server.ip_bans().add(Ban::new(address));
            server.ip_bans().remove(address.to_string());
        })
    })
    .await;

    heard(
        &mut called,
        vec![
            served(server_order::Order::SetWhitelistEnabled(true)),
            served(server_order::Order::WhitelistAdd("Carol".to_owned())),
            served(server_order::Order::WhitelistRemove("Dave".to_owned())),
            served(server_order::Order::OperatorAdd("Alice".to_owned())),
            served(server_order::Order::OperatorRemove("Bob".to_owned())),
            served(server_order::Order::BanAdd(v1::Ban {
                target: "Steve".to_owned(),
                reason: Some("grief".to_owned()),
                expires_ms: Some(1_000_000),
            })),
            served(server_order::Order::BanRemove("Steve".to_owned())),
            served(server_order::Order::IpBanAdd(v1::Ban {
                target: "203.0.113.7".to_owned(),
                reason: None,
                expires_ms: None,
            })),
            served(server_order::Order::IpBanRemove("203.0.113.7".to_owned())),
        ],
    )
    .await;
}
