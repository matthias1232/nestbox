//! Sandbox resource samples, their rollup, and the inventory snapshot.
//!
//! Two time tables, two cadences (`0020_sandbox_metrics.sql`):
//!
//! * `sandbox_metrics` — raw `docker stats` points every 5 s, daily
//!   partitions, 24 h retention;
//! * `sandbox_metrics_1m` — one-minute averages, 7 d retention, written only by
//!   the `metrics_rollup` job.
//!
//! The reads are one shape: both tables answer a window, both are grouped by a
//! step, and both come back as [`MetricBucket`]. That is what makes
//! `range=1h` and `range=7d` the same drawing code in the console.
//!
//! Every query is bounded by the window it serves and by a `LIMIT`, so a console
//! that asks for a week reads at most 10 080 buckets. The step is never taken
//! from the request: the service derives it from the window, so a request cannot
//! ask for a million points.
//!
//! The inventory snapshot (`sandbox_inventory`) is here because it is the same
//! work package and no other file owns it; it shares nothing with the time
//! series and is separated by a section heading below.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::access::Inventory;
use crate::domain::metrics::{MetricBucket, MetricSample};
use crate::error::StoreError;

/// The parent table of the raw series.
const RAW_PARENT: &str = "sandbox_metrics";
/// The catch-all partition, which must never be treated as a day.
const RAW_DEFAULT: &str = "sandbox_metrics_default";

// ---------------------------------------------------------------- raw samples

/// Insert one batch of raw samples in a single round trip.
///
/// Arrays plus `UNNEST` rather than a statement per sample: a five-second tick
/// for fifty sandboxes would otherwise be fifty round trips, and the sampler has
/// five seconds. `ON CONFLICT DO NOTHING` makes a retried batch a no-op instead
/// of an error, because the primary key *is* "one point per sandbox per instant".
pub async fn insert_samples(
    scope: &mut TenantScope<'_>,
    samples: &[MetricSample],
) -> Result<u64, StoreError> {
    if samples.is_empty() {
        return Ok(0);
    }

    let environment_id = scope.environment_id();
    let mut sandbox_ids = Vec::with_capacity(samples.len());
    let mut environments = Vec::with_capacity(samples.len());
    let mut stamps = Vec::with_capacity(samples.len());
    let mut cpu = Vec::with_capacity(samples.len());
    let mut memory = Vec::with_capacity(samples.len());
    let mut limits = Vec::with_capacity(samples.len());
    let mut rx = Vec::with_capacity(samples.len());
    let mut tx = Vec::with_capacity(samples.len());
    let mut read = Vec::with_capacity(samples.len());
    let mut written = Vec::with_capacity(samples.len());
    let mut pids = Vec::with_capacity(samples.len());

    for sample in samples {
        sandbox_ids.push(sample.sandbox_id);
        environments.push(environment_id);
        stamps.push(sample.ts);
        cpu.push(sample.cpu_pct);
        memory.push(sample.mem_bytes);
        limits.push(sample.mem_limit);
        rx.push(sample.net_rx);
        tx.push(sample.net_tx);
        read.push(sample.blk_read);
        written.push(sample.blk_write);
        pids.push(sample.pids);
    }

    let inserted = sqlx::query(
        "INSERT INTO sandbox_metrics
            (sandbox_id, environment_id, ts, cpu_pct, mem_bytes, mem_limit,
             net_rx, net_tx, blk_read, blk_write, pids)
         SELECT * FROM UNNEST(
            $1::uuid[], $2::uuid[], $3::timestamptz[], $4::real[],
            $5::bigint[], $6::bigint[], $7::bigint[], $8::bigint[],
            $9::bigint[], $10::bigint[], $11::integer[])
         ON CONFLICT (sandbox_id, ts) DO NOTHING",
    )
    .bind(&sandbox_ids)
    .bind(&environments)
    .bind(&stamps)
    .bind(&cpu)
    .bind(&memory)
    .bind(&limits)
    .bind(&rx)
    .bind(&tx)
    .bind(&read)
    .bind(&written)
    .bind(&pids)
    .execute(scope.connection())
    .await?
    .rows_affected();

    Ok(inserted)
}

