use std::net::IpAddr;

use rspigot_domain::player::ProfileProperty;
use rspigot_protocol::codec::{Decode, VarInt, decode_string};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const VELOCITY_CHANNEL: &str = "velocity:player_info";
pub const MAX_FORWARDING_VERSION: u8 = 4;
pub const BUNGEE_MISSING: &str =
    "If you wish to use IP forwarding, please enable it in your BungeeCord config as well!";
pub const VELOCITY_REQUIRED: &str = "This server requires you to connect with Velocity.";
pub const VELOCITY_BAD_SIGNATURE: &str =
    "Unable to verify player details. Is the forwarding secret the same as on the proxy?";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyForwarding {
    #[default]
    Off,
    BungeeCord,
    Velocity(Vec<u8>),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProxyError {
    #[error("the handshake carries no forwarded data")]
    NotForwarded,
    #[error("the forwarded data is malformed")]
    Malformed,
    #[error("the forwarding signature does not match")]
    BadSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forwarded {
    pub address: IpAddr,
    pub id: Uuid,
    pub name: Option<String>,
    pub properties: Vec<ProfileProperty>,
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(data);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn parse_bungee(host: &str) -> Result<Forwarded, ProxyError> {
    let mut parts = host.split('\0');
    let _real_host = parts.next();
    let address = parts.next().ok_or(ProxyError::NotForwarded)?;
    let id = parts.next().ok_or(ProxyError::NotForwarded)?;
    let address: IpAddr = address.parse().map_err(|_| ProxyError::Malformed)?;
    let id = Uuid::parse_str(id).map_err(|_| ProxyError::Malformed)?;
    let properties = match parts.next().filter(|p| !p.is_empty()) {
        None => Vec::new(),
        Some(json) => parse_properties(json)?,
    };
    Ok(Forwarded {
        address: address.to_canonical(),
        id,
        name: None,
        properties,
    })
}

fn parse_properties(json: &str) -> Result<Vec<ProfileProperty>, ProxyError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ProxyError::Malformed)?;
    value
        .as_array()
        .ok_or(ProxyError::Malformed)?
        .iter()
        .map(|entry| {
            let text = |key: &str| entry.get(key).and_then(serde_json::Value::as_str);
            Ok(ProfileProperty {
                name: text("name").ok_or(ProxyError::Malformed)?.to_owned(),
                value: text("value").ok_or(ProxyError::Malformed)?.to_owned(),
                signature: text("signature").map(str::to_owned),
            })
        })
        .collect()
}

pub fn verify_velocity(secret: &[u8], payload: &[u8]) -> Result<Forwarded, ProxyError> {
    if payload.len() < 32 {
        return Err(ProxyError::Malformed);
    }
    let (signature, data) = payload.split_at(32);
    if !equal(signature, &hmac_sha256(secret, data)) {
        return Err(ProxyError::BadSignature);
    }
    parse_velocity(data)
}

fn parse_velocity(data: &[u8]) -> Result<Forwarded, ProxyError> {
    let malformed = |_| ProxyError::Malformed;
    let mut input = data;
    let version = VarInt::decode(&mut input).map_err(malformed)?.0;
    if !(1..=i32::from(MAX_FORWARDING_VERSION)).contains(&version) {
        return Err(ProxyError::Malformed);
    }
    let address = decode_string(&mut input, 45).map_err(malformed)?;
    let address: IpAddr = address.parse().map_err(|_| ProxyError::Malformed)?;
    let id = Uuid::decode(&mut input).map_err(malformed)?;
    let name = decode_string(&mut input, 16).map_err(malformed)?;
    let count = VarInt::decode(&mut input).map_err(malformed)?.0;
    let count = usize::try_from(count).map_err(|_| ProxyError::Malformed)?;
    if count > 64 {
        return Err(ProxyError::Malformed);
    }
    let mut properties = Vec::with_capacity(count);
    for _ in 0..count {
        let name = decode_string(&mut input, 64).map_err(malformed)?;
        let value = decode_string(&mut input, 32767).map_err(malformed)?;
        let signature = if bool::decode(&mut input).map_err(malformed)? {
            Some(decode_string(&mut input, 1024).map_err(malformed)?)
        } else {
            None
        };
        properties.push(ProfileProperty {
            name,
            value,
            signature,
        });
    }
    Ok(Forwarded {
        address: address.to_canonical(),
        id,
        name: Some(name),
        properties,
    })
}

#[cfg(test)]
mod tests {
    use rspigot_protocol::codec::Encode;

    use super::*;

    fn velocity_data(version: i32, address: &str, id: Uuid, name: &str) -> Vec<u8> {
        let mut out = Vec::new();
        VarInt(version).encode(&mut out);
        address.to_owned().encode(&mut out);
        id.encode(&mut out);
        name.to_owned().encode(&mut out);
        VarInt(1).encode(&mut out);
        "textures".to_owned().encode(&mut out);
        "value".to_owned().encode(&mut out);
        true.encode(&mut out);
        "sig".to_owned().encode(&mut out);
        out
    }

    fn signed(secret: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = hmac_sha256(secret, data).to_vec();
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn hmac_matches_the_rfc_4231_first_vector() {
        let mac = hmac_sha256(&[0x0b; 20], b"Hi There");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn a_signed_velocity_payload_yields_the_forwarded_player() {
        let id = Uuid::from_u128(0x1234);
        let payload = signed(b"secret", &velocity_data(4, "203.0.113.9", id, "Steve"));
        let forwarded = verify_velocity(b"secret", &payload).unwrap();
        assert_eq!(forwarded.address, "203.0.113.9".parse::<IpAddr>().unwrap());
        assert_eq!(forwarded.id, id);
        assert_eq!(forwarded.name.as_deref(), Some("Steve"));
        assert_eq!(forwarded.properties[0].signature.as_deref(), Some("sig"));
    }

    #[test]
    fn a_wrong_secret_or_a_tampered_payload_is_refused() {
        let data = velocity_data(1, "203.0.113.9", Uuid::nil(), "Steve");
        let payload = signed(b"secret", &data);
        assert_eq!(
            verify_velocity(b"other", &payload),
            Err(ProxyError::BadSignature)
        );
        let mut tampered = payload.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert_eq!(
            verify_velocity(b"secret", &tampered),
            Err(ProxyError::BadSignature)
        );
        assert_eq!(
            verify_velocity(b"secret", &[0; 5]),
            Err(ProxyError::Malformed)
        );
    }

    #[test]
    fn unsupported_forwarding_versions_are_refused() {
        let payload = signed(b"s", &velocity_data(9, "10.0.0.1", Uuid::nil(), "A"));
        assert_eq!(verify_velocity(b"s", &payload), Err(ProxyError::Malformed));
    }

    #[test]
    fn bungee_hosts_carry_ip_uuid_and_properties() {
        let host = "mc.example.com\x00198.51.100.4\x0011111111222233334444555555555555\x00[{\"name\":\"textures\",\"value\":\"v\",\"signature\":\"s\"}]";
        let forwarded = parse_bungee(host).unwrap();
        assert_eq!(forwarded.address, "198.51.100.4".parse::<IpAddr>().unwrap());
        assert_eq!(
            forwarded.id,
            Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap()
        );
        assert_eq!(forwarded.properties.len(), 1);
        assert_eq!(forwarded.name, None);
    }

    #[test]
    fn a_plain_host_or_garbage_is_not_bungee_data() {
        assert_eq!(parse_bungee("localhost"), Err(ProxyError::NotForwarded));
        assert_eq!(
            parse_bungee("h\x00not-an-ip\x0011111111222233334444555555555555"),
            Err(ProxyError::Malformed)
        );
        assert_eq!(
            parse_bungee("h\x001.2.3.4\x00zz"),
            Err(ProxyError::Malformed)
        );
    }
}
