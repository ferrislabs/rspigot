use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use rspigot_core::status::StatusBoard;
use tokio::net::UdpSocket;
use tokio::sync::watch;

const MAGIC: [u8; 2] = [0xFE, 0xFD];
const HANDSHAKE: u8 = 9;
const STAT: u8 = 0;
const TOKEN_LIFETIME: Duration = Duration::from_secs(30);
const FULL_STAT_LENGTH: usize = 15;
const SESSION_MASK: u32 = 0x0F0F_0F0F;

#[derive(Debug, Clone)]
pub struct QueryConfig {
    pub bind: SocketAddr,
    pub motd: String,
    pub map: String,
    pub version: String,
    pub server_port: u16,
    pub server_ip: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryInfo {
    pub motd: String,
    pub map: String,
    pub version: String,
    pub online: u32,
    pub max_players: u32,
    pub server_port: u16,
    pub server_ip: String,
    pub players: Vec<String>,
}

#[derive(Default)]
pub struct Challenges {
    issued: HashMap<IpAddr, (i32, Instant)>,
}

impl Challenges {
    fn issue(&mut self, address: IpAddr, now: Instant, token: i32) -> i32 {
        self.issued
            .retain(|_, (_, at)| now.duration_since(*at) < TOKEN_LIFETIME);
        self.issued.insert(address, (token, now));
        token
    }

