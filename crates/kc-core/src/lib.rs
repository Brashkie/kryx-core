//! # kc-core
//!
//! Pure Rust core logic for the `@kryxjs/core` package.
//! No Node.js dependencies — usable standalone in any Rust project.

pub mod buffer;
pub mod error;
pub mod ffi;
pub mod io;
pub mod pipeline;
pub mod sync;
pub mod types;
pub mod utils;

pub use buffer::{
    acquire_guard, shared_pool, BufferPool, MediaBuffer, PoolGuard, PoolStats, SharedPool,
};
pub use error::{MediaError, Result};
pub use pipeline::Pipeline;
pub use types::{MediaType, Timestamp};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod smoke_tests {
    //! Cross-module smoke tests — verify that all public APIs link together.

    use super::*;
    use crate::buffer::{FrameFlags, MediaBuffer};
    use crate::pipeline::{PassthroughStage, Pipeline};
    use crate::types::{CodecId, MediaType, Timebase, Timestamp};

    #[test]
    fn version_constant_set() {
        assert!(!VERSION.is_empty());
    }

    #[tokio::test]
    async fn end_to_end_pipeline_flow() {
        let buf = MediaBuffer::builder(MediaType::Video)
            .codec(CodecId::H264)
            .pts(Timestamp::new(90_000))
            .timebase(Timebase::VIDEO_90K)
            .flags(FrameFlags::KEYFRAME)
            .data(vec![0xAB_u8; 256])
            .build()
            .expect("buffer build");

        let pipeline = Pipeline::builder()
            .name("smoke")
            .stage(PassthroughStage)
            .build()
            .expect("pipeline build");

        let out = pipeline.process(buf).await.expect("process");
        assert_eq!(out.len(), 1);
        assert!(out[0].is_keyframe());
    }
}
