//! Real-time & streaming primitives (Phase 5).
//!
//! Low-latency, network-aware building blocks, all generic and clock-agnostic so
//! they test without sleeping and carry no codec or transport knowledge:
//!
//! - [`JitterBuffer`] — reorders frames by presentation timestamp and holds a
//!   small target depth, smoothing out arrival jitter before playback.
//! - [`BitrateEstimator`] — a sliding-window bits-per-second estimate; the caller
//!   supplies the clock (seconds), so it's deterministic in tests.
//! - [`RealtimePolicy`] — a [`BackpressurePolicy`] that sheds frames when a
//!   latency deadline is blown or the queue nears full. Concrete timing logic,
//!   no codec specifics — so it belongs in core.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::buffer::MediaBuffer;
use crate::pipeline::{BackpressureDecision, BackpressurePolicy, LoadMetrics};

// ─── JitterBuffer ─────────────────────────────────────────────────────────────

/// What happened to a frame pushed into a [`JitterBuffer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitterPush {
    /// Accepted; the buffer is still below its target depth (keep filling).
    Buffered,
    /// Accepted; the buffer has reached or passed target depth — pop now.
    Ready,
    /// Dropped: its timestamp is at or before the last frame already released
    /// (it arrived too late to be played in order).
    TooLate,
    /// Rejected: the buffer is at capacity — pop before pushing again.
    Full,
}

/// A frame keyed by `(pts, seq)` for stable min-ordering in the heap.
#[derive(Debug)]
struct Keyed {
    pts: i64,
    seq: u64,
    frame: MediaBuffer,
}
impl PartialEq for Keyed {
    fn eq(&self, other: &Self) -> bool {
        self.pts == other.pts && self.seq == other.seq
    }
}
impl Eq for Keyed {}
impl PartialOrd for Keyed {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Keyed {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pts.cmp(&other.pts).then(self.seq.cmp(&other.seq))
    }
}

/// A presentation-timestamp reorder buffer with a target fill depth.
///
/// Frames arrive possibly out of order (network reordering); the buffer holds
/// them in a min-heap by `pts` and releases the earliest once it has buffered
/// `target_depth` frames (the smoothing delay) — or immediately once it hits
/// `capacity`. A frame whose `pts` is at or before the last released frame is
/// too late and is dropped.
#[derive(Debug)]
pub struct JitterBuffer {
    target_depth: usize,
    capacity: usize,
    heap: BinaryHeap<Reverse<Keyed>>,
    seq: u64,
    last_released_pts: Option<i64>,
    dropped_late: u64,
    /// Set once the buffer first reaches target depth. After priming it releases
    /// continuously; it does not re-buffer to target on every pop.
    primed: bool,
}

impl JitterBuffer {
    /// Create a buffer that starts releasing at `target_depth` frames and never
    /// holds more than `capacity`. `capacity` is clamped to at least
    /// `target_depth.max(1)`.
    pub fn new(target_depth: usize, capacity: usize) -> Self {
        let target_depth = target_depth.max(1);
        let capacity = capacity.max(target_depth);
        Self {
            target_depth,
            capacity,
            // Pre-size the heap: we never exceed `capacity`, so it never reallocs.
            heap: BinaryHeap::with_capacity(capacity),
            seq: 0,
            last_released_pts: None,
            dropped_late: 0,
            primed: false,
        }
    }

    /// Push a frame. Frames without a valid pts are ordered by arrival.
    pub fn push(&mut self, frame: MediaBuffer) -> JitterPush {
        let pts = {
            let p = frame.pts();
            if p.is_valid() {
                p.pts()
            } else {
                // No pts → order by arrival after everything seen so far.
                self.last_released_pts.map_or(i64::MIN, |l| l) + 1
            }
        };
        if let Some(last) = self.last_released_pts {
            if pts <= last {
                self.dropped_late += 1;
                return JitterPush::TooLate;
            }
        }
        if self.heap.len() >= self.capacity {
            return JitterPush::Full;
        }
        self.seq += 1;
        self.heap.push(Reverse(Keyed {
            pts,
            seq: self.seq,
            frame,
        }));
        if self.heap.len() >= self.target_depth {
            self.primed = true;
            JitterPush::Ready
        } else {
            JitterPush::Buffered
        }
    }

