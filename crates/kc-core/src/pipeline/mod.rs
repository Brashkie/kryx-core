//! Async composable media processing pipelines.

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::buffer::MediaBuffer;
use crate::error::{PipelineError, Result};
use crate::types::MediaType;

// ─── StageContext ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StageContext {
    pub pipeline_name: Arc<str>,
    pub frame_count: u64,
    pub started_at: Instant,
}

impl StageContext {
    fn new(name: impl Into<Arc<str>>) -> Self {
        Self { pipeline_name: name.into(), frame_count: 0, started_at: Instant::now() }
    }
    pub fn elapsed_ms(&self) -> u128 { self.started_at.elapsed().as_millis() }
}

// ─── Stage ───────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Stage: Send + Sync + fmt::Debug {
    fn name(&self) -> &'static str;
    async fn process(&self, buf: MediaBuffer, ctx: &StageContext) -> Result<Vec<MediaBuffer>>;
    async fn on_start(&self) -> Result<()> { Ok(()) }
    async fn on_stop(&self)  -> Result<()> { Ok(()) }
    fn accepts(&self) -> Option<&[MediaType]> { None }
}

// ─── Pipeline ────────────────────────────────────────────────────────────────

pub struct Pipeline {
    name: Arc<str>,
    stages: Vec<Arc<dyn Stage>>,
    ctx: RwLock<StageContext>,
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("name", &self.name)
            .field("stages", &self.stages.iter().map(|s| s.name()).collect::<Vec<_>>())
            .finish()
    }
}

impl Pipeline {
    pub fn builder() -> PipelineBuilder { PipelineBuilder::new() }
    pub fn name(&self) -> &str { &self.name }
    pub fn stage_count(&self) -> usize { self.stages.len() }
    pub fn stage_names(&self) -> Vec<&str> { self.stages.iter().map(|s| s.name()).collect() }

    pub async fn start(&self) -> Result<()> {
        for s in &self.stages { s.on_start().await?; }
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        for s in self.stages.iter().rev() { s.on_stop().await?; }
        Ok(())
    }

    pub async fn process(&self, buf: MediaBuffer) -> Result<Vec<MediaBuffer>> {
        if self.stages.is_empty() { return Err(PipelineError::Empty.into()); }

        let ctx = {
            let mut ctx = self.ctx.write();
            ctx.frame_count += 1;
            ctx.clone()
        };

        let mut current = vec![buf];
        for stage in &self.stages {
            let mut next = Vec::new();
            for frame in current {
                next.extend(stage.process(frame, &ctx).await?);
            }
            current = next;
            if current.is_empty() { break; }
        }
        Ok(current)
    }

    pub async fn process_batch(&self, bufs: Vec<MediaBuffer>) -> Result<Vec<MediaBuffer>> {
        let mut out = Vec::new();
        for buf in bufs { out.extend(self.process(buf).await?); }
        Ok(out)
    }
}

// ─── PipelineBuilder ─────────────────────────────────────────────────────────

pub struct PipelineBuilder {
    name: String,
    stages: Vec<Arc<dyn Stage>>,
}

impl PipelineBuilder {
    fn new() -> Self { Self { name: "pipeline".into(), stages: Vec::new() } }

    #[must_use] pub fn name(mut self, n: impl Into<String>) -> Self { self.name = n.into(); self }
    #[must_use] pub fn stage(mut self, s: impl Stage + 'static) -> Self { self.stages.push(Arc::new(s)); self }
    #[must_use] pub fn stage_arc(mut self, s: Arc<dyn Stage>) -> Self { self.stages.push(s); self }

    pub fn build(self) -> Result<Pipeline> {
        if self.stages.is_empty() { return Err(PipelineError::Empty.into()); }
        let name: Arc<str> = Arc::from(self.name.as_str());
        Ok(Pipeline {
            ctx: RwLock::new(StageContext::new(Arc::clone(&name))),
            name,
            stages: self.stages,
        })
    }
}

// ─── Built-in stages ─────────────────────────────────────────────────────────

#[derive(Debug)] pub struct PassthroughStage;
#[derive(Debug)] pub struct DropStage;

#[async_trait]
impl Stage for PassthroughStage {
    fn name(&self) -> &'static str { "passthrough" }
    async fn process(&self, buf: MediaBuffer, _: &StageContext) -> Result<Vec<MediaBuffer>> {
        Ok(vec![buf])
    }
}

#[async_trait]
impl Stage for DropStage {
    fn name(&self) -> &'static str { "drop" }
    async fn process(&self, _: MediaBuffer, _: &StageContext) -> Result<Vec<MediaBuffer>> {
        Ok(vec![])
    }
}

#[derive(Debug, Default)]
pub struct CounterStage { count: std::sync::atomic::AtomicU64 }

impl CounterStage {
    pub fn new() -> Self { Self::default() }
    pub fn count(&self) -> u64 { self.count.load(std::sync::atomic::Ordering::Relaxed) }
}

#[async_trait]
impl Stage for CounterStage {
    fn name(&self) -> &'static str { "counter" }
    async fn process(&self, buf: MediaBuffer, _: &StageContext) -> Result<Vec<MediaBuffer>> {
        self.count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(vec![buf])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MediaType;

    fn buf() -> MediaBuffer { MediaBuffer::builder(MediaType::Video).data(vec![0u8; 32]).build().unwrap() }

    #[tokio::test] async fn passthrough() {
        let p = Pipeline::builder().stage(PassthroughStage).build().unwrap();
        assert_eq!(p.process(buf()).await.unwrap().len(), 1);
    }

    #[tokio::test] async fn drop_stage() {
        let p = Pipeline::builder().stage(DropStage).build().unwrap();
        assert!(p.process(buf()).await.unwrap().is_empty());
    }

    #[tokio::test] async fn empty_pipeline_errors() {
        assert!(Pipeline::builder().build().is_err());
    }
}
