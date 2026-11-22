mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::*;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};
use rspigot_core::config::ChatSecurity;
use rspigot_core::events::{
    ChatMessage, ChatSessionKey, ClientEvent, LastSeenUpdate, MessageSignature, PlayerInput,
    SignedChat, SignedCommand,
};
use rspigot_core::plugins::events::AsyncPlayerChatPatch;
use rspigot_core::plugins::{EventKind, EventPatch};
use rspigot_domain::text::Component;
use sha1::Sha1;
use sha2::{Digest, Sha256};

const PLAYER_KEY: &str = include_str!("../../rspigot-domain/src/chat_chain/player.hex");
const SERVICES_KEY: &str = include_str!("../../rspigot-domain/src/chat_chain/other.hex");
const SHA1_PREFIX: [u8; 15] = [
    0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
];
const SHA256_PREFIX: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];
const KEY_EXPIRES_MS: i64 = 5_000_000;
const SESSION: u128 = 0xABCD;

fn clock() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_000)
}

fn private(hex: &str) -> RsaPrivateKey {
    let der: Vec<u8> = (0..hex.trim().len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    RsaPrivateKey::from_pkcs1_der(&der).unwrap()
}

fn public_der(key: &RsaPrivateKey) -> Vec<u8> {
    key.to_public_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec()
}

fn sign(key: &RsaPrivateKey, prefix: &[u8], digest: &[u8]) -> Vec<u8> {
    key.sign(
        Pkcs1v15Sign {
            hash_len: Some(digest.len()),
            prefix: prefix.to_vec().into_boxed_slice(),
        },
        digest,
    )
    .unwrap()
}

fn secure_server() -> rspigot_core::server::Server<Link> {
    configured(|config| {
        config.chat = ChatSecurity {
            enforce_secure_profile: true,
            services_keys: vec![public_der(&private(SERVICES_KEY))],
            ..ChatSecurity::default()
        }
    })
    .with_clock(clock)
}

fn session_key(player: &Player, expires_at_ms: i64) -> ChatSessionKey {
    let player_key = private(PLAYER_KEY);
    let public = public_der(&player_key);
    let mut payload = player.id.0.as_bytes().to_vec();
    payload.extend(expires_at_ms.to_be_bytes());
    payload.extend(&public);
    ChatSessionKey {
        session: uuid::Uuid::from_u128(SESSION),
        expires_at_ms,
        key_signature: sign(
            &private(SERVICES_KEY),
            &SHA1_PREFIX,
            &Sha1::digest(&payload),
        ),
        public_key: public,
    }
}

fn payload(
    player: &Player,
    index: i32,
    content: &str,
    timestamp_ms: i64,
    salt: i64,
    seen: &[MessageSignature],
) -> Vec<u8> {
    let mut payload = 1i32.to_be_bytes().to_vec();
    payload.extend(player.id.0.as_bytes());
    payload.extend(uuid::Uuid::from_u128(SESSION).as_bytes());
    payload.extend(index.to_be_bytes());
    payload.extend(salt.to_be_bytes());
    payload.extend((timestamp_ms / 1000).to_be_bytes());
    payload.extend((content.len() as i32).to_be_bytes());
    payload.extend(content.as_bytes());
    payload.extend((seen.len() as i32).to_be_bytes());
    for signature in seen {
        payload.extend(signature.0);
    }
    payload
}

fn sign_message(
    player: &Player,
    index: i32,
    content: &str,
    timestamp_ms: i64,
    salt: i64,
) -> MessageSignature {
    sign_seen(player, index, content, timestamp_ms, salt, &[])
}

fn sign_seen(
    player: &Player,
    index: i32,
    content: &str,
    timestamp_ms: i64,
    salt: i64,
    seen: &[MessageSignature],
) -> MessageSignature {
    let payload = payload(player, index, content, timestamp_ms, salt, seen);
    MessageSignature(
        sign(
            &private(PLAYER_KEY),
            &SHA256_PREFIX,
            &Sha256::digest(&payload),
        )
        .try_into()
        .unwrap(),
    )
}

fn chat(player: &Player, index: i32, content: &str, timestamp_ms: i64) -> PlayerInput {
    PlayerInput::ChatMessage(Box::new(ChatMessage {
        content: content.to_owned(),
        timestamp_ms,
        salt: 9,
        signature: Some(sign_message(player, index, content, timestamp_ms, 9)),
        last_seen: LastSeenUpdate::default(),
    }))
}

fn heard(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::PlayerChat { message, .. } => Some(format!("chat:{message}")),
            ClientEvent::SignedChat(signed) => Some(format!("signed:{}", signed.content)),
            ClientEvent::SystemChat { message, .. } => Some(format!(
                "system:{}",
                match &message.content {
                    rspigot_domain::text::Content::Translatable { key, .. } => key.clone(),
                    other => format!("{other:?}"),
                }
            )),
            _ => None,
        })
        .collect()
}

