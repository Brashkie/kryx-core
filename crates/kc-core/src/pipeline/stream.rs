//! Streaming pipeline execution — concurrent stages over bounded queues.
//!
//! [`Pipeline::process`](super::Pipeline::process) runs the stages *sequentially*
//! for one frame at a time. [`StreamPipeline`] instead runs **every stage as its
//! own concurrent task**, connected by **bounded** channels, so while stage 2
//! works on frame *n*, stage 1 is already on frame *n+1* — true pipeline
//! parallelism. This is the Phase 2 execution model, and it ties together
//! everything built in 0.2.x:
//!
//! - **Bounded queues** between stages give natural producer/consumer flow: a
//!   fast stage can't outrun a slow one, because a full queue makes its `send`
//!   wait (the queue capacity is the buffering budget).
//! - **Backpressure becomes real.** Before a stage processes a frame it derives
//!   generic [`LoadMetrics`](super::LoadMetrics) from its live counters and asks
//!   the caller's [`BackpressurePolicy`](super::BackpressurePolicy) what to do —
//!   `ShedLoad` drops the frame, `Pause` waits. Core still never interprets the
//!   numbers; the policy (in a higher layer) decides.
//! - **Metrics** ([`StageMetrics`]) are fed exactly as in `process`.
//! - **Multi-source**: several input streams are muxed into the first queue.
//!
//! # Example
//!
//! ```
//! # use kc_core::pipeline::{StreamPipeline, PassthroughStage};
//! # use kc_core::buffer::MediaBuffer;
//! # use kc_core::types::MediaType;
//! # async fn demo() {
//! let f = || MediaBuffer::builder(MediaType::Video).data(vec![0u8; 16]).build().unwrap();
//! let sp = StreamPipeline::builder()
//!     .capacity(32)             // bounded queue depth between stages
//!     .stage(PassthroughStage)
//!     .build()
//!     .unwrap();
//!
//! // Two sources, muxed into the pipeline, run concurrently through the stage.
//! let out = sp.run_to_vec(vec![vec![f(), f()], vec![f()]]).await.unwrap();
//! assert_eq!(out.len(), 3);
//! # }
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::sync::mpsc;

use super::{
    BackpressureDecision, BackpressurePolicy, NoBackpressure, PipelineMetrics, Stage, StageContext,
    StageMetrics,
};
use crate::buffer::MediaBuffer;
use crate::error::{PipelineError, Result};

/// Default bounded-queue capacity between stages.
const DEFAULT_CAPACITY: usize = 64;
/// How long a `Pause` decision backs off before re-checking.
const PAUSE_BACKOFF: Duration = Duration::from_micros(100);

/// A pipeline that runs its stages concurrently over bounded queues.
pub struct StreamPipeline {
    name: Arc<str>,
    stages: Vec<Arc<dyn Stage>>,
    metrics: Arc<Vec<StageMetrics>>,
    capacity: usize,
    policy: Arc<Mutex<dyn BackpressurePolicy>>,
}

