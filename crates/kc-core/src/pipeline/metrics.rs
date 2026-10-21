//! Per-stage pipeline metrics — the observability layer (Phase 3).
//!
//! Core measures *what it can see* and nothing about meaning. A [`StageMetrics`]
//! counts frames in and out, bytes moved, drops, and cumulative latency, using
//! lock-free atomics so a stage running under `&self` can record without a lock.
//! The pipeline fills these as it runs; [`Pipeline::metrics`] returns a snapshot.
//!
//! This closes the loop opened in v0.2.6: the backpressure hook defined
//! [`LoadMetrics`] but nothing produced them. Now [`StageMetrics::to_load_metrics`]
//! turns real measurements into the generic load snapshot a
//! [`BackpressurePolicy`] reads — core still never interprets the numbers.
//!
//! [`Pipeline::metrics`]: super::Pipeline::metrics
//! [`LoadMetrics`]: super::LoadMetrics
//! [`BackpressurePolicy`]: super::BackpressurePolicy

use std::sync::atomic::{AtomicU64, Ordering};

use super::LoadMetrics;

/// Live, lock-free counters for one stage. Updated as the pipeline runs.
///
/// All reads/writes use `Relaxed` ordering: these are statistics, not
/// synchronization, so the cheapest atomic ordering is the right one. Take a
/// [`StageMetricsSnapshot`] with [`snapshot`](Self::snapshot) for a consistent
/// readout to report or compute on.
#[derive(Debug)]
pub struct StageMetrics {
    frames_in: AtomicU64,
    frames_out: AtomicU64,
    frames_dropped: AtomicU64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    /// Cumulative processing time across all frames, in nanoseconds.
    latency_ns_total: AtomicU64,
    /// Smallest per-frame latency seen (ns). Starts at `u64::MAX` (unset).
    latency_ns_min: AtomicU64,
    /// Largest per-frame latency seen (ns).
    latency_ns_max: AtomicU64,
}

impl Default for StageMetrics {
    fn default() -> Self {
        Self {
            frames_in: AtomicU64::new(0),
            frames_out: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            latency_ns_total: AtomicU64::new(0),
            // Min starts at MAX so the first `fetch_min` records the real value.
            latency_ns_min: AtomicU64::new(u64::MAX),
            latency_ns_max: AtomicU64::new(0),
        }
    }
}

impl StageMetrics {
    /// New, all-zero metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one frame entering the stage, carrying `bytes`.
    #[inline]
    pub fn record_in(&self, bytes: u64) {
        self.frames_in.fetch_add(1, Ordering::Relaxed);
        self.bytes_in.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record `count` frames leaving the stage, carrying `bytes` in total, after
    /// `latency_ns` of processing. When `count` is 0 the frame was dropped.
    #[inline]
    pub fn record_out(&self, count: u64, bytes: u64, latency_ns: u64) {
        if count == 0 {
            self.frames_dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.frames_out.fetch_add(count, Ordering::Relaxed);
            self.bytes_out.fetch_add(bytes, Ordering::Relaxed);
        }
        self.latency_ns_total
            .fetch_add(latency_ns, Ordering::Relaxed);
        // Track the latency spread for jitter. Only meaningful latencies (a
        // processed frame) are recorded; a pure drop passes latency_ns = 0 which
        // we skip so it doesn't collapse the min to zero.
        if latency_ns > 0 {
            self.latency_ns_min.fetch_min(latency_ns, Ordering::Relaxed);
            self.latency_ns_max.fetch_max(latency_ns, Ordering::Relaxed);
        }
    }

    /// Take a consistent-enough snapshot of the current counters.
    pub fn snapshot(&self) -> StageMetricsSnapshot {
        let min_raw = self.latency_ns_min.load(Ordering::Relaxed);
        StageMetricsSnapshot {
            frames_in: self.frames_in.load(Ordering::Relaxed),
            frames_out: self.frames_out.load(Ordering::Relaxed),
            frames_dropped: self.frames_dropped.load(Ordering::Relaxed),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            latency_ns_total: self.latency_ns_total.load(Ordering::Relaxed),
            // Normalize the "unset" sentinel to 0 for consumers.
            latency_ns_min: if min_raw == u64::MAX { 0 } else { min_raw },
            latency_ns_max: self.latency_ns_max.load(Ordering::Relaxed),
        }
    }

    /// Reset every counter to zero (min back to its unset sentinel).
    pub fn reset(&self) {
        self.frames_in.store(0, Ordering::Relaxed);
        self.frames_out.store(0, Ordering::Relaxed);
        self.frames_dropped.store(0, Ordering::Relaxed);
        self.bytes_in.store(0, Ordering::Relaxed);
        self.bytes_out.store(0, Ordering::Relaxed);
        self.latency_ns_total.store(0, Ordering::Relaxed);
        self.latency_ns_min.store(u64::MAX, Ordering::Relaxed);
        self.latency_ns_max.store(0, Ordering::Relaxed);
    }

    /// Produce a generic [`LoadMetrics`] from the live counters, for a
    /// [`BackpressurePolicy`](super::BackpressurePolicy) to read.
    ///
    /// `queue_depth` is `frames_in - frames_out - frames_dropped` (frames still
    /// in flight), `memory_bytes` is bytes retained in flight, and `latency_ms`
    /// is the mean per-frame latency so far. `queue_capacity` is left 0 (unknown)
    /// since core doesn't impose one; set it on the returned value if you have a
    /// bound. Core produces the numbers; it never decides what they mean.
    pub fn to_load_metrics(&self) -> LoadMetrics {
        let s = self.snapshot();
        let in_flight = s
            .frames_in
            .saturating_sub(s.frames_out)
            .saturating_sub(s.frames_dropped);
        let bytes_in_flight = s.bytes_in.saturating_sub(s.bytes_out);
        LoadMetrics::new()
            .with_queue(in_flight.min(u64::from(u32::MAX)) as u32, 0)
            .with_memory(bytes_in_flight)
            .with_latency_ms(s.mean_latency_ms())
            .with_dropped(s.frames_dropped)
    }
}

/// An immutable readout of a [`StageMetrics`] at one instant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageMetricsSnapshot {
    pub frames_in: u64,
    pub frames_out: u64,
    pub frames_dropped: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub latency_ns_total: u64,
    /// Smallest per-frame latency seen (ns); 0 if nothing processed.
    pub latency_ns_min: u64,
    /// Largest per-frame latency seen (ns); 0 if nothing processed.
    pub latency_ns_max: u64,
}

