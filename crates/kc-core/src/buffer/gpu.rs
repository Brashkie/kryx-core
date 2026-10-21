//! Backend-agnostic GPU buffer abstraction (Phase 4, foundation).
//!
//! Kryx owns the *architecture* of GPU memory, not any one graphics API. This
//! module defines the shape — an opaque device buffer behind a [`GpuBackend`]
//! trait — without pulling in Vulkan, Metal, or D3D12. A real backend implements
//! the trait in a separate crate/feature later; [`CpuFallbackBackend`] implements
//! it with host memory today, so the whole abstraction is usable and testable
//! right now with zero graphics dependencies.
//!
//! The model is deliberately minimal: allocate device memory, upload host bytes
//! into it, download them back, free it. Ownership is RAII — a [`GpuBuffer`]
//! frees its device allocation on drop. Zero-copy CPU↔GPU paths (mapped memory,
//! shared buffers) are a backend concern layered on top; the fallback copies.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//! use kc_core::buffer::{CpuFallbackBackend, GpuBuffer};
//!
//! let backend = Arc::new(CpuFallbackBackend::new());
//! let gpu = GpuBuffer::from_slice(backend.clone(), &[1, 2, 3, 4]).unwrap();
//! assert_eq!(gpu.len(), 4);
//! assert_eq!(gpu.download().unwrap(), vec![1, 2, 3, 4]);
//! // Freed automatically when `gpu` drops.
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use parking_lot::Mutex;

/// Errors from GPU buffer operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// The handle doesn't refer to a live device buffer.
    InvalidHandle(u64),
    /// Upload data didn't fit the target buffer.
    SizeMismatch { buffer: usize, data: usize },
    /// The backend ran out of device memory.
    OutOfMemory { requested: usize },
    /// Backend-specific failure.
    Backend(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandle(id) => write!(f, "invalid GPU buffer handle: {id}"),
            Self::SizeMismatch { buffer, data } => {
                write!(f, "upload of {data} bytes into a {buffer}-byte GPU buffer")
            }
            Self::OutOfMemory { requested } => {
                write!(f, "GPU out of memory requesting {requested} bytes")
            }
            Self::Backend(msg) => write!(f, "GPU backend error: {msg}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// Result alias for GPU operations.
pub type GpuResult<T> = std::result::Result<T, GpuError>;

/// The operations a GPU backend provides over opaque device buffers.
///
/// A handle (`u64`) names a device allocation; the backend interprets it. Core
/// only orchestrates alloc/upload/download/free — it never assumes a particular
/// device, queue, or memory model. Implementations must be `Send + Sync` so a
/// buffer can cross threads and stages.
pub trait GpuBackend: Send + Sync + fmt::Debug {
    /// A short backend name (e.g. `"cpu-fallback"`, `"vulkan"`).
    fn name(&self) -> &'static str;

    /// Allocate `size` bytes of device memory, returning an opaque handle.
    fn alloc(&self, size: usize) -> GpuResult<u64>;

    /// Upload host bytes into device buffer `id` (must match its size).
    fn upload(&self, id: u64, data: &[u8]) -> GpuResult<()>;

    /// Download device buffer `id` back into host memory.
    fn download(&self, id: u64) -> GpuResult<Vec<u8>>;

    /// Free device buffer `id`.
    fn free(&self, id: u64) -> GpuResult<()>;

    /// Total bytes currently allocated across live buffers.
    fn allocated_bytes(&self) -> usize;
}

/// An RAII handle to a device buffer on some [`GpuBackend`].
///
/// Dropping it frees the device allocation. Cloning the backend `Arc` is cheap;
/// the buffer itself is not `Clone` (one owner frees it).
pub struct GpuBuffer {
    backend: Arc<dyn GpuBackend>,
    id: u64,
    size: usize,
}

impl GpuBuffer {
    /// Allocate an uninitialized device buffer of `size` bytes.
    pub fn alloc(backend: Arc<dyn GpuBackend>, size: usize) -> GpuResult<Self> {
        let id = backend.alloc(size)?;
        Ok(Self { backend, id, size })
    }

    /// Allocate a device buffer and upload `data` into it in one step.
    pub fn from_slice(backend: Arc<dyn GpuBackend>, data: &[u8]) -> GpuResult<Self> {
        let buf = Self::alloc(backend, data.len())?;
        buf.upload(data)?;
        Ok(buf)
    }

    /// Upload host bytes into this buffer (must match its size).
    pub fn upload(&self, data: &[u8]) -> GpuResult<()> {
        self.backend.upload(self.id, data)
    }

    /// Download this buffer's contents back to host memory.
    pub fn download(&self) -> GpuResult<Vec<u8>> {
        self.backend.download(self.id)
    }

    /// Size in bytes.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Whether the buffer is zero-length.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// The opaque backend handle (for interop with backend-specific code).
    pub fn handle(&self) -> u64 {
        self.id
    }

    /// The backend's name.
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }
}

impl fmt::Debug for GpuBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuBuffer")
            .field("backend", &self.backend.name())
            .field("handle", &self.id)
            .field("size", &self.size)
            .finish()
    }
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        // Best effort: a backend that already lost the handle is not an error
        // worth panicking over during cleanup.
        let _ = self.backend.free(self.id);
    }
}

