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
}
