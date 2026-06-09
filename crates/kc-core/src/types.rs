//! Shared types across the Kryx ecosystem.

use std::fmt;
use std::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ─── Timestamp ───────────────────────────────────────────────────────────────

/// A media timestamp in a given timebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Unknown / unset timestamp (equivalent to `AV_NOPTS_VALUE`).
    pub const NONE: Self = Self(i64::MIN);
    /// Zero — start of stream.
    pub const ZERO: Self = Self(0);

    #[inline] pub const fn new(pts: i64) -> Self { Self(pts) }
    #[inline] pub const fn is_valid(self) -> bool { self.0 != i64::MIN }
    #[inline] pub const fn pts(self) -> i64 { self.0 }

    /// Convert to [`Duration`] using the given timebase.
    pub fn to_duration(self, tb: Timebase) -> Duration {
        let secs = self.0 as f64 * tb.num as f64 / tb.den as f64;
        Duration::from_secs_f64(secs.max(0.0))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE { write!(f, "NONE") } else { write!(f, "pts({})", self.0) }
    }
}

// ─── Timebase ────────────────────────────────────────────────────────────────

/// Rational number representing time units per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Timebase {
    pub num: u32,
    pub den: u32,
}

impl Timebase {
    pub const MILLISECOND: Self = Self { num: 1, den: 1_000 };
    pub const VIDEO_90K:   Self = Self { num: 1, den: 90_000 };
    pub const AUDIO_48K:   Self = Self { num: 1, den: 48_000 };
    pub const AUDIO_44K:   Self = Self { num: 1, den: 44_100 };

    #[inline] pub const fn new(num: u32, den: u32) -> Self { Self { num, den } }

    /// Rescale a timestamp from this timebase to `target`.
    pub fn rescale(self, ts: Timestamp, target: Self) -> Timestamp {
        if !ts.is_valid() { return Timestamp::NONE; }
        let scaled = ts.0 as i128
            * self.num as i128 * target.den as i128
            / (self.den as i128 * target.num as i128);
        Timestamp::new(scaled as i64)
    }
}

impl fmt::Display for Timebase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

// ─── MediaType ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum MediaType { Video, Audio, Subtitle, Data }

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Video    => write!(f, "video"),
            Self::Audio    => write!(f, "audio"),
            Self::Subtitle => write!(f, "subtitle"),
            Self::Data     => write!(f, "data"),
        }
    }
}

// ─── CodecId ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum CodecId {
    // Video
    H264, H265, Av1, Vp8, Vp9,
    // Audio
    Aac, Mp3, Opus, Flac, Pcm16Le, PcmF32Le,
    // Subtitle
    Srt, Ass, WebVtt,
    Unknown,
}

impl fmt::Display for CodecId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::H264    => "h264",    Self::H265  => "h265",
            Self::Av1     => "av1",     Self::Vp8   => "vp8",
            Self::Vp9     => "vp9",     Self::Aac   => "aac",
            Self::Mp3     => "mp3",     Self::Opus  => "opus",
            Self::Flac    => "flac",    Self::Pcm16Le => "pcm_s16le",
            Self::PcmF32Le => "pcm_f32le", Self::Srt => "srt",
            Self::Ass     => "ass",     Self::WebVtt => "webvtt",
            Self::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

// ─── PixelFormat / SampleFormat ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum PixelFormat {
    Yuv420p, Yuv422p, Yuv444p, Rgba, Bgra, Rgb24, Nv12, Yuv420p10Le,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum SampleFormat { S16, S32, F32, F64, S16P, F32P }

// ─── StreamInfo ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StreamInfo {
    pub index: u32,
    pub media_type: MediaType,
    pub codec_id: CodecId,
    pub timebase: Timebase,
    pub duration: Option<Timestamp>,
    pub extradata: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn timestamp_none_invalid() { assert!(!Timestamp::NONE.is_valid()); }

    #[test] fn timebase_rescale_90k_to_ms() {
        let ts = Timebase::VIDEO_90K.rescale(Timestamp::new(90_000), Timebase::MILLISECOND);
        assert_eq!(ts.pts(), 1000);
    }
}
