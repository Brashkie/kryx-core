//! Zero-copy media buffers with frame metadata.

use std::fmt;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};

use crate::error::{BufferError, Result};
use crate::types::{CodecId, MediaType, PixelFormat, SampleFormat, Timestamp, Timebase};

pub const MAX_BUFFER_SIZE: usize = 256 * 1024 * 1024; // 256 MiB

// ─── FrameFlags ──────────────────────────────────────────────────────────────

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FrameFlags: u32 {
        const KEYFRAME       = 0b0000_0001;
        const CORRUPT        = 0b0000_0010;
        const DISCARD        = 0b0000_0100;
        const END_OF_STREAM  = 0b0000_1000;
        const DECODED        = 0b0001_0000;
        const ENCODED        = 0b0010_0000;
    }
}

// ─── Frame metadata ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub dar_num: u32,
    pub dar_den: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMeta {
    pub sample_rate: u32,
    pub channels: u8,
    pub sample_format: SampleFormat,
    pub nb_samples: u32,
}

#[derive(Debug, Clone)]
pub enum FrameMeta {
    Video(VideoMeta),
    Audio(AudioMeta),
    None,
}

// ─── MediaBuffer ─────────────────────────────────────────────────────────────

/// The fundamental data unit in the Kryx pipeline.
/// Cloning is zero-copy — only a reference count is incremented.
#[derive(Clone)]
pub struct MediaBuffer {
    data: Bytes,
    pts: Timestamp,
    dts: Timestamp,
    duration: Timestamp,
    timebase: Timebase,
    media_type: MediaType,
    codec_id: CodecId,
    flags: FrameFlags,
    stream_index: u32,
    meta: Arc<FrameMeta>,
}

impl fmt::Debug for MediaBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaBuffer")
            .field("media_type", &self.media_type)
            .field("codec_id", &self.codec_id)
            .field("pts", &self.pts)
            .field("flags", &self.flags)
            .field("data_len", &self.data.len())
            .finish()
    }
}

impl MediaBuffer {
    pub fn builder(media_type: MediaType) -> MediaBufferBuilder {
        MediaBufferBuilder::new(media_type)
    }

    pub fn end_of_stream(media_type: MediaType) -> Self {
        let mut buf = Self::builder(media_type).build().unwrap();
        buf.flags |= FrameFlags::END_OF_STREAM;
        buf
    }

    // ── Accessors ──────────────────────────────────────────────────────────
    #[inline] pub fn data(&self)         -> &[u8]       { &self.data }
    #[inline] pub fn len(&self)          -> usize        { self.data.len() }
    #[inline] pub fn is_empty(&self)     -> bool         { self.data.is_empty() }
    #[inline] pub fn pts(&self)          -> Timestamp    { self.pts }
    #[inline] pub fn dts(&self)          -> Timestamp    { self.dts }
    #[inline] pub fn duration(&self)     -> Timestamp    { self.duration }
    #[inline] pub fn timebase(&self)     -> Timebase     { self.timebase }
    #[inline] pub fn media_type(&self)   -> MediaType    { self.media_type }
    #[inline] pub fn codec_id(&self)     -> CodecId      { self.codec_id }
    #[inline] pub fn flags(&self)        -> FrameFlags   { self.flags }
    #[inline] pub fn stream_index(&self) -> u32          { self.stream_index }
    #[inline] pub fn meta(&self)         -> &FrameMeta   { &self.meta }

    #[inline] pub fn is_keyframe(&self) -> bool { self.flags.contains(FrameFlags::KEYFRAME) }
    #[inline] pub fn is_eos(&self)      -> bool { self.flags.contains(FrameFlags::END_OF_STREAM) }
    #[inline] pub fn is_decoded(&self)  -> bool { self.flags.contains(FrameFlags::DECODED) }

    #[must_use]
    pub fn with_pts(mut self, pts: Timestamp) -> Self { self.pts = pts; self }

    #[must_use]
    pub fn with_flags(mut self, flags: FrameFlags) -> Self { self.flags = flags; self }

