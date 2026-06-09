//! Unified error types for the Kryx ecosystem.

use std::fmt;
use thiserror::Error;

pub type Result<T, E = MediaError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MediaError {
    #[error("buffer error: {0}")]
    Buffer(#[from] BufferError),

    #[error("pipeline error: {0}")]
    Pipeline(#[from] PipelineError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ffi error: {context} — {message}")]
    Ffi { context: &'static str, message: String },

    #[error("sync error: {0}")]
    Sync(#[from] SyncError),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("invalid timestamp: {pts}")]
    InvalidTimestamp { pts: i64 },

    #[error("resource closed")]
    Closed,

    #[error("timeout after {ms}ms")]
    Timeout { ms: u64 },

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BufferError {
    #[error("capacity {requested} exceeds maximum {maximum}")]
    CapacityExceeded { requested: usize, maximum: usize },

    #[error("buffer is empty")]
    Empty,

    #[error("slice [{start}..{end}] out of bounds for len {len}")]
    OutOfBounds { start: usize, end: usize, len: usize },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PipelineError {
    #[error("pipeline has no stages")]
    Empty,

    #[error("channel closed at stage {stage}")]
    ChannelClosed { stage: String },

    #[error("type mismatch at stage {stage}: expected {expected}, got {actual}")]
    TypeMismatch { stage: String, expected: String, actual: String },
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SyncError {
    #[error("master clock not initialized")]
    ClockUninitialized,

    #[error("a/v drift {drift_ms}ms exceeds threshold {threshold_ms}ms")]
    DriftExceeded { drift_ms: i64, threshold_ms: i64 },
}

impl MediaError {
    pub fn internal(msg: impl fmt::Display) -> Self { Self::Internal(msg.to_string()) }
    pub fn unsupported(msg: impl fmt::Display) -> Self { Self::Unsupported(msg.to_string()) }
    pub fn ffi(context: &'static str, message: impl fmt::Display) -> Self {
        Self::Ffi { context, message: message.to_string() }
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::Sync(SyncError::DriftExceeded { .. }) | Self::Timeout { .. } | Self::Buffer(BufferError::Empty))
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Internal(_) | Self::Closed)
    }
}
