use thiserror::Error;

use super::PlayerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RoundError {
    #[error("the round has already started")]
    AlreadyStarted,
    #[error("the round is full")]
    Full,
    #[error("player {0:?} is already in the round")]
    AlreadyJoined(PlayerId),
}
