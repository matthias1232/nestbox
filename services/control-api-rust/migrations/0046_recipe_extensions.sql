-- 0046_recipe_extensions — operator-selected packages, signed repositories and MCP assignments.
-- Values are declarative and secret-free: repository credentials remain vault locators.
ALTER TABLE recipes
    ADD COLUMN extra_packages text[] NOT NULL DEFAULT '{}',
    ADD COLUMN repositories jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN mcp_server_keys text[] NOT NULL DEFAULT '{}';
