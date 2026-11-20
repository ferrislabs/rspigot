use rspigot_core::ports::StoreError;
use rspigot_domain::ChunkPos;

#[derive(Debug, thiserror::Error)]
pub enum AnvilError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("nbt error: {0}")]
    Nbt(#[from] rspigot_nbt::NbtError),
    #[error("corrupt data: {0}")]
    Corrupt(String),
    #[error("unsupported compression id {0}")]
    UnsupportedCompression(u8),
    #[error("chunk data version {found} is newer than the supported {supported}")]
    NewerDataVersion { found: i32, supported: i32 },
    #[error(
        "chunk is still in the pre-1.18 layout (Level/Sections): upgrade the map with a server's --forceUpgrade first"
    )]
    LegacyLayout,
        "world data version {found} is older than the oldest supported version {oldest}; open it with a vanilla server first"
    )]
    TooOldDataVersion { found: i32, oldest: i32 },
    #[error("invalid world path: {0}")]
    InvalidPath(String),
}

impl AnvilError {
    pub fn corrupt(reason: impl Into<String>) -> Self {
        Self::Corrupt(reason.into())
    }

    pub fn into_store(self, pos: ChunkPos) -> StoreError {
        match self {
            Self::Io(error) => StoreError::Io(error),
            other => StoreError::Corrupt {
                pos,
                reason: other.to_string(),
            },
        }
    }

    pub fn into_store_io(self) -> StoreError {
        match self {
            Self::Io(error) => StoreError::Io(error),
            other => StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                other.to_string(),
            )),
        }
    }
}
