use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const LAST_SEEN_COUNT: usize = 20;
pub const SIGNATURE_BYTES: usize = 256;

const SHA1_DIGEST_INFO: [u8; 15] = [
    0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
];
const SHA256_DIGEST_INFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];
const MESSAGE_VERSION: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; SIGNATURE_BYTES]);

impl MessageSignature {
    pub fn checksum(&self) -> i32 {
        self.0.iter().fold(1i32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(i32::from(*byte as i8))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey(RsaPublicKey);

impl PublicKey {
    pub fn from_der(der: &[u8]) -> Option<Self> {
        RsaPublicKey::from_public_key_der(der).ok().map(Self)
    }

    pub fn verify_sha256(&self, payload: &[u8], signature: &[u8]) -> bool {
        let digest = Sha256::digest(payload);
        let scheme = Pkcs1v15Sign {
            hash_len: Some(digest.len()),
            prefix: SHA256_DIGEST_INFO.to_vec().into_boxed_slice(),
        };
        self.0.verify(scheme, &digest, signature).is_ok()
    }

    pub fn verify_sha1(&self, payload: &[u8], signature: &[u8]) -> bool {
        let digest = Sha1::digest(payload);
        let scheme = Pkcs1v15Sign {
            hash_len: Some(digest.len()),
            prefix: SHA1_DIGEST_INFO.to_vec().into_boxed_slice(),
        };
        self.0.verify(scheme, &digest, signature).is_ok()
    }
}

pub fn session_key_payload(profile: Uuid, expires_at_ms: i64, public_key_der: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(24 + public_key_der.len());
    payload.extend_from_slice(profile.as_bytes());
    payload.extend_from_slice(&expires_at_ms.to_be_bytes());
    payload.extend_from_slice(public_key_der);
    payload
}

pub fn session_key_is_signed(
    services: &[PublicKey],
    profile: Uuid,
    expires_at_ms: i64,
    public_key_der: &[u8],
    key_signature: &[u8],
) -> bool {
    let payload = session_key_payload(profile, expires_at_ms, public_key_der);
    services
        .iter()
        .any(|key| key.verify_sha1(&payload, key_signature))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    MissingProfileKey,
    ChainBroken,
    ExpiredProfileKey,
    InvalidSignature,
    OutOfOrderChat,
}

impl ChainError {
    pub const fn translation_key(self) -> &'static str {
        match self {
            Self::MissingProfileKey => "chat.disabled.missingProfileKey",
            Self::ChainBroken => "chat.disabled.chain_broken",
            Self::ExpiredProfileKey => "chat.disabled.expiredProfileKey",
            Self::InvalidSignature => "chat.disabled.invalid_signature",
            Self::OutOfOrderChat => "chat.disabled.out_of_order_chat",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LastSeenError {
    #[error("offset {offset} is out of range, at most {max} messages can be acknowledged")]
    Offset { offset: i32, max: i32 },
    #[error("acknowledged bit {length} is past the {limit} tracked messages")]
    TooManyBits { length: usize, limit: usize },
    #[error("client acknowledged a message it was never sent at index {0}")]
    UnknownMessage(usize),
    #[error("client dropped an already acknowledged message at index {0}")]
    DroppedMessage(usize),
    #[error("checksum mismatch on last seen update, client and server have desynced")]
    Checksum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tracked {
    signature: MessageSignature,
    pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastSeenValidator {
    tracked: Vec<Option<Tracked>>,
    last_pending: Option<MessageSignature>,
}

impl Default for LastSeenValidator {
    fn default() -> Self {
        Self {
            tracked: vec![None; LAST_SEEN_COUNT],
            last_pending: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LastSeenUpdate {
    pub offset: i32,
    pub acknowledged: u32,
    pub checksum: i8,
}

impl LastSeenUpdate {
    fn bit_length(self) -> usize {
        (u32::BITS - self.acknowledged.leading_zeros()) as usize
    }

    fn is_set(self, index: usize) -> bool {
        index < 32 && self.acknowledged & (1 << index) != 0
    }
}

pub fn checksum_of(entries: &[MessageSignature]) -> i8 {
    let hash = entries.iter().fold(1i32, |hash, signature| {
        hash.wrapping_mul(31).wrapping_add(signature.checksum())
    });
    match hash as i8 {
        0 => 1,
        byte => byte,
    }
}

impl LastSeenValidator {
    pub fn add_pending(&mut self, signature: MessageSignature) {
        if self.last_pending != Some(signature) {
            self.tracked.push(Some(Tracked {
                signature,
                pending: true,
            }));
            self.last_pending = Some(signature);
        }
    }

    pub fn tracked_count(&self) -> usize {
        self.tracked.len()
    }

    pub fn apply_offset(&mut self, offset: i32) -> Result<(), LastSeenError> {
        let max = i32::try_from(self.tracked.len() - LAST_SEEN_COUNT).unwrap_or(i32::MAX);
        if offset < 0 || offset > max {
            return Err(LastSeenError::Offset { offset, max });
        }
        self.tracked.drain(..offset as usize);
        Ok(())
    }

    pub fn apply_update(
        &mut self,
        update: LastSeenUpdate,
    ) -> Result<Vec<MessageSignature>, LastSeenError> {
        self.apply_offset(update.offset)?;
        if update.bit_length() > LAST_SEEN_COUNT {
            return Err(LastSeenError::TooManyBits {
                length: update.bit_length(),
                limit: LAST_SEEN_COUNT,
            });
        }
        let mut seen = Vec::new();
        for index in 0..LAST_SEEN_COUNT {
            let entry = self.tracked[index].as_mut();
            if update.is_set(index) {
                let Some(entry) = entry else {
                    return Err(LastSeenError::UnknownMessage(index));
                };
                entry.pending = false;
                seen.push(entry.signature);
            } else {
                if entry.is_some_and(|entry| !entry.pending) {
                    return Err(LastSeenError::DroppedMessage(index));
                }
                self.tracked[index] = None;
            }
        }
        if update.checksum != 0 && update.checksum != checksum_of(&seen) {
            return Err(LastSeenError::Checksum);
        }
        Ok(seen)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Link {
    index: i32,
    sender: Uuid,
    session: Uuid,
}

impl Link {
    fn advance(self) -> Option<Self> {
        (self.index != i32::MAX).then_some(Self {
            index: self.index + 1,
            ..self
        })
    }
}

pub struct MessageBody<'a> {
    pub content: &'a str,
    pub timestamp_ms: i64,
    pub salt: i64,
    pub last_seen: &'a [MessageSignature],
}

fn signed_payload(link: Link, body: &MessageBody<'_>) -> Vec<u8> {
    let content = body.content.as_bytes();
    let mut payload =
        Vec::with_capacity(64 + content.len() + body.last_seen.len() * SIGNATURE_BYTES);
    payload.extend(MESSAGE_VERSION.to_be_bytes());
    payload.extend_from_slice(link.sender.as_bytes());
    payload.extend_from_slice(link.session.as_bytes());
    payload.extend(link.index.to_be_bytes());
    payload.extend(body.salt.to_be_bytes());
    payload.extend(body.timestamp_ms.div_euclid(1000).to_be_bytes());
    payload.extend((content.len() as i32).to_be_bytes());
    payload.extend_from_slice(content);
    payload.extend((body.last_seen.len() as i32).to_be_bytes());
    for signature in body.last_seen {
        payload.extend_from_slice(&signature.0);
    }
    payload
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Keyed {
    key: PublicKey,
    expires_at_ms: i64,
    next: Option<Link>,
    last_timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decoder {
    keyed: Option<Keyed>,
}

impl Decoder {
    pub fn unsigned() -> Self {
        Self::default()
    }

    pub fn keyed(sender: Uuid, session: Uuid, key: PublicKey, expires_at_ms: i64) -> Self {
        Self {
            keyed: Some(Keyed {
                key,
                expires_at_ms,
                next: Some(Link {
                    index: 0,
                    sender,
                    session,
                }),
                last_timestamp_ms: 0,
            }),
        }
    }

    pub fn set_chain_broken(&mut self) {
        if let Some(keyed) = &mut self.keyed {
            keyed.next = None;
        }
    }

    pub fn unpack(
        &mut self,
        enforces_secure_chat: bool,
        now_ms: i64,
        signature: Option<&MessageSignature>,
        body: &MessageBody<'_>,
    ) -> Result<(), ChainError> {
        let Some(keyed) = &mut self.keyed else {
            return if enforces_secure_chat {
                Err(ChainError::MissingProfileKey)
            } else {
                Ok(())
            };
        };
        let signature = signature.ok_or(ChainError::MissingProfileKey)?;
        if keyed.expires_at_ms < now_ms {
            return Err(ChainError::ExpiredProfileKey);
        }
        let link = keyed.next.ok_or(ChainError::ChainBroken)?;
        if body.timestamp_ms < keyed.last_timestamp_ms {
            keyed.next = None;
            return Err(ChainError::OutOfOrderChat);
        }
        keyed.last_timestamp_ms = body.timestamp_ms;
        if !keyed
            .key
            .verify_sha256(&signed_payload(link, body), &signature.0)
        {
            keyed.next = None;
            return Err(ChainError::InvalidSignature);
        }
        keyed.next = link.advance();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatState {
    pub last_seen: LastSeenValidator,
    pub decoder: Decoder,
}

#[cfg(test)]
mod tests;
