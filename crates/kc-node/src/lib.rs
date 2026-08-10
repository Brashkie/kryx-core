//! Node.js native addon for @kryxjs/core
//! Exposes kc-core types to JavaScript via napi-rs.

#![deny(clippy::all)]
#![allow(clippy::new_without_default)]

use kc_core::{
    buffer::MediaBuffer as RustMediaBuffer,
    types::{CodecId, MediaType, Timebase, Timestamp},
};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

// ─── Timestamp ───────────────────────────────────────────────────────────────

/// A media timestamp.
#[napi]
pub struct JsTimestamp {
    inner: Timestamp,
}

#[napi]
impl JsTimestamp {
    #[napi(constructor)]
    pub fn new(pts: i64) -> Self {
        Self {
            inner: Timestamp::new(pts),
        }
    }

    #[napi(factory)]
    pub fn none() -> Self {
        Self {
            inner: Timestamp::NONE,
        }
    }

    #[napi(factory)]
    pub fn zero() -> Self {
        Self {
            inner: Timestamp::ZERO,
        }
    }

    #[napi(getter)]
    pub fn pts(&self) -> i64 {
        self.inner.pts()
    }

    #[napi(getter)]
    pub fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

// ─── MediaBuffer ─────────────────────────────────────────────────────────────

/// A zero-copy media frame buffer.
#[napi]
pub struct JsMediaBuffer {
    inner: RustMediaBuffer,
}

#[napi]
impl JsMediaBuffer {
    /// Create a video buffer.
    #[napi(factory)]
    pub fn video(data: Buffer, pts: i64) -> Self {
        Self {
            inner: RustMediaBuffer::builder(MediaType::Video)
                .codec(CodecId::H264)
                .pts(Timestamp::new(pts))
                .timebase(Timebase::VIDEO_90K)
                .data(data.as_ref().to_vec())
                .build()
                .expect("failed to create video buffer"),
        }
    }

    /// Create an audio buffer.
    #[napi(factory)]
    pub fn audio(data: Buffer, pts: i64) -> Self {
        Self {
            inner: RustMediaBuffer::builder(MediaType::Audio)
                .codec(CodecId::Aac)
                .pts(Timestamp::new(pts))
                .timebase(Timebase::AUDIO_48K)
                .data(data.as_ref().to_vec())
                .build()
                .expect("failed to create audio buffer"),
        }
    }

    /// Create an end-of-stream sentinel.
    #[napi(factory)]
    pub fn eos_video() -> Self {
        Self {
            inner: RustMediaBuffer::end_of_stream(MediaType::Video),
        }
    }

    #[napi(getter)]
    pub fn pts(&self) -> i64 {
        self.inner.pts().pts()
    }

    #[napi(getter)]
    pub fn len(&self) -> u32 {
        self.inner.len() as u32
    }

    #[napi(getter)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[napi(getter)]
    pub fn is_keyframe(&self) -> bool {
        self.inner.is_keyframe()
    }

    #[napi(getter)]
    pub fn is_eos(&self) -> bool {
        self.inner.is_eos()
    }

    #[napi(getter)]
    pub fn media_type(&self) -> String {
        self.inner.media_type().to_string()
    }

    #[napi(getter)]
    pub fn codec_id(&self) -> String {
        self.inner.codec_id().to_string()
    }

    /// Raw data as a Buffer.
    #[napi]
    pub fn data(&self) -> Buffer {
        self.inner.data().to_vec().into()
    }
}

// ─── Version ─────────────────────────────────────────────────────────────────

/// Returns the native module version.
#[napi]
pub fn version() -> String {
    kc_core::VERSION.to_string()
}
