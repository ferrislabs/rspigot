use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::ResourcePackStatus;
use rspigot_core::ports::{ClientLink, SessionVerifier};
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_protocol::common::KeepAlive;
use rspigot_protocol::ids;
use rspigot_protocol::play::ServerboundPlay;
use rspigot_protocol::split_packet_id;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::connection::{
    Configured, ConnectionError, PacketReader, PacketWriter, Raw, Session, reconfigure,
};
use crate::decode::{Decoded, decode};
use crate::encode::Encoder;
use crate::link::{NetLink, Outgoing};
use crate::listener::Shared;

const MAX_BATCH_BYTES: usize = 256 * 1024;

struct KeepAliveState {
    pending: AtomicI64,
    answered_at: parking_lot::Mutex<Instant>,
}

pub async fn run<V: SessionVerifier>(
    shared: Arc<Shared<V>>,
    session: Session,
    profile: GameProfile,
    configured: Configured,
) -> Result<(), ConnectionError> {
    let Configured {
        settings,
        packs,
        inputs,
    } = configured;
    let (sender, receiver) = unbounded_channel();
    let connected = Arc::new(AtomicBool::new(true));
    let link = NetLink::new(sender, Arc::clone(&connected));
    let reader_link = link.clone();
    let suspension = link.suspension();
    let player = profile.id;
    let name = profile.name.clone();
    let id = shared.next_session();
    if shared
        .game
        .send(Inbound::Join {
            profile,
            settings,
            link,
            address: session.addr.ip(),
            session: id,
        })
        .is_err()
    {
        return Err(ConnectionError::Rejected("game loop stopped".to_owned()));
    }
    for (pack, status) in packs {
        let input = PlayerInput::ResourcePack { id: pack, status };
        let _ = shared.game.send(Inbound::Input { player, input });
    }
    for input in inputs {
        let _ = shared.game.send(Inbound::Input { player, input });
    }
    let Session {
        reader,
        writer,
        addr,
    } = session;
    let keep_alive = Arc::new(KeepAliveState {
        pending: AtomicI64::new(0),
        answered_at: parking_lot::Mutex::new(Instant::now()),
    });
    let read_task = tokio::spawn(read_loop(
        Arc::clone(&shared),
        reader,
        player,
        Arc::clone(&keep_alive),
        reader_link,
    ));
    let result = write_loop(&shared, writer, receiver, &keep_alive, &suspension).await;
    connected.store(false, Ordering::Release);
    read_task.abort();
    let _ = shared.game.send(Inbound::Leave {
        player,
        session: id,
    });
    match &result {
        Ok(reason) => tracing::info!("{name} ({addr}) lost connection: {reason}"),
        Err(error) => tracing::info!("{name} ({addr}) lost connection: {error}"),
    }
    result.map(|_| ())
}

async fn read_loop<V: SessionVerifier>(
    shared: Arc<Shared<V>>,
    mut reader: PacketReader,
    player: PlayerId,
    keep_alive: Arc<KeepAliveState>,
    link: NetLink,
) {
    let mut limiter = crate::rate_limit::RateLimiter::new(shared.config.rate_limit, Instant::now());
    loop {
        let packet = match reader.next().await {
            Ok(packet) => packet,
            Err(error) => {
                tracing::debug!(%error, "read loop ended");
                return;
            }
        };
        if limiter.record(Instant::now()) {
            link.disconnect(Component::translatable(
                crate::rate_limit::DISCONNECT_KEY,
                Vec::new(),
            ));
            return;
        }
        let Ok((id, body)) = split_packet_id(&packet) else {
            return;
        };
        let parsed = match ServerboundPlay::decode(id, body) {
            Ok(ServerboundPlay::ConfigurationAcknowledged(_)) => {
                if !link.is_suspended() {
                    continue;
                }
                match reconfigure(&shared, &mut reader, &link).await {
                    Ok(configured) => {
                        *keep_alive.answered_at.lock() = Instant::now();
                        keep_alive.pending.store(0, Ordering::Release);
                        link.resume();
                        let back = Inbound::Input {
                            player,
                            input: PlayerInput::Reconfigured,
                        };
                        if shared.game.send(back).is_err() {
                            return;
                        }
                        for (pack, status) in configured.packs {
                            report_pack(&shared, player, pack, status);
                        }
                        for input in configured.inputs {
                            let _ = shared.game.send(Inbound::Input { player, input });
                        }
                        continue;
                    }
                    Err(error) => {
                        tracing::debug!(%error, "reconfiguration ended");
                        return;
                    }
                }
            }
            Ok(parsed) => parsed,
            Err(error) => {
                let name = ids::packet_name(ids::State::Play, ids::Direction::Serverbound, id)
                    .unwrap_or("unknown");
                tracing::debug!(%error, packet = name, "dropping undecodable packet");
                continue;
            }
        };
        match decode(parsed) {
            Decoded::Input(input) => {
                if shared.game.send(Inbound::Input { player, input }).is_err() {
                    return;
                }
            }
            Decoded::KeepAlive(id) => {
                if keep_alive
                    .pending
                    .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    *keep_alive.answered_at.lock() = Instant::now();
                    let input = PlayerInput::Latency((millis_since_epoch() - id) as i32);
                    if shared.game.send(Inbound::Input { player, input }).is_err() {
                        return;
                    }
                }
            }
            Decoded::Ping(payload) => link.pong(payload),
            Decoded::Ignored => {}
        }
    }
}

