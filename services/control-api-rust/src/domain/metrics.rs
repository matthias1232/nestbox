//! A sandbox's resource use over time.
//!
//! The runner samples `docker stats` every five seconds and posts a batch. The
//! control API stores the raw points for a day and a one-minute rollup for a
//! week. The types here are the sample, the rollup bucket, and the query that
//! picks between them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// One `docker stats` sample.
///
/// `cpu_pct` is the container's share of all host cores, as Docker reports it
/// (a container pinned to one core of four reads 25 at full load). Memory and
/// I/O are cumulative byte counts except `mem_bytes`, which is the current
/// usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetricSample {
    pub sandbox_id: Uuid,
    pub ts: DateTime<Utc>,
    pub cpu_pct: f32,
    pub mem_bytes: i64,
    pub mem_limit: i64,
    pub net_rx: i64,
    pub net_tx: i64,
    pub blk_read: i64,
    pub blk_write: i64,
    pub pids: i32,
}

/// A one-minute aggregate of samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetricBucket {
    pub sandbox_id: Uuid,
    pub bucket: DateTime<Utc>,
    pub cpu_pct_avg: f32,
    pub cpu_pct_max: f32,
    pub mem_bytes_avg: i64,
    pub mem_bytes_max: i64,
    pub mem_limit: i64,
    pub net_rx: i64,
    pub net_tx: i64,
    pub blk_read: i64,
    pub blk_write: i64,
    pub pids_max: i32,
    pub samples: i32,
}

/// The window a metrics query covers.
///
/// A closed set, so the console has three buttons and the server has three
/// query plans. `OneHour` reads raw samples; the others read the rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricRange {
    OneHour,
    OneDay,
    SevenDays,
}

impl MetricRange {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "1h" => Self::OneHour,
            "24h" => Self::OneDay,
            "7d" => Self::SevenDays,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneHour => "1h",
            Self::OneDay => "24h",
            Self::SevenDays => "7d",
        }
    }

    /// How far back the window reaches.
    pub fn duration(self) -> chrono::Duration {
        match self {
            Self::OneHour => chrono::Duration::hours(1),
            Self::OneDay => chrono::Duration::hours(24),
            Self::SevenDays => chrono::Duration::days(7),
        }
    }

    /// Whether the query reads raw samples or the rollup.
    pub fn uses_raw(self) -> bool {
        matches!(self, Self::OneHour)
    }

    /// The largest number of points the console asks for. One hour of 5 s
    /// samples is 720; the rollups are bounded by the window in minutes.
    pub fn max_points(self) -> usize {
        match self {
            Self::OneHour => 720,
            Self::OneDay => 1440,
            Self::SevenDays => 10_080,
        }
    }

    /// The length of the window in seconds.
    pub fn window_seconds(self) -> u32 {
        match self {
            Self::OneHour => 3_600,
            Self::OneDay => 86_400,
            Self::SevenDays => 604_800,
        }
    }

    /// The finest step this window can afford without exceeding its budget.
    ///
    /// It falls out of the budget rather than being chosen: 3 600 s / 720 = 5 s
    /// for the hour, 86 400 s / 1 440 = 60 s for the day, and the same 60 s for
    /// the week. A finer answer would be more points than the console draws.
    pub fn step_seconds(self) -> u32 {
        self.window_seconds() / self.max_points() as u32
    }

    /// The step a request actually gets.
    ///
    /// A caller may ask for a coarser step (fewer points), never a finer one:
    /// clamping at [`Self::step_seconds`] is what keeps the answer inside the
    /// point budget, and the upper clamp stops a silly value from asking for a
    /// single bucket covering the whole window by accident.
    pub fn effective_step(self, requested: Option<u32>) -> u32 {
        let base = self.step_seconds();
        requested
            .map(|step| step.clamp(base, self.window_seconds()))
            .unwrap_or(base)
    }
}

