use std::num::NonZeroU8;

use rspigot_core::events::{
    ChatMessage, ChatSessionKey, ClickInput, ComponentHasher, ContainerClick, DigAction, Direction,
    Hand, HashedItem, HashedSlot, InputState, LastSeenUpdate, MessageSignature, MovementAction,
    PlayerInput, SignedCommand,
};
use rspigot_core::plugins::events::ResourcePackStatus;
use rspigot_core::recipe_book::BookKind;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::GameMode;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, ItemId, Rotation, Vec3};
use rspigot_nbt::Tag;
use rspigot_protocol::codec::{Position, decode_string};
use rspigot_protocol::common::ResourcePackAction;
use rspigot_protocol::play::ServerboundPlay;
use rspigot_protocol::play::serverbound::{
    ClientCommandAction, ContainerInput, PlayerActionKind, PlayerCommandAction, RecipeBookType,
    SeenAdvancements, SignTextSlot,
};
use rspigot_protocol::play::slot::{HashedStack, ItemStack as WireStack};
use rspigot_protocol::play::types::{
    Difficulty as WireDifficulty, Direction as WireDirection, GameMode as WireGameMode,
    Hand as WireHand,
};

use crate::connection::client_settings;

fn pack_status(action: ResourcePackAction) -> ResourcePackStatus {
    match action {
        ResourcePackAction::SuccessfullyLoaded => ResourcePackStatus::SuccessfullyLoaded,
        ResourcePackAction::Declined => ResourcePackStatus::Declined,
        ResourcePackAction::FailedDownload => ResourcePackStatus::FailedDownload,
        ResourcePackAction::Accepted => ResourcePackStatus::Accepted,
        ResourcePackAction::Downloaded => ResourcePackStatus::Downloaded,
        ResourcePackAction::InvalidUrl => ResourcePackStatus::InvalidUrl,
        ResourcePackAction::FailedReload => ResourcePackStatus::FailedReload,
        ResourcePackAction::Discarded => ResourcePackStatus::Discarded,
    }
}

fn tag_json(tag: &Tag) -> serde_json::Value {
    use serde_json::{Number, Value};
    let number = |value: f64| Number::from_f64(value).map_or(Value::Null, Value::Number);
    match tag {
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => number(f64::from(*v)),
        Tag::Double(v) => number(*v),
        Tag::String(v) => Value::from(v.as_str()),
        Tag::ByteArray(v) => v.iter().map(|b| Value::from(*b as i8)).collect(),
        Tag::IntArray(v) => v.iter().map(|n| Value::from(*n)).collect(),
        Tag::LongArray(v) => v.iter().map(|n| Value::from(*n)).collect(),
        Tag::List(v) => v.iter().map(tag_json).collect(),
        Tag::Compound(v) => v
            .iter()
            .map(|(key, value)| (key.to_owned(), tag_json(value)))
            .collect(),
    }
}

const BRAND_CHANNEL: &str = "minecraft:brand";
const MAX_BRAND_CHARS: usize = 32_767;

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
        components: crate::components::decode_components(&stack.components),
    })
}

const UNKNOWN_COMPONENT: &str = "minecraft:unknown";

fn hashed(stack: &HashedStack) -> HashedSlot {
    HashedSlot(stack.0.as_ref().map(|item| {
        HashedItem {
            item: item.item,
            count: item.count,
            added: item
                .added
                .iter()
                .map(|component| {
                    (
                        crate::components::component_name(component.type_id)
                            .unwrap_or(UNKNOWN_COMPONENT)
                            .to_owned(),
                        component.hash,
                    )
                })
                .collect(),
            removed: item
                .removed
                .iter()
                .map(|type_id| {
                    crate::components::component_name(*type_id)
                        .unwrap_or(UNKNOWN_COMPONENT)
                        .to_owned()
                })
                .collect(),
            hasher: ComponentHasher(crate::components::component_hash),
        }
    }))
}

