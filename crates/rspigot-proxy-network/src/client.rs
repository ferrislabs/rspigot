use std::net::SocketAddr;
use std::sync::Arc;

use rspigot_core::ports::SessionVerifier;
use rspigot_domain::player::PlayerId;
use rspigot_protocol::frame::Compression;
use rspigot_protocol::handshake::{ClientIntent, ServerboundHandshake};
use rspigot_protocol::login::LoginAcknowledged;
use rspigot_protocol::login::{
    ClientboundHello, LoginCompression, LoginDisconnect, ServerboundLogin,
};
use rspigot_protocol::split_packet_id;
use rspigot_protocol::status::{PongResponse, ServerboundStatus, StatusResponse};
use rspigot_protocol::types::{JsonText, ProfileProperty};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::backend;
use crate::forwarding::ForwardedPlayer;
use crate::session::{Link, Session};
use crate::status;
use crate::wire::{self, Reader, Writer};
use crate::{ProxyError, Shared};

pub async fn handle<V: SessionVerifier>(
    stream: TcpStream,
    peer: SocketAddr,
    shared: Arc<Shared<V>>,
) -> Result<(), ProxyError> {
    let (mut reader, mut writer) = wire::split(stream);
    let login_deadline = shared.settings.login_timeout;
    let intention = timeout(login_deadline, async {
        let packet = reader.next().await?;
        let (id, body) = split_packet_id(&packet)?;
        let ServerboundHandshake::Intention(intention) = ServerboundHandshake::decode(id, body)?;
        Ok::<_, ProxyError>(intention)
    })
    .await
    .map_err(|_| ProxyError::Timeout)??;
    match intention.intent {
        ClientIntent::Status => timeout(
            login_deadline,
            answer_status(&mut reader, &mut writer, &shared),
        )
        .await
        .map_err(|_| ProxyError::Timeout)?,
        ClientIntent::Login | ClientIntent::Transfer => {
            if intention.protocol_version != rspigot_data::PROTOCOL_VERSION {
                return disconnect(&mut writer, &status::incompatible()).await;
            }
            let player = timeout(
                login_deadline,
                authenticate(&mut reader, &mut writer, &shared, peer),
            )
            .await
            .map_err(|_| ProxyError::Timeout)??;
            let Some(player) = player else {
                return Ok(());
            };
            join(reader, writer, &shared, player).await
        }
    }
}

async fn answer_status<V>(
    reader: &mut Reader,
    writer: &mut Writer,
    shared: &Shared<V>,
) -> Result<(), ProxyError> {
    loop {
        let packet = reader.next().await?;
        let (id, body) = split_packet_id(&packet)?;
        match ServerboundStatus::decode(id, body)? {
            ServerboundStatus::StatusRequest(_) => {
                let online = u32::try_from(shared.state().roster.online()).unwrap_or(u32::MAX);
                writer
                    .send(&StatusResponse {
                        json: status::response_json(&shared.settings, online),
                    })
                    .await?;
            }
            ServerboundStatus::PingRequest(ping) => {
                writer.send(&PongResponse { time: ping.time }).await?;
                return Ok(());
            }
        }
    }
}

async fn next_login(reader: &mut Reader) -> Result<ServerboundLogin, ProxyError> {
    let packet = reader.next().await?;
    let (id, body) = split_packet_id(&packet)?;
    Ok(ServerboundLogin::decode(id, body)?)
}

async fn authenticate<V: SessionVerifier>(
    reader: &mut Reader,
    writer: &mut Writer,
    shared: &Arc<Shared<V>>,
    peer: SocketAddr,
) -> Result<Option<ForwardedPlayer>, ProxyError> {
    let ServerboundLogin::Hello(hello) = next_login(reader).await? else {
        return Err(ProxyError::Rejected("expected hello".to_owned()));
    };
    if !valid_name(&hello.name) {
        disconnect(
            writer,
            &status::translatable("multiplayer.disconnect.invalid_player_data"),
        )
        .await?;
        return Ok(None);
    }
    if !shared.settings.online_mode {
        return Ok(Some(ForwardedPlayer {
            address: peer.ip().to_canonical(),
            id: PlayerId::offline(&hello.name).0,
            name: hello.name,
            properties: Vec::new(),
        }));
    }
    let challenge: [u8; 4] = rand::random();
    writer
        .send(&ClientboundHello {
            server_id: String::new(),
            public_key: shared.key.public_der().to_vec(),
            challenge: challenge.to_vec(),
            should_authenticate: true,
        })
        .await?;
    let ServerboundLogin::Key(key) = next_login(reader).await? else {
        return Err(ProxyError::Rejected("expected key".to_owned()));
    };
    if shared.key.decrypt(&key.encrypted_challenge)? != challenge {
        return Err(ProxyError::Rejected("challenge mismatch".to_owned()));
    }
    let secret: [u8; 16] = shared
        .key
        .decrypt(&key.encrypted_secret)?
        .try_into()
        .map_err(|_| ProxyError::Rejected("bad shared secret".to_owned()))?;
    reader.enable_encryption(&secret);
    writer.enable_encryption(&secret);
    let hash = rspigot_protocol::crypto::server_hash("", &secret, shared.key.public_der());
    let verifier = Arc::clone(shared);
    let name = hello.name.clone();
    let verified =
        tokio::task::spawn_blocking(move || verifier.verifier.verify(&name, &hash, None))
            .await
            .map_err(|error| ProxyError::Rejected(error.to_string()))?;
    match verified {
        Ok(profile) => Ok(Some(ForwardedPlayer {
            address: peer.ip().to_canonical(),
            id: profile.id.0,
            name: profile.name,
            properties: profile
                .properties
                .into_iter()
                .map(|property| ProfileProperty {
                    name: property.name,
                    value: property.value,
                    signature: property.signature,
                })
                .collect(),
        })),
        Err(error) => {
            tracing::info!("Disconnecting {} ({peer}): {error}", hello.name);
            disconnect(
                writer,
                &status::translatable("multiplayer.disconnect.unverified_username"),
            )
            .await?;
            Ok(None)
        }
    }
}