    /// Release the earliest frame. While still priming (before the buffer first
    /// reached target depth) it returns `None`; once primed it releases
    /// continuously as long as a frame is buffered. A full buffer always
    /// releases, even before priming.
    pub fn pop(&mut self) -> Option<MediaBuffer> {
        if !self.primed && self.heap.len() < self.capacity {
            return None;
        }
        self.heap.pop().map(|Reverse(k)| {
            self.last_released_pts = Some(k.pts);
            k.frame
        })
    }

    /// Drain every buffered frame in presentation order, ignoring target depth.
    pub fn flush(&mut self) -> Vec<MediaBuffer> {
        let mut out = Vec::with_capacity(self.heap.len());
        while let Some(Reverse(k)) = self.heap.pop() {
            self.last_released_pts = Some(k.pts);
            out.push(k.frame);
        }
        out
    }

    /// Frames currently buffered.
    #[inline]
    pub fn len(&self) -> usize {
        self.heap.len()
    }
    /// Whether the buffer holds no frames.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
    /// Whether the buffer is at capacity.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.heap.len() >= self.capacity
    }
    /// How many frames were dropped for arriving too late.
    pub fn dropped_late(&self) -> u64 {
        self.dropped_late
    }
}

// ─── BitrateEstimator ─────────────────────────────────────────────────────────

/// A sliding-window bitrate estimate in bits per second.
///
/// The caller supplies a monotonic time in seconds with each sample, so the
/// estimator is fully deterministic (no wall clock) and testable. Samples older
/// than `window_secs` behind the newest are evicted.
#[derive(Debug)]
pub struct BitrateEstimator {
    window_secs: f64,
    samples: VecDeque<(f64, u64)>,
    bytes_in_window: u64,
}

impl BitrateEstimator {
    /// Create an estimator over a window of `window_secs` (clamped to a small
    /// positive minimum).
    pub fn new(window_secs: f64) -> Self {
        Self {
            window_secs: window_secs.max(1.0e-3),
            samples: VecDeque::new(),
            bytes_in_window: 0,
        }
    }

