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

// ─── Backend selection ───────────────────────────────────────────────────────

/// Return the best available GPU backend as a trait object.
///
/// With the `gpu-wgpu` feature enabled this tries a real wgpu backend first
/// (Vulkan / Metal / D3D12 under the hood) and falls back to the CPU backend if
/// no adapter is available. Without the feature it always returns the CPU
/// fallback. Call sites never change — only the backend behind the handle does.
pub fn best_backend() -> Arc<dyn GpuBackend> {
    #[cfg(feature = "gpu-wgpu")]
    {
        if let Ok(b) = wgpu_backend::WgpuBackend::new() {
            return Arc::new(b);
        }
    }
    Arc::new(CpuFallbackBackend::new())
}

// ─── Real GPU backend (wgpu), feature-gated ──────────────────────────────────

#[cfg(feature = "gpu-wgpu")]
mod wgpu_backend {
    //! A real [`GpuBackend`](super::GpuBackend) over `wgpu` (Vulkan / Metal /
    //! D3D12 / WebGPU). Enabled only with the `gpu-wgpu` feature; the crate
    //! builds and tests without it, on the CPU fallback.
    //!
    //! ## Version note
    //!
    //! Written & verified to compile against **wgpu 0.20.1** + **pollster 0.3**.
    //! wgpu's API shifts between releases; on a newer wgpu these spots most
    //! likely need a small tweak: `DeviceDescriptor` gains a `memory_hints` field
    //! in wgpu 22.x (add `memory_hints: Default::default(),`), `request_adapter`/
    //! `request_device` return types, and `Maintain::Wait` (later `PollType`).
    //! The `GpuBackend` contract and everything above this module stay the same.
    //!
    //! The trait is synchronous; wgpu is async. Device creation blocks via
    //! `pollster`, and buffer readback blocks on `device.poll(Maintain::Wait)`.

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use parking_lot::Mutex;

    use super::{GpuBackend, GpuError, GpuResult};

    /// Round `n` up to the nearest multiple of 4 (wgpu copy alignment).
    #[inline]
    fn align4(n: usize) -> usize {
        (n + 3) & !3
    }

    struct Entry {
        buffer: wgpu::Buffer,
        /// Logical size requested by the caller (buffer may be padded to align).
        logical: usize,
    }

    /// GPU-backed buffers via wgpu.
    pub struct WgpuBackend {
        device: wgpu::Device,
        queue: wgpu::Queue,
        state: Mutex<HashMap<u64, Entry>>,
        next_id: AtomicU64,
    }

    impl std::fmt::Debug for WgpuBackend {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WgpuBackend").finish()
        }
    }

    impl WgpuBackend {
        /// Request an adapter and device, blocking until ready. Fails if no GPU
        /// adapter is available (e.g. headless CI without a software adapter).
        pub fn new() -> GpuResult<Self> {
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(
                instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
            )
            .ok_or_else(|| GpuError::Backend("no GPU adapter available".into()))?;
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("kryx-wgpu-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    // NOTE: wgpu 0.20 has no `memory_hints` field; it was added in
                    // wgpu 22.x. On a newer wgpu add: `memory_hints: Default::default(),`.
                },
                None,
            ))
            .map_err(|e| GpuError::Backend(e.to_string()))?;
            Ok(Self {
                device,
                queue,
                state: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(0),
            })
        }
    }

    impl GpuBackend for WgpuBackend {
        fn name(&self) -> &'static str {
            "wgpu"
        }

        fn alloc(&self, size: usize) -> GpuResult<u64> {
            let padded = align4(size).max(4);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("kryx-gpu-buffer"),
                size: padded as u64,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
            self.state.lock().insert(
                id,
                Entry {
                    buffer,
                    logical: size,
                },
            );
            Ok(id)
        }

        fn upload(&self, id: u64, data: &[u8]) -> GpuResult<()> {
            let st = self.state.lock();
            let entry = st.get(&id).ok_or(GpuError::InvalidHandle(id))?;
            if data.len() != entry.logical {
                return Err(GpuError::SizeMismatch {
                    buffer: entry.logical,
                    data: data.len(),
                });
            }
            // write_buffer is applied at the next submit; flush immediately.
            self.queue.write_buffer(&entry.buffer, 0, data);
            self.queue.submit(std::iter::empty::<wgpu::CommandBuffer>());
            Ok(())
        }

        fn download(&self, id: u64) -> GpuResult<Vec<u8>> {
            let st = self.state.lock();
            let entry = st.get(&id).ok_or(GpuError::InvalidHandle(id))?;
            let padded = align4(entry.logical).max(4);

            // Staging buffer we can map for reading.
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("kryx-gpu-staging"),
                size: padded as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            enc.copy_buffer_to_buffer(&entry.buffer, 0, &staging, 0, padded as u64);
            self.queue.submit(Some(enc.finish()));

            // Map and block until the GPU is done.
            let slice = staging.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
            let _ = self.device.poll(wgpu::Maintain::Wait);
            rx.recv()
                .map_err(|_| GpuError::Backend("map channel closed".into()))?
                .map_err(|e| GpuError::Backend(format!("buffer map failed: {e:?}")))?;

            let mapped = slice.get_mapped_range();
            let mut out = mapped[..entry.logical].to_vec();
            out.truncate(entry.logical);
            drop(mapped);
            staging.unmap();
            Ok(out)
        }

        fn free(&self, id: u64) -> GpuResult<()> {
            self.state
                .lock()
                .remove(&id)
                .ok_or(GpuError::InvalidHandle(id))?;
            Ok(())
        }

        fn allocated_bytes(&self) -> usize {
            self.state.lock().values().map(|e| e.logical).sum()
        }
    }
}

#[cfg(feature = "gpu-wgpu")]
pub use wgpu_backend::WgpuBackend;

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

    #[test]
    fn best_backend_works_through_the_trait() {
        // Without the gpu-wgpu feature this is the CPU fallback; with it, it's
        // wgpu (or the CPU fallback if no adapter). Call sites don't change.
        let backend = best_backend();
        let gpu = GpuBuffer::from_slice(backend, &[7, 7, 7, 7]).unwrap();
        assert_eq!(gpu.download().unwrap(), vec![7, 7, 7, 7]);
    }
}