async fn write_loop<V: SessionVerifier>(
    shared: &Shared<V>,
    mut writer: PacketWriter,
    mut receiver: UnboundedReceiver<Outgoing>,
    keep_alive: &KeepAliveState,
    suspension: &AtomicBool,
) -> Result<String, ConnectionError> {
    let mut encoder = Encoder::new(shared.config.online_mode);
    let mut ticker = tokio::time::interval(shared.config.keep_alive_interval);
    ticker.tick().await;
    let timeout = shared.config.keep_alive_timeout;
    loop {
        tokio::select! {
            message = receiver.recv() => {
                let Some(message) = message else {
                    return Ok("Disconnected".to_owned());
                };
                match handle(&mut encoder, shared, &mut writer, message)? {
                    Step::Continue => {}
                    Step::Disconnect(reason) => {
                        return finish(&mut writer, &reason, suspension).await;
                    }
                    Step::Close => return close(&mut writer).await,
                }
                while writer.pending.len() < MAX_BATCH_BYTES {
                    let Ok(message) = receiver.try_recv() else {
                        break;
                    };
                    match handle(&mut encoder, shared, &mut writer, message)? {
                        Step::Continue => {}
                        Step::Disconnect(reason) => {
                            return finish(&mut writer, &reason, suspension).await;
                        }
                        Step::Close => return close(&mut writer).await,
                    }
                }
                writer.flush().await?;
            }
            _ = ticker.tick() => {
                if suspension.load(Ordering::Acquire) {
                    continue;
                }
                if keep_alive.pending.load(Ordering::Acquire) != 0 && keep_alive.answered_at.lock().elapsed() > timeout {
                    return finish(&mut writer, &Component::translatable("disconnect.timeout", Vec::new()), suspension).await;
                }
                if keep_alive.pending.load(Ordering::Acquire) == 0 {
                    let id = millis_since_epoch().max(1);
                    keep_alive.pending.store(id, Ordering::Release);
                    writer.send(ids::play::clientbound::KEEP_ALIVE, &KeepAlive { id }).await?;
                }
            }
        }
    }
}

fn millis_since_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as i64)
}

fn handle<V: SessionVerifier>(
    encoder: &mut Encoder,
    shared: &Shared<V>,
    writer: &mut PacketWriter,
    message: Outgoing,
) -> Result<Step, ConnectionError> {
    match message {
        Outgoing::Disconnect(reason) => Ok(Step::Disconnect(reason)),
        Outgoing::Packet { id, body } => {
            writer.queue(id, &Raw(&body))?;
            Ok(Step::Continue)
        }
        Outgoing::Close => Ok(Step::Close),
        Outgoing::Event(event) => {
            if let ClientEvent::Disconnect(reason) = *event {
                return Ok(Step::Disconnect(reason));
            }
            encoder.encode(
                &event,
                writer,
                &shared.chunks,
                shared.config.compression_threshold,
                shared.config.compression_level,
            )?;
            Ok(Step::Continue)
        }
        Outgoing::Pong(payload) => {
            writer.queue(
                ids::play::clientbound::PONG_RESPONSE,
                &rspigot_protocol::status::PongResponse { time: payload },
            )?;
            Ok(Step::Continue)
        }
    }
}

enum Step {
    Continue,
    Disconnect(Component),
    Close,
}

fn report_pack<V: SessionVerifier>(
    shared: &Shared<V>,
    player: PlayerId,
    pack: uuid::Uuid,
    status: ResourcePackStatus,
) {
    let input = PlayerInput::ResourcePack { id: pack, status };
    let _ = shared.game.send(Inbound::Input { player, input });
}

async fn close(writer: &mut PacketWriter) -> Result<String, ConnectionError> {
    writer.flush().await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    writer.shutdown().await;
    Ok("Disconnected".to_owned())
}

async fn finish(
    writer: &mut PacketWriter,
    reason: &Component,
    suspension: &AtomicBool,
) -> Result<String, ConnectionError> {
    let packet = rspigot_protocol::common::Disconnect {
        reason: rspigot_protocol::types::Text(crate::text::to_nbt(reason)),
    };
    let id = if suspension.load(Ordering::Acquire) {
        ids::configuration::clientbound::DISCONNECT
    } else {
        ids::play::clientbound::DISCONNECT
    };
    writer.queue(id, &packet)?;
    writer.flush().await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    writer.shutdown().await;
    Ok(rspigot_core::console::console_text(reason))
}
