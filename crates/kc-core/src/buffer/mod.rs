//! Zero-copy media buffers with frame metadata.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};

use crate::error::{BufferError, Result};
use crate::types::{CodecId, MediaType, PixelFormat, SampleFormat, Timebase, Timestamp};

mod pool;
pub use pool::{BufferPool, PoolStats};

pub const MAX_BUFFER_SIZE: usize = 256 * 1024 * 1024; // 256 MiB

/// A shared, single-threaded handle to a [`BufferPool`].
///
/// The pool is not thread-safe by design (a lock-free primitive is fastest when
/// each pipeline stage owns its pool). `SharedPool` wraps it in `Rc<RefCell<_>>`
/// so a [`MediaBufferMut`] can hold onto the pool and return its scratch buffer
/// on freeze, within a single thread. Across threads, give each worker its own
/// pool.
pub type SharedPool = Rc<RefCell<BufferPool>>;

/// Create a new shared pool with default settings.
pub fn shared_pool() -> SharedPool {
    Rc::new(RefCell::new(BufferPool::new()))
}

/// An RAII wrapper around a pooled buffer that recycles itself on drop.
///
/// This is the ergonomic layer over the explicit [`BufferPool::acquire`] /
/// [`BufferPool::recycle`] primitive: [`acquire_guard`] hands you a guard that
/// derefs to the underlying [`BytesMut`], and when it goes out of scope the
/// buffer returns to the pool automatically — no manual `recycle` call, no way
/// to forget it. Use it where scope-based cleanup reads better than the explicit
/// call; the explicit API stays available for the cases (like `MediaBufferMut`)
/// where the buffer's fate depends on how it's frozen.
///
/// ```
/// use kc_core::buffer::{acquire_guard, shared_pool};
///
/// let pool = shared_pool();
/// {
///     let mut buf = acquire_guard(&pool, 64 * 1024);
///     buf.extend_from_slice(b"scratch work");
/// } // buf recycled here, automatically
/// assert_eq!(pool.borrow().pooled_count(), 1);
/// ```
pub struct PoolGuard {
    // Always Some until drop; Option lets us move the buffer out in Drop.
    buf: Option<BytesMut>,
    pool: SharedPool,
}

/// Acquire a buffer from `pool` wrapped in a self-recycling [`PoolGuard`].
///
/// The guard derefs to `BytesMut`, so it can be written to directly. When it
/// drops, the buffer is cleared and returned to `pool`.
pub fn acquire_guard(pool: &SharedPool, size: usize) -> PoolGuard {
    let buf = pool.borrow_mut().acquire(size);
    PoolGuard {
        buf: Some(buf),
        pool: pool.clone(),
    }
}

impl PoolGuard {
    /// Take the buffer out of the guard, giving up automatic recycling.
    ///
    /// Use this when the buffer needs to outlive the guard's scope — ownership
    /// transfers to the caller and nothing returns to the pool.
    pub fn into_inner(mut self) -> BytesMut {
        self.buf.take().expect("buffer present until drop")
    }
}

impl std::ops::Deref for PoolGuard {
    type Target = BytesMut;
    fn deref(&self) -> &BytesMut {
        self.buf.as_ref().expect("buffer present until drop")
    }
}

impl std::ops::DerefMut for PoolGuard {
    fn deref_mut(&mut self) -> &mut BytesMut {
        self.buf.as_mut().expect("buffer present until drop")
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        if let Some(mut buf) = self.buf.take() {
            buf.clear();
            self.pool.borrow_mut().recycle(buf);
        }
    }
}

