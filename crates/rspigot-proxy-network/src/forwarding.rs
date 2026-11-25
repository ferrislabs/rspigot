use std::net::IpAddr;

use hmac::{Hmac, Mac};
use rspigot_protocol::codec::{Encode, VarInt};
use rspigot_protocol::types::ProfileProperty;
use sha2::Sha256;
use uuid::Uuid;

pub const CHANNEL: &str = "velocity:player_info";
pub const MODERN_DEFAULT: u8 = 1;
pub const MODERN_LAZY_SESSION: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedPlayer {
    pub address: IpAddr,
    pub id: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

pub fn negotiate(requested: Option<u8>) -> u8 {
    match requested {
        Some(version) if version >= MODERN_LAZY_SESSION => MODERN_LAZY_SESSION,
        _ => MODERN_DEFAULT,
    }
}

pub fn signed_payload(secret: &[u8], version: u8, player: &ForwardedPlayer) -> Vec<u8> {
    let mut data = Vec::with_capacity(256);
    VarInt(i32::from(version)).encode(&mut data);
    player.address.to_string().encode(&mut data);
    player.id.encode(&mut data);
    player.name.encode(&mut data);
    player.properties.encode(&mut data);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(&data);
    let mut payload = mac.finalize().into_bytes().to_vec();
    payload.extend_from_slice(&data);
    payload
}

#[cfg(test)]
mod tests {
    use rspigot_network::verify_velocity;

    use super::*;

    fn player() -> ForwardedPlayer {
        ForwardedPlayer {
            address: "203.0.113.9".parse().unwrap(),
            id: Uuid::from_u128(0xabcd),
            name: "Steve".to_owned(),
            properties: vec![ProfileProperty {
                name: "textures".to_owned(),
                value: "skin".to_owned(),
                signature: Some("sig".to_owned()),
            }],
        }
    }

    #[test]
    fn rspigot_accepts_the_payload_signed_by_the_proxy() {
        for version in [MODERN_DEFAULT, MODERN_LAZY_SESSION] {
            let payload = signed_payload(b"secret", version, &player());
            let forwarded = verify_velocity(b"secret", &payload).unwrap();
            assert_eq!(forwarded.address, player().address);
            assert_eq!(forwarded.id, player().id);
            assert_eq!(forwarded.name.as_deref(), Some("Steve"));
            assert_eq!(forwarded.properties.len(), 1);
            assert_eq!(forwarded.properties[0].value, "skin");
            assert_eq!(forwarded.properties[0].signature.as_deref(), Some("sig"));
        }
    }

    #[test]
    fn a_different_secret_is_refused_by_rspigot() {
        let payload = signed_payload(b"secret", MODERN_DEFAULT, &player());
        assert!(verify_velocity(b"other", &payload).is_err());
    }

    #[test]
    fn the_version_follows_what_the_backend_asks_for() {
        assert_eq!(negotiate(None), MODERN_DEFAULT);
        assert_eq!(negotiate(Some(1)), MODERN_DEFAULT);
        assert_eq!(negotiate(Some(3)), MODERN_DEFAULT);
        assert_eq!(negotiate(Some(4)), MODERN_LAZY_SESSION);
        assert_eq!(negotiate(Some(9)), MODERN_LAZY_SESSION);
    }
}
