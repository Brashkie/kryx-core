//! A/V synchronization primitives.

use std::sync::{Arc, atomic::{AtomicI64, Ordering}};
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use crate::error::{Result, SyncError};
use crate::types::{Timestamp, Timebase};

pub const MAX_DRIFT_MS: i64 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSource { Audio, Video, External }

#[derive(Debug)]
pub struct MasterClock {
    pub source: ClockSource,
    pub timebase: Timebase,
    current_pts: AtomicI64,
    wall_ref: Mutex<Option<(Instant, i64)>>,
}

impl MasterClock {
    pub fn new(source: ClockSource, timebase: Timebase) -> Arc<Self> {
        Arc::new(Self { source, timebase, current_pts: AtomicI64::new(i64::MIN), wall_ref: Mutex::new(None) })
    }
    pub fn init(&self, pts: Timestamp) {
        self.current_pts.store(pts.pts(), Ordering::Release);
        *self.wall_ref.lock() = Some((Instant::now(), pts.pts()));
    }
    pub fn update(&self, pts: Timestamp) { if pts.is_valid() { self.current_pts.store(pts.pts(), Ordering::Release); } }
    pub fn current_pts(&self) -> Timestamp { Timestamp::new(self.current_pts.load(Ordering::Acquire)) }
    pub fn is_initialized(&self) -> bool { self.current_pts.load(Ordering::Acquire) != i64::MIN }
    pub fn source(&self) -> ClockSource { self.source }
    pub fn timebase(&self) -> Timebase { self.timebase }
}

#[derive(Debug)]
pub struct StreamClock {
    master: Arc<MasterClock>,
    stream_timebase: Timebase,
    last_pts: AtomicI64,
}

impl StreamClock {
    pub fn new(master: Arc<MasterClock>, stream_timebase: Timebase) -> Self {
        Self { master, stream_timebase, last_pts: AtomicI64::new(i64::MIN) }
    }
    pub fn update(&self, pts: Timestamp) { if pts.is_valid() { self.last_pts.store(pts.pts(), Ordering::Release); } }

    pub fn drift_ms(&self) -> Result<i64> {
        if !self.master.is_initialized() { return Err(SyncError::ClockUninitialized.into()); }
        let stream_pts = self.last_pts.load(Ordering::Acquire);
        if stream_pts == i64::MIN { return Err(SyncError::ClockUninitialized.into()); }
        let rescaled = self.stream_timebase.rescale(Timestamp::new(stream_pts), self.master.timebase());
        let master_pts = self.master.current_pts();
        if !master_pts.is_valid() { return Err(SyncError::ClockUninitialized.into()); }
        let delta = rescaled.pts() - master_pts.pts();
        Ok(delta * self.master.timebase().num as i64 * 1000 / self.master.timebase().den as i64)
    }

    pub fn is_in_sync(&self) -> bool { self.drift_ms().map(|d| d.abs() <= MAX_DRIFT_MS).unwrap_or(false) }
    pub fn should_skip(&self) -> bool { self.drift_ms().map(|d| d < -MAX_DRIFT_MS).unwrap_or(false) }
    pub fn wait_duration(&self) -> Option<Duration> {
        let drift = self.drift_ms().ok()?;
        if drift > 0 { Some(Duration::from_millis(drift as u64)) } else { None }
    }
}
