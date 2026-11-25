use rspigot_protocol::frame::{Compression, FrameDecoder, FrameEncoder};
use std::sync::atomic::{AtomicU8, Ordering};

use rspigot_protocol::common::Disconnect;
use rspigot_protocol::types::Text;
use rspigot_protocol::{Packet, VarInt, write_packet, write_packet_with_id};
use rspigot_proxy_core::Phase;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::ProxyError;

const READ_CHUNK: usize = 16 * 1024;

pub struct Reader {
    half: OwnedReadHalf,
    decoder: FrameDecoder,
    chunk: Box<[u8]>,
}

pub struct Writer {
    half: OwnedWriteHalf,
    encoder: FrameEncoder,
    out: Vec<u8>,
    scratch: Vec<u8>,
}

pub fn split(stream: TcpStream) -> (Reader, Writer) {
    let _ = stream.set_nodelay(true);
    let (read, write) = stream.into_split();
    (
        Reader {
            half: read,
            decoder: FrameDecoder::new(),
            chunk: vec![0; READ_CHUNK].into_boxed_slice(),
        },
        Writer {
            half: write,
            encoder: FrameEncoder::new(),
            out: Vec::with_capacity(READ_CHUNK),
            scratch: Vec::new(),
        },
    )
}

impl Reader {
    pub async fn next(&mut self) -> Result<Vec<u8>, ProxyError> {
        loop {
            if let Some(packet) = self.decoder.next_packet()? {
                return Ok(packet);
            }
            let read = self.half.read(&mut self.chunk).await?;
            if read == 0 {
                return Err(ProxyError::Closed);
            }
            self.decoder.feed(&self.chunk[..read]);
        }
    }

    pub fn ready(&mut self) -> Result<Option<Vec<u8>>, ProxyError> {
        Ok(self.decoder.next_packet()?)
    }

    pub fn set_compression(&mut self, threshold: Option<usize>) {
        self.decoder.set_compression(threshold);
    }

    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.decoder.enable_encryption(secret);
    }
}

impl Writer {
    pub fn queue(&mut self, packet: &[u8]) -> Result<(), ProxyError> {
        self.encoder.encode_packet(packet, &mut self.out)?;
        Ok(())
    }

    pub fn queue_packet<P: Packet>(&mut self, packet: &P) -> Result<(), ProxyError> {
        self.scratch.clear();
        write_packet(packet, &mut self.scratch);
        self.encoder.encode_packet(&self.scratch, &mut self.out)?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), ProxyError> {
        if !self.out.is_empty() {
            self.half.write_all(&self.out).await?;
            self.out.clear();
        }
        Ok(())
    }

    pub async fn send<P: Packet>(&mut self, packet: &P) -> Result<(), ProxyError> {
        self.queue_packet(packet)?;
        self.flush().await
    }

    pub async fn send_disconnect(&mut self, phase: Phase, reason: &Text) -> Result<(), ProxyError> {
        self.scratch.clear();
        write_packet_with_id(
            phase.disconnect_id(),
            &Disconnect {
                reason: reason.clone(),
            },
            &mut self.scratch,
        );
        self.encoder.encode_packet(&self.scratch, &mut self.out)?;
        self.flush().await
    }

    pub fn set_compression(&mut self, compression: Option<Compression>) {
        self.encoder.set_compression(compression);
    }

    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.encoder.enable_encryption(secret);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Client,
    Kicked,
}

fn packet_id(packet: &[u8]) -> Option<i32> {
    VarInt::decode_partial(packet)
        .ok()
        .flatten()
        .map(|(id, _)| id)
}

fn backend_lost(error: ProxyError) -> ProxyError {
    match error {
        ProxyError::Closed | ProxyError::Io(_) | ProxyError::Frame(_) => ProxyError::BackendLost,
        other => other,
    }
}

pub async fn pump_serverbound(
    client: &mut Reader,
    backend: &mut Writer,
    phase: &AtomicU8,
) -> Result<Ended, ProxyError> {
    let observe = |packet: &[u8]| {
        if let Some(id) = packet_id(packet) {
            let current = Phase::from_u8(phase.load(Ordering::Relaxed));
            phase.store(current.after_serverbound(id).as_u8(), Ordering::Relaxed);
        }
    };
    loop {
        let packet = match client.next().await {
            Ok(packet) => packet,
            Err(ProxyError::Closed | ProxyError::Io(_)) => return Ok(Ended::Client),
            Err(error) => return Err(error),
        };
        observe(&packet);
        backend.queue(&packet)?;
        while let Some(packet) = client.ready()? {
            observe(&packet);
            backend.queue(&packet)?;
        }
        backend.flush().await.map_err(backend_lost)?;
    }
}

pub async fn pump_clientbound(
    backend: &mut Reader,
    client: &mut Writer,
    phase: &AtomicU8,
) -> Result<Ended, ProxyError> {
    loop {
        let packet = backend.next().await.map_err(backend_lost)?;
        let disconnect = Phase::from_u8(phase.load(Ordering::Relaxed)).disconnect_id();
        let mut kicked = packet_id(&packet) == Some(disconnect);
        client.queue(&packet)?;
        while !kicked && let Some(packet) = backend.ready().map_err(backend_lost)? {
            kicked = packet_id(&packet) == Some(disconnect);
            client.queue(&packet)?;
        }
        client.flush().await?;
        if kicked {
            return Ok(Ended::Kicked);
        }
    }
}