/// Read raw samples for one sandbox, grouped into `step_seconds` buckets.
///
/// One bucket per step, oldest first. `LIMIT` keeps the newest `limit` buckets
/// and the outer sort puts them back in drawing order, so a caller that asks for
/// too much gets the recent end of the window rather than the ancient end.
pub async fn raw_series(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    step_seconds: u32,
    limit: i64,
) -> Result<Vec<MetricBucket>, StoreError> {
    let rows = sqlx::query(
        "WITH series AS (
            SELECT date_bin(make_interval(secs => $4::double precision), ts, TIMESTAMPTZ '2000-01-01 00:00:00+00') AS bucket,
                   avg(cpu_pct)::real AS cpu_pct_avg,
                   max(cpu_pct)::real AS cpu_pct_max,
                   avg(mem_bytes)::bigint AS mem_bytes_avg,
                   max(mem_bytes)::bigint AS mem_bytes_max,
                   max(mem_limit)::bigint AS mem_limit,
                   max(net_rx)::bigint AS net_rx,
                   max(net_tx)::bigint AS net_tx,
                   max(blk_read)::bigint AS blk_read,
                   max(blk_write)::bigint AS blk_write,
                   max(pids)::integer AS pids_max,
                   count(*)::integer AS samples
              FROM sandbox_metrics
             WHERE environment_id = $1
               AND sandbox_id = $2
               AND ts >= $3
               AND ts < $5
             GROUP BY bucket
             ORDER BY bucket DESC
             LIMIT $6
         )
         SELECT $2::uuid AS sandbox_id, * FROM series ORDER BY bucket ASC",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(since)
    .bind(f64::from(step_seconds))
    .bind(until)
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;

    rows.iter().map(row_to_bucket).collect()
}

/// Read the one-minute rollup for one sandbox, grouped into `step_seconds`.
///
/// The rollup is already aggregated, so a coarser step averages averages. The
/// cumulative counters take `max` for the same reason the raw read does: they
/// only grow, so the highest value in a bucket is the one at its end.
pub async fn rollup_series(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    step_seconds: u32,
    limit: i64,
) -> Result<Vec<MetricBucket>, StoreError> {
    let rows = sqlx::query(
        "WITH series AS (
            SELECT date_bin(make_interval(secs => $4::double precision), bucket, TIMESTAMPTZ '2000-01-01 00:00:00+00') AS step_start,
                   avg(cpu_pct_avg)::real AS cpu_pct_avg,
                   max(cpu_pct_max)::real AS cpu_pct_max,
                   avg(mem_bytes_avg)::bigint AS mem_bytes_avg,
                   max(mem_bytes_max)::bigint AS mem_bytes_max,
                   max(mem_limit)::bigint AS mem_limit,
                   max(net_rx)::bigint AS net_rx,
                   max(net_tx)::bigint AS net_tx,
                   max(blk_read)::bigint AS blk_read,
                   max(blk_write)::bigint AS blk_write,
                   max(pids_max)::integer AS pids_max,
                   sum(samples)::integer AS samples
              FROM sandbox_metrics_1m
             WHERE environment_id = $1
               AND sandbox_id = $2
               AND bucket >= $3
               AND bucket < $5
             GROUP BY step_start
             ORDER BY step_start DESC
             LIMIT $6
         )
         SELECT $2::uuid AS sandbox_id, * FROM series ORDER BY step_start ASC",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(since)
    .bind(f64::from(step_seconds))
    .bind(until)
    .bind(limit)
    .fetch_all(scope.connection())
    .await?;

    // The alias differs from `bucket` in the raw query, so the row mapper takes
    // the column name rather than assuming one.
    rows.iter().map(row_to_bucket).collect()
}

/// Aggregate every completed minute of raw samples into the rollup.
///
/// `ON CONFLICT ... DO UPDATE` is what makes a re-run idempotent: the same
/// minute, aggregated from the same rows, produces the same values and the same
/// row. A minute that is still running is excluded — its bucket would be
/// rewritten on the next tick, and a graph that shows a half minute as final
/// reads as a drop in load that did not happen.
///
/// Returns the number of rollup rows written.
pub async fn rollup_minutes(
    scope: &mut TenantScope<'_>,
    since: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let written = sqlx::query(
        "INSERT INTO sandbox_metrics_1m
            (sandbox_id, environment_id, bucket, cpu_pct_avg, cpu_pct_max,
             mem_bytes_avg, mem_bytes_max, mem_limit, net_rx, net_tx,
             blk_read, blk_write, pids_max, samples)
         SELECT sandbox_id, environment_id, date_trunc('minute', ts) AS bucket,
                avg(cpu_pct)::real, max(cpu_pct)::real,
                avg(mem_bytes)::bigint, max(mem_bytes)::bigint,
                max(mem_limit)::bigint, max(net_rx)::bigint, max(net_tx)::bigint,
                max(blk_read)::bigint, max(blk_write)::bigint,
                max(pids)::integer, count(*)::integer
           FROM sandbox_metrics
          WHERE environment_id = $1
            AND ts >= $2
            AND ts < date_trunc('minute', now())
          GROUP BY sandbox_id, environment_id, bucket
         ON CONFLICT (sandbox_id, bucket) DO UPDATE SET
            cpu_pct_avg = EXCLUDED.cpu_pct_avg,
            cpu_pct_max = EXCLUDED.cpu_pct_max,
            mem_bytes_avg = EXCLUDED.mem_bytes_avg,
            mem_bytes_max = EXCLUDED.mem_bytes_max,
            mem_limit = EXCLUDED.mem_limit,
            net_rx = EXCLUDED.net_rx,
            net_tx = EXCLUDED.net_tx,
            blk_read = EXCLUDED.blk_read,
            blk_write = EXCLUDED.blk_write,
            pids_max = EXCLUDED.pids_max,
            samples = EXCLUDED.samples",
    )
    .bind(scope.environment_id())
    .bind(since)
    .execute(scope.connection())
    .await?
    .rows_affected();

    Ok(written)
}

