use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Sign, RsaPrivateKey};
use sha2::{Digest, Sha256};

use super::*;

const VECTOR: &str = include_str!("vector.txt");
const PLAYER: &str = include_str!("player.hex");

fn unhex(text: &str) -> Vec<u8> {
    (0..text.trim().len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn vector(name: &str) -> String {
    VECTOR
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .unwrap()
        .to_owned()
}

fn private(hex: &str) -> RsaPrivateKey {
    RsaPrivateKey::from_pkcs1_der(&unhex(hex)).unwrap()
}

fn der_of(key: &RsaPrivateKey) -> Vec<u8> {
    key.to_public_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec()
}

fn sign_sha256(key: &RsaPrivateKey, payload: &[u8]) -> Vec<u8> {
    let digest = Sha256::digest(payload);
    let scheme = Pkcs1v15Sign {
        hash_len: Some(digest.len()),
        prefix: SHA256_DIGEST_INFO.to_vec().into_boxed_slice(),
    };
    key.sign(scheme, &digest).unwrap()
}

fn signature(bytes: Vec<u8>) -> MessageSignature {
    MessageSignature(bytes.try_into().unwrap())
}

fn sender() -> Uuid {
    Uuid::from_u128(1)
}

fn session() -> Uuid {
    Uuid::from_u128(2)
}

struct Player {
    key: RsaPrivateKey,
    decoder: Decoder,
    index: i32,
}

impl Player {
    fn new() -> Self {
        let key = private(PLAYER);
        let public = PublicKey::from_der(&der_of(&key)).unwrap();
        Self {
            decoder: Decoder::keyed(sender(), session(), public, 2_000_000),
            key,
            index: 0,
        }
    }

    fn sign(&self, index: i32, content: &str, timestamp_ms: i64) -> MessageSignature {
        let link = Link {
            index,
            sender: sender(),
            session: session(),
        };
        let body = MessageBody {
            content,
            timestamp_ms,
            salt: 5,
            last_seen: &[],
        };
        signature(sign_sha256(&self.key, &signed_payload(link, &body)))
    }

    fn say(&mut self, content: &str, timestamp_ms: i64) -> Result<(), ChainError> {
        let signed = self.sign(self.index, content, timestamp_ms);
        let result = self.decoder.unpack(
            true,
            1_000_000,
            Some(&signed),
            &MessageBody {
                content,
                timestamp_ms,
                salt: 5,
                last_seen: &[],
            },
        );
        if result.is_ok() {
            self.index += 1;
        }
        result
    }
}

#[test]
fn a_message_signed_by_the_vanilla_client_code_verifies_with_the_same_payload() {
    let key = PublicKey::from_der(&unhex(&vector("player_pub"))).unwrap();
    let seen = [
        signature(unhex(&vector("seen1"))),
        signature(unhex(&vector("seen2"))),
    ];
    let link = Link {
        index: 7,
        sender: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        session: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap(),
    };
    let body = MessageBody {
        content: "héllo wörld §",
        timestamp_ms: 1_700_000_123_456,
        salt: 987_654_321_012_345,
        last_seen: &seen,
    };
    let signed = unhex(&vector("sig"));
    assert!(key.verify_sha256(&signed_payload(link, &body), &signed));
    let other = MessageBody {
        content: "héllo wörld",
        ..body
    };
    assert!(!key.verify_sha256(&signed_payload(link, &other), &signed));
    assert_eq!(
        i32::from(checksum_of(&seen)),
        vector("checksum").parse::<i32>().unwrap()
    );
}

#[test]
fn a_session_key_signed_by_the_services_key_verifies_like_vanilla() {
    let services = PublicKey::from_der(&unhex(&vector("services_pub"))).unwrap();
    let profile = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
    let key = unhex(&vector("player_pub"));
    let key_signature = unhex(&vector("keysig"));
    let expires: i64 = vector("expires").parse().unwrap();
    let services = [services];
    assert!(session_key_is_signed(
        &services,
        profile,
        expires,
        &key,
        &key_signature
    ));
    assert!(!session_key_is_signed(
        &services,
        profile,
        expires + 1,
        &key,
        &key_signature
    ));
    assert!(!session_key_is_signed(
        &services,
        Uuid::nil(),
        expires,
        &key,
        &key_signature
    ));
    let other = PublicKey::from_der(&der_of(&private(include_str!("other.hex")))).unwrap();
    assert!(!session_key_is_signed(
        &[other],
        profile,
        expires,
        &key,
        &key_signature
    ));
    assert!(!session_key_is_signed(
        &[],
        profile,
        expires,
        &key,
        &key_signature
    ));
}

#[test]
fn signed_messages_chain_by_index_and_replays_break_the_chain() {
    let mut player = Player::new();
    assert_eq!(player.say("one", 100), Ok(()));
    assert_eq!(player.say("two", 200), Ok(()));
    let replay = player.sign(0, "again", 300);
    let body = MessageBody {
        content: "again",
        timestamp_ms: 300,
        salt: 5,
        last_seen: &[],
    };
    assert_eq!(
        player.decoder.unpack(true, 1_000_000, Some(&replay), &body),
        Err(ChainError::InvalidSignature)
    );
    let next = player.sign(2, "three", 400);
    let body = MessageBody {
        content: "three",
        timestamp_ms: 400,
        salt: 5,
        last_seen: &[],
    };
    assert_eq!(
        player.decoder.unpack(true, 1_000_000, Some(&next), &body),
        Err(ChainError::ChainBroken)
    );
}

#[test]
fn a_tampered_message_is_refused() {
    let mut player = Player::new();
    let signed = player.sign(0, "hello", 100);
    let body = MessageBody {
        content: "hellp",
        timestamp_ms: 100,
        salt: 5,
        last_seen: &[],
    };
    assert_eq!(
        player.decoder.unpack(true, 1_000_000, Some(&signed), &body),
        Err(ChainError::InvalidSignature)
    );
}

#[test]
fn a_timestamp_before_the_previous_one_is_out_of_order() {
    let mut player = Player::new();
    assert_eq!(player.say("one", 500), Ok(()));
    assert_eq!(player.say("two", 400), Err(ChainError::OutOfOrderChat));
    assert_eq!(player.say("three", 600), Err(ChainError::ChainBroken));
}

#[test]
fn a_missing_signature_or_an_expired_key_is_refused_without_breaking_the_chain() {
    let mut player = Player::new();
    let body = MessageBody {
        content: "x",
        timestamp_ms: 100,
        salt: 5,
        last_seen: &[],
    };
    assert_eq!(
        player.decoder.unpack(false, 0, None, &body),
        Err(ChainError::MissingProfileKey)
    );
    let signed = player.sign(0, "x", 100);
    assert_eq!(
        player.decoder.unpack(true, 2_000_001, Some(&signed), &body),
        Err(ChainError::ExpiredProfileKey)
    );
    assert_eq!(player.say("x", 100), Ok(()));
}

#[test]
fn an_unsigned_decoder_only_accepts_messages_while_secure_chat_is_not_enforced() {
    let mut decoder = Decoder::unsigned();
    let body = MessageBody {
        content: "x",
        timestamp_ms: 0,
        salt: 0,
        last_seen: &[],
    };
    assert_eq!(decoder.unpack(false, 0, None, &body), Ok(()));
    assert_eq!(
        decoder.unpack(true, 0, None, &body),
        Err(ChainError::MissingProfileKey)
    );
}

fn sig(byte: u8) -> MessageSignature {
    MessageSignature([byte; SIGNATURE_BYTES])
}

#[test]
fn the_offset_cannot_pass_the_messages_the_server_has_sent() {
    let mut validator = LastSeenValidator::default();
    assert_eq!(validator.apply_offset(0), Ok(()));
    assert_eq!(
        validator.apply_offset(1),
        Err(LastSeenError::Offset { offset: 1, max: 0 })
    );
    assert!(validator.apply_offset(-1).is_err());
    validator.add_pending(sig(1));
    validator.add_pending(sig(1));
    assert_eq!(validator.tracked_count(), LAST_SEEN_COUNT + 1);
    assert_eq!(validator.apply_offset(1), Ok(()));
    assert_eq!(validator.tracked_count(), LAST_SEEN_COUNT);
}

fn two_pending() -> LastSeenValidator {
    let mut validator = LastSeenValidator::default();
    validator.add_pending(sig(1));
    validator.add_pending(sig(2));
    validator
}

fn window(bits: &[usize], offset: i32, checksum: i8) -> LastSeenUpdate {
    LastSeenUpdate {
        offset,
        acknowledged: bits.iter().fold(0, |mask, bit| mask | 1 << bit),
        checksum,
    }
}

#[test]
fn acknowledging_the_pending_messages_returns_them_in_order() {
    let mut validator = two_pending();
    let checksum = checksum_of(&[sig(1), sig(2)]);
    assert_eq!(
        validator.apply_update(window(&[18, 19], 2, checksum)),
        Ok(vec![sig(1), sig(2)])
    );
    assert_eq!(
        validator.apply_update(window(&[18, 19], 0, 0)),
        Ok(vec![sig(1), sig(2)])
    );
}

#[test]
fn acknowledging_a_message_that_was_never_sent_is_refused() {
    let mut validator = two_pending();
    assert_eq!(
        validator.apply_update(window(&[0], 2, 0)),
        Err(LastSeenError::UnknownMessage(0))
    );
}

#[test]
fn a_wrong_checksum_means_the_client_and_server_desynced() {
    let mut validator = two_pending();
    let wrong = checksum_of(&[sig(1), sig(2)]).wrapping_add(1);
    assert_eq!(
        validator.apply_update(window(&[18, 19], 2, wrong)),
        Err(LastSeenError::Checksum)
    );
}

#[test]
fn a_message_the_client_acknowledged_cannot_be_forgotten_again() {
    let mut validator = two_pending();
    validator.apply_update(window(&[18, 19], 2, 0)).unwrap();
    assert_eq!(
        validator.apply_update(window(&[19], 0, 0)),
        Err(LastSeenError::DroppedMessage(18))
    );
}

#[test]
fn bits_past_the_window_are_refused() {
    let mut validator = two_pending();
    assert_eq!(
        validator.apply_update(window(&[20], 0, 0)),
        Err(LastSeenError::TooManyBits {
            length: 21,
            limit: 20
        })
    );
}

#[test]
fn a_pending_message_the_client_has_not_seen_yet_may_stay_unacknowledged() {
    let mut validator = two_pending();
    assert_eq!(validator.apply_update(window(&[], 0, 0)), Ok(vec![]));
}
