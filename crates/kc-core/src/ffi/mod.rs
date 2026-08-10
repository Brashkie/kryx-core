//! FFI bridge — Rust ↔ Zig low-level modules.
//!
//! This module defines the C-compatible interface for calling into Zig
//! libraries (`@kryxjs/codecs`, `@kryxjs/containers`, ...).
//!
//! # Linking strategy
//!
//! - By **default**, this module compiles with internal Rust stubs so the
//!   crate is always usable without a linked Zig library.
//! - When the `link-zig` feature is enabled, real Zig symbols are linked
//!   from the system / build environment.
//!
//! # Safety
//!
//! All raw FFI is `unsafe`. Safe wrappers are exposed at the bottom of
//! this module — never call the raw extern functions directly from
//! application code.

use std::ffi::CStr;

#[cfg(feature = "link-zig")]
use std::ffi::c_char;

use crate::error::{MediaError, Result};

// ─── ZigResult ───────────────────────────────────────────────────────────────

/// Return code from Zig functions. Mirrors `media_result_t` in Zig.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZigResult {
    /// Success.
    Ok = 0,
    /// Out of memory.
    OutOfMemory = 1,
    /// Invalid input from caller.
    InvalidInput = 2,
    /// Codec / format not supported by the linked Zig module.
    UnsupportedCodec = 3,
    /// Generic internal error inside Zig.
    InternalError = 4,
}

impl ZigResult {
    /// Convert this C-style result to a Rust `Result`.
    pub fn into_result(self, context: &'static str) -> Result<()> {
        match self {
            Self::Ok => Ok(()),
            Self::OutOfMemory => Err(MediaError::ffi(context, "out of memory")),
            Self::InvalidInput => Err(MediaError::ffi(context, "invalid input")),
            Self::UnsupportedCodec => Err(MediaError::unsupported(context)),
            Self::InternalError => Err(MediaError::ffi(context, "internal error")),
        }
    }
}

// ─── ZigBufferView ───────────────────────────────────────────────────────────

/// Non-owning view into a byte buffer, passed across the FFI boundary.
/// Zig receives this as `[*]const u8` + `usize`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ZigBufferView {
    pub ptr: *const u8,
    pub len: usize,
}

impl ZigBufferView {
    /// Build a view from a Rust slice.
    ///
    /// # Safety
    ///
    /// The slice must outlive every Zig call that uses this view.
    pub unsafe fn from_slice(data: &[u8]) -> Self {
        Self {
            ptr: data.as_ptr(),
            len: data.len(),
        }
    }

    pub const fn null() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub const fn is_null(&self) -> bool {
        self.ptr.is_null()
    }
}

// SAFETY: the view itself is just a pointer + length. Safe to send if
// the data it points to is `Send`. We mark Send/Sync explicitly because
// raw pointers don't implement them automatically.
unsafe impl Send for ZigBufferView {}
unsafe impl Sync for ZigBufferView {}

// ─── ZigBufferOut ─────────────────────────────────────────────────────────────

/// A Zig-allocated output buffer. Must be freed with [`zig_buffer_free`].
#[repr(C)]
#[derive(Debug)]
pub struct ZigBufferOut {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl ZigBufferOut {
    pub fn is_null(&self) -> bool {
        self.ptr.is_null()
    }

    /// View the data as a Rust slice.
    ///
    /// # Safety
    ///
    /// Caller must guarantee the buffer is still valid (not freed).
    pub unsafe fn as_slice(&self) -> &[u8] {
        if self.is_null() || self.len == 0 {
            &[]
        } else {
            // SAFETY: caller's contract guarantees ptr+len is valid and live.
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    /// Copy data into a Rust `Vec<u8>` and free the Zig allocation.
    ///
    /// # Safety
    ///
    /// Must only be called once per buffer. The pointer is invalidated.
    pub unsafe fn into_vec(self) -> Vec<u8> {
        if self.is_null() || self.len == 0 {
            return Vec::new();
        }
        // SAFETY: caller's contract guarantees the buffer is live until we free it below.
        let v = unsafe { self.as_slice() }.to_vec();
        // SAFETY: we own this buffer per the contract; safe to free now.
        unsafe { zig_buffer_free(self) };
        v
    }
}

unsafe impl Send for ZigBufferOut {}
unsafe impl Sync for ZigBufferOut {}

// ─── Raw FFI: real Zig linkage (feature-gated) ───────────────────────────────

#[cfg(feature = "link-zig")]
extern "C" {
    pub fn zig_buffer_free(buf: ZigBufferOut);
    pub fn zig_version() -> *const c_char;
    pub fn zig_probe() -> ZigResult;
}

// ─── Internal stubs (default — no Zig linked) ────────────────────────────────

#[cfg(not(feature = "link-zig"))]
mod stubs {
    use super::{ZigBufferOut, ZigResult};
    use std::ffi::c_char;

    /// Stub: in default mode nothing was allocated by Zig, so this is a no-op.
    ///
    /// # Safety
    /// Signature matches the real Zig symbol; unsafe to mirror the contract.
    pub unsafe fn zig_buffer_free(_buf: ZigBufferOut) {
        // No-op
    }

    /// Stub: returns a static C string identifying this as the stub build.
    ///
    /// # Safety
    /// Returns a pointer to a static `'static` C string.
    pub unsafe fn zig_version() -> *const c_char {
        b"stub-0.0.0\0".as_ptr() as *const c_char
    }

    /// Stub: always reports `Ok`.
    ///
    /// # Safety
    /// Mirrors the unsafe signature of the real Zig symbol.
    pub unsafe fn zig_probe() -> ZigResult {
        ZigResult::Ok
    }
}

#[cfg(not(feature = "link-zig"))]
pub use stubs::{zig_buffer_free, zig_probe, zig_version};

// ─── Safe wrappers ────────────────────────────────────────────────────────────

/// Returns the version string reported by the linked Zig module,
/// or `None` if the module is missing or returned null.
pub fn zig_module_version() -> Option<String> {
    // SAFETY: zig_version returns either a static C string (stub) or
    // a static string owned by the Zig module. Both are safe to read.
    unsafe {
        let ptr = zig_version();
        if ptr.is_null() {
            return None;
        }
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

/// Probe that the Zig runtime is reachable.
/// Call this at startup to verify the native module is correctly linked.
pub fn probe_zig_runtime() -> Result<()> {
    // SAFETY: probe is a no-op call into Zig that returns a status code.
    unsafe { zig_probe().into_result("zig_probe") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zig_result_ok_is_ok() {
        assert!(ZigResult::Ok.into_result("test").is_ok());
    }

    #[test]
    fn zig_result_oom_is_err() {
        let err = ZigResult::OutOfMemory.into_result("test").unwrap_err();
        assert!(err.to_string().contains("out of memory"));
    }

    #[test]
    fn zig_result_unsupported() {
        let err = ZigResult::UnsupportedCodec.into_result("h266").unwrap_err();
        assert!(matches!(err, MediaError::Unsupported(_)));
    }

    #[test]
    fn zig_probe_stub_succeeds() {
        assert!(probe_zig_runtime().is_ok());
    }

    #[test]
    fn zig_version_stub_returns_some() {
        let v = zig_module_version().unwrap();
        assert!(v.starts_with("stub-"));
    }

    #[test]
    fn buffer_view_null_is_null() {
        let view = ZigBufferView::null();
        assert!(view.is_null());
        assert_eq!(view.len, 0);
    }
}