    fn valid(&self, address: IpAddr, now: Instant, token: i32) -> bool {
        self.issued.get(&address).is_some_and(|(issued, at)| {
            *issued == token && now.duration_since(*at) < TOKEN_LIFETIME
        })
    }
}

fn cstring(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    out.push(0);
}

fn header(kind: u8, session: u32) -> Vec<u8> {
    let mut out = vec![kind];
    out.extend_from_slice(&session.to_be_bytes());
    out
}

pub fn basic_stat(session: u32, info: &QueryInfo) -> Vec<u8> {
    let mut out = header(STAT, session);
    cstring(&mut out, &info.motd);
    cstring(&mut out, "SMP");
    cstring(&mut out, &info.map);
    cstring(&mut out, &info.online.to_string());
    cstring(&mut out, &info.max_players.to_string());
    out.extend_from_slice(&info.server_port.to_le_bytes());
    cstring(&mut out, &info.server_ip);
    out
}

pub fn full_stat(session: u32, info: &QueryInfo) -> Vec<u8> {
    let mut out = header(STAT, session);
    out.extend_from_slice(b"splitnum\0\x80\0");
    for (key, value) in [
        ("hostname", info.motd.as_str()),
        ("gametype", "SMP"),
        ("game_id", "MINECRAFT"),
        ("version", info.version.as_str()),
        ("plugins", ""),
        ("map", info.map.as_str()),
        ("numplayers", &info.online.to_string()),
        ("maxplayers", &info.max_players.to_string()),
        ("hostport", &info.server_port.to_string()),
        ("hostip", info.server_ip.as_str()),
    ] {
        cstring(&mut out, key);
        cstring(&mut out, value);
    }
    out.push(0);
    cstring(&mut out, "\u{1}player_");
    out.push(0);
    for name in &info.players {
        cstring(&mut out, name);
    }
    out.push(0);
    out
}

pub fn respond(
    datagram: &[u8],
    peer: IpAddr,
    now: Instant,
    challenges: &mut Challenges,
    random_token: i32,
    info: impl FnOnce() -> QueryInfo,
) -> Option<Vec<u8>> {
    if datagram.len() < 7 || datagram[..2] != MAGIC {
        return None;
    }
    let kind = datagram[2];
    let session = u32::from_be_bytes(datagram[3..7].try_into().ok()?) & SESSION_MASK;
    match kind {
        HANDSHAKE => {
            let token = challenges.issue(peer, now, random_token & 0x00FF_FFFF);
            let mut out = header(HANDSHAKE, session);
            cstring(&mut out, &token.to_string());
            Some(out)
        }
        STAT => {
            let token = i32::from_be_bytes(datagram.get(7..11)?.try_into().ok()?);
            if !challenges.valid(peer, now, token) {
                return None;
            }
            let info = info();
            Some(if datagram.len() == FULL_STAT_LENGTH {
                full_stat(session, &info)
            } else {
                basic_stat(session, &info)
            })
        }
        _ => None,
    }
}

pub async fn serve_query(
    config: QueryConfig,
    status: StatusBoard,
    mut shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    let socket = UdpSocket::bind(config.bind).await?;
    tracing::info!("Query running on {}", socket.local_addr()?);
    let mut challenges = Challenges::default();
    let mut buffer = [0u8; 1460];
    loop {
        tokio::select! {
            received = socket.recv_from(&mut buffer) => {
                let (length, peer) = received?;
                let snapshot = status.snapshot();
                let reply = respond(
                    &buffer[..length],
                    peer.ip().to_canonical(),
                    Instant::now(),
                    &mut challenges,
                    rand::random(),
                    || QueryInfo {
                        motd: config.motd.clone(),
                        map: config.map.clone(),
                        version: config.version.clone(),
                        online: snapshot.online,
                        max_players: snapshot.max_players,
                        server_port: config.server_port,
                        server_ip: config.server_ip.clone(),
                        players: snapshot.sample.iter().map(|p| p.name.clone()).collect(),
                    },
                );
                if let Some(reply) = reply {
                    let _ = socket.send_to(&reply, peer).await;
                }
            }
            _ = shutdown.changed() => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> QueryInfo {
        QueryInfo {
            motd: "Hello".to_owned(),
            map: "world".to_owned(),
            version: "26.3".to_owned(),
            online: 2,
            max_players: 20,
            server_port: 25565,
            server_ip: "127.0.0.1".to_owned(),
            players: vec!["Ann".to_owned(), "Bob".to_owned()],
        }
    }

    fn peer() -> IpAddr {
        "198.51.100.7".parse().unwrap()
    }

    fn handshake(session: u32) -> Vec<u8> {
        let mut out = vec![0xFE, 0xFD, 9];
        out.extend_from_slice(&session.to_be_bytes());
        out
    }

    fn stat(session: u32, token: i32, full: bool) -> Vec<u8> {
        let mut out = vec![0xFE, 0xFD, 0];
        out.extend_from_slice(&session.to_be_bytes());
        out.extend_from_slice(&token.to_be_bytes());
        if full {
            out.extend_from_slice(&[0; 4]);
        }
        out
    }

    fn token_from(reply: &[u8]) -> i32 {
        let text = std::str::from_utf8(&reply[5..reply.len() - 1]).unwrap();
        text.parse().unwrap()
    }

    #[test]
    fn the_handshake_returns_the_masked_session_and_a_decimal_token() {
        let mut challenges = Challenges::default();
        let reply = respond(
            &handshake(0x0102_0304),
            peer(),
            Instant::now(),
            &mut challenges,
            0x1234_5678,
            info,
        )
        .unwrap();
        assert_eq!(reply[0], 9);
        assert_eq!(&reply[1..5], &0x0102_0304u32.to_be_bytes());
        assert_eq!(token_from(&reply), 0x0034_5678);
        assert_eq!(*reply.last().unwrap(), 0);
    }

    #[test]
    fn a_stat_request_needs_the_issued_token() {
        let mut challenges = Challenges::default();
        let now = Instant::now();
        let reply = respond(&handshake(1), peer(), now, &mut challenges, 77, info).unwrap();
        let token = token_from(&reply);
        assert!(
            respond(
                &stat(1, token + 1, false),
                peer(),
                now,
                &mut challenges,
                0,
                info
            )
            .is_none()
        );
        let other: IpAddr = "198.51.100.8".parse().unwrap();
        assert!(respond(&stat(1, token, false), other, now, &mut challenges, 0, info).is_none());
        assert!(
            respond(
                &stat(1, token, false),
                peer(),
                now,
                &mut challenges,
                0,
                info
            )
            .is_some()
        );
    }

    #[test]
    fn tokens_expire_after_thirty_seconds() {
        let mut challenges = Challenges::default();
        let now = Instant::now();
        respond(&handshake(1), peer(), now, &mut challenges, 77, info).unwrap();
        let later = now + Duration::from_secs(31);
        assert!(respond(&stat(1, 77, false), peer(), later, &mut challenges, 0, info).is_none());
    }

    #[test]
    fn the_basic_stat_lists_the_fixed_fields_with_a_little_endian_port() {
        let bytes = basic_stat(5, &info());
        let mut expected = vec![0, 0, 0, 0, 5];
        expected.extend_from_slice(b"Hello\0SMP\0world\x002\x0020\0");
        expected.extend_from_slice(&25565u16.to_le_bytes());
        expected.extend_from_slice(b"127.0.0.1\0");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn the_full_stat_is_key_values_then_players() {
        let mut challenges = Challenges::default();
        let now = Instant::now();
        respond(&handshake(1), peer(), now, &mut challenges, 9, info).unwrap();
        let bytes = respond(&stat(1, 9, true), peer(), now, &mut challenges, 0, info).unwrap();
        let body = &bytes[5..];
        assert!(body.starts_with(
            b"splitnum\0\x80\0hostname\0Hello\0gametype\0SMP\0game_id\0MINECRAFT\0version\0"
        ));
        assert!(body.ends_with(b"hostip\0127.0.0.1\0\0\x01player_\0\0Ann\0Bob\0\0"));
        assert!(body.windows(13).any(|w| w == b"numplayers\x002\0"));
    }

    #[test]
    fn garbage_and_unknown_types_are_ignored() {
        let mut challenges = Challenges::default();
        let now = Instant::now();
        assert!(respond(&[1, 2, 3], peer(), now, &mut challenges, 0, info).is_none());
        assert!(
            respond(
                &[0xFE, 0xFD, 5, 0, 0, 0, 1],
                peer(),
                now,
                &mut challenges,
                0,
                info
            )
            .is_none()
        );
        assert!(
            respond(
                &[0, 0, 9, 0, 0, 0, 1],
                peer(),
                now,
                &mut challenges,
                0,
                info
            )
            .is_none()
        );
    }
}
