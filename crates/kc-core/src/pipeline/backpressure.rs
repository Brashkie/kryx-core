//! Generic backpressure signalling for pipelines.
//!
//! Core provides the *mechanism*, never the *policy*. A stage reports how loaded
//! it is as [`LoadMetrics`] — queue depth, retained memory, latency — in units
//! core understands (counts, bytes, milliseconds). A caller in a higher layer
//! (a codec, an encoder, an application) implements [`BackpressurePolicy`] to
//! turn those raw numbers into a [`BackpressureDecision`].
//!
//! This keeps the Kryx layering intact: `kc-core` knows saturation, not meaning.
//! A codec that wants rate-distortion or perceptual (JND) control reads these
//! generic metrics and decides *inside the codec* what to prune or how to
//! requantize — core never learns what a dictionary atom or a perceptual
//! threshold is. Interpretation rises to the layer that has the knowledge.
//!
//! # Example
//!
//! ```
//! use kc_core::pipeline::{BackpressureDecision, BackpressurePolicy, LoadMetrics};
//!
//! // A policy lives in the caller's layer, not in core.
//! struct DropWhenFull;
//! impl BackpressurePolicy for DropWhenFull {
//!     fn decide(&mut self, m: &LoadMetrics) -> BackpressureDecision {
//!         if m.queue_depth_ratio() > 0.8 {
//!             BackpressureDecision::ShedLoad { severity: 1.0 }
//!         } else {
//!             BackpressureDecision::Proceed
//!         }
//!     }
//! }
//!
//! let mut policy = DropWhenFull;
//! let metrics = LoadMetrics::new().with_queue(90, 100);
//! assert!(matches!(
//!     policy.decide(&metrics),
//!     BackpressureDecision::ShedLoad { .. }
//! ));
//! ```

/// A snapshot of how loaded a stage or pipeline is, in units core understands.
///
/// All fields are raw, uninterpreted measurements. Core fills what it can
/// observe; a stage may set more via the builder methods. Nothing here carries
/// any notion of quality, perception, or codec semantics — those live in the
/// layer that reads this.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LoadMetrics {
    /// Items currently queued ahead of this stage.
    pub queue_depth: u32,
    /// Capacity of that queue (0 means "unbounded / unknown").
    pub queue_capacity: u32,
    /// Bytes currently retained in buffers (e.g. a pool's `memory_used()`).
    pub memory_bytes: u64,
    /// Most recent per-frame processing latency, in milliseconds.
    pub latency_ms: f64,
    /// Frames dropped so far (whatever the caller's policy chose to drop).
    pub dropped: u64,
}

impl LoadMetrics {
    /// An empty snapshot (all zero).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set queue depth and capacity.
    pub fn with_queue(mut self, depth: u32, capacity: u32) -> Self {
        self.queue_depth = depth;
        self.queue_capacity = capacity;
        self
    }

    /// Set retained memory in bytes.
    pub fn with_memory(mut self, bytes: u64) -> Self {
        self.memory_bytes = bytes;
        self
    }

    /// Set the most recent per-frame latency in milliseconds.
    pub fn with_latency_ms(mut self, ms: f64) -> Self {
        self.latency_ms = ms;
        self
    }

    /// Set the running dropped-frame count.
    pub fn with_dropped(mut self, dropped: u64) -> Self {
        self.dropped = dropped;
        self
    }

    /// Queue fill as a ratio in `[0.0, 1.0]`, or `0.0` when capacity is unknown.
    ///
    /// This is the single most useful derived signal for a policy: how close the
    /// queue is to saturation, without the policy needing to know the capacity.
    pub fn queue_depth_ratio(&self) -> f64 {
        if self.queue_capacity == 0 {
            0.0
        } else {
            (self.queue_depth as f64 / self.queue_capacity as f64).min(1.0)
        }
    }
}

/// What a [`BackpressurePolicy`] decides the pipeline should do next.
///
/// Core carries this verdict back to whoever is driving the pipeline; core does
/// not act on quality itself. `ShedLoad`'s `severity` is a normalized dial the
/// caller interprets (drop more, quantize harder, raise a perceptual threshold —
/// core does not define which).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackpressureDecision {
    /// Keep going at full fidelity.
    Proceed,
    /// Reduce load. `severity` in `[0.0, 1.0]` says how aggressively; its
    /// meaning (frame-drop, requantization, sparsity) belongs to the caller.
    ShedLoad { severity: f64 },
    /// Pause intake until load falls; the caller decides for how long.
    Pause,
}

