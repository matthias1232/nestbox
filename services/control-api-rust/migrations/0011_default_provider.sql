-- 0011_default_provider — the inference endpoint this appliance ships with.
--
-- Two rows, not one, because they answer different questions:
--
--   secret_references says *where* a credential lives (a locator, never a value,
--   ADR 0005), and provider_profiles says *which endpoint* a sandbox should use.
--
-- Both are ordinary editable data. An installation that points at OpenAI
-- instead of the local router edits the profile in the console; nothing in the
-- code assumes this endpoint. The values below are the appliance's documented
-- default and are visible as such in the UI.
--
-- The key file itself is not created here: a migration cannot write outside the
-- database, and a secret in a migration would be a secret in the repository. The
-- deployment script creates /secrets/omniroute_api_key with mode 0600.

INSERT INTO secret_references (environment_id, name, locator, kind)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    'omniroute-api-key',
    'file:///secrets/omniroute_api_key',
    'api_key'
)
ON CONFLICT (environment_id, name) DO NOTHING;

INSERT INTO provider_profiles (
    environment_id, name, kind, endpoint, model, secret_ref, labels
)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    'omniroute',
    'omniroute',
    'http://10.90.92.30:20128/v1',
    'opencode-go-combo',
    'file:///secrets/omniroute_api_key',
    jsonb_build_object(
        'default_for', 'agent-packs',
        'note', 'Appliance default. Edit the endpoint here, not in the code.'
    )
)
ON CONFLICT (environment_id, name) DO NOTHING;
