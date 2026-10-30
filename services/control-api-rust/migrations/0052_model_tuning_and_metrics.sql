-- 0052_model_tuning_and_metrics
--
-- Two additions to a local model install:
--
--   * `settings` holds the runtime flags an operator tuned (context, GPU
--     layers, batching, KV cache type, ...). It is JSON because the set grows
--     with the runtime and an unknown key must not need a migration; the
--     service validates it and the runner only ever sees an argument vector.
--   * `model_metric_samples` keeps the numbers a served model reports, so a
--     graph survives a page reload and an operator can see a spike after it
--     happened. Samples are per environment and per model, never global.

ALTER TABLE model_installs
    ADD COLUMN settings jsonb NOT NULL DEFAULT '{}'::jsonb;

CREATE TABLE model_metric_samples (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    model_key      text NOT NULL,
    node_id        uuid NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    captured_at    timestamptz NOT NULL DEFAULT now(),
    metrics        jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX model_metric_samples_series
    ON model_metric_samples (environment_id, model_key, captured_at DESC);

ALTER TABLE model_metric_samples ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_metric_samples FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON model_metric_samples
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON model_metric_samples TO nestbox_app;