// ─── CPU fallback backend ────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct CpuState {
    next_id: u64,
    buffers: HashMap<u64, Vec<u8>>,
}

/// A [`GpuBackend`] that keeps "device" buffers in host memory.
///
/// It implements the whole abstraction with plain `Vec<u8>`s, so code written
/// against `GpuBuffer` runs and is testable with no GPU present. A real backend
/// swaps in later without changing call sites — that's the point of the trait.
#[derive(Debug, Default)]
pub struct CpuFallbackBackend {
    inner: Mutex<CpuState>,
}

impl CpuFallbackBackend {
    /// Create a new CPU-backed fallback.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many live buffers it currently holds.
    pub fn buffer_count(&self) -> usize {
        self.inner.lock().buffers.len()
    }
}

impl GpuBackend for CpuFallbackBackend {
    fn name(&self) -> &'static str {
        "cpu-fallback"
    }

    fn alloc(&self, size: usize) -> GpuResult<u64> {
        let mut st = self.inner.lock();
        st.next_id += 1;
        let id = st.next_id;
        st.buffers.insert(id, vec![0u8; size]);
        Ok(id)
    }

    fn upload(&self, id: u64, data: &[u8]) -> GpuResult<()> {
        let mut st = self.inner.lock();
        let buf = st.buffers.get_mut(&id).ok_or(GpuError::InvalidHandle(id))?;
        if buf.len() != data.len() {
            return Err(GpuError::SizeMismatch {
                buffer: buf.len(),
                data: data.len(),
            });
        }
        buf.copy_from_slice(data);
        Ok(())
    }

    fn download(&self, id: u64) -> GpuResult<Vec<u8>> {
        let st = self.inner.lock();
        st.buffers
            .get(&id)
            .cloned()
            .ok_or(GpuError::InvalidHandle(id))
    }

    fn free(&self, id: u64) -> GpuResult<()> {
        let mut st = self.inner.lock();
        st.buffers.remove(&id).ok_or(GpuError::InvalidHandle(id))?;
        Ok(())
    }

    fn allocated_bytes(&self) -> usize {
        self.inner.lock().buffers.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_cpu_fallback() {
        let backend = Arc::new(CpuFallbackBackend::new());
        let gpu = GpuBuffer::from_slice(backend.clone(), &[10, 20, 30]).unwrap();
        assert_eq!(gpu.len(), 3);
        assert_eq!(gpu.backend_name(), "cpu-fallback");
        assert_eq!(gpu.download().unwrap(), vec![10, 20, 30]);
        assert_eq!(backend.allocated_bytes(), 3);
    }

    #[test]
    fn drop_frees_device_memory() {
        let backend = Arc::new(CpuFallbackBackend::new());
        {
            let _gpu = GpuBuffer::alloc(backend.clone(), 1024).unwrap();
            assert_eq!(backend.buffer_count(), 1);
            assert_eq!(backend.allocated_bytes(), 1024);
        } // dropped here
        assert_eq!(backend.buffer_count(), 0);
        assert_eq!(backend.allocated_bytes(), 0);
    }

    #[test]
    fn upload_size_mismatch_is_an_error() {
        let backend = Arc::new(CpuFallbackBackend::new());
        let gpu = GpuBuffer::alloc(backend, 4).unwrap();
        assert_eq!(
            gpu.upload(&[1, 2, 3]),
            Err(GpuError::SizeMismatch { buffer: 4, data: 3 })
        );
    }

    #[test]
    fn re_upload_updates_contents() {
        let backend = Arc::new(CpuFallbackBackend::new());
        let gpu = GpuBuffer::from_slice(backend, &[1, 1, 1, 1]).unwrap();
        gpu.upload(&[9, 8, 7, 6]).unwrap();
        assert_eq!(gpu.download().unwrap(), vec![9, 8, 7, 6]);
    }

    #[test]
    fn multiple_buffers_are_independent() {
        let backend = Arc::new(CpuFallbackBackend::new());
        let a = GpuBuffer::from_slice(backend.clone(), &[1, 2]).unwrap();
        let b = GpuBuffer::from_slice(backend.clone(), &[3, 4, 5]).unwrap();
        assert_ne!(a.handle(), b.handle());
        assert_eq!(a.download().unwrap(), vec![1, 2]);
        assert_eq!(b.download().unwrap(), vec![3, 4, 5]);
        assert_eq!(backend.allocated_bytes(), 5);
    }
}