async fn join<V>(
    mut client_reader: Reader,
    mut client_writer: Writer,
    shared: &Shared<V>,
    player: ForwardedPlayer,
) -> Result<(), ProxyError> {
    let login = shared.state().roster.login(player.id, &player.name);
    if let Err(error) = login {
        tracing::info!("Refusing {}: {error}", player.name);
        return disconnect(
            &mut client_writer,
            &status::text("You are already connected to this proxy!"),
        )
        .await;
    }
    let result = play(&mut client_reader, &mut client_writer, shared, &player).await;
    shared.state().roster.logout(player.id);
    tracing::info!("{} left the proxy", player.name);
    match result {
        Err(ProxyError::Closed) => Ok(()),
        other => other,
    }
}

async fn play<V>(
    client_reader: &mut Reader,
    client_writer: &mut Writer,
    shared: &Shared<V>,
    player: &ForwardedPlayer,
) -> Result<(), ProxyError> {
    let settings = &shared.settings;
    let candidates = shared.state().registry.join_candidates();
    let mut reached = None;
    for server in candidates {
        let attempt = timeout(
            settings.connect_timeout,
            backend::login(
                &server.address,
                player,
                &settings.forwarding_secret,
                settings.compression_level,
            ),
        )
        .await
        .unwrap_or(Err(ProxyError::Timeout));
        match attempt {
            Ok(link) => {
                reached = Some((server, link));
                break;
            }
            Err(ProxyError::BackendRefused(reason)) => {
                tracing::info!(
                    "{} was refused by {}: {}",
                    player.name,
                    server.name,
                    reason.0
                );
                return disconnect(client_writer, &reason).await;
            }
            Err(error) => {
                tracing::warn!("{} could not reach {}: {error}", player.name, server.name);
            }
        }
    }
    let Some((server, link)) = reached else {
        return disconnect(
            client_writer,
            &status::text("Could not connect to a default or fallback server."),
        )
        .await;
    };
    let backend::BackendLink {
        reader: backend_reader,
        writer: mut backend_writer,
        finished,
    } = link;
    if let Some(threshold) = settings.compression_threshold {
        client_writer
            .send(&LoginCompression {
                threshold: i32::try_from(threshold).unwrap_or(i32::MAX),
            })
            .await?;
        client_writer.set_compression(Some(Compression {
            threshold,
            level: settings.compression_level,
        }));
        client_reader.set_compression(Some(threshold));
    }
    client_writer.send(&finished).await?;
    loop {
        match next_login(client_reader).await? {
            ServerboundLogin::LoginAcknowledged(_) => break,
            ServerboundLogin::CustomQueryAnswer(_) | ServerboundLogin::CookieResponse(_) => {}
            _ => return Err(ProxyError::Rejected("unexpected login packet".to_owned())),
        }
    }
    backend_writer.send(&LoginAcknowledged).await?;
    shared.state().roster.connected(player.id, &server.name);
    tracing::info!("{} ({}) joined {}", player.name, player.id, server.name);
    let link = Link {
        server,
        reader: backend_reader,
        writer: backend_writer,
    };
    Session::new(shared, player, client_reader, client_writer, link)
        .run()
        .await
}

async fn disconnect(writer: &mut Writer, reason: &JsonText) -> Result<(), ProxyError> {
    writer
        .send(&LoginDisconnect {
            reason: reason.clone(),
        })
        .await
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_names_follow_the_vanilla_rules() {
        assert!(valid_name("Steve"));
        assert!(valid_name("a_b_9"));
        assert!(!valid_name(""));
        assert!(!valid_name("seventeen_chars__"));
        assert!(!valid_name("bad name"));
        assert!(!valid_name("é"));
    }
}