/// Drop rollup buckets older than the retention window.
///
/// The rollup is one table rather than partitions, so its retention deletes
/// rows. That is affordable at one row per sandbox per minute — 10 080 rows per
/// sandbox for the whole week — where the raw table could not be.
pub async fn prune_rollup(
    scope: &mut TenantScope<'_>,
    retain_days: i32,
) -> Result<u64, StoreError> {
    let cutoff = Utc::now() - Duration::days(i64::from(retain_days.max(1)));
    let removed = sqlx::query(
        "DELETE FROM sandbox_metrics_1m
          WHERE environment_id = $1 AND bucket < $2",
    )
    .bind(scope.environment_id())
    .bind(cutoff)
    .execute(scope.connection())
    .await?
    .rows_affected();

    Ok(removed)
}

// -------------------------------------------------------- partition maintenance

/// Create the daily partition for one day and return its table name.
///
/// Two paths, because PostgreSQL refuses the simple one when the default
/// partition already holds a row for that day (`updated partition constraint for
/// default partition would be violated`). The simple path creates the partition
/// directly. The careful path builds a table with the same shape, moves the day's
/// rows out of the default partition, and attaches it — so a day that arrived
/// late is kept rather than refused.
///
/// Public because the maintenance job is not the only caller: an operator
/// backfilling a missed day wants the same function, not a second DDL path.
pub async fn create_day_partition(
    pool: &sqlx::PgPool,
    day: NaiveDate,
) -> Result<String, StoreError> {
    let name = partition_name(day);
    if !is_plain_identifier(&name) {
        return Err(StoreError::Database(sqlx::Error::Configuration(
            String::from("refusing to build a partition name that is not an identifier").into(),
        )));
    }
    if partition_exists(pool, &name).await? {
        return Ok(name);
    }

    let start = day.format("%Y-%m-%d").to_string();
    let end = (day + Duration::days(1)).format("%Y-%m-%d").to_string();

    if default_holds_rows(pool, day).await? {
        // Build, move, attach. In that order: the rows have to leave the default
        // partition before the bound can be attached to the parent.
        let move_sql = format!(
            "WITH moved AS (
                DELETE FROM {RAW_DEFAULT}
                 WHERE ts >= '{start}' AND ts < '{end}'
                RETURNING sandbox_id, environment_id, ts, cpu_pct, mem_bytes,
                          mem_limit, net_rx, net_tx, blk_read, blk_write, pids
             )
             INSERT INTO \"{name}\"
             SELECT * FROM moved"
        );
        let attach_sql = format!(
            "ALTER TABLE {RAW_PARENT} ATTACH PARTITION \"{name}\"
             FOR VALUES FROM ('{start}') TO ('{end}')"
        );
        let build_sql = format!(
            "CREATE TABLE IF NOT EXISTS \"{name}\" (LIKE {RAW_PARENT} INCLUDING DEFAULTS)"
        );

        let mut tx = pool.begin().await?;
        sqlx::query(&build_sql).execute(&mut *tx).await?;
        sqlx::query(&move_sql).execute(&mut *tx).await?;
        sqlx::query(&attach_sql).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(name);
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{name}\" PARTITION OF {RAW_PARENT}
         FOR VALUES FROM ('{start}') TO ('{end}')"
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(name)
}