/// Which table answered a series, so the console can say what it is drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    /// Raw `docker stats` samples, five-second cadence, kept for a day.
    Raw,
    /// The one-minute rollup, kept for a week.
    Rollup,
}

impl MetricSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Rollup => "rollup",
        }
    }
}

/// One series as the console receives it.
///
/// `step_seconds` is returned rather than assumed: the client asked for a
/// window, the server decided the resolution, and a graph that knows its own
/// step does not have to guess where a gap came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetricSeries {
    pub sandbox_id: Uuid,
    pub range: MetricRange,
    pub step_seconds: u32,
    pub source: MetricSource,
    pub points: Vec<MetricBucket>,
}

/// One point as a node posts it.
///
/// The node knows container names, not UUIDs, so the batch names the sandbox and
/// the service resolves the id inside the caller's environment. A wrong name is
/// dropped rather than rejected: one stale container must not cost a whole
/// batch, and the count of drops is what makes the loss visible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RawPoint {
    pub sandbox: String,
    pub ts: DateTime<Utc>,
    pub cpu_pct: f32,
    pub mem_bytes: i64,
    pub mem_limit: i64,
    pub net_rx: i64,
    pub net_tx: i64,
    pub blk_read: i64,
    pub blk_write: i64,
    pub pids: i32,
}

impl RawPoint {
    /// Whether the numbers can be true.
    ///
    /// A negative byte count or a PID count below zero is a broken sampler, not
    /// a measurement, and storing it would draw a graph that explains nothing.
    /// `cpu_pct` is bounded by the core count Docker reports, so a generous
    /// ceiling catches a unit error (fractions vs percent) without arguing
    /// about how many cores a node has.
    pub fn is_plausible(&self) -> bool {
        self.cpu_pct.is_finite()
            && (0.0..=102_400.0).contains(&self.cpu_pct)
            && self.mem_bytes >= 0
            && self.mem_limit >= 0
            && self.net_rx >= 0
            && self.net_tx >= 0
            && self.blk_read >= 0
            && self.blk_write >= 0
            && self.pids >= 0
    }

    /// The stored shape, once the service has resolved the sandbox id.
    pub fn into_sample(self, sandbox_id: Uuid) -> MetricSample {
        MetricSample {
            sandbox_id,
            ts: self.ts,
            cpu_pct: self.cpu_pct,
            mem_bytes: self.mem_bytes,
            mem_limit: self.mem_limit,
            net_rx: self.net_rx,
            net_tx: self.net_tx,
            blk_read: self.blk_read,
            blk_write: self.blk_write,
            pids: self.pids,
        }
    }
}

/// A batch of raw points from one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetricBatch {
    pub samples: Vec<RawPoint>,
}

impl MetricBatch {
    /// The most samples one request may carry.
    ///
    /// A batch is one five-second tick for every sandbox on a node, so 500 is
    /// generous for a node with many sandboxes and still a bound: one request
    /// cannot fill the table.
    pub const MAX_SAMPLES: usize = 500;

    /// Whether the batch is larger than one request may carry.
    pub fn is_oversized(&self) -> bool {
        self.samples.len() > Self::MAX_SAMPLES
    }
}

/// The runner's process table for one sandbox, as it arrived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProcessTable {
    pub taken_at: DateTime<Utc>,
    /// The column titles the container's `ps` printed, kept so a row can be
    /// read without guessing what each column was.
    pub columns: Vec<String>,
    pub items: Vec<ProcessEntry>,
    /// Whether the runner cut the table at its own cap.
    pub truncated: bool,
}

/// What the runner's fixed inventory probe reported, before it is stored.
///
/// The facts stay an opaque document here: the runner may gain a section, and
/// the store has fixed columns, so the mapping belongs to the service that owns
/// the snapshot. One rule follows from that: this type is not what a client
/// sees, and the service is what decides which keys survive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct InventoryProbe {
    pub taken_at: DateTime<Utc>,
    pub facts: serde_json::Value,
    /// Why the snapshot is partial, if it is.
    pub note: Option<String>,
}

