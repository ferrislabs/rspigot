use std::fmt;

use rspigot_plugin_protocol::v1;
use tonic::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActionErrorKind {
    InvalidArgument,
    TargetGone,
    IncompatibleState,
    Cancelled,
    NotFound,
    LimitReached,
}

impl ActionErrorKind {
    const fn label(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid argument",
            Self::TargetGone => "target gone",
            Self::IncompatibleState => "incompatible state",
            Self::Cancelled => "cancelled",
            Self::NotFound => "not found",
            Self::LimitReached => "limit reached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionError {
    pub kind: ActionErrorKind,
    pub reason: String,
}

impl ActionError {
    pub(crate) fn from_wire(error: v1::ActionError) -> Self {
        let kind = match v1::ActionErrorKind::try_from(error.kind) {
            Ok(v1::ActionErrorKind::TargetGone) => ActionErrorKind::TargetGone,
            Ok(v1::ActionErrorKind::IncompatibleState) => ActionErrorKind::IncompatibleState,
            Ok(v1::ActionErrorKind::Cancelled) => ActionErrorKind::Cancelled,
            Ok(v1::ActionErrorKind::NotFound) => ActionErrorKind::NotFound,
            Ok(v1::ActionErrorKind::LimitReached) => ActionErrorKind::LimitReached,
            Ok(v1::ActionErrorKind::InvalidArgument | v1::ActionErrorKind::Unspecified)
            | Err(_) => ActionErrorKind::InvalidArgument,
        };
        Self {
            kind,
            reason: error.reason,
        }
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.label(), self.reason)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("`{0}` is not a valid rspigot URL, expected http://host:port")]
    Url(String),
    #[error("rspigot refused the plugin: {}", .0.message())]
    Refused(Status),
    #[error("replaced by a newer connection under the same plugin name")]
    Replaced,
    #[error("not connected to rspigot")]
    Disconnected,
    #[error("rspigot did not answer in time")]
    Timeout,
    #[error("rspigot does not know it")]
    NotFound,
    #[error("rspigot rejected the request: {0}")]
    Rejected(String),
    #[error("rspigot refused the action, {0}")]
    Action(ActionError),
    #[error("rspigot sent {0}")]
    Malformed(String),
}