/// Create the partitions for today and the next `days_ahead` days.
///
/// Required rather than optional: a sample for a day with no partition lands in
/// the default partition, which is a safety net and not a design. The job runs
/// daily and creates two days ahead, so a missed run is a warning rather than a
/// lost day.
pub async fn ensure_partitions(
    pool: &sqlx::PgPool,
    days_ahead: i32,
) -> Result<Vec<String>, StoreError> {
    let rows = sqlx::query("SELECT (current_date + i) AS day FROM generate_series(0, $1) AS i")
        .bind(days_ahead.clamp(0, 31))
        .fetch_all(pool)
        .await?;

    let mut created = Vec::new();
    for row in rows {
        let day: NaiveDate = row.try_get("day")?;
        created.push(create_day_partition(pool, day).await?);
    }
    Ok(created)
}

/// Drop the daily partitions whose whole range is older than the retention.
///
/// A drop rather than a delete: that is the point of partitioning, and it is
/// what keeps a day of 864 000 rows from costing a vacuum. The default partition
/// is never a candidate.
pub async fn drop_expired_partitions(
    pool: &sqlx::PgPool,
    retain_days: i32,
) -> Result<Vec<String>, StoreError> {
    let cutoff = Utc::now() - Duration::days(i64::from(retain_days.max(1)));

    let rows = sqlx::query(
        "SELECT c.relname AS name
           FROM pg_class c
           JOIN pg_inherits i ON i.inhrelid = c.oid
           JOIN pg_class p ON p.oid = i.inhparent
          WHERE p.relname = $1
            AND c.relname <> $2
            AND pg_get_expr(c.relpartbound, c.oid) ~ 'TO \\('
            AND substring(pg_get_expr(c.relpartbound, c.oid) FROM 'TO \\(''([^'']+)''')::timestamptz < $3",
    )
    .bind(RAW_PARENT)
    .bind(RAW_DEFAULT)
    .bind(cutoff)
    .fetch_all(pool)
    .await?;

    let mut dropped = Vec::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        if !is_plain_identifier(&name) {
            // A catalog name that is not a plain identifier did not come from
            // this schema; quoting it blindly would be worse than skipping it.
            tracing::warn!(partition = %name, "skipping a partition name that is not an identifier");
            continue;
        }
        // The name comes from the catalog, and is quoted as an identifier.
        let sql = format!("DROP TABLE IF EXISTS \"{name}\"");
        sqlx::query(&sql).execute(pool).await?;
        dropped.push(name);
    }
    Ok(dropped)
}

/// The table name for a day, so the job and the console agree on it.
fn partition_name(day: NaiveDate) -> String {
    format!("sandbox_metrics_{}", day.format("%Y%m%d"))
}

/// Whether a generated name is only lowercase letters, digits, and underscores.
///
/// The names are built here from a date, never from input. The check is the
/// second control: a name that reached SQL quoted but malformed is a mistake
/// worth failing on rather than guessing about.
fn is_plain_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether a table is already attached to the raw parent.
async fn partition_exists(pool: &sqlx::PgPool, name: &str) -> Result<bool, StoreError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM pg_inherits i
              JOIN pg_class c ON c.oid = i.inhrelid
              JOIN pg_class p ON p.oid = i.inhparent
             WHERE p.relname = $1 AND c.relname = $2
         )",
    )
    .bind(RAW_PARENT)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Whether the catch-all partition holds a row for that day.
async fn default_holds_rows(pool: &sqlx::PgPool, day: NaiveDate) -> Result<bool, StoreError> {
    let start = day.format("%Y-%m-%d").to_string();
    let end = (day + Duration::days(1)).format("%Y-%m-%d").to_string();
    let sql = format!(
        "SELECT EXISTS (
            SELECT 1 FROM {RAW_DEFAULT}
             WHERE ts >= '{start}' AND ts < '{end}'
         )"
    );
    let holds: bool = sqlx::query_scalar(&sql).fetch_one(pool).await?;
    Ok(holds)
}

/// The ids of the sandboxes with these names, inside the caller's environment.
///
/// This is the node-scoping check in one query: a name that is not in this
/// environment resolves to nothing, so a batch cannot write another tenant's
/// series. Unknown names are simply absent from the answer rather than an error,
/// because a container that was removed between the list and the post is normal.
pub async fn resolve_sandbox_names(
    scope: &mut TenantScope<'_>,
    names: &[String],
) -> Result<std::collections::HashMap<String, Uuid>, StoreError> {
    if names.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let rows = sqlx::query(
        "SELECT id, name
           FROM sandboxes
          WHERE environment_id = $1 AND name = ANY($2)",
    )
    .bind(scope.environment_id())
    .bind(names)
    .fetch_all(scope.connection())
    .await?;

    let mut resolved = std::collections::HashMap::with_capacity(rows.len());
    for row in rows {
        resolved.insert(row.try_get("name")?, row.try_get("id")?);
    }
    Ok(resolved)
}

