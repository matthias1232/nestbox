-- 0058_accounting — what an agent request cost, and how long the evidence lives.
--
-- `accounting_requests` is the raw evidence: one row per proxy request, with the
-- tokens the provider reported and the cost derived from them. Two properties
-- matter more than the columns:
--
--   1. `cost_micros` is NULL when no price was known. It is never 0 as a
--      stand-in: a zero reads as "this was free", and a total built from that is
--      a number nobody can defend. `price_source` says where the price came
--      from (`catalog`, `provider`, `unknown`).
--   2. `content_digest` is a hash, never the content. The accounting table is
--      not a place from which a prompt can be read back.
--
-- `accounting_daily` is the long-term rollup. Raw rows are purged after the
-- environment's retention window (default 30 days); the rollup is what survives,
-- so a year of usage still has a shape without a million rows. Its token
-- columns are `bigint` because a day of traffic outgrows `int` long before the
-- table does; `requests` is an `int`, and the rollup saturates at its maximum
-- rather than wrapping a busy day into a negative count.
--
-- `model_prices` is the price list. `environment_id IS NULL` is the global row
-- every environment falls back to; a row with an environment overrides it. That
-- is why the unique constraint is `NULLS NOT DISTINCT` and named: an ordinary
-- UNIQUE treats every NULL as distinct, and the upsert targets the constraint by
-- name because inference through a nullable key is only reliable that way.
--
-- Prices are micros per million tokens, as integers. A float would make the
-- stored price and the computed cost disagree at the last digit, and the number
-- an invoice is compared against is exactly the number that must not drift.
--
-- Two indexes serve the two reads that exist:
--   * `accounting_requests_sandbox_idx` serves "newest first for this sandbox";
--   * `accounting_requests_env_idx` serves the org-level window and the rollup.
--
-- Row-level security is enabled and FORCEd on all four tables, so the table
-- owner is not an escape hatch either. `model_prices` is the one deliberate
-- exception in its USING clause: a global row has no environment and must be
-- readable from every environment, the same shape `models` uses
-- (`0009_rls.sql`). Its WITH CHECK is strict, so a tenant cannot write a global
-- price; only a migration or the owner can.

CREATE TABLE accounting_requests (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id    uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id        uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    -- Which agent inside the sandbox made the request (the sandbox may run
    -- several). NULL when the proxy could not attribute it.
    agent_instance    text,
    provider          text,
    model             text,
    -- `in` is a request arriving at the sandbox, `out` one leaving it. The
    -- proxy's vocabulary, and the direction the policy was applied in.
    direction         text NOT NULL CHECK (direction IN ('in', 'out')),
    endpoint          text,
    -- The HTTP status the proxy returned, so a refused request is still visible
    -- in the accounting window without a join to the traffic log.
    status            int NOT NULL DEFAULT 0,
    latency_ms        int NOT NULL DEFAULT 0,
    prompt_tokens     int NOT NULL DEFAULT 0,
    completion_tokens int NOT NULL DEFAULT 0,
    -- Cached prompt tokens are a subset of `prompt_tokens`. They are stored
    -- separately because they are billed at a different rate.
    cached_tokens     int NOT NULL DEFAULT 0,
    -- Reasoning tokens are reported by some providers and are already inside
    -- `completion_tokens`; recorded for analysis, never billed twice.
    reasoning_tokens  int NOT NULL DEFAULT 0,
    -- SHA-256 of the request content, lowercase hex. Never the content itself.
    content_digest    text,
    -- NULL means "no price was known", not "free". See the file comment.
    cost_micros       bigint,
    price_source      text NOT NULL DEFAULT 'unknown'
                      CHECK (price_source IN ('catalog', 'provider', 'unknown')),
    privacy_action    text,
    privacy_detector  text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT accounting_requests_tokens_sane CHECK (
        prompt_tokens >= 0 AND completion_tokens >= 0
        AND cached_tokens >= 0 AND reasoning_tokens >= 0
    ),
    -- A cached count larger than the prompt is a provider bug or a mapping
    -- error; both would make the cost larger than the input bill it describes.
    CONSTRAINT accounting_requests_cached_within_prompt
        CHECK (cached_tokens <= prompt_tokens),
    CONSTRAINT accounting_requests_cost_nonnegative
        CHECK (cost_micros IS NULL OR cost_micros >= 0),
    -- A digest is an index key, not a document. Bounded so a caller that sent
    -- the body by mistake cannot turn this row into a stored copy of it.
    CONSTRAINT accounting_requests_digest_bounded
        CHECK (content_digest IS NULL OR length(content_digest) <= 128)
);