impl StageMetricsSnapshot {
    /// Smallest per-frame latency in milliseconds.
    pub fn min_latency_ms(&self) -> f64 {
        self.latency_ns_min as f64 / 1.0e6
    }

    /// Largest per-frame latency in milliseconds.
    pub fn max_latency_ms(&self) -> f64 {
        self.latency_ns_max as f64 / 1.0e6
    }

    /// Latency spread (max − min) in milliseconds — a simple jitter measure.
    /// A steady stage has near-zero jitter; a bursty one spreads wide.
    pub fn jitter_ms(&self) -> f64 {
        self.max_latency_ms() - self.min_latency_ms()
    }

    /// Mean per-frame processing latency in milliseconds, over frames that
    /// entered the stage. Returns 0.0 when nothing has been processed.
    pub fn mean_latency_ms(&self) -> f64 {
        let n = self.frames_in;
        if n == 0 {
            0.0
        } else {
            (self.latency_ns_total as f64 / n as f64) / 1.0e6
        }
    }

    /// Fan-out ratio: output frames per input frame (1.0 = pass-through,
    /// <1.0 = dropping/merging, >1.0 = splitting). 0.0 when no input yet.
    pub fn fanout(&self) -> f64 {
        if self.frames_in == 0 {
            0.0
        } else {
            self.frames_out as f64 / self.frames_in as f64
        }
    }
}

/// A snapshot of a whole pipeline's metrics: one entry per stage, in order.
#[derive(Debug, Clone, Default)]
pub struct PipelineMetrics {
    /// `(stage_name, snapshot)` for each stage, in pipeline order.
    pub stages: Vec<(&'static str, StageMetricsSnapshot)>,
}

impl PipelineMetrics {
    /// Total frames that entered the first stage.
    pub fn frames_processed(&self) -> u64 {
        self.stages.first().map_or(0, |(_, s)| s.frames_in)
    }

    /// Total frames dropped across every stage.
    pub fn total_dropped(&self) -> u64 {
        self.stages.iter().map(|(_, s)| s.frames_dropped).sum()
    }

