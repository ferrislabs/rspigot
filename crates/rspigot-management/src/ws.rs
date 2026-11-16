use std::collections::HashMap;

use base64::Engine;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_HEAD: usize = 16 * 1024;
pub const MAX_MESSAGE: usize = 65536;

#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed http request")]
    BadRequest,
    #[error("frame too large")]
    TooLarge,
    #[error("protocol violation")]
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub path: String,
    pub headers: HashMap<String, String>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn is_upgrade(&self) -> bool {
        self.header("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
            && self.header("sec-websocket-key").is_some()
    }
}

pub fn parse_head(raw: &str) -> Result<Head, WsError> {
    let mut lines = raw.split("\r\n");
    let request = lines.next().ok_or(WsError::BadRequest)?;
    let mut parts = request.split(' ');
    if parts.next() != Some("GET") {
        return Err(WsError::BadRequest);
    }
    let path = parts.next().ok_or(WsError::BadRequest)?.to_owned();
    let mut headers = HashMap::new();
    for line in lines.take_while(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(WsError::BadRequest)?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    Ok(Head { path, headers })
}

pub async fn read_head<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Head, WsError> {
    let mut buffer = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while !buffer.ends_with(b"\r\n\r\n") {
        if buffer.len() >= MAX_HEAD {
            return Err(WsError::TooLarge);
        }
        if reader.read(&mut byte).await? == 0 {
            return Err(WsError::BadRequest);
        }
        buffer.push(byte[0]);
    }
    parse_head(&String::from_utf8(buffer).map_err(|_| WsError::BadRequest)?)
}

pub fn accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

pub fn upgrade_response(key: &str, protocol: Option<&str>) -> String {
    let mut out = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n",
        accept_key(key)
    );
    if let Some(protocol) = protocol {
        out.push_str(&format!("Sec-WebSocket-Protocol: {protocol}\r\n"));
    }
    out.push_str("\r\n");
    out
}

pub fn rejection(status: u16, reason: &str, message: &str) -> String {
    let body = serde_json::json!({"error": reason, "message": message}).to_string();
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Ping(Vec<u8>),
    Pong,
    Close,
}

pub fn encode(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126 => out.push(n as u8),
        n if n <= usize::from(u16::MAX) => {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(payload);
    out
}

pub fn text_frame(text: &str) -> Vec<u8> {
    encode(0x1, text.as_bytes())
}

pub fn pong_frame(payload: &[u8]) -> Vec<u8> {
    encode(0xA, payload)
}

pub fn close_frame() -> Vec<u8> {
    encode(0x8, &[])
}

async fn read_one<R: AsyncRead + Unpin>(reader: &mut R) -> Result<(bool, u8, Vec<u8>), WsError> {
    let mut head = [0u8; 2];
    reader.read_exact(&mut head).await?;
    let fin = head[0] & 0x80 != 0;
    let opcode = head[0] & 0x0F;
    if head[1] & 0x80 == 0 {
        return Err(WsError::Protocol);
    }
    let length = match head[1] & 0x7F {
        126 => {
            let mut raw = [0u8; 2];
            reader.read_exact(&mut raw).await?;
            usize::from(u16::from_be_bytes(raw))
        }
        127 => {
            let mut raw = [0u8; 8];
            reader.read_exact(&mut raw).await?;
            usize::try_from(u64::from_be_bytes(raw)).map_err(|_| WsError::TooLarge)?
        }
        n => usize::from(n),
    };
    if length > MAX_MESSAGE {
        return Err(WsError::TooLarge);
    }
    let mut mask = [0u8; 4];
    reader.read_exact(&mut mask).await?;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).await?;
    for (i, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[i % 4];
    }
    Ok((fin, opcode, payload))
}

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Frame, WsError> {
    let mut message: Option<Vec<u8>> = None;
    loop {
        let (fin, opcode, payload) = read_one(reader).await?;
        match opcode {
            0x8 => return Ok(Frame::Close),
            0x9 => return Ok(Frame::Ping(payload)),
            0xA => return Ok(Frame::Pong),
            0x1 | 0x0 => {
                if opcode == 0x1 && message.is_some() {
                    return Err(WsError::Protocol);
                }
                if opcode == 0x0 && message.is_none() {
                    return Err(WsError::Protocol);
                }
                let buffer = message.get_or_insert_with(Vec::new);
                buffer.extend_from_slice(&payload);
                if buffer.len() > MAX_MESSAGE {
                    return Err(WsError::TooLarge);
                }
                if fin {
                    let text = String::from_utf8(message.take().unwrap_or_default())
                        .map_err(|_| WsError::Protocol)?;
                    return Ok(Frame::Text(text));
                }
            }
            _ => return Err(WsError::Protocol),
        }
    }
}

pub async fn write_all<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<(), WsError> {
    writer.write_all(bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn masked(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        let mask = [1u8, 2, 3, 4];
        let mut out = vec![if fin { 0x80 } else { 0 } | opcode];
        out.push(0x80 | payload.len() as u8);
        out.extend_from_slice(&mask);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        out
    }

    #[test]
    fn the_accept_key_matches_the_rfc_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn heads_parse_with_lowercase_headers() {
        let head = parse_head(
            "GET / HTTP/1.1\r\nHost: x\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: abc\r\n\r\n",
        )
        .unwrap();
        assert!(head.is_upgrade());
        assert_eq!(head.header("host"), Some("x"));
        assert!(parse_head("POST / HTTP/1.1\r\n\r\n").is_err());
    }

    #[tokio::test]
    async fn a_masked_text_frame_is_unmasked() {
        let bytes = masked(0x1, true, b"hello");
        let mut reader = &bytes[..];
        assert_eq!(
            read_frame(&mut reader).await.unwrap(),
            Frame::Text("hello".into())
        );
    }

    #[tokio::test]
    async fn fragments_are_joined() {
        let mut bytes = masked(0x1, false, b"hel");
        bytes.extend(masked(0x0, true, b"lo"));
        let mut reader = &bytes[..];
        assert_eq!(
            read_frame(&mut reader).await.unwrap(),
            Frame::Text("hello".into())
        );
    }

    #[tokio::test]
    async fn unmasked_client_frames_are_refused() {
        let bytes = encode(0x1, b"hi");
        let mut reader = &bytes[..];
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(WsError::Protocol)
        ));
    }

    #[tokio::test]
    async fn server_frames_use_extended_lengths() {
        let payload = vec![b'a'; 300];
        let frame = text_frame(std::str::from_utf8(&payload).unwrap());
        assert_eq!(&frame[..4], &[0x81, 126, 1, 44]);
    }

    #[tokio::test]
    async fn control_frames_are_reported() {
        let bytes = masked(0x9, true, b"p");
        let mut reader = &bytes[..];
        assert_eq!(
            read_frame(&mut reader).await.unwrap(),
            Frame::Ping(b"p".to_vec())
        );
    }
}