CREATE INDEX accounting_requests_sandbox_idx
    ON accounting_requests (sandbox_id, created_at DESC);
CREATE INDEX accounting_requests_env_idx
    ON accounting_requests (environment_id, created_at DESC);

CREATE TABLE accounting_daily (
    environment_id    uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    -- No foreign key on purpose: the rollup outlives the sandbox it describes.
    -- A cascade would delete a month of billing history the moment an operator
    -- removes a sandbox, which is the opposite of what a rollup is for. The id
    -- stays as a historical reference, and the environment key still removes the
    -- rows when the tenant itself goes.
    sandbox_id        uuid NOT NULL,
    -- `unknown` is the rollup of rows whose request named no model. It is a
    -- real bucket: dropping those rows would make the daily total smaller than
    -- the raw total it replaced.
    model             text NOT NULL DEFAULT 'unknown',
    day               date NOT NULL,
    -- `int`, not `bigint`: the bucket is one sandbox on one model on one day,
    -- and two billion requests in a day is not a case this table has to hold.
    -- The rollup saturates at the column's maximum rather than wrapping, so a
    -- count beyond the range stays a positive number.
    requests          int NOT NULL DEFAULT 0,
    prompt_tokens     bigint NOT NULL DEFAULT 0,
    completion_tokens bigint NOT NULL DEFAULT 0,
    cached_tokens     bigint NOT NULL DEFAULT 0,
    -- NULL when no request in the bucket had a price, for the same reason as
    -- `accounting_requests.cost_micros`.
    cost_micros       bigint,
    PRIMARY KEY (environment_id, sandbox_id, model, day)
);

-- The primary key leads with the sandbox, which is what the per-sandbox read
-- wants. The org-level window is ordered by day instead, so it gets its own.
CREATE INDEX accounting_daily_env_day_idx
    ON accounting_daily (environment_id, day DESC);

CREATE TABLE model_prices (
    id                      uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- NULL is the global price list, not a bug. See the file comment.
    environment_id          uuid REFERENCES environments(id) ON DELETE CASCADE,
    model                   text NOT NULL,
    input_micros_per_mtok   bigint NOT NULL,
    output_micros_per_mtok  bigint NOT NULL,
    -- NULL means the provider charges cached tokens like ordinary input, which
    -- is the common case; the cost maths falls back to the input rate.
    cached_micros_per_mtok  bigint,
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT model_prices_scope
        UNIQUE NULLS NOT DISTINCT (environment_id, model),
    CONSTRAINT model_prices_model_named
        CHECK (length(model) BETWEEN 1 AND 200),
    CONSTRAINT model_prices_rates_nonnegative CHECK (
        input_micros_per_mtok >= 0 AND output_micros_per_mtok >= 0
        AND (cached_micros_per_mtok IS NULL OR cached_micros_per_mtok >= 0)
    )
);

-- `accounting_settings` is one row per environment, so the environment is the
-- primary key. A missing row is not an error: the service answers with the
-- defaults from `domain::accounting`, and the first save writes the row.
CREATE TABLE accounting_settings (
    environment_id uuid PRIMARY KEY REFERENCES environments(id) ON DELETE CASCADE,
    retention_days int NOT NULL DEFAULT 30
                   CHECK (retention_days BETWEEN 1 AND 3650),
    -- Whether the daily rollup is kept when raw rows are purged. False is for an
    -- environment that wants raw retention and no long-term history.
    keep_daily     boolean NOT NULL DEFAULT true,
    updated_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at     timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE accounting_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE accounting_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON accounting_requests
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE accounting_daily ENABLE ROW LEVEL SECURITY;
ALTER TABLE accounting_daily FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON accounting_daily
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE model_prices ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_prices FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON model_prices
    USING (
        environment_id IS NULL
        OR environment_id = current_setting('app.environment_id', true)::uuid
    )
    WITH CHECK (
        environment_id = current_setting('app.environment_id', true)::uuid
    );

ALTER TABLE accounting_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE accounting_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON accounting_settings
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

GRANT SELECT, INSERT, DELETE ON accounting_requests TO nestbox_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON accounting_daily TO nestbox_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON model_prices TO nestbox_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON accounting_settings TO nestbox_app;
