-- 0020_sandbox_metrics — a time series per sandbox, held here rather than in a
-- metrics system.
--
-- The console draws CPU, memory, network, and disk over 1 h / 24 h / 7 d. The
-- runner samples `docker stats` every 5 s and posts a batch; the control API
-- stores the raw points for a day and a one-minute rollup for a week. That is
-- enough for "what happened" and keeps the appliance to one store (ADR 0001).
--
-- Size: 50 sandboxes x 17 280 samples/day = 864 000 rows/day. Daily partitions
-- let retention drop a table instead of deleting rows.

CREATE TABLE sandbox_metrics (
    sandbox_id   uuid NOT NULL,
    environment_id uuid NOT NULL,
    ts           timestamptz NOT NULL,
    cpu_pct      real NOT NULL DEFAULT 0,
    mem_bytes    bigint NOT NULL DEFAULT 0,
    mem_limit    bigint NOT NULL DEFAULT 0,
    net_rx       bigint NOT NULL DEFAULT 0,
    net_tx       bigint NOT NULL DEFAULT 0,
    blk_read     bigint NOT NULL DEFAULT 0,
    blk_write    bigint NOT NULL DEFAULT 0,
    pids         integer NOT NULL DEFAULT 0,
    PRIMARY KEY (sandbox_id, ts)
) PARTITION BY RANGE (ts);

CREATE INDEX sandbox_metrics_env_ts_idx ON sandbox_metrics (environment_id, ts DESC);

-- A default partition catches a sample for a day whose partition has not been
-- created yet; the maintenance job creates partitions ahead and moves rows out.
CREATE TABLE sandbox_metrics_default PARTITION OF sandbox_metrics DEFAULT;

-- Pre-create today and the next two days so a fresh install does not depend on
-- the maintenance job having run.
DO $$
DECLARE
    start_day date := current_date;
    i integer;
    part_start date;
    part_end date;
    part_name text;
BEGIN
    FOR i IN 0..2 LOOP
        part_start := start_day + i;
        part_end := start_day + i + 1;
        part_name := format('sandbox_metrics_%s', to_char(part_start, 'YYYYMMDD'));
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF sandbox_metrics FOR VALUES FROM (%L) TO (%L)',
            part_name, part_start, part_end
        );
    END LOOP;
END $$;

-- One-minute averages, kept for seven days. Written by the rollup job, never by
-- the ingest path, so a slow rollup cannot delay a sample.
CREATE TABLE sandbox_metrics_1m (
    sandbox_id     uuid NOT NULL,
    environment_id uuid NOT NULL,
    bucket         timestamptz NOT NULL,
    cpu_pct_avg    real NOT NULL,
    cpu_pct_max    real NOT NULL,
    mem_bytes_avg  bigint NOT NULL,
    mem_bytes_max  bigint NOT NULL,
    mem_limit      bigint NOT NULL,
    net_rx         bigint NOT NULL,
    net_tx         bigint NOT NULL,
    blk_read       bigint NOT NULL,
    blk_write      bigint NOT NULL,
    pids_max       integer NOT NULL,
    samples        integer NOT NULL,
    PRIMARY KEY (sandbox_id, bucket)
);

CREATE INDEX sandbox_metrics_1m_env_bucket_idx ON sandbox_metrics_1m (environment_id, bucket DESC);

COMMENT ON TABLE sandbox_metrics IS
  'Raw docker stats samples, 5 s cadence, daily partitions, 24 h retention.';
COMMENT ON TABLE sandbox_metrics_1m IS
  'One-minute rollup of sandbox_metrics, 7 d retention, written by the metrics_rollup job.';

-- The operational schedules that keep the tables bounded.
INSERT INTO schedules (environment_id, name, kind, cron, payload) VALUES
    ('00000000-0000-0000-0000-000000000002', 'metrics-partition-maintenance',
     'metrics_partition_maintenance', '15 0 * * *', '{"days_ahead": 2, "retain_days": 1}'::jsonb),
    ('00000000-0000-0000-0000-000000000002', 'metrics-rollup',
     'metrics_rollup', '*/5 * * * *', '{"retain_days": 7}'::jsonb)
ON CONFLICT (environment_id, name) DO NOTHING;