    /// Record `bytes` observed at time `t_secs`, then evict samples that have
    /// fallen out of the window behind this newest time.
    pub fn sample(&mut self, t_secs: f64, bytes: u64) {
        self.samples.push_back((t_secs, bytes));
        self.bytes_in_window += bytes;
        let cutoff = t_secs - self.window_secs;
        while let Some(&(t, b)) = self.samples.front() {
            if t < cutoff {
                self.bytes_in_window -= b;
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Estimated bitrate in bits per second over the window. Returns 0.0 with no
    /// samples. Uses the actual spanned time while the window is still filling,
    /// then the full window once it's covered — avoiding an early over-estimate.
    pub fn bits_per_sec(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let newest = self.samples.back().map(|&(t, _)| t).unwrap_or(0.0);
        let oldest = self.samples.front().map(|&(t, _)| t).unwrap_or(0.0);
        let span = (newest - oldest).max(self.window_secs).max(1.0e-6);
        (self.bytes_in_window as f64 * 8.0) / span
    }

    /// Total bytes currently inside the window.
    pub fn bytes_in_window(&self) -> u64 {
        self.bytes_in_window
    }
}

// ─── RealtimePolicy ───────────────────────────────────────────────────────────

/// A [`BackpressurePolicy`] for low-latency operation: shed frames when a
/// per-frame latency deadline is exceeded or the queue nears full.
///
/// This is concrete *timing* logic — no codec or perceptual knowledge — so it
/// ships in core as a ready-made policy. (Perceptual / rate-distortion dropping
/// still belongs in a codec layer that reads the generic metrics.)
#[derive(Debug, Clone, Copy)]
pub struct RealtimePolicy {
    /// Per-frame latency budget in ms; above this, start shedding.
    pub max_latency_ms: f64,
    /// Queue fill ratio above which to shed regardless of latency.
    pub shed_queue_ratio: f64,
}

impl RealtimePolicy {
    /// A policy with a latency budget and a queue-fill shed threshold.
    pub fn new(max_latency_ms: f64, shed_queue_ratio: f64) -> Self {
        Self {
            max_latency_ms,
            shed_queue_ratio: shed_queue_ratio.clamp(0.0, 1.0),
        }
    }
}

impl Default for RealtimePolicy {
    fn default() -> Self {
        Self::new(16.0, 0.9) // ~one 60fps frame of latency, shed near-full
    }
}

impl BackpressurePolicy for RealtimePolicy {
    fn decide(&mut self, m: &LoadMetrics) -> BackpressureDecision {
        let queue = m.queue_depth_ratio();
        let over_latency = m.latency_ms > self.max_latency_ms;
        if over_latency || queue > self.shed_queue_ratio {
            // Severity scales with how far past the queue threshold we are.
            let severity = queue.max(if over_latency { 0.5 } else { 0.0 }).min(1.0);
            BackpressureDecision::ShedLoad { severity }
        } else {
            BackpressureDecision::Proceed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MediaType, Timestamp};

    fn frame_pts(pts: i64, bytes: usize) -> MediaBuffer {
        MediaBuffer::builder(MediaType::Video)
            .pts(Timestamp::new(pts))
            .data(vec![0u8; bytes])
            .build()
            .unwrap()
    }

    #[test]
    fn jitter_buffer_reorders_by_pts() {
        let mut jb = JitterBuffer::new(3, 8);
        // Arrive out of order: 30, 10, 20.
        assert_eq!(jb.push(frame_pts(30, 1)), JitterPush::Buffered);
        assert_eq!(jb.push(frame_pts(10, 1)), JitterPush::Buffered);
        assert_eq!(jb.push(frame_pts(20, 1)), JitterPush::Ready); // hit depth 3
                                                                  // Released in pts order: 10, 20, 30.
        assert_eq!(jb.pop().unwrap().pts().pts(), 10);
        assert_eq!(jb.pop().unwrap().pts().pts(), 20);
        assert_eq!(jb.pop().unwrap().pts().pts(), 30);
        assert!(jb.pop().is_none());
    }

    #[test]
    fn jitter_buffer_holds_until_target_depth() {
        let mut jb = JitterBuffer::new(3, 8);
        jb.push(frame_pts(10, 1));
        jb.push(frame_pts(20, 1));
        // Only 2 buffered, target is 3 → not ready yet.
        assert!(jb.pop().is_none());
        jb.push(frame_pts(30, 1));
        assert!(jb.pop().is_some());
    }

    #[test]
    fn jitter_buffer_drops_late_frames() {
        let mut jb = JitterBuffer::new(1, 8);
        jb.push(frame_pts(10, 1));
        jb.pop(); // releases pts 10, last_released = 10
                  // A frame at pts 5 is now too late.
        assert_eq!(jb.push(frame_pts(5, 1)), JitterPush::TooLate);
        assert_eq!(jb.dropped_late(), 1);
    }

    #[test]
    fn jitter_buffer_reports_full() {
        let mut jb = JitterBuffer::new(1, 2);
        assert_eq!(jb.push(frame_pts(10, 1)), JitterPush::Ready);
        assert_eq!(jb.push(frame_pts(20, 1)), JitterPush::Ready);
        assert_eq!(jb.push(frame_pts(30, 1)), JitterPush::Full); // at capacity
    }

    #[test]
    fn bitrate_estimator_sliding_window() {
        let mut est = BitrateEstimator::new(1.0); // 1 second window
                                                  // 1000 bytes at t=0, 1000 at t=0.5 → 2000 bytes in window.
        est.sample(0.0, 1000);
        est.sample(0.5, 1000);
        assert_eq!(est.bytes_in_window(), 2000);
        // 2000 bytes * 8 / 1.0s window = 16000 bps.
        assert!((est.bits_per_sec() - 16000.0).abs() < 1.0);
    }

    #[test]
    fn bitrate_estimator_evicts_old_samples() {
        let mut est = BitrateEstimator::new(1.0);
        est.sample(0.0, 5000); // this will fall out
        est.sample(2.0, 1000); // 2s later → evicts the t=0 sample
        assert_eq!(est.bytes_in_window(), 1000);
    }

    #[test]
    fn realtime_policy_sheds_over_latency() {
        let mut p = RealtimePolicy::new(16.0, 0.9);
        // Low latency, empty queue → proceed.
        let calm = LoadMetrics::new().with_latency_ms(5.0);
        assert_eq!(p.decide(&calm), BackpressureDecision::Proceed);
        // Latency over budget → shed.
        let late = LoadMetrics::new().with_latency_ms(40.0);
        assert!(late_is_shedding(&mut p, &late));
        // Queue near full → shed.
        let full = LoadMetrics::new().with_queue(95, 100);
        assert!(late_is_shedding(&mut p, &full));
    }

    fn late_is_shedding(p: &mut RealtimePolicy, m: &LoadMetrics) -> bool {
        matches!(p.decide(m), BackpressureDecision::ShedLoad { .. })
    }
}
