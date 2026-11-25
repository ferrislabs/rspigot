use rspigot_protocol::frame::{Compression, FrameDecoder, FrameEncoder};
use rspigot_protocol::{Packet, write_packet};
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

    pub fn set_compression(&mut self, compression: Option<Compression>) {
        self.encoder.set_compression(compression);
    }

    pub fn enable_encryption(&mut self, secret: &[u8; 16]) {
        self.encoder.enable_encryption(secret);
    }
}

pub async fn pump(from: &mut Reader, to: &mut Writer) -> Result<(), ProxyError> {
    loop {
        let packet = from.next().await?;
        to.queue(&packet)?;
        while let Some(packet) = from.ready()? {
            to.queue(&packet)?;
        }
        to.flush().await?;
    }
}
