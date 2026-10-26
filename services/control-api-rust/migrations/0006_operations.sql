-- 0006_operations — jobs, schedules, audit, telemetry, migrations.
--
-- Two distinct things, kept apart on purpose:
--   * a job is one-shot work with a target set and partial failures
--   * a schedule is a recurring definition that creates jobs
-- Conflating them makes "retry this bulk action" and "run the sync every 20
-- minutes" the same object, and neither is then expressible cleanly.

CREATE TABLE jobs (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    kind           text NOT NULL,
    status         text NOT NULL DEFAULT 'pending',
    total          integer NOT NULL DEFAULT 0,
    completed      integer NOT NULL DEFAULT 0,
    failed         integer NOT NULL DEFAULT 0,
    payload        jsonb NOT NULL DEFAULT '{}'::jsonb,
    result         jsonb,
    error          text,
    -- Set when the job was created by a schedule, so a run is traceable to it.
    schedule_id    uuid,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    started_at     timestamptz,
    finished_at    timestamptz,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT jobs_status_valid
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled', 'partial')),
    CONSTRAINT jobs_counts_sane CHECK (total >= 0 AND completed >= 0 AND failed >= 0)
);

CREATE INDEX jobs_env_created_idx ON jobs (environment_id, created_at DESC, id DESC);
CREATE INDEX jobs_active_idx ON jobs (environment_id, updated_at DESC)
    WHERE status IN ('pending', 'running');

CREATE TABLE job_logs (
    id         bigserial PRIMARY KEY,
    job_id     uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    ts         timestamptz NOT NULL DEFAULT now(),
    level      text NOT NULL DEFAULT 'info',
    message    text NOT NULL,
    detail     jsonb,
    CONSTRAINT job_logs_level_valid CHECK (level IN ('debug', 'info', 'warn', 'error'))
);

CREATE INDEX job_logs_job_idx ON job_logs (job_id, ts);

CREATE TABLE schedules (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name           text NOT NULL,
    kind           text NOT NULL,
    -- A five-field cron expression, evaluated in the environment's timezone.
    cron           text NOT NULL,
    payload        jsonb NOT NULL DEFAULT '{}'::jsonb,
    enabled        boolean NOT NULL DEFAULT true,
    last_run_at    timestamptz,
    next_run_at    timestamptz,
    last_status    text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT schedules_name_unique UNIQUE (environment_id, name),
    CONSTRAINT schedules_last_status_valid
        CHECK (last_status IS NULL OR last_status IN ('succeeded', 'failed', 'partial'))
);

CREATE INDEX schedules_due_idx ON schedules (next_run_at) WHERE enabled;

-- Append-only. There is deliberately no update or delete path in the API: an
-- audit record that can be changed proves nothing.
CREATE TABLE audit_events (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    actor_id       uuid REFERENCES users(id) ON DELETE SET NULL,
    actor_label    text NOT NULL DEFAULT 'system',
    kind           text NOT NULL,
    subject_type   text,
    subject_id     uuid,
    -- Redacted before insert. The redaction happens in the writer, not here, so
    -- there is exactly one place that can forget.
    detail         jsonb NOT NULL DEFAULT '{}'::jsonb,
    request_id     text,
    created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_env_created_idx
    ON audit_events (environment_id, created_at DESC, id DESC);
CREATE INDEX audit_events_subject_idx ON audit_events (subject_type, subject_id);

-- ---------------------------------------------------------------------------
-- Telemetry, partitioned by month (ADR 0010).
--
-- Retention is DROP PARTITION, which is metadata-only. A DELETE over a table
-- this size takes locks and leaves vacuum debt that grows with the table.
-- ---------------------------------------------------------------------------

CREATE TABLE telemetry_events (
    id             uuid NOT NULL DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL,
    sandbox_id     uuid,
    gateway_id     uuid,
    -- network | dns | inference | log | policy
    kind           text NOT NULL,
    source         text,
    ts             timestamptz NOT NULL,
    -- The event body. Redacted before insert.
    detail         jsonb NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (id, ts)
) PARTITION BY RANGE (ts);

COMMENT ON TABLE telemetry_events IS
  'Append-only event log. Partitioned monthly; retention drops partitions.';

CREATE INDEX telemetry_events_env_ts_idx ON telemetry_events (environment_id, ts DESC);
CREATE INDEX telemetry_events_sandbox_ts_idx ON telemetry_events (sandbox_id, ts DESC);
CREATE INDEX telemetry_events_kind_ts_idx ON telemetry_events (kind, ts DESC);

-- A default partition is a safety net: an insert for a month whose partition
-- has not been created yet lands here instead of failing. The maintenance job
-- creates real partitions ahead of time and moves rows out of default.
CREATE TABLE telemetry_events_default PARTITION OF telemetry_events DEFAULT;

-- Pre-create the current and next two months so a fresh install does not
-- depend on the maintenance job having run.
DO $$
DECLARE
    start_month date := date_trunc('month', now())::date;
    i integer;
    part_start date;
    part_end date;
    part_name text;
BEGIN
    FOR i IN 0..2 LOOP
        part_start := (start_month + (i || ' month')::interval)::date;
        part_end := (start_month + ((i + 1) || ' month')::interval)::date;
        part_name := 'telemetry_events_' || to_char(part_start, 'YYYY_MM');
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF telemetry_events FOR VALUES FROM (%L) TO (%L)',
            part_name, part_start, part_end
        );
    END LOOP;
END $$;

-- Sandbox movement between gateways. The data model is complete in M1; the
-- execution path is M2 (see docs/architecture/scope.md).
CREATE TABLE sandbox_migrations (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id      uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    from_gateway_id uuid REFERENCES gateways(id) ON DELETE SET NULL,
    to_gateway_id   uuid REFERENCES gateways(id) ON DELETE SET NULL,
    state           text NOT NULL DEFAULT 'pending',
    steps           jsonb NOT NULL DEFAULT '[]'::jsonb,
    error           text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT sandbox_migrations_state_valid
        CHECK (state IN ('pending', 'draining', 'copying', 'switching', 'done', 'failed', 'rolled_back'))
);
