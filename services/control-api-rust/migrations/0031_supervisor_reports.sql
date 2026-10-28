-- 0031_supervisor_reports — a supervisor can finally say what it sees.
--
-- The supervisor row existed since 0002 and nothing wrote `status`, `version`,
-- `config_dirtiness`, `last_seen` or `last_policy_applied_at`; the values came
-- from the registration request. These columns are what the process in the
-- sandbox reports, and the history table is what makes "it said X at 12:04"
-- answerable after the fact.

-- The desired configuration, delivered to the sandbox and applied by the
-- supervisor. Not a policy object (ADR 0012): the supervisor has no rules of its
-- own, only what to apply and whether it may restart its agent.
ALTER TABLE supervisors ADD COLUMN settings jsonb NOT NULL DEFAULT '{}'::jsonb;

-- The last report as it arrived, so a reader does not have to join the history
-- for the current picture.
ALTER TABLE supervisors ADD COLUMN reported jsonb;

-- SHA-256 of the token the supervisor presents. The plaintext is generated at
-- sandbox creation, lives in the sandbox environment and is never stored here;
-- the same rule as every other credential (ADR 0005).
ALTER TABLE supervisors ADD COLUMN token_hash text;

CREATE TABLE supervisor_reports (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    supervisor_id  uuid NOT NULL REFERENCES supervisors(id) ON DELETE CASCADE,
    sandbox_id     uuid REFERENCES sandboxes(id) ON DELETE CASCADE,
    ts             timestamptz NOT NULL DEFAULT now(),
    status         text NOT NULL,
    version        text,
    config_dirtiness integer NOT NULL DEFAULT 0,
    last_policy_applied_at timestamptz,
    checks         jsonb NOT NULL DEFAULT '[]'::jsonb,
    facts          jsonb NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT supervisor_reports_dirtiness_sane CHECK (config_dirtiness >= 0)
);

CREATE INDEX supervisor_reports_supervisor_idx
    ON supervisor_reports (supervisor_id, ts DESC);

ALTER TABLE supervisor_reports ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON supervisor_reports
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

COMMENT ON COLUMN supervisors.settings IS
  'What the supervisor should apply: self_healing, desired policy version. Delivered in the sandbox environment, not read from here.';
COMMENT ON COLUMN supervisors.token_hash IS
  'SHA-256 of the supervisor token. The value itself is only ever in the sandbox environment.';