impl std::fmt::Debug for StreamPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamPipeline")
            .field("name", &self.name)
            .field(
                "stages",
                &self.stages.iter().map(|s| s.name()).collect::<Vec<_>>(),
            )
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl StreamPipeline {
    /// Start building a streaming pipeline.
    pub fn builder() -> StreamPipelineBuilder {
        StreamPipelineBuilder::new()
    }

    /// The bounded-queue capacity between stages.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Snapshot the per-stage metrics gathered so far.
    pub fn metrics(&self) -> PipelineMetrics {
        PipelineMetrics {
            stages: self
                .stages
                .iter()
                .zip(self.metrics.iter())
                .map(|(stage, m)| (stage.name(), m.snapshot()))
                .collect(),
        }
    }

    /// Run the pipeline to completion over one or more sources, muxed into the
    /// first stage, and collect the output.
    ///
    /// Each stage runs as its own task joined by bounded channels; sources are
    /// fed by concurrent producer tasks. Returns once every source is exhausted
    /// and all frames have drained through. A stage that errors on a frame drops
    /// that frame (counted in metrics) and keeps running — one bad frame doesn't
    /// tear down the stream.
    pub async fn run_to_vec(&self, sources: Vec<Vec<MediaBuffer>>) -> Result<Vec<MediaBuffer>> {
        let n = self.stages.len();
        if n == 0 {
            return Err(PipelineError::Empty.into());
        }

        // n+1 bounded channels: chan[0] feeds stage 0, chan[i] feeds stage i,
        // chan[n] is the output. Wrapped in Option so receivers can be moved out.
        let mut txs = Vec::with_capacity(n + 1);
        let mut rxs = Vec::with_capacity(n + 1);
        for _ in 0..=n {
            let (tx, rx) = mpsc::channel::<MediaBuffer>(self.capacity);
            txs.push(tx);
            rxs.push(Some(rx));
        }

        let base_ctx = StageContext::new(Arc::clone(&self.name));
        let mut output_rx = rxs[n].take().expect("output receiver present");

        // Spawn one task per stage.
        let mut handles = Vec::with_capacity(n);
        for i in 0..n {
            let stage = Arc::clone(&self.stages[i]);
            let metrics = Arc::clone(&self.metrics);
            let policy = Arc::clone(&self.policy);
            let ctx = base_ctx.clone();
            let capacity = self.capacity;
            let mut rx = rxs[i].take().expect("stage input receiver present");
            let tx = txs[i + 1].clone();

            handles.push(tokio::spawn(async move {
                let mut frame_count = 0u64;
                while let Some(frame) = rx.recv().await {
                    frame_count += 1;
                    let m = &metrics[i];
                    m.record_in(frame.len() as u64);

                    // Generic backpressure check: the stage's own load, plus the
                    // queue capacity, handed to the caller-supplied policy.
                    let mut load = m.to_load_metrics();
                    load.queue_capacity = capacity.min(u32::MAX as usize) as u32;
                    // Take the decision into a binding so the (non-Send) lock
                    // guard drops here — never held across the `.await` below.
                    let decision = policy.lock().decide(&load);
                    match decision {
                        BackpressureDecision::ShedLoad { .. } => {
                            m.record_out(0, 0, 0); // shed under pressure
                            continue;
                        }
                        BackpressureDecision::Pause => {
                            tokio::time::sleep(PAUSE_BACKOFF).await;
                        }
                        BackpressureDecision::Proceed => {}
                    }

                    let mut ctx_i = ctx.clone();
                    ctx_i.frame_count = frame_count;
                    let started = Instant::now();
                    let produced = match stage.process(frame, &ctx_i).await {
                        Ok(v) => v,
                        Err(_) => {
                            m.record_out(0, 0, started.elapsed().as_nanos() as u64);
                            continue;
                        }
                    };
                    let latency_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                    let out_bytes: u64 = produced.iter().map(|b| b.len() as u64).sum();
                    m.record_out(produced.len() as u64, out_bytes, latency_ns);

                    for out in produced {
                        if tx.send(out).await.is_err() {
                            return; // downstream closed — stop
                        }
                    }
                }
            }));
        }

        // Keep the input sender; drop every other original so each channel closes
        // once the task feeding it finishes (each stage task holds its own clone).
        let input_tx = txs[0].clone();
        drop(txs);

        // Mux sources: one producer task per source, all into the first queue.
        let mut feeders = Vec::with_capacity(sources.len());
        for src in sources {
            let tx = input_tx.clone();
            feeders.push(tokio::spawn(async move {
                for frame in src {
                    if tx.send(frame).await.is_err() {
                        break;
                    }
                }
            }));
        }
        drop(input_tx); // our copy gone → chan[0] closes when all feeders finish

        // Drain the output as it arrives.
        let mut out = Vec::new();
        while let Some(frame) = output_rx.recv().await {
            out.push(frame);
        }

        for f in feeders {
            let _ = f.await;
        }
        for h in handles {
            let _ = h.await;
        }
        Ok(out)
    }
}

/// Builder for [`StreamPipeline`].
pub struct StreamPipelineBuilder {
    name: String,
    stages: Vec<Arc<dyn Stage>>,
    capacity: usize,
    policy: Arc<Mutex<dyn BackpressurePolicy>>,
}