impl fmt::Debug for PoolGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PoolGuard")
            .field("len", &self.buf.as_ref().map(|b| b.len()).unwrap_or(0))
            .field("capacity", &self.buf.as_ref().map(|b| b.capacity()).unwrap_or(0))
            .finish()
    }
}

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
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    #[inline]
    pub fn pts(&self) -> Timestamp {
        self.pts
    }
    #[inline]
    pub fn dts(&self) -> Timestamp {
        self.dts
    }
    #[inline]
    pub fn duration(&self) -> Timestamp {
        self.duration
    }
    #[inline]
    pub fn timebase(&self) -> Timebase {
        self.timebase
    }
    #[inline]
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }
    #[inline]
    pub fn codec_id(&self) -> CodecId {
        self.codec_id
    }
    #[inline]
    pub fn flags(&self) -> FrameFlags {
        self.flags
    }
    #[inline]
    pub fn stream_index(&self) -> u32 {
        self.stream_index
    }
    #[inline]
    pub fn meta(&self) -> &FrameMeta {
        &self.meta
    }

    #[inline]
    pub fn is_keyframe(&self) -> bool {
        self.flags.contains(FrameFlags::KEYFRAME)
    }
    #[inline]
    pub fn is_eos(&self) -> bool {
        self.flags.contains(FrameFlags::END_OF_STREAM)
    }
    #[inline]
    pub fn is_decoded(&self) -> bool {
        self.flags.contains(FrameFlags::DECODED)
    }

    #[must_use]
    pub fn with_pts(mut self, pts: Timestamp) -> Self {
        self.pts = pts;
        self
    }

    #[must_use]
    pub fn with_flags(mut self, flags: FrameFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Zero-copy slice.
    pub fn slice(&self, start: usize, end: usize) -> Result<Self> {
        if end > self.data.len() || start > end {
            return Err(BufferError::OutOfBounds {
                start,
                end,
                len: self.data.len(),
            }
            .into());
        }
        let mut cloned = self.clone();
        cloned.data = self.data.slice(start..end);
        Ok(cloned)
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

pub struct MediaBufferBuilder {
    media_type: MediaType,
    data: Option<Vec<u8>>,
    pts: Timestamp,
    dts: Timestamp,
    duration: Timestamp,
    timebase: Timebase,
    codec_id: CodecId,
    flags: FrameFlags,
    stream_index: u32,
    meta: FrameMeta,
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

    #[must_use]
    pub fn data(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.data = Some(data.into());
        self
    }
    #[must_use]
    pub fn pts(mut self, pts: Timestamp) -> Self {
        self.pts = pts;
        self
    }
    #[must_use]
    pub fn dts(mut self, dts: Timestamp) -> Self {
        self.dts = dts;
        self
    }
    #[must_use]
    pub fn duration(mut self, d: Timestamp) -> Self {
        self.duration = d;
        self
    }
    #[must_use]
    pub fn timebase(mut self, tb: Timebase) -> Self {
        self.timebase = tb;
        self
    }
    #[must_use]
    pub fn codec(mut self, c: CodecId) -> Self {
        self.codec_id = c;
        self
    }
    #[must_use]
    pub fn flags(mut self, f: FrameFlags) -> Self {
        self.flags = f;
        self
    }
    #[must_use]
    pub fn stream_index(mut self, i: u32) -> Self {
        self.stream_index = i;
        self
    }
    #[must_use]
    pub fn video_meta(mut self, m: VideoMeta) -> Self {
        self.meta = FrameMeta::Video(m);
        self
    }
    #[must_use]
    pub fn audio_meta(mut self, m: AudioMeta) -> Self {
        self.meta = FrameMeta::Audio(m);
        self
    }

    pub fn build(self) -> Result<MediaBuffer> {
        let data = self.data.unwrap_or_default();
        if data.len() > MAX_BUFFER_SIZE {
            return Err(BufferError::CapacityExceeded {
                requested: data.len(),
                maximum: MAX_BUFFER_SIZE,
            }
            .into());
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
///
/// It can optionally draw its scratch allocation from a [`BufferPool`] and
/// return it there when frozen — see [`MediaBufferMut::with_pool`]. Two freeze
/// routes trade off copy-vs-reuse; pick by how long the resulting data lives:
///
/// - [`freeze`](Self::freeze): copies the scratch into fresh [`Bytes`] and
///   recycles the scratch back to the pool. Best for a pipeline processing many
///   successive frames — the write buffer is reused, memory stays bounded.
/// - [`freeze_zero_copy`](Self::freeze_zero_copy): converts the scratch into
///   [`Bytes`] with no copy (the allocation is shared), so nothing returns to
///   the pool. Best when the data will live a long time and the copy would cost
///   more than the lost reuse.
pub struct MediaBufferMut {
    inner: BytesMut,
    media_type: MediaType,
    /// Optional pool the scratch came from; `freeze` returns the buffer here.
    pool: Option<SharedPool>,
}

impl MediaBufferMut {
    /// Create a standalone growable buffer (no pool involvement).
    pub fn with_capacity(media_type: MediaType, capacity: usize) -> Self {
        Self {
            inner: BytesMut::with_capacity(capacity),
            media_type,
            pool: None,
        }
    }

    /// Create a growable buffer whose scratch allocation is drawn from `pool`.
    ///
    /// On [`freeze`](Self::freeze) the scratch returns to this pool for reuse.
    /// On [`freeze_zero_copy`](Self::freeze_zero_copy) it does not (ownership of
    /// the allocation transfers to the resulting [`MediaBuffer`]).
    pub fn with_pool(pool: &SharedPool, media_type: MediaType, capacity: usize) -> Self {
        let inner = pool.borrow_mut().acquire(capacity);
        Self {
            inner,
            media_type,
            pool: Some(pool.clone()),
        }
    }

    pub fn extend(&mut self, data: &[u8]) {
        self.inner.extend_from_slice(data);
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Freeze into a [`MediaBuffer`], **copying** the bytes into a fresh
    /// allocation and **recycling** the scratch back to the pool (if any).
    ///
    /// Use this in a steady-state pipeline: the scratch buffer is reused for the
    /// next frame, so allocation churn stays low and memory bounded.
    pub fn freeze(self) -> Result<MediaBuffer> {
        let MediaBufferMut {
            mut inner,
            media_type,
            pool,
        } = self;
        // Copy the written bytes out before the scratch goes back to the pool.
        let out = MediaBuffer::builder(media_type).data(inner.to_vec()).build();
        if let Some(pool) = pool {
            inner.clear();
            pool.borrow_mut().recycle(inner);
        }
        out
    }

    /// Freeze into a [`MediaBuffer`] with **no copy**: the scratch allocation is
    /// converted directly into shared [`Bytes`]. Nothing returns to the pool —
    /// the resulting buffer owns the allocation for as long as it lives.
    ///
    /// Use this when the data will outlive the current frame and the copy would
    /// cost more than the reuse you give up.
    pub fn freeze_zero_copy(self) -> Result<MediaBuffer> {
        let len = self.inner.len();
        if len > MAX_BUFFER_SIZE {
            return Err(BufferError::CapacityExceeded {
                requested: len,
                maximum: MAX_BUFFER_SIZE,
            }
            .into());
        }
        // BytesMut::freeze() shares the allocation — no memcpy.
        Ok(MediaBuffer {
            data: self.inner.freeze(),
            pts: Timestamp::NONE,
            dts: Timestamp::NONE,
            duration: Timestamp::NONE,
            timebase: Timebase::VIDEO_90K,
            media_type: self.media_type,
            codec_id: CodecId::Unknown,
            flags: FrameFlags::empty(),
            stream_index: 0,
            meta: Arc::new(FrameMeta::None),
        })
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
            .build()
            .unwrap();

        assert!(buf.is_keyframe());
        assert_eq!(buf.pts(), Timestamp::new(90_000));
        assert_eq!(buf.len(), 512);
    }

    #[test]
    fn clone_is_zero_copy() {
        let buf = MediaBuffer::builder(MediaType::Video)
            .data(vec![0u8; 4096])
            .build()
            .unwrap();
        let clone = buf.clone();
        assert_eq!(buf.data().as_ptr(), clone.data().as_ptr());
    }

    #[test]
    fn eos_sentinel() {
        let eos = MediaBuffer::end_of_stream(MediaType::Video);
        assert!(eos.is_eos() && eos.is_empty());
    }

    #[test]
    fn pooled_freeze_recycles_scratch() {
        // with_pool draws scratch from the pool; freeze() copies out and returns
        // the scratch, so a second with_pool of the same size reuses it.
        let pool = shared_pool();
        let mut buf = MediaBufferMut::with_pool(&pool, MediaType::Audio, 64 * 1024);
        buf.extend(&[1, 2, 3, 4]);
        let frame = buf.freeze().unwrap();
        assert_eq!(frame.data(), &[1, 2, 3, 4]);
        // The scratch went back: one buffer pooled, and its reuse shows as a hit.
        assert_eq!(pool.borrow().pooled_count(), 1);

        let buf2 = MediaBufferMut::with_pool(&pool, MediaType::Audio, 64 * 1024);
        assert_eq!(pool.borrow().stats().hits, 1);
        drop(buf2);
    }

    #[test]
    fn zero_copy_freeze_does_not_recycle() {
        // freeze_zero_copy shares the allocation with the MediaBuffer, so nothing
        // returns to the pool — the frame owns it.
        let pool = shared_pool();
        let mut buf = MediaBufferMut::with_pool(&pool, MediaType::Audio, 64 * 1024);
        buf.extend(&[9, 8, 7]);
        let frame = buf.freeze_zero_copy().unwrap();
        assert_eq!(frame.data(), &[9, 8, 7]);
        // Scratch was NOT recycled — the frame holds the allocation.
        assert_eq!(pool.borrow().pooled_count(), 0);
    }

    #[test]
    fn with_capacity_still_works_without_a_pool() {
        // The original poolless path is unchanged.
        let mut buf = MediaBufferMut::with_capacity(MediaType::Video, 128);
        buf.extend(&[0xAB; 10]);
        let frame = buf.freeze().unwrap();
        assert_eq!(frame.len(), 10);
    }

    #[test]
    fn pool_guard_recycles_on_drop() {
        let pool = shared_pool();
        {
            let mut buf = acquire_guard(&pool, 64 * 1024);
            buf.extend_from_slice(b"scratch");
            assert_eq!(buf.len(), 7);
        } // dropped here → recycled
        assert_eq!(pool.borrow().pooled_count(), 1);
        assert_eq!(pool.borrow().stats().recycled, 1);
    }

    #[test]
    fn pool_guard_into_inner_does_not_recycle() {
        let pool = shared_pool();
        let buf = acquire_guard(&pool, 64 * 1024);
        let owned = buf.into_inner(); // ownership taken out
        assert_eq!(pool.borrow().pooled_count(), 0);
        drop(owned); // dropping the raw BytesMut does NOT touch the pool
        assert_eq!(pool.borrow().pooled_count(), 0);
    }

    #[test]
    fn pool_guard_reuse_across_scopes() {
        let pool = shared_pool();
        {
            let _b = acquire_guard(&pool, 4 * 1024);
        } // recycled
        {
            let _b = acquire_guard(&pool, 4 * 1024); // reuses the recycled one
        }
        assert_eq!(pool.borrow().stats().hits, 1);
    }
}