    /// Export every stage's snapshot to a caller-supplied [`MetricsSink`].
    ///
    /// Core walks the stages and hands each one to the sink, then flushes. The
    /// sink decides the wire format and destination (Prometheus, StatsD, logs) —
    /// core never depends on any of them. Same boundary as backpressure: the
    /// mechanism lives here, the exporter in the caller's layer.
    pub fn export(&self, sink: &mut impl MetricsSink) {
        for (name, snap) in &self.stages {
            sink.record(name, snap);
        }
        sink.flush();
    }
}

/// A caller-supplied destination for exporting pipeline metrics.
///
/// Implement this to ship metrics anywhere — a Prometheus registry, a StatsD
/// socket, a log line, a test buffer. Core produces the snapshots and calls
/// [`record`](Self::record) once per stage, then [`flush`](Self::flush) once;
/// it never pulls in an exporter dependency of its own.
pub trait MetricsSink {
    /// Receive one stage's name and current snapshot.
    fn record(&mut self, stage: &str, snapshot: &StageMetricsSnapshot);

    /// Called once after all stages in an export pass. Default: no-op.
    fn flush(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_and_out() {
        let m = StageMetrics::new();
        m.record_in(1000);
        m.record_out(1, 900, 2_000_000); // 2 ms
        let s = m.snapshot();
        assert_eq!(s.frames_in, 1);
        assert_eq!(s.frames_out, 1);
        assert_eq!(s.bytes_in, 1000);
        assert_eq!(s.bytes_out, 900);
        assert_eq!(s.mean_latency_ms(), 2.0);
    }

    #[test]
    fn zero_count_is_a_drop() {
        let m = StageMetrics::new();
        m.record_in(500);
        m.record_out(0, 0, 1_000_000); // dropped
        let s = m.snapshot();
        assert_eq!(s.frames_dropped, 1);
        assert_eq!(s.frames_out, 0);
    }

    #[test]
    fn fanout_reflects_split_and_drop() {
        let split = StageMetrics::new();
        split.record_in(100);
        split.record_out(3, 300, 0); // one in, three out
        assert_eq!(split.snapshot().fanout(), 3.0);
    }

    #[test]
    fn to_load_metrics_computes_in_flight() {
        let m = StageMetrics::new();
        m.record_in(1000);
        m.record_in(1000);
        m.record_out(1, 1000, 1_000_000); // one completed
                                          // Two in, one out, none dropped → one still in flight.
        let load = m.to_load_metrics();
        assert_eq!(load.queue_depth, 1);
        assert_eq!(load.memory_bytes, 1000); // 2000 in - 1000 out
        assert_eq!(load.dropped, 0);
    }

    #[test]
    fn reset_clears() {
        let m = StageMetrics::new();
        m.record_in(10);
        m.record_out(1, 10, 100);
        m.reset();
        assert_eq!(m.snapshot(), StageMetricsSnapshot::default());
    }

    #[test]
    fn tracks_latency_min_max_and_jitter() {
        let m = StageMetrics::new();
        m.record_in(10);
        m.record_out(1, 10, 2_000_000); // 2 ms
        m.record_in(10);
        m.record_out(1, 10, 5_000_000); // 5 ms
        m.record_in(10);
        m.record_out(1, 10, 1_000_000); // 1 ms
        let s = m.snapshot();
        assert_eq!(s.min_latency_ms(), 1.0);
        assert_eq!(s.max_latency_ms(), 5.0);
        assert_eq!(s.jitter_ms(), 4.0); // 5 - 1
    }

    #[test]
    fn drops_do_not_collapse_min_latency() {
        let m = StageMetrics::new();
        m.record_in(10);
        m.record_out(1, 10, 3_000_000); // 3 ms processed
        m.record_in(10);
        m.record_out(0, 0, 0); // dropped — latency 0, must not set min to 0
        let s = m.snapshot();
        assert_eq!(s.min_latency_ms(), 3.0);
    }

    #[test]
    fn exports_to_a_sink() {
        // A sink that just collects what core hands it.
        #[derive(Default)]
        struct Collector {
            rows: Vec<(String, u64)>,
            flushed: bool,
        }
        impl MetricsSink for Collector {
            fn record(&mut self, stage: &str, s: &StageMetricsSnapshot) {
                self.rows.push((stage.to_string(), s.frames_in));
            }
            fn flush(&mut self) {
                self.flushed = true;
            }
        }

        let pm = PipelineMetrics {
            stages: vec![
                (
                    "decode",
                    StageMetricsSnapshot {
                        frames_in: 10,
                        ..Default::default()
                    },
                ),
                (
                    "scale",
                    StageMetricsSnapshot {
                        frames_in: 9,
                        ..Default::default()
                    },
                ),
            ],
        };
        let mut sink = Collector::default();
        pm.export(&mut sink);
        assert_eq!(sink.rows, vec![("decode".into(), 10), ("scale".into(), 9)]);
        assert!(sink.flushed);
    }
}
