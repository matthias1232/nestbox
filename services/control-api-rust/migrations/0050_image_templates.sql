-- 0047_image_templates — reusable selections of tools, repositories, packages and MCP servers.
--
-- A template is a saved builder selection, not an image: it names catalog keys
-- and package names an operator assembled once, so a team can reuse it. Two
-- flags keep sharing explicit: `is_shared` offers it to the environment's other
-- operators, `is_default` marks the one a new recipe starts from.
--
-- Environment-scoped like every tenant table, because a package selection names
-- internal repositories and internal MCP endpoints.

CREATE TABLE image_templates (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id  uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    name            text NOT NULL,
    description     text NOT NULL DEFAULT '',
    baseline_key    text NOT NULL REFERENCES baselines(key),
    tools           text[] NOT NULL DEFAULT '{}',
    extra_packages  text[] NOT NULL DEFAULT '{}',
    repositories    jsonb NOT NULL DEFAULT '[]'::jsonb,
    mcp_server_keys text[] NOT NULL DEFAULT '{}',
    is_default      boolean NOT NULL DEFAULT false,
    is_shared       boolean NOT NULL DEFAULT true,
    created_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT image_templates_name_unique UNIQUE (environment_id, name)
);

-- At most one default per environment: "the default" is a single answer, and a
-- second one would make a new recipe depend on row order.
CREATE UNIQUE INDEX image_templates_one_default
    ON image_templates (environment_id)
    WHERE is_default;

ALTER TABLE image_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE image_templates FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON image_templates
    USING (environment_id = current_setting('app.environment_id', true)::uuid)
    WITH CHECK (environment_id = current_setting('app.environment_id', true)::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON image_templates TO nestbox_app;