/// What a sandbox's process table looks like right now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProcessEntry {
    pub pid: i64,
    pub ppid: Option<i64>,
    pub user: String,
    pub cpu_pct: Option<f32>,
    pub mem_pct: Option<f32>,
    pub elapsed: Option<String>,
    pub command: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hour_window_reads_raw_samples_and_the_rest_read_rollups() {
        assert!(MetricRange::OneHour.uses_raw());
        assert!(!MetricRange::OneDay.uses_raw());
        assert!(!MetricRange::SevenDays.uses_raw());
    }

    #[test]
    fn ranges_round_trip_through_their_names() {
        for range in [MetricRange::OneHour, MetricRange::OneDay, MetricRange::SevenDays] {
            assert_eq!(MetricRange::parse(range.as_str()), Some(range));
        }
        assert_eq!(MetricRange::parse("forever"), None);
    }

    #[test]
    fn point_budgets_match_the_cadence() {
        // 3600 s / 5 s per sample.
        assert_eq!(MetricRange::OneHour.max_points(), 720);
        // 24 h of one-minute buckets.
        assert_eq!(MetricRange::OneDay.max_points(), 1440);
    }

    #[test]
    fn a_window_never_returns_more_points_than_its_budget() {
        for range in [MetricRange::OneHour, MetricRange::OneDay, MetricRange::SevenDays] {
            let points = range.window_seconds() as usize / range.effective_step(None) as usize;
            assert!(
                points <= range.max_points(),
                "{} returns {points} points",
                range.as_str()
            );
        }
        assert_eq!(MetricRange::OneHour.step_seconds(), 5);
        assert_eq!(MetricRange::OneDay.step_seconds(), 60);
        assert_eq!(MetricRange::SevenDays.step_seconds(), 60);
    }

    #[test]
    fn a_finer_step_than_the_window_affords_is_raised_not_honoured() {
        // One second for an hour would be 3 600 points; the budget wins.
        assert_eq!(MetricRange::OneHour.effective_step(Some(1)), 5);
        assert_eq!(MetricRange::OneHour.effective_step(Some(300)), 300);
        // A coarser request is honoured, never widened past the window.
        assert_eq!(
            MetricRange::OneHour.effective_step(Some(10_000_000)),
            MetricRange::OneHour.window_seconds()
        );
    }

    #[test]
    fn impossible_numbers_are_not_plausible() {
        let point = |cpu: f32, mem: i64, pids: i32| RawPoint {
            sandbox: "hermes-01".into(),
            ts: Utc::now(),
            cpu_pct: cpu,
            mem_bytes: mem,
            mem_limit: 1,
            net_rx: 0,
            net_tx: 0,
            blk_read: 0,
            blk_write: 0,
            pids,
        };
        assert!(point(25.0, 1_048_576, 12).is_plausible());
        assert!(!point(-1.0, 1_048_576, 12).is_plausible());
        assert!(!point(f32::NAN, 1_048_576, 12).is_plausible());
        assert!(!point(25.0, -5, 12).is_plausible());
        assert!(!point(25.0, 1_048_576, -1).is_plausible());
    }

    #[test]
    fn a_batch_carries_at_most_one_tick_for_every_sandbox() {
        let sample = RawPoint {
            sandbox: "a".into(),
            ts: Utc::now(),
            cpu_pct: 1.0,
            mem_bytes: 0,
            mem_limit: 0,
            net_rx: 0,
            net_tx: 0,
            blk_read: 0,
            blk_write: 0,
            pids: 1,
        };
        let small = MetricBatch {
            samples: vec![sample.clone(); MetricBatch::MAX_SAMPLES],
        };
        assert!(!small.is_oversized());
        let over = MetricBatch {
            samples: vec![sample; MetricBatch::MAX_SAMPLES + 1],
        };
        assert!(over.is_oversized());
    }
}