fn last_seen(update: &rspigot_protocol::play::chat::LastSeenUpdate) -> LastSeenUpdate {
    let [low, mid, high] = update.acknowledged.0;
    LastSeenUpdate {
        offset: update.offset,
        acknowledged: u32::from(low) | u32::from(mid) << 8 | u32::from(high) << 16,
        checksum: update.checksum,
    }
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

fn difficulty(value: WireDifficulty) -> Difficulty {
    match value {
        WireDifficulty::Peaceful => Difficulty::Peaceful,
        WireDifficulty::Easy => Difficulty::Easy,
        WireDifficulty::Normal => Difficulty::Normal,
        WireDifficulty::Hard => Difficulty::Hard,
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
        ServerboundPlay::MoveVehicle(p)
            if finite(p.position.x) && finite(p.position.y) && finite(p.position.z) =>
        {
            input(PlayerInput::MoveVehicle {
                position: Vec3::new(p.position.x, p.position.y, p.position.z),
                rotation: Rotation::new(p.y_rot, p.x_rot),
                on_ground: p.on_ground,
            })
        }
        ServerboundPlay::PaddleBoat(p) => input(PlayerInput::PaddleBoat {
            left: p.left,
            right: p.right,
        }),
        ServerboundPlay::MovePlayerStatusOnly(p) => input(PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: p.flags.on_ground,
        }),
        ServerboundPlay::Chat(p) => input(PlayerInput::ChatMessage(Box::new(ChatMessage {
            content: p.message,
            timestamp_ms: p.timestamp,
            salt: p.salt,
            signature: p.signature.map(|signature| MessageSignature(signature.0)),
            last_seen: last_seen(&p.last_seen),
        }))),
        ServerboundPlay::ChatAck(p) => input(PlayerInput::ChatAck(p.offset)),
        ServerboundPlay::ChatCommand(p) => input(PlayerInput::Command(p.command)),
        ServerboundPlay::ChatCommandSigned(p) => input(PlayerInput::SignedCommand(SignedCommand {
            command: p.command,
            timestamp_ms: p.timestamp,
            salt: p.salt,
            arguments: p
                .argument_signatures
                .into_iter()
                .map(|entry| (entry.name, MessageSignature(entry.signature.0)))
                .collect(),
            last_seen: last_seen(&p.last_seen),
        })),
        ServerboundPlay::ChunkBatchReceived(p) => input(PlayerInput::ChunkBatchReceived {
            chunks_per_tick: p.desired_chunks_per_tick,
        }),
        ServerboundPlay::ClientCommand(p) => match p.action {
            ClientCommandAction::PerformRespawn => input(PlayerInput::Respawn),
            ClientCommandAction::RequestStats => input(PlayerInput::RequestStats),
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
            PlayerCommandAction::StartRidingJump => input(PlayerInput::Movement(
                MovementAction::StartRidingJump(p.data),
            )),
            PlayerCommandAction::StopRidingJump => {
                input(PlayerInput::Movement(MovementAction::StopRidingJump))
            }
            PlayerCommandAction::OpenInventory => {
                input(PlayerInput::Movement(MovementAction::OpenVehicleInventory))
            }
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
        ServerboundPlay::ContainerSlotStateChanged(p) => input(PlayerInput::ContainerSlotState {
            window: p.container_id,
            slot: p.slot_id,
            enabled: p.new_state,
        }),
        ServerboundPlay::SelectTrade(p) => input(PlayerInput::SelectTrade(p.item)),
        ServerboundPlay::RenameItem(p) => input(PlayerInput::RenameItem(p.name)),
        ServerboundPlay::SeenAdvancements(action) => {
            input(PlayerInput::AdvancementTab(match action {
                SeenAdvancements::OpenedTab { tab } => Some(tab),
                SeenAdvancements::ClosedScreen => None,
            }))
        }
        ServerboundPlay::PlaceRecipe(p) => input(PlayerInput::PlaceRecipe {
            window: p.container_id,
            display_id: p.recipe,
            max: p.use_max_items,
        }),
        ServerboundPlay::RecipeBookSeen(p) => input(PlayerInput::RecipeSeen(p.recipe)),
        ServerboundPlay::RecipeBookSettings(p) => input(PlayerInput::RecipeBookSettings {
            kind: match p.book {
                RecipeBookType::Crafting => BookKind::Crafting,
                RecipeBookType::Furnace => BookKind::Furnace,
                RecipeBookType::BlastFurnace => BookKind::BlastFurnace,
                RecipeBookType::Smoker => BookKind::Smoker,
            },
            open: p.open,
            filtering: p.filtering,
        }),
        ServerboundPlay::EditBook(p) => input(PlayerInput::EditBook {
            slot: p.slot,
            pages: p.pages.clone(),
            title: p.title.clone(),
        }),
        ServerboundPlay::SignUpdate(p) => input(PlayerInput::SignUpdate {
            pos: block_pos(p.pos),
            front: matches!(p.slot, SignTextSlot::Front),
            lines: p.lines.to_vec(),
        }),
        ServerboundPlay::ResourcePack(p) => input(PlayerInput::ResourcePack {
            id: p.id,
            status: pack_status(p.action),
        }),
        ServerboundPlay::CustomClickAction(p) => input(PlayerInput::CustomClick {
            id: p.id.to_string(),
            data: p.payload.as_ref().map(|tag| tag_json(tag).to_string()),
        }),
        ServerboundPlay::ChangeDifficulty(p) => {
            input(PlayerInput::ChangeDifficulty(difficulty(p.difficulty)))
        }
        ServerboundPlay::LockDifficulty(p) => input(PlayerInput::LockDifficulty(p.locked)),
        ServerboundPlay::BlockEntityTagQuery(p) => input(PlayerInput::BlockTagQuery {
            transaction: p.transaction_id,
            pos: block_pos(p.pos),
        }),
        ServerboundPlay::EntityTagQuery(p) => input(PlayerInput::EntityTagQuery {
            transaction: p.transaction_id,
            entity: EntityId(p.entity_id),
        }),
        ServerboundPlay::TeleportToEntity(p) => input(PlayerInput::TeleportToEntity(p.target)),
        ServerboundPlay::SetGameRule(p) => input(PlayerInput::SetGameRules(
            p.entries
                .into_iter()
                .map(|entry| (entry.rule.as_str().to_owned(), entry.value))
                .collect(),
        )),
        ServerboundPlay::ChatSessionUpdate(p) => input(PlayerInput::ChatSession(ChatSessionKey {
            session: p.session.session_id,
            expires_at_ms: p.session.expires_at,
            public_key: p.session.public_key,
            key_signature: p.session.key_signature,
        })),
        ServerboundPlay::CustomPayload(p) if p.channel.as_str() == BRAND_CHANNEL => {
            match decode_string(&mut p.data.as_slice(), MAX_BRAND_CHARS) {
                Ok(brand) => input(PlayerInput::ClientBrand(brand)),
                Err(_) => Decoded::Ignored,
            }
        }
        ServerboundPlay::PickItemFromEntity(p) => input(PlayerInput::PickEntity {
            entity: EntityId(p.entity_id),
            include_data: p.include_data,
        }),
        ServerboundPlay::KeepAlive(p) => Decoded::KeepAlive(p.id),
        ServerboundPlay::PingRequest(p) => Decoded::Ping(p.time),
        _ => Decoded::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_protocol::codec::Encode;
    use rspigot_protocol::ids::play::serverbound as sb;

    use super::*;

    fn decoded(id: i32, body: &[u8]) -> PlayerInput {
        match decode(ServerboundPlay::decode(id, body).unwrap()) {
            Decoded::Input(input) => input,
            _ => panic!("packet {id} was not turned into a player input"),
        }
    }

    #[test]
    fn vehicle_moves_become_inputs() {
        let mut body = Vec::new();
        for value in [1.0f64, 2.0, 3.0] {
            body.extend_from_slice(&value.to_be_bytes());
        }
        body.extend_from_slice(&90.0f32.to_be_bytes());
        body.extend_from_slice(&0.0f32.to_be_bytes());
        body.push(1);
        assert_eq!(
            decoded(sb::MOVE_VEHICLE, &body),
            PlayerInput::MoveVehicle {
                position: Vec3::new(1.0, 2.0, 3.0),
                rotation: Rotation::new(90.0, 0.0),
                on_ground: true,
            }
        );
    }

    #[test]
    fn a_creative_slot_keeps_the_components_of_the_client_stack() {
        let body = [0, 36, 1, 1, 1, 1, 6, 4, 8, 0, 1, 0x41, 3];
        let PlayerInput::CreativeSlot { slot, stack } = decoded(sb::SET_CREATIVE_MODE_SLOT, &body)
        else {
            panic!("not a creative slot");
        };
        assert_eq!(slot, 36);
        let stack = stack.unwrap();
        assert_eq!(stack.item, ItemId(1));
        assert_eq!(
            stack.components.get("minecraft:custom_name"),
            Some(&Tag::String("A".to_owned()))
        );
    }

    #[test]
    fn paddles_become_inputs() {
        assert_eq!(
            decoded(sb::PADDLE_BOAT, &[0, 1]),
            PlayerInput::PaddleBoat {
                left: false,
                right: true,
            }
        );
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
                        added: Vec::new(),
                        removed: Vec::new(),
                        hasher: ComponentHasher(crate::components::component_hash),
                    }))
                )],
                carried: HashedSlot::EMPTY,
            })
        );
    }

    #[test]
    fn a_vanilla_component_hash_matches_the_stack_the_server_holds() {
        let custom_name = crate::components::component_type("minecraft:custom_name").unwrap();
        let mut body = vec![0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1];
        body.push(u8::try_from(custom_name).unwrap());
        body.extend(815_466_656_i32.to_be_bytes());
        body.push(0);
        let PlayerInput::ContainerClick(click) = decoded(sb::CONTAINER_CLICK, &body) else {
            panic!("not a click");
        };
        let mut held = ItemStack::new(ItemId(1), NonZeroU8::new(1).unwrap());
        held.components
            .insert("minecraft:custom_name", Tag::String("Hello".to_owned()));
        assert!(click.carried.matches(&Some(held.clone())));
        held.components
            .insert("minecraft:custom_name", Tag::String("Other".to_owned()));
        assert!(!click.carried.matches(&Some(held)));
    }

    #[test]
    fn a_signed_chat_keeps_its_timestamp_salt_signature_and_last_seen_update() {
        let mut body = vec![2, b'h', b'i'];
        body.extend(1_700_000_000_123_i64.to_be_bytes());
        body.extend(77_i64.to_be_bytes());
        body.push(1);
        body.extend([9u8; 256]);
        body.extend([3, 0b0000_0101, 0b0000_0010, 0b0000_1000, 0xFE]);
        assert_eq!(
            decoded(sb::CHAT, &body),
            PlayerInput::ChatMessage(Box::new(ChatMessage {
                content: "hi".to_owned(),
                timestamp_ms: 1_700_000_000_123,
                salt: 77,
                signature: Some(MessageSignature([9; 256])),
                last_seen: LastSeenUpdate {
                    offset: 3,
                    acknowledged: 0b0000_1000_0000_0010_0000_0101,
                    checksum: -2,
                },
            }))
        );
    }

    #[test]
    fn an_unsigned_chat_has_no_signature() {
        let mut body = vec![1, b'x'];
        body.extend(5_i64.to_be_bytes());
        body.extend(6_i64.to_be_bytes());
        body.push(0);
        body.extend([0, 0, 0, 0, 0]);
        let PlayerInput::ChatMessage(message) = decoded(sb::CHAT, &body) else {
            panic!("not a chat");
        };
        assert_eq!(message.signature, None);
        assert_eq!(message.last_seen, LastSeenUpdate::default());
    }

    #[test]
    fn a_chat_acknowledgement_carries_its_offset() {
        assert_eq!(decoded(sb::CHAT_ACK, &[5]), PlayerInput::ChatAck(5));
    }

    #[test]
    fn a_signed_command_keeps_each_argument_signature() {
        let mut body = vec![9];
        body.extend(b"msg a hey");
        body.extend(10_i64.to_be_bytes());
        body.extend(11_i64.to_be_bytes());
        body.push(1);
        body.extend([7, b'm', b'e', b's', b's', b'a', b'g', b'e']);
        body.extend([4u8; 256]);
        body.extend([0, 0, 0, 0, 0]);
        assert_eq!(
            decoded(sb::CHAT_COMMAND_SIGNED, &body),
            PlayerInput::SignedCommand(SignedCommand {
                command: "msg a hey".to_owned(),
                timestamp_ms: 10,
                salt: 11,
                arguments: vec![("message".to_owned(), MessageSignature([4; 256]))],
                last_seen: LastSeenUpdate::default(),
            })
        );
    }

    #[test]
    fn renaming_carries_the_typed_name() {
        assert_eq!(
            decoded(sb::RENAME_ITEM, &[2, b'h', b'i']),
            PlayerInput::RenameItem("hi".into())
        );
    }

    #[test]
    fn selecting_a_trade_carries_its_index() {
        assert_eq!(decoded(sb::SELECT_TRADE, &[2]), PlayerInput::SelectTrade(2));
    }

    #[test]
    fn container_click_with_component_hashes_keeps_each_hash_by_component_name() {
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
                added: vec![("minecraft:minimum_attack_charge".to_owned(), 9)],
                removed: Vec::new(),
                hasher: ComponentHasher(crate::components::component_hash),
            }))
        );
    }

    #[test]
    fn placing_a_recipe_carries_the_window_the_display_id_and_the_maximum() {
        assert_eq!(
            decoded(sb::PLACE_RECIPE, &[1, 5, 1]),
            PlayerInput::PlaceRecipe {
                window: 1,
                display_id: 5,
                max: true,
            }
        );
        assert_eq!(
            decoded(sb::RECIPE_BOOK_SEEN_RECIPE, &[5]),
            PlayerInput::RecipeSeen(5)
        );
    }

    #[test]
    fn opening_a_tab_or_closing_the_screen_decodes_to_the_tab() {
        let mut opened = vec![0];
        "minecraft:story/root".to_owned().encode(&mut opened);
        assert_eq!(
            decoded(sb::SEEN_ADVANCEMENTS, &opened),
            PlayerInput::AdvancementTab(Some("minecraft:story/root".to_owned()))
        );
        assert_eq!(
            decoded(sb::SEEN_ADVANCEMENTS, &[1]),
            PlayerInput::AdvancementTab(None)
        );
    }

    #[test]
    fn a_client_command_asks_for_the_stats() {
        assert_eq!(decoded(sb::CLIENT_COMMAND, &[1]), PlayerInput::RequestStats);
    }

    #[test]
    fn recipe_book_settings_carry_the_book_and_both_flags() {
        assert_eq!(
            decoded(sb::RECIPE_BOOK_CHANGE_SETTINGS, &[3, 1, 1]),
            PlayerInput::RecipeBookSettings {
                kind: BookKind::Smoker,
                open: true,
                filtering: true,
            }
        );
    }

    #[test]
    fn an_edit_book_carries_its_slot_pages_and_optional_title() {
        let mut body = vec![0, 2];
        for page in ["a", "bc"] {
            page.to_owned().encode(&mut body);
        }
        body.push(1);
        "T".to_owned().encode(&mut body);
        assert_eq!(
            decoded(sb::EDIT_BOOK, &body),
            PlayerInput::EditBook {
                slot: 0,
                pages: vec!["a".to_owned(), "bc".to_owned()],
                title: Some("T".to_owned()),
            }
        );
    }

    #[test]
    fn a_sign_update_carries_its_side_and_four_lines() {
        let mut body = Vec::new();
        Position { x: 1, y: 64, z: 2 }.encode(&mut body);
        for line in ["a", "", "c", "d"] {
            line.to_owned().encode(&mut body);
        }
        body.push(0);
        assert_eq!(
            decoded(sb::SIGN_UPDATE, &body),
            PlayerInput::SignUpdate {
                pos: BlockPos::new(1, 64, 2),
                front: false,
                lines: ["a", "", "c", "d"].map(str::to_owned).to_vec(),
            }
        );
        let last = body.len() - 1;
        body[last] = 1;
        assert!(matches!(
            decoded(sb::SIGN_UPDATE, &body),
            PlayerInput::SignUpdate { front: true, .. }
        ));
    }

    #[test]
    fn a_resource_pack_answer_becomes_an_input() {
        let mut body = 7u128.to_be_bytes().to_vec();
        body.push(1);
        assert_eq!(
            decoded(sb::RESOURCE_PACK, &body),
            PlayerInput::ResourcePack {
                id: uuid::Uuid::from_u128(7),
                status: ResourcePackStatus::Declined,
            }
        );
    }

    #[test]
    fn a_custom_click_carries_its_payload_as_json() {
        let mut body = vec![8];
        body.extend_from_slice(b"shop:buy");
        let mut nbt = vec![10, 3, 0, 1, b'n', 0, 0, 0, 5, 0];
        body.push(u8::try_from(nbt.len()).unwrap());
        body.append(&mut nbt);
        assert_eq!(
            decoded(sb::CUSTOM_CLICK_ACTION, &body),
            PlayerInput::CustomClick {
                id: "shop:buy".to_owned(),
                data: Some("{\"n\":5}".to_owned()),
            }
        );
        let mut bare = vec![8];
        bare.extend_from_slice(b"shop:buy");
        bare.extend_from_slice(&[1, 0]);
        assert_eq!(
            decoded(sb::CUSTOM_CLICK_ACTION, &bare),
            PlayerInput::CustomClick {
                id: "shop:buy".to_owned(),
                data: None,
            }
        );
    }

    #[test]
    fn difficulty_packets_become_a_change_and_a_lock() {
        assert_eq!(
            decoded(sb::CHANGE_DIFFICULTY, &[3]),
            PlayerInput::ChangeDifficulty(Difficulty::Hard)
        );
        assert_eq!(
            decoded(sb::LOCK_DIFFICULTY, &[1]),
            PlayerInput::LockDifficulty(true)
        );
    }

    #[test]
    fn tag_queries_become_inputs() {
        let mut body = vec![5];
        Position { x: 1, y: 64, z: 2 }.encode(&mut body);
        assert_eq!(
            decoded(sb::BLOCK_ENTITY_TAG_QUERY, &body),
            PlayerInput::BlockTagQuery {
                transaction: 5,
                pos: BlockPos::new(1, 64, 2),
            }
        );
        assert_eq!(
            decoded(sb::ENTITY_TAG_QUERY, &[5, 9]),
            PlayerInput::EntityTagQuery {
                transaction: 5,
                entity: EntityId(9),
            }
        );
    }

    #[test]
    fn teleporting_to_an_entity_carries_its_uuid() {
        assert_eq!(
            decoded(sb::TELEPORT_TO_ENTITY, &9u128.to_be_bytes()),
            PlayerInput::TeleportToEntity(uuid::Uuid::from_u128(9))
        );
    }

    #[test]
    fn game_rule_entries_keep_their_key_and_text_value() {
        let mut body = vec![1];
        for text in ["minecraft:keep_inventory", "true"] {
            text.to_owned().encode(&mut body);
        }
        assert_eq!(
            decoded(sb::SET_GAME_RULE, &body),
            PlayerInput::SetGameRules(vec![(
                "minecraft:keep_inventory".to_owned(),
                "true".to_owned()
            )])
        );
    }

    #[test]
    fn a_chat_session_update_carries_its_key_material() {
        let mut body = 5u128.to_be_bytes().to_vec();
        body.extend_from_slice(&1_700_000_000_000i64.to_be_bytes());
        body.extend_from_slice(&[3, 1, 2, 3, 2, 9, 8]);
        assert_eq!(
            decoded(sb::CHAT_SESSION_UPDATE, &body),
            PlayerInput::ChatSession(ChatSessionKey {
                session: uuid::Uuid::from_u128(5),
                expires_at_ms: 1_700_000_000_000,
                public_key: vec![1, 2, 3],
                key_signature: vec![9, 8],
            })
        );
    }

    #[test]
    fn the_brand_payload_becomes_the_client_brand_and_other_channels_are_ignored() {
        let mut body = Vec::new();
        "minecraft:brand".to_owned().encode(&mut body);
        "vanilla".to_owned().encode(&mut body);
        assert_eq!(
            decoded(sb::CUSTOM_PAYLOAD, &body),
            PlayerInput::ClientBrand("vanilla".to_owned())
        );
        let mut other = Vec::new();
        "shop:buy".to_owned().encode(&mut other);
        other.push(1);
        assert!(matches!(
            decode(ServerboundPlay::decode(sb::CUSTOM_PAYLOAD, &other).unwrap()),
            Decoded::Ignored
        ));
    }

    #[test]
    fn picking_from_a_block_or_an_entity_keeps_the_target_and_the_data_flag() {
        let mut body = Vec::new();
        Position { x: 1, y: 64, z: 2 }.encode(&mut body);
        body.push(1);
        assert_eq!(
            decoded(sb::PICK_ITEM_FROM_BLOCK, &body),
            PlayerInput::PickBlock {
                pos: BlockPos::new(1, 64, 2),
                include_data: true,
            }
        );
        assert_eq!(
            decoded(sb::PICK_ITEM_FROM_ENTITY, &[9, 0]),
            PlayerInput::PickEntity {
                entity: EntityId(9),
                include_data: false,
            }
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
