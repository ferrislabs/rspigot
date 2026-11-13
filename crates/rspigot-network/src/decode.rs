use std::num::NonZeroU8;

use rspigot_core::events::{
    ClickInput, ContainerClick, DigAction, Direction, Hand, HashedItem, HashedSlot, InputState,
    MovementAction, PlayerInput,
};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, EntityId, ItemId, Rotation, Vec3};
use rspigot_nbt::Compound;
use rspigot_protocol::codec::Position;
use rspigot_protocol::play::ServerboundPlay;
use rspigot_protocol::play::serverbound::{
    ClientCommandAction, ContainerInput, PlayerActionKind, PlayerCommandAction,
};
use rspigot_protocol::play::slot::{HashedStack, ItemStack as WireStack};
use rspigot_protocol::play::types::{
    Direction as WireDirection, GameMode as WireGameMode, Hand as WireHand,
};

use crate::connection::client_settings;

pub enum Decoded {
    Input(PlayerInput),
    KeepAlive(i64),
    Ping(i64),
    Ignored,
}

fn block_pos(pos: Position) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

fn hand(hand: WireHand) -> Hand {
    match hand {
        WireHand::MainHand => Hand::Main,
        WireHand::OffHand => Hand::Off,
    }
}

fn direction(direction: WireDirection) -> Direction {
    match direction {
        WireDirection::Down => Direction::Down,
        WireDirection::Up => Direction::Up,
        WireDirection::North => Direction::North,
        WireDirection::South => Direction::South,
        WireDirection::West => Direction::West,
        WireDirection::East => Direction::East,
    }
}

fn game_mode(mode: WireGameMode) -> GameMode {
    match mode {
        WireGameMode::Survival => GameMode::Survival,
        WireGameMode::Creative => GameMode::Creative,
        WireGameMode::Adventure => GameMode::Adventure,
        WireGameMode::Spectator => GameMode::Spectator,
    }
}

pub fn item_stack(stack: &WireStack) -> Option<ItemStack> {
    if stack.is_empty() {
        return None;
    }
    let item = u16::try_from(stack.item).ok()?;
    let count = NonZeroU8::new(u8::try_from(stack.count.clamp(1, 99)).ok()?)?;
    Some(ItemStack {
        item: ItemId(item),
        count,
        components: Compound::new(),
    })
}

fn hashed(stack: &HashedStack) -> HashedSlot {
    HashedSlot(stack.0.as_ref().map(|item| HashedItem {
        item: item.item,
        count: item.count,
        patched: !item.added.is_empty() || !item.removed.is_empty(),
    }))
}

fn click_input(input: ContainerInput) -> ClickInput {
    match input {
        ContainerInput::Pickup => ClickInput::Pickup,
        ContainerInput::QuickMove => ClickInput::QuickMove,
        ContainerInput::Swap => ClickInput::Swap,
        ContainerInput::Clone => ClickInput::Clone,
        ContainerInput::Throw => ClickInput::Throw,
        ContainerInput::QuickCraft => ClickInput::QuickCraft,
        ContainerInput::PickupAll => ClickInput::PickupAll,
    }
}

fn finite(value: f64) -> bool {
    value.is_finite()
}

fn input(value: PlayerInput) -> Decoded {
    Decoded::Input(value)
}

