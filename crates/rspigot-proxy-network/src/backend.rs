use rspigot_protocol::codec::decode_exact;
use rspigot_protocol::frame::Compression;
use rspigot_protocol::handshake::{ClientIntent, Intention};
use rspigot_protocol::ids::login::clientbound;
use rspigot_protocol::login::{
    CustomQuery, CustomQueryAnswer, LoginCompression, LoginDisconnect, LoginFinished,
    ServerboundHello,
};
use rspigot_protocol::split_packet_id;
use tokio::net::TcpStream;

use crate::ProxyError;
use crate::forwarding::{self, ForwardedPlayer};
use crate::wire::{self, Reader, Writer};

pub struct BackendLink {
    pub reader: Reader,
    pub writer: Writer,
    pub finished: LoginFinished,
}

pub async fn login(
    address: &str,
    player: &ForwardedPlayer,
    secret: &[u8],
    compression_level: u32,
) -> Result<BackendLink, ProxyError> {
    let stream = TcpStream::connect(address).await?;
    let (mut reader, mut writer) = wire::split(stream);
    let (host, port) = host_and_port(address);
    writer.queue_packet(&Intention {
        protocol_version: rspigot_data::PROTOCOL_VERSION,
        host,
        port,
        intent: ClientIntent::Login,
    })?;
    writer.queue_packet(&ServerboundHello {
        name: player.name.clone(),
        profile_id: player.id,
    })?;
    writer.flush().await?;
    loop {
        let packet = reader.next().await?;
        let (id, body) = split_packet_id(&packet)?;
        match id {
            clientbound::CUSTOM_QUERY => {
                let query: CustomQuery = decode_exact(body)?;
                let payload = (query.channel.as_str() == forwarding::CHANNEL).then(|| {
                    let version = forwarding::negotiate(query.data.first().copied());
                    forwarding::signed_payload(secret, version, player)
                });
                writer
                    .send(&CustomQueryAnswer {
                        transaction_id: query.transaction_id,
                        payload,
                    })
                    .await?;
            }
            clientbound::LOGIN_COMPRESSION => {
                let LoginCompression { threshold } = decode_exact(body)?;
                let threshold = usize::try_from(threshold).ok();
                reader.set_compression(threshold);
                writer.set_compression(threshold.map(|threshold| Compression {
                    threshold,
                    level: compression_level,
                }));
            }
            clientbound::LOGIN_FINISHED => {
                return Ok(BackendLink {
                    reader,
                    writer,
                    finished: decode_exact(body)?,
                });
            }
            clientbound::LOGIN_DISCONNECT => {
                let LoginDisconnect { reason } = decode_exact(body)?;
                return Err(ProxyError::BackendRefused(reason));
            }
            clientbound::HELLO => {
                return Err(ProxyError::Rejected(
                    "the backend asks for encryption; run it in offline mode behind the proxy"
                        .to_owned(),
                ));
            }
            other => {
                return Err(ProxyError::Rejected(format!(
                    "unexpected backend login packet {other}"
                )));
            }
        }
    }
}

fn host_and_port(address: &str) -> (String, u16) {
    match address.rsplit_once(':') {
        Some((host, port)) => (
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned(),
            port.parse().unwrap_or(25565),
        ),
        None => (address.to_owned(), 25565),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_names_the_backend_host_and_port() {
        assert_eq!(host_and_port("lobby:25566"), ("lobby".to_owned(), 25566));
        assert_eq!(host_and_port("[::1]:25565"), ("::1".to_owned(), 25565));
        assert_eq!(host_and_port("lobby"), ("lobby".to_owned(), 25565));
    }
}
