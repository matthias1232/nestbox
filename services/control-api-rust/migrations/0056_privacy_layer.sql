-- 0056_privacy_layer — what a sandbox's chat content may carry, and what the
-- policy did with it.
--
-- `privacy_settings` is the policy. A row with `sandbox_id` NULL is the
-- **environment default**: every sandbox without a row of its own inherits it.
-- That is why the unique constraint is `NULLS NOT DISTINCT` — an ordinary
-- UNIQUE treats every NULL as distinct, so the default row would stack a new
-- copy on every save and "the" default would be whichever row came first. The
-- constraint is named (`privacy_settings_scope`) because the upsert targets it
-- by name (`ON CONFLICT ON CONSTRAINT`), the only form that also works for the
-- NULL default row.
--
-- `privacy_incidents` is the evidence: which detector fired, what the policy
-- did, a masked excerpt, and a SHA-256 fingerprint of the value. There is
-- deliberately no column for the value itself and there will not be one. The
-- fingerprint is what lets two incidents be recognised as the same secret — a
-- repeat of one leaked key — without being able to read it back.
--
-- Index `privacy_incidents_sandbox_idx` serves the incident list ("newest first
-- for this sandbox"); `privacy_incidents_retention_idx` serves the retention
-- sweep (`store::postgres::privacy::purge_before`, oldest first per tenant).

CREATE TABLE privacy_settings (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- NULL is the environment default row, not a bug. A sandbox row overrides it.
    sandbox_id         uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    environment_id     uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    mode               text NOT NULL DEFAULT 'strict'
                       CHECK (mode IN ('off', 'observe', 'review', 'strict')),
    -- Detector names from `domain::privacy::Detector`. An empty array means the
    -- default set (every detector): a save that forgot the field must not be
    -- able to switch detection off, which only `mode = 'off'` does on purpose.
    detectors          text[] NOT NULL DEFAULT '{}',
    -- Per-detector override, keyed by detector name, e.g.
    -- {"email_addresses": "redact"}. A missing key falls back to the detector's
    -- own default action, so this column can grow a detector without a
    -- migration for the data.
    actions            jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- Whether a `block` action is enforced. With this false, a blocking detector
    -- still redacts the content but does not refuse the request; an environment
    -- that only wants a record while it tunes the detectors says so here.
    block_cloud_egress boolean NOT NULL DEFAULT true,
    retention_days     int NOT NULL DEFAULT 30
                       CHECK (retention_days BETWEEN 1 AND 3650),
    updated_by         uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT privacy_settings_scope
        UNIQUE NULLS NOT DISTINCT (environment_id, sandbox_id),
    CONSTRAINT privacy_settings_detectors_known CHECK (
        detectors <@ ARRAY['credentials', 'api_keys', 'email_addresses',
                           'person_names', 'ip_addresses', 'hostnames',
                           'phone_numbers', 'iban', 'credit_cards',
                           'financial_data']::text[]
    ),
    CONSTRAINT privacy_settings_actions_object CHECK (jsonb_typeof(actions) = 'object')
);

CREATE TABLE privacy_incidents (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id    uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    sandbox_id        uuid NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    -- The detector name (`domain::privacy::Detector`), so a new detector is a
    -- code change and not a migration. No CHECK: the list is the code's.
    detector          text NOT NULL,
    action            text NOT NULL
                      CHECK (action IN ('blocked', 'redacted', 'replaced', 'allowed')),
    -- `error` is not a severity here: an inspection that failed is not an
    -- incident, it is a 5xx and no row is written.
    severity          text NOT NULL DEFAULT 'warning'
                      CHECK (severity IN ('info', 'warning', 'critical')),
    -- First two and last two characters. Never the value, never longer than 300
    -- so a pathological match cannot become a stored copy of the content.
    masked_value      text,
    -- Lowercase SHA-256 of the matched value: recognises a repeat, reveals
    -- nothing. Shape-checked so a truncated or upper-case hash cannot land.
    value_fingerprint text,
    destination       text,
    cloud_delivery    boolean NOT NULL DEFAULT false,
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT privacy_incidents_masked_short
        CHECK (masked_value IS NULL OR length(masked_value) <= 300),
    CONSTRAINT privacy_incidents_fingerprint_shape
        CHECK (value_fingerprint IS NULL OR value_fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT privacy_incidents_destination_sane
        CHECK (destination IS NULL OR length(destination) <= 300)
);

CREATE INDEX privacy_incidents_sandbox_idx
    ON privacy_incidents (sandbox_id, created_at DESC);
CREATE INDEX privacy_incidents_retention_idx
    ON privacy_incidents (environment_id, created_at);

ALTER TABLE privacy_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE privacy_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON privacy_settings
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

ALTER TABLE privacy_incidents ENABLE ROW LEVEL SECURITY;
ALTER TABLE privacy_incidents FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON privacy_incidents
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON privacy_settings TO nestbox_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON privacy_incidents TO nestbox_app;