fn two_players(server: &mut rspigot_core::server::Server<Link>) -> (Player, Player) {
    let alice = arrive(server, "Alice");
    let bob = arrive(server, "Bob");
    send_input(
        server,
        &alice,
        PlayerInput::ChatSession(session_key(&alice, KEY_EXPIRES_MS)),
    );
    alice.link.take();
    bob.link.take();
    (alice, bob)
}

#[test]
fn a_session_key_signed_by_the_services_key_is_accepted() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    let key = session_key(&alice, KEY_EXPIRES_MS);
    send_input(&mut server, &alice, PlayerInput::ChatSession(key.clone()));
    assert_eq!(server.chat_session_of(alice.id), Some(key));
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn a_session_key_the_services_did_not_sign_disconnects_the_player() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    let mut key = session_key(&alice, KEY_EXPIRES_MS);
    key.expires_at_ms += 1;
    send_input(&mut server, &alice, PlayerInput::ChatSession(key));
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::translatable(
            "multiplayer.disconnect.invalid_public_key_signature",
            Vec::new()
        ))
    );
    assert_eq!(server.chat_session_of(alice.id), None);
}

#[test]
fn without_services_keys_the_session_is_ignored() {
    let mut server = configured(|_| {}).with_clock(clock);
    let alice = arrive(&mut server, "Alice");
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatSession(session_key(&alice, KEY_EXPIRES_MS)),
    );
    assert_eq!(server.chat_session_of(alice.id), None);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn signed_messages_are_broadcast_in_chain_order() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 0, "one", 100_000));
    send_input(&mut server, &alice, chat(&alice, 1, "two", 200_000));
    assert_eq!(heard(&bob), ["signed:one", "signed:two"]);
}

#[test]
fn an_unsigned_message_is_refused_when_the_profile_is_enforced() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert_eq!(heard(&alice), ["system:chat.disabled.missingProfileKey"]);
    assert!(heard(&bob).is_empty());
}

#[test]
fn a_message_without_a_session_is_refused_when_the_profile_is_enforced() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert_eq!(heard(&alice), ["system:chat.disabled.missingProfileKey"]);
    assert!(heard(&bob).is_empty());
}

#[test]
fn unsigned_chat_passes_when_the_profile_is_not_enforced() {
    let mut server = configured(|_| {}).with_clock(clock);
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert_eq!(heard(&bob), ["chat:hello"]);
}

#[test]
fn a_bad_signature_is_refused_and_breaks_the_chain() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 5, "forged", 100_000));
    assert_eq!(heard(&alice), ["system:chat.disabled.invalid_signature"]);
    send_input(&mut server, &alice, chat(&alice, 0, "after", 200_000));
    assert_eq!(heard(&alice), ["system:chat.disabled.chain_broken"]);
    assert!(heard(&bob).is_empty());
}

#[test]
fn an_older_timestamp_is_out_of_order() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 0, "late", 300_000));
    alice.link.take();
    send_input(&mut server, &alice, chat(&alice, 1, "early", 100_000));
    assert_eq!(heard(&alice), ["system:chat.disabled.out_of_order_chat"]);
}

#[test]
fn an_expired_key_stops_the_chat_without_a_kick() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatSession(session_key(&alice, 900_000)),
    );
    alice.link.take();
    send_input(&mut server, &alice, chat(&alice, 0, "late", 300_000));
    assert_eq!(heard(&alice), ["system:chat.disabled.expiredProfileKey"]);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn an_acknowledgement_of_messages_never_sent_disconnects_the_player() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    send_input(&mut server, &alice, PlayerInput::ChatAck(3));
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::translatable(
            "multiplayer.disconnect.chat_validation_failed",
            Vec::new()
        ))
    );
}

#[test]
fn a_last_seen_update_that_claims_unknown_messages_disconnects_the_player() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatMessage(Box::new(ChatMessage {
            content: "hi".to_owned(),
            timestamp_ms: 100_000,
            salt: 9,
            signature: Some(sign_message(&alice, 0, "hi", 100_000, 9)),
            last_seen: LastSeenUpdate {
                offset: 0,
                acknowledged: 1,
                checksum: 0,
            },
        })),
    );
    assert!(alice.link.disconnected_with().is_some());
}

