//! Unified I/O abstractions — source and sink traits.

use std::fmt;
use async_trait::async_trait;
use crate::buffer::MediaBuffer;
use crate::error::Result;
use crate::types::StreamInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekMode { Backward, Forward, Exact }

#[async_trait]
pub trait MediaSource: Send + Sync + fmt::Debug {
    fn name(&self) -> &str;
    async fn open(&mut self)  -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    async fn read(&mut self)  -> Result<Option<MediaBuffer>>;
    async fn seek(&mut self, position: i64, mode: SeekMode) -> Result<()>;
    fn streams(&self) -> &[StreamInfo];
    fn is_seekable(&self) -> bool { false }
    fn duration_us(&self) -> Option<i64> { None }
}

#[async_trait]
pub trait MediaSink: Send + Sync + fmt::Debug {
    fn name(&self) -> &str;
    async fn open(&mut self)  -> Result<()>;
    async fn close(&mut self) -> Result<()>;
    async fn write(&mut self, buf: MediaBuffer) -> Result<()>;
    async fn flush(&mut self) -> Result<()>;
}

// ─── In-memory implementations (testing / pipelines) ─────────────────────────

#[derive(Debug)]
pub struct MemorySource {
    name: String,
    buffers: std::collections::VecDeque<MediaBuffer>,
    streams: Vec<StreamInfo>,
}

impl MemorySource {
    pub fn new(name: impl Into<String>, buffers: Vec<MediaBuffer>) -> Self {
        Self { name: name.into(), buffers: buffers.into(), streams: vec![] }
    }
    pub fn remaining(&self) -> usize { self.buffers.len() }
}

#[async_trait]
impl MediaSource for MemorySource {
    fn name(&self) -> &str { &self.name }
    async fn open(&mut self)  -> Result<()> { Ok(()) }
    async fn close(&mut self) -> Result<()> { self.buffers.clear(); Ok(()) }
    async fn read(&mut self)  -> Result<Option<MediaBuffer>> { Ok(self.buffers.pop_front()) }
    async fn seek(&mut self, _: i64, _: SeekMode) -> Result<()> { Ok(()) }
    fn streams(&self) -> &[StreamInfo] { &self.streams }
}

#[derive(Debug, Default)]
pub struct MemorySink { buffers: Vec<MediaBuffer> }

impl MemorySink {
    pub fn new() -> Self { Self::default() }
    pub fn buffers(&self) -> &[MediaBuffer] { &self.buffers }
    pub fn take(&mut self) -> Vec<MediaBuffer> { std::mem::take(&mut self.buffers) }
}

#[async_trait]
impl MediaSink for MemorySink {
    fn name(&self) -> &str { "memory-sink" }
    async fn open(&mut self)  -> Result<()> { Ok(()) }
    async fn close(&mut self) -> Result<()> { Ok(()) }
    async fn write(&mut self, buf: MediaBuffer) -> Result<()> { self.buffers.push(buf); Ok(()) }
    async fn flush(&mut self) -> Result<()> { Ok(()) }
}