pub fn decode(packet: ServerboundPlay) -> Decoded {
    match packet {
        ServerboundPlay::AcceptTeleportation(p) => {
            input(PlayerInput::TeleportConfirm(p.teleport_id))
        }
        ServerboundPlay::MovePlayerPos(p) if finite(p.x) && finite(p.y) && finite(p.z) => {
            input(PlayerInput::Move {
                position: Some(Vec3::new(p.x, p.y, p.z)),
                rotation: None,
                on_ground: p.flags.on_ground,
            })
        }
        ServerboundPlay::MovePlayerPosRot(p) if finite(p.x) && finite(p.y) && finite(p.z) => {
            input(PlayerInput::Move {
                position: Some(Vec3::new(p.x, p.y, p.z)),
                rotation: Some(Rotation::new(p.y_rot, p.x_rot)),
                on_ground: p.flags.on_ground,
            })
        }
        ServerboundPlay::MovePlayerRot(p) => input(PlayerInput::Move {
            position: None,
            rotation: Some(Rotation::new(p.y_rot, p.x_rot)),
            on_ground: p.flags.on_ground,
        }),
        ServerboundPlay::MovePlayerStatusOnly(p) => input(PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: p.flags.on_ground,
        }),
        ServerboundPlay::Chat(p) => input(PlayerInput::Chat(p.message)),
        ServerboundPlay::ChatCommand(p) => input(PlayerInput::Command(p.command)),
        ServerboundPlay::ChatCommandSigned(p) => input(PlayerInput::Command(p.command)),
        ServerboundPlay::ChunkBatchReceived(p) => input(PlayerInput::ChunkBatchReceived {
            chunks_per_tick: p.desired_chunks_per_tick,
        }),
        ServerboundPlay::ClientCommand(p) => match p.action {
            ClientCommandAction::PerformRespawn => input(PlayerInput::Respawn),
            _ => Decoded::Ignored,
        },
        ServerboundPlay::ClientInformation(info) => {
            input(PlayerInput::Settings(client_settings(&info)))
        }
        ServerboundPlay::CommandSuggestion(p) => input(PlayerInput::CommandSuggestion {
            id: p.id,
            text: p.command,
        }),
        ServerboundPlay::PlayerAction(p) => {
            let action = match p.action {
                PlayerActionKind::StartDestroyBlock => DigAction::Start,
                PlayerActionKind::AbortDestroyBlock => DigAction::Cancel,
                PlayerActionKind::StopDestroyBlock => DigAction::Finish,
                PlayerActionKind::DropAllItems => DigAction::DropStack,
                PlayerActionKind::DropItem => DigAction::DropItem,
                PlayerActionKind::ReleaseUseItem => DigAction::ReleaseUseItem,
                PlayerActionKind::SwapItemWithOffhand => DigAction::SwapHands,
                PlayerActionKind::Stab => DigAction::Stab,
                PlayerActionKind::ChangeDestroyDirection => return Decoded::Ignored,
            };
            input(PlayerInput::Dig {
                action,
                pos: block_pos(p.pos),
                face: direction(p.direction),
                sequence: p.sequence,
            })
        }
        ServerboundPlay::UseItemOn(p) => input(PlayerInput::UseItemOn {
            hand: hand(p.hand),
            pos: block_pos(p.hit.pos),
            face: direction(p.hit.direction),
            cursor: Vec3::new(
                f64::from(p.hit.cursor_x),
                f64::from(p.hit.cursor_y),
                f64::from(p.hit.cursor_z),
            ),
            inside_block: p.hit.inside,
            sequence: p.sequence,
        }),
        ServerboundPlay::UseItem(p) => input(PlayerInput::UseItem {
            hand: hand(p.hand),
            sequence: p.sequence,
            rotation: Rotation::new(p.y_rot, p.x_rot),
        }),
        ServerboundPlay::SetCarriedItem(p) => match u8::try_from(p.slot) {
            Ok(slot) if slot < 9 => input(PlayerInput::HeldSlot(slot)),
            _ => Decoded::Ignored,
        },
        ServerboundPlay::SetCreativeModeSlot(p) => input(PlayerInput::CreativeSlot {
            slot: p.slot,
            stack: item_stack(&p.item),
        }),
        ServerboundPlay::Punch(_) => input(PlayerInput::Swing(Hand::Main)),
        ServerboundPlay::PlayerCommand(p) => match p.action {
            PlayerCommandAction::StartSprinting => {
                input(PlayerInput::Movement(MovementAction::StartSprinting))
            }
            PlayerCommandAction::StopSprinting => {
                input(PlayerInput::Movement(MovementAction::StopSprinting))
            }
            PlayerCommandAction::StopSleeping => {
                input(PlayerInput::Movement(MovementAction::LeaveBed))
            }
            PlayerCommandAction::StartFallFlying => {
                input(PlayerInput::Movement(MovementAction::StartFallFlying))
            }
            _ => Decoded::Ignored,
        },
        ServerboundPlay::PlayerInput(p) => input(PlayerInput::Input(InputState {
            forward: p.forward,
            backward: p.backward,
            left: p.left,
            right: p.right,
            jump: p.jump,
            shift: p.shift,
            sprint: p.sprint,
        })),
        ServerboundPlay::PlayerLoaded(_) => input(PlayerInput::Loaded),
        ServerboundPlay::ClientTickEnd(_) => input(PlayerInput::ClientTickEnd),
        ServerboundPlay::PlayerAbilities(p) => input(PlayerInput::Abilities { flying: p.flying }),
        ServerboundPlay::ChangeGameMode(p) => input(PlayerInput::ChangeGameMode(game_mode(p.mode))),
        ServerboundPlay::PickItemFromBlock(p) => input(PlayerInput::PickBlock {
            pos: block_pos(p.pos),
            include_data: p.include_data,
        }),
        ServerboundPlay::Attack(p) => input(PlayerInput::Attack(EntityId(p.entity_id))),
        ServerboundPlay::Interact(p) => input(PlayerInput::Interact {
            target: EntityId(p.entity_id),
            hand: hand(p.hand),
            sneaking: p.using_secondary_action,
            location: Vec3::new(p.location.0.x, p.location.0.y, p.location.0.z),
        }),
        ServerboundPlay::ContainerClick(p) => input(PlayerInput::ContainerClick(ContainerClick {
            container: p.container_id,
            state_id: p.state_id,
            slot: p.slot,
            button: p.button,
            input: click_input(p.input),
            changed: p
                .changed_slots
                .iter()
                .map(|changed| (changed.slot, hashed(&changed.item)))
                .collect(),
            carried: hashed(&p.carried),
        })),
        ServerboundPlay::ContainerClose(p) => input(PlayerInput::ContainerClose(p.container_id)),
        ServerboundPlay::SetBeacon(p) => input(PlayerInput::SetBeacon {
            primary: p.primary,
            secondary: p.secondary,
        }),
        ServerboundPlay::ContainerButtonClick(p) => input(PlayerInput::ContainerButtonClick {
            window: p.container_id,
            button: p.button_id,
        }),
        ServerboundPlay::SelectTrade(p) => input(PlayerInput::SelectTrade(p.item)),
        ServerboundPlay::KeepAlive(p) => Decoded::KeepAlive(p.id),
        ServerboundPlay::PingRequest(p) => Decoded::Ping(p.time),
        _ => Decoded::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_protocol::ids::play::serverbound as sb;

    use super::*;

    fn decoded(id: i32, body: &[u8]) -> PlayerInput {
        match decode(ServerboundPlay::decode(id, body).unwrap()) {
            Decoded::Input(input) => input,
            _ => panic!("packet {id} was not turned into a player input"),
        }
    }

    #[test]
    fn container_click_golden_bytes_become_a_click() {
        let body = [0, 3, 0xFC, 0x19, 1, 4, 1, 0, 36, 1, 1, 4, 0, 0, 0];
        assert_eq!(
            decoded(sb::CONTAINER_CLICK, &body),
            PlayerInput::ContainerClick(ContainerClick {
                container: 0,
                state_id: 3,
                slot: -999,
                button: 1,
                input: ClickInput::Throw,
                changed: vec![(
                    36,
                    HashedSlot(Some(HashedItem {
                        item: 1,
                        count: 4,
                        patched: false,
                    }))
                )],
                carried: HashedSlot::EMPTY,
            })
        );
    }

    #[test]
    fn selecting_a_trade_carries_its_index() {
        assert_eq!(decoded(sb::SELECT_TRADE, &[2]), PlayerInput::SelectTrade(2));
    }

    #[test]
    fn container_click_with_component_hashes_is_marked_patched() {
        let body = [0, 1, 0, 36, 0, 0, 0, 1, 1, 5, 1, 7, 0, 0, 0, 9, 0];
        let PlayerInput::ContainerClick(click) = decoded(sb::CONTAINER_CLICK, &body) else {
            panic!("not a click");
        };
        assert_eq!(click.input, ClickInput::Pickup);
        assert!(click.changed.is_empty());
        assert_eq!(
            click.carried,
            HashedSlot(Some(HashedItem {
                item: 1,
                count: 5,
                patched: true,
            }))
        );
    }

    #[test]
    fn container_close_golden_bytes_become_a_close() {
        assert_eq!(
            decoded(sb::CONTAINER_CLOSE, &[0]),
            PlayerInput::ContainerClose(0)
        );
    }
}