#[test]
fn a_whisper_signed_argument_advances_the_chain() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(
        &mut server,
        &alice,
        PlayerInput::SignedCommand(SignedCommand {
            command: "msg Bob hi there".to_owned(),
            timestamp_ms: 100_000,
            salt: 9,
            arguments: vec![(
                "message".to_owned(),
                sign_message(&alice, 0, "hi there", 100_000, 9),
            )],
            last_seen: LastSeenUpdate::default(),
        }),
    );
    assert!(
        heard(&alice)
            .iter()
            .all(|line| !line.starts_with("system:chat.disabled"))
    );
    send_input(&mut server, &alice, chat(&alice, 1, "next", 200_000));
    assert_eq!(heard(&bob).last().map(String::as_str), Some("signed:next"));
}

#[test]
fn a_signed_command_for_arguments_the_server_does_not_expect_is_refused() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    send_input(
        &mut server,
        &alice,
        PlayerInput::SignedCommand(SignedCommand {
            command: "time set day".to_owned(),
            timestamp_ms: 100_000,
            salt: 9,
            arguments: vec![(
                "message".to_owned(),
                sign_message(&alice, 0, "day", 100_000, 9),
            )],
            last_seen: LastSeenUpdate::default(),
        }),
    );
    assert_eq!(
        heard(&alice),
        ["system:chat.disabled.invalid_command_signature"]
    );
}

#[test]
fn an_unsigned_command_with_signable_arguments_is_refused_when_enforced() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    send_input(
        &mut server,
        &alice,
        PlayerInput::Command("msg Bob hi".to_owned()),
    );
    assert_eq!(
        heard(&alice),
        ["system:chat.disabled.invalid_command_signature"]
    );
}

#[test]
fn a_later_key_replaces_the_session_and_an_identical_key_changes_nothing() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    let first = session_key(&alice, KEY_EXPIRES_MS);
    send_input(&mut server, &alice, PlayerInput::ChatSession(first.clone()));
    let renamed = ChatSessionKey {
        session: uuid::Uuid::from_u128(SESSION + 1),
        ..first.clone()
    };
    send_input(&mut server, &alice, PlayerInput::ChatSession(renamed));
    assert_eq!(server.chat_session_of(alice.id), Some(first));
    let later = session_key(&alice, KEY_EXPIRES_MS + 1_000);
    send_input(&mut server, &alice, PlayerInput::ChatSession(later.clone()));
    assert_eq!(server.chat_session_of(alice.id), Some(later));
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn a_key_that_expires_before_the_known_one_disconnects_the_player() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatSession(session_key(&alice, KEY_EXPIRES_MS)),
    );
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatSession(session_key(&alice, KEY_EXPIRES_MS - 1_000)),
    );
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::translatable(
            "multiplayer.disconnect.expired_public_key",
            Vec::new()
        ))
    );
}

fn signed_events(player: &Player) -> Vec<std::sync::Arc<SignedChat>> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SignedChat(signed) => Some(signed),
            _ => None,
        })
        .collect()
}

#[test]
fn the_relayed_signature_verifies_against_the_senders_public_key() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 0, "one", 100_000));
    let received = signed_events(&bob);
    assert_eq!(received.len(), 1);
    let first = &received[0];
    assert_eq!(first.sender, alice.id);
    assert_eq!(first.index, 0);
    assert_eq!(first.unsigned_content, None);
    let key =
        rspigot_domain::chat_chain::PublicKey::from_der(&public_der(&private(PLAYER_KEY))).unwrap();
    let signed = payload(
        &alice,
        first.index,
        &first.content,
        first.timestamp_ms,
        first.salt,
        &first.last_seen,
    );
    assert!(key.verify_sha256(&signed, &first.signature.0));
}

#[test]
fn the_second_message_lists_the_first_among_the_messages_the_sender_saw() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 0, "one", 100_000));
    let first = signed_events(&bob).remove(0);
    alice.link.take();
    let second_signature = sign_seen(&alice, 1, "two", 200_000, 9, &[first.signature]);
    send_input(
        &mut server,
        &alice,
        PlayerInput::ChatMessage(Box::new(ChatMessage {
            content: "two".to_owned(),
            timestamp_ms: 200_000,
            salt: 9,
            signature: Some(second_signature),
            last_seen: LastSeenUpdate {
                offset: 1,
                acknowledged: 1 << 19,
                checksum: 0,
            },
        })),
    );
    let second = signed_events(&bob).remove(0);
    assert_eq!(second.index, 1);
    assert_eq!(second.last_seen, vec![first.signature]);
    assert_eq!(second.signature, second_signature);
}