// ------------------------------------------------------------ inventory snapshot

/// Store one inventory snapshot, replacing the previous one for that sandbox.
///
/// Upsert rather than insert-and-prune: the table holds the current state of a
/// sandbox, not a history, so there is exactly one row per sandbox and "Stand
/// vor X min" is the `taken_at` of that row. The `WHERE` on the conflict keeps a
/// write from moving a row into another tenant.
pub async fn upsert_inventory(
    scope: &mut TenantScope<'_>,
    snapshot: &Inventory,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO sandbox_inventory
            (sandbox_id, environment_id, taken_at, os, packages, agents,
             tool_versions, listening, env_keys, mounts, image, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (sandbox_id) DO UPDATE SET
            taken_at = EXCLUDED.taken_at,
            os = EXCLUDED.os,
            packages = EXCLUDED.packages,
            agents = EXCLUDED.agents,
            tool_versions = EXCLUDED.tool_versions,
            listening = EXCLUDED.listening,
            env_keys = EXCLUDED.env_keys,
            mounts = EXCLUDED.mounts,
            image = EXCLUDED.image,
            note = EXCLUDED.note
          WHERE sandbox_inventory.environment_id = EXCLUDED.environment_id",
    )
    .bind(snapshot.sandbox_id)
    .bind(scope.environment_id())
    .bind(snapshot.taken_at)
    .bind(&snapshot.os)
    .bind(&snapshot.packages)
    .bind(&snapshot.agents)
    .bind(&snapshot.tool_versions)
    .bind(&snapshot.listening)
    .bind(&snapshot.env_keys)
    .bind(&snapshot.mounts)
    .bind(&snapshot.image)
    .bind(&snapshot.note)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Read the stored snapshot for one sandbox.
///
/// `None` is a real answer: a sandbox that was created a minute ago has no
/// inventory yet, and the console says "not taken" rather than drawing an empty
/// one as if it had been.
pub async fn read_inventory(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Option<Inventory>, StoreError> {
    let row = sqlx::query(
        "SELECT sandbox_id, taken_at, os, packages, agents, tool_versions,
                listening, env_keys, mounts, image, note
           FROM sandbox_inventory
          WHERE environment_id = $1 AND sandbox_id = $2",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .fetch_optional(scope.connection())
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(Inventory {
        sandbox_id: row.try_get("sandbox_id")?,
        taken_at: row.try_get("taken_at")?,
        os: row.try_get("os")?,
        packages: row.try_get("packages")?,
        agents: row.try_get("agents")?,
        tool_versions: row.try_get("tool_versions")?,
        listening: row.try_get("listening")?,
        env_keys: row.try_get("env_keys")?,
        mounts: row.try_get("mounts")?,
        image: row.try_get("image")?,
        note: row.try_get("note")?,
    }))
}

// ------------------------------------------------------------------ row mapping

/// Map one aggregated row to a bucket.
///
/// The bucket column is called `bucket` in the raw read and `step_start` in the
/// rollup read, because `sandbox_metrics_1m` already has a column named
/// `bucket` and two columns with one name would be ambiguous. One mapper reads
/// both rather than two mappers that could drift apart.
fn row_to_bucket(row: &sqlx::postgres::PgRow) -> Result<MetricBucket, StoreError> {
    let bucket: DateTime<Utc> = match row.try_get("bucket") {
        Ok(value) => value,
        Err(_) => row.try_get("step_start")?,
    };
    Ok(MetricBucket {
        sandbox_id: row.try_get("sandbox_id")?,
        bucket,
        cpu_pct_avg: row.try_get("cpu_pct_avg")?,
        cpu_pct_max: row.try_get("cpu_pct_max")?,
        mem_bytes_avg: row.try_get("mem_bytes_avg")?,
        mem_bytes_max: row.try_get("mem_bytes_max")?,
        mem_limit: row.try_get("mem_limit")?,
        net_rx: row.try_get("net_rx")?,
        net_tx: row.try_get("net_tx")?,
        blk_read: row.try_get("blk_read")?,
        blk_write: row.try_get("blk_write")?,
        pids_max: row.try_get("pids_max")?,
        samples: row.try_get("samples")?,
    })
}