    /// Zero-copy slice.
    pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
        if end > self.data.len() || start > end {
            return Err(BufferError::OutOfBounds { start, end, len: self.data.len() }.into());
        }
        let mut cloned = self.clone();
        cloned.data = self.data.slice(start..end);
        Ok(cloned)
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

pub struct MediaBufferBuilder {
    media_type: MediaType,
    data:         Option<Vec<u8>>,
    pts:          Timestamp,
    dts:          Timestamp,
    duration:     Timestamp,
    timebase:     Timebase,
    codec_id:     CodecId,
    flags:        FrameFlags,
    stream_index: u32,
    meta:         FrameMeta,
}

impl MediaBufferBuilder {
    fn new(media_type: MediaType) -> Self {
        Self {
            media_type,
            data: None,
            pts: Timestamp::NONE,
            dts: Timestamp::NONE,
            duration: Timestamp::NONE,
            timebase: Timebase::VIDEO_90K,
            codec_id: CodecId::Unknown,
            flags: FrameFlags::empty(),
            stream_index: 0,
            meta: FrameMeta::None,
        }
    }

    #[must_use] pub fn data(mut self, data: impl Into<Vec<u8>>) -> Self { self.data = Some(data.into()); self }
    #[must_use] pub fn pts(mut self, pts: Timestamp) -> Self { self.pts = pts; self }
    #[must_use] pub fn dts(mut self, dts: Timestamp) -> Self { self.dts = dts; self }
    #[must_use] pub fn duration(mut self, d: Timestamp) -> Self { self.duration = d; self }
    #[must_use] pub fn timebase(mut self, tb: Timebase) -> Self { self.timebase = tb; self }
    #[must_use] pub fn codec(mut self, c: CodecId) -> Self { self.codec_id = c; self }
    #[must_use] pub fn flags(mut self, f: FrameFlags) -> Self { self.flags = f; self }
    #[must_use] pub fn stream_index(mut self, i: u32) -> Self { self.stream_index = i; self }
    #[must_use] pub fn video_meta(mut self, m: VideoMeta) -> Self { self.meta = FrameMeta::Video(m); self }
    #[must_use] pub fn audio_meta(mut self, m: AudioMeta) -> Self { self.meta = FrameMeta::Audio(m); self }

    pub fn build(self) -> Result<MediaBuffer> {
        let data = self.data.unwrap_or_default();
        if data.len() > MAX_BUFFER_SIZE {
            return Err(BufferError::CapacityExceeded { requested: data.len(), maximum: MAX_BUFFER_SIZE }.into());
        }
        Ok(MediaBuffer {
            data: Bytes::from(data),
            pts: self.pts,
            dts: self.dts,
            duration: self.duration,
            timebase: self.timebase,
            media_type: self.media_type,
            codec_id: self.codec_id,
            flags: self.flags,
            stream_index: self.stream_index,
            meta: Arc::new(self.meta),
        })
    }
}

// ─── MediaBufferMut ──────────────────────────────────────────────────────────

/// Growable buffer for writing data incrementally (e.g. inside a decoder).
pub struct MediaBufferMut {
    inner: BytesMut,
    media_type: MediaType,
}

impl MediaBufferMut {
    pub fn with_capacity(media_type: MediaType, capacity: usize) -> Self {
        Self { inner: BytesMut::with_capacity(capacity), media_type }
    }
    pub fn extend(&mut self, data: &[u8]) { self.inner.extend_from_slice(data); }
    pub fn len(&self) -> usize { self.inner.len() }
    pub fn is_empty(&self) -> bool { self.inner.is_empty() }

    pub fn freeze(self) -> Result<MediaBuffer> {
        MediaBuffer::builder(self.media_type).data(self.inner.to_vec()).build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_roundtrip() {
        let buf = MediaBuffer::builder(MediaType::Video)
            .codec(CodecId::H264)
            .pts(Timestamp::new(90_000))
            .flags(FrameFlags::KEYFRAME)
            .data(vec![0xAB; 512])
            .build().unwrap();

        assert!(buf.is_keyframe());
        assert_eq!(buf.pts(), Timestamp::new(90_000));
        assert_eq!(buf.len(), 512);
    }

    #[test]
    fn clone_is_zero_copy() {
        let buf = MediaBuffer::builder(MediaType::Video).data(vec![0u8; 4096]).build().unwrap();
        let clone = buf.clone();
        assert_eq!(buf.data().as_ptr(), clone.data().as_ptr());
    }

    #[test]
    fn eos_sentinel() {
        let eos = MediaBuffer::end_of_stream(MediaType::Video);
        assert!(eos.is_eos() && eos.is_empty());
    }
}