#[test]
fn a_newcomer_receives_the_sessions_of_the_players_already_online() {
    let mut server = secure_server();
    let (alice, _bob) = two_players(&mut server);
    let carol = arrive(&mut server, "Carol");
    let sessions: Vec<_> = carol
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::ChatSessions(sessions) => Some(sessions),
            _ => None,
        })
        .collect();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].len(), 1);
    assert_eq!(sessions[0][0].0, alice.id);
}

#[test]
fn a_session_update_is_announced_to_every_player() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    let key = session_key(&alice, KEY_EXPIRES_MS);
    send_input(&mut server, &alice, PlayerInput::ChatSession(key.clone()));
    for player in [&alice, &bob] {
        assert!(
            player
                .link
                .take()
                .contains(&ClientEvent::ChatSessions(vec![(alice.id, key.clone())]))
        );
    }
}

#[test]
fn a_message_without_a_session_is_relayed_as_plain_chat_when_not_enforced() {
    let mut server = configured(|config| {
        config.chat = ChatSecurity {
            enforce_secure_profile: false,
            services_keys: vec![public_der(&private(SERVICES_KEY))],
            ..ChatSecurity::default()
        }
    })
    .with_clock(clock);
    let alice = arrive(&mut server, "Alice");
    let bob = arrive(&mut server, "Bob");
    alice.link.take();
    bob.link.take();
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert_eq!(heard(&bob), ["chat:hello"]);
}

#[test]
fn a_player_without_a_key_cannot_chat_when_the_profile_is_enforced() {
    let mut server = secure_server();
    let alice = arrive(&mut server, "Alice");
    alice.link.take();
    send_input(&mut server, &alice, PlayerInput::Chat("hello".to_owned()));
    assert_eq!(heard(&alice), ["system:chat.disabled.missingProfileKey"]);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn deleting_a_relayed_message_reaches_every_player() {
    let mut server = secure_server();
    let (alice, bob) = two_players(&mut server);
    send_input(&mut server, &alice, chat(&alice, 0, "one", 100_000));
    let signature = signed_events(&bob).remove(0).signature;
    alice.link.take();
    assert!(server.delete_chat(alice.id, signature));
    server.deliver();
    for player in [&alice, &bob] {
        assert!(
            player
                .link
                .take()
                .contains(&ClientEvent::DeleteChat { signature })
        );
    }
    assert!(!server.delete_chat(alice.id, signature));
    assert!(!server.delete_chat(bob.id, signature));
}

fn plugin_scene() -> (Scene, Player, Player) {
    let mut s = scene_on(
        secure_server(),
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::AsyncPlayerChat)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    for player in [&alice, &bob] {
        send_input(
            &mut s.server,
            player,
            PlayerInput::ChatSession(session_key(player, KEY_EXPIRES_MS)),
        );
    }
    alice.link.take();
    bob.link.take();
    (s, alice, bob)
}

fn judged(patch: Option<AsyncPlayerChatPatch>) -> Vec<String> {
    let (mut s, alice, bob) = plugin_scene();
    send_input(&mut s.server, &alice, chat(&alice, 0, "orig", 100_000));
    let asked = last_asked(&s.plugin, EventKind::AsyncPlayerChat);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        patch.map(EventPatch::AsyncPlayerChat),
    );
    bob.link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SignedChat(signed) => Some(format!(
                "signed:{}:{:?}",
                signed.content, signed.unsigned_content
            )),
            ClientEvent::SystemChat { message, .. } => {
                Some(rspigot_core::console::console_text(&message))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_chat_the_plugin_leaves_alone_stays_signed() {
    assert_eq!(judged(None), ["signed:orig:None"]);
}

#[test]
fn a_message_the_plugin_edits_is_relayed_signed_with_unsigned_content() {
    let patch = AsyncPlayerChatPatch {
        message: Some("edited".to_owned()),
        ..AsyncPlayerChatPatch::default()
    };
    assert_eq!(judged(Some(patch)), ["signed:orig:Some(\"edited\")"]);
}

#[test]
fn a_format_the_plugin_changes_is_relayed_as_system_text() {
    let patch = AsyncPlayerChatPatch {
        format: Some("[%s] %s".to_owned()),
        ..AsyncPlayerChatPatch::default()
    };
    assert_eq!(judged(Some(patch)), ["[Alice] orig"]);
}