impl StreamPipelineBuilder {
    fn new() -> Self {
        Self {
            name: "stream".into(),
            stages: Vec::new(),
            capacity: DEFAULT_CAPACITY,
            policy: Arc::new(Mutex::new(NoBackpressure)),
        }
    }

    /// Name the pipeline (shows up in `StageContext`).
    #[must_use]
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = n.into();
        self
    }

    /// Set the bounded-queue capacity between stages (the buffering budget).
    #[must_use]
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Append a stage.
    #[must_use]
    pub fn stage(mut self, s: impl Stage + 'static) -> Self {
        self.stages.push(Arc::new(s));
        self
    }

    /// Append a stage already behind an `Arc`.
    #[must_use]
    pub fn stage_arc(mut self, s: Arc<dyn Stage>) -> Self {
        self.stages.push(s);
        self
    }

    /// Install a caller-supplied backpressure policy. The policy lives in the
    /// caller's layer and decides what the generic load numbers mean.
    #[must_use]
    pub fn policy(mut self, p: impl BackpressurePolicy + 'static) -> Self {
        self.policy = Arc::new(Mutex::new(p));
        self
    }

    /// Finish building.
    pub fn build(self) -> Result<StreamPipeline> {
        if self.stages.is_empty() {
            return Err(PipelineError::Empty.into());
        }
        let name: Arc<str> = Arc::from(self.name.as_str());
        let metrics = (0..self.stages.len())
            .map(|_| StageMetrics::new())
            .collect();
        Ok(StreamPipeline {
            name,
            stages: self.stages,
            metrics: Arc::new(metrics),
            capacity: self.capacity,
            policy: self.policy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{DropStage, LoadMetrics, PassthroughStage};
    use crate::types::MediaType;

    fn frame(n: usize) -> MediaBuffer {
        MediaBuffer::builder(MediaType::Video)
            .data(vec![0u8; n])
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn streams_frames_through_a_stage() {
        let sp = StreamPipeline::builder()
            .stage(PassthroughStage)
            .build()
            .unwrap();
        let out = sp
            .run_to_vec(vec![vec![frame(16), frame(16), frame(16)]])
            .await
            .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(sp.metrics().frames_processed(), 3);
    }

    #[tokio::test]
    async fn muxes_multiple_sources() {
        let sp = StreamPipeline::builder()
            .stage(PassthroughStage)
            .build()
            .unwrap();
        // Three sources with 2 + 1 + 3 frames → 6 out.
        let out = sp
            .run_to_vec(vec![
                vec![frame(8), frame(8)],
                vec![frame(8)],
                vec![frame(8), frame(8), frame(8)],
            ])
            .await
            .unwrap();
        assert_eq!(out.len(), 6);
    }

    #[tokio::test]
    async fn two_stages_run_as_a_pipeline() {
        let sp = StreamPipeline::builder()
            .capacity(4)
            .stage(PassthroughStage)
            .stage(DropStage) // second stage drops everything
            .build()
            .unwrap();
        let out = sp.run_to_vec(vec![vec![frame(8), frame(8)]]).await.unwrap();
        assert!(out.is_empty());
        let m = sp.metrics();
        assert_eq!(m.stages[0].1.frames_out, 2); // passthrough emitted 2
        assert_eq!(m.stages[1].1.frames_dropped, 2); // drop shed 2
    }

    #[tokio::test]
    async fn policy_sheds_under_pressure() {
        // A policy that always sheds — every frame should be dropped.
        #[derive(Debug)]
        struct AlwaysShed;
        impl BackpressurePolicy for AlwaysShed {
            fn decide(&mut self, _m: &LoadMetrics) -> BackpressureDecision {
                BackpressureDecision::ShedLoad { severity: 1.0 }
            }
        }
        let sp = StreamPipeline::builder()
            .stage(PassthroughStage)
            .policy(AlwaysShed)
            .build()
            .unwrap();
        let out = sp.run_to_vec(vec![vec![frame(8), frame(8)]]).await.unwrap();
        assert!(out.is_empty());
        assert_eq!(sp.metrics().total_dropped(), 2);
    }

    #[tokio::test]
    async fn empty_pipeline_errors() {
        assert!(StreamPipeline::builder().build().is_err());
    }
}