impl BackpressureDecision {
    /// True when this decision asks for any load reduction.
    pub fn is_shedding(&self) -> bool {
        !matches!(self, BackpressureDecision::Proceed)
    }
}

/// A caller-supplied rule that turns raw [`LoadMetrics`] into a
/// [`BackpressureDecision`].
///
/// Core never implements this with any perceptual or codec-specific logic — it
/// only defines the trait so a higher layer can plug one in. The method takes
/// `&mut self` so a policy may keep state across calls (a smoothed average, a
/// Lagrange multiplier, a hysteresis band — all of which live in the caller).
pub trait BackpressurePolicy: Send + Sync {
    /// Decide what to do given the current load.
    fn decide(&mut self, metrics: &LoadMetrics) -> BackpressureDecision;
}

/// A trivial policy that always proceeds — the default when no backpressure is
/// wanted. Useful as a null object so call sites don't special-case `None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoBackpressure;

impl BackpressurePolicy for NoBackpressure {
    fn decide(&mut self, _metrics: &LoadMetrics) -> BackpressureDecision {
        BackpressureDecision::Proceed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_ratio_is_bounded_and_safe() {
        assert_eq!(LoadMetrics::new().queue_depth_ratio(), 0.0); // unknown cap
        assert_eq!(
            LoadMetrics::new().with_queue(50, 100).queue_depth_ratio(),
            0.5
        );
        // Over-full never exceeds 1.0.
        assert_eq!(
            LoadMetrics::new().with_queue(200, 100).queue_depth_ratio(),
            1.0
        );
    }

    #[test]
    fn builder_sets_fields() {
        let m = LoadMetrics::new()
            .with_queue(10, 20)
            .with_memory(4096)
            .with_latency_ms(2.5)
            .with_dropped(3);
        assert_eq!(m.queue_depth, 10);
        assert_eq!(m.queue_capacity, 20);
        assert_eq!(m.memory_bytes, 4096);
        assert_eq!(m.latency_ms, 2.5);
        assert_eq!(m.dropped, 3);
    }

    #[test]
    fn no_backpressure_always_proceeds() {
        let mut p = NoBackpressure;
        let busy = LoadMetrics::new().with_queue(99, 100);
        assert_eq!(p.decide(&busy), BackpressureDecision::Proceed);
    }

    #[test]
    fn decision_is_shedding_reports_correctly() {
        assert!(!BackpressureDecision::Proceed.is_shedding());
        assert!(BackpressureDecision::ShedLoad { severity: 0.5 }.is_shedding());
        assert!(BackpressureDecision::Pause.is_shedding());
    }

    #[test]
    fn a_caller_policy_reads_generic_metrics_only() {
        // Models a codec-layer policy: it reads ONLY generic numbers (queue
        // ratio, memory) — never anything core doesn't expose. Proves the
        // boundary: core needs no codec knowledge for this to work.
        struct SheddingPolicy {
            budget_bytes: u64,
        }
        impl BackpressurePolicy for SheddingPolicy {
            fn decide(&mut self, m: &LoadMetrics) -> BackpressureDecision {
                let pressure = m.queue_depth_ratio();
                if m.memory_bytes > self.budget_bytes || pressure > 0.9 {
                    BackpressureDecision::ShedLoad {
                        severity: pressure.max(0.5),
                    }
                } else if pressure > 0.7 {
                    BackpressureDecision::ShedLoad { severity: 0.25 }
                } else {
                    BackpressureDecision::Proceed
                }
            }
        }

        let mut policy = SheddingPolicy {
            budget_bytes: 1 << 20,
        };
        assert_eq!(
            policy.decide(&LoadMetrics::new().with_queue(10, 100)),
            BackpressureDecision::Proceed
        );
        assert!(policy
            .decide(&LoadMetrics::new().with_queue(95, 100))
            .is_shedding());
        assert!(policy
            .decide(&LoadMetrics::new().with_memory(2 << 20))
            .is_shedding());
    }
}
