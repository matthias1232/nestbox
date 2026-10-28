-- 0033_policy_presets — a preset library, and the policy a new sandbox starts with.
--
-- Two facts make this migration what it is:
--
-- 1. A template is a document with a name (ADR 0020). Presets therefore live in
--    `policy_templates`, not in code: an operator can read one, change one, or
--    add their own, and none of that needs a deploy.
-- 2. "Best practice" has to be the default and still be overridable. That needs
--    a policy, not a template — a template produces nothing until someone
--    instantiates it — so the same document is seeded twice: once as the
--    template an operator picks from, once as the policy every new sandbox is
--    assigned. The policy's document is copied from the template in this
--    migration, so the two cannot drift at birth.
--
-- The defaults are deny-by-default with three named exceptions an agent needs to
-- work at all: model providers, package registries, and source control. That is
-- a choice, and it is written here so a reader can disagree with it in one
-- place. Everything else is implicit default-deny, which is the property the
-- policy screen promises.
--
-- Templates are per environment because `policy_templates.environment_id` is
-- NOT NULL. This seeds every environment that exists at migration time; an
-- environment created later copies the library through the same statement in
-- the environment-create path.

-- ---------------------------------------------------------------------------
-- 1. The preset library
-- ---------------------------------------------------------------------------

INSERT INTO policy_templates (environment_id, name, description, spec)
SELECT e.id, seed.name, seed.description, seed.spec
  FROM environments e
 CROSS JOIN (
    VALUES
    (
        'Safe defaults (best practice)',
        'Deny by default, with the three things an agent needs to work: model providers, package registries, and source control. The workspace is writable, nothing else is.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"},
              {"path": "/tmp", "access": "read_write"},
              {"path": "/etc/ssl/certs", "access": "read"}
            ]
          },
          "landlock": {
            "rules": [
              {"path": "/workspace", "access": ["read", "write"]},
              {"path": "/tmp", "access": ["read", "write"]}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "env_allow": ["PATH", "HOME", "LANG", "TERM", "TZ"],
            "max_processes": 256
          },
          "network_policies": {
            "model-providers": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "api.anthropic.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "api.openai.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "generativelanguage.googleapis.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "openrouter.ai", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            },
            "package-registries": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "registry.npmjs.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "pypi.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "files.pythonhosted.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "crates.io", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "static.crates.io", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "proxy.golang.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "sum.golang.org", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            },
            "source-control": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "github.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "api.github.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "codeload.github.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "objects.githubusercontent.com", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'Model access only',
        'Nothing but the inference APIs. For an agent whose whole job is a prompt and an answer.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"},
              {"path": "/etc/ssl/certs", "access": "read"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "env_allow": ["PATH", "HOME", "LANG", "TERM"],
            "max_processes": 64
          },
          "network_policies": {
            "model-providers": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "api.anthropic.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "api.openai.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "openrouter.ai", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'REST API allowlist, observing',
        'A list of HTTPS APIs that a service depends on, in audit mode first: every call is logged and allowed, so the list can be checked against reality before it starts blocking.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"},
              {"path": "/tmp", "access": "read_write"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "max_processes": 256
          },
          "network_policies": {
            "service-apis": {
              "enforcement": "audit",
              "endpoints": [
                {"host": "api.stripe.com", "port": 443, "protocol": "rest", "access": "allow", "methods": ["GET", "POST"]},
                {"host": "slack.com", "port": 443, "protocol": "rest", "access": "allow", "methods": ["POST"]},
                {"host": "api.notion.com", "port": 443, "protocol": "rest", "access": "allow", "methods": ["GET", "POST", "PATCH"]}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'Read-only workspace',
        'The sandbox may look and think, and change nothing. For review, analysis, and reporting agents.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read"},
              {"path": "/etc/ssl/certs", "access": "read"}
            ]
          },
          "landlock": {
            "rules": [
              {"path": "/workspace", "access": ["read"]}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "env_allow": ["PATH", "HOME", "LANG"],
            "max_processes": 64
          },
          "network_policies": {
            "model-providers": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "api.anthropic.com", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "api.openai.com", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'Air-gapped',
        'No outbound traffic at all. The network section exists and names nothing, which is a statement, not an omission.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "max_processes": 64
          },
          "network_policies": {}
        }'::jsonb
    ),
    (
        'WebSocket streaming',
        'Long-lived sockets to a streaming service, with the handshake and the frames allowed on one host.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "max_processes": 128
          },
          "network_policies": {
            "streaming": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "stream.example.com", "port": 443, "protocol": "websocket", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'MCP server client',
        'The sandbox may talk to named MCP servers and nothing else. MCP is its own protocol in the rule vocabulary, so the proxy can tell an MCP call from a plain HTTP one.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "max_processes": 128
          },
          "network_policies": {
            "mcp-servers": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "mcp.example.com", "port": 443, "protocol": "mcp", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    ),
    (
        'Package installs',
        'Registries only: an agent that may install dependencies but reach nothing else.',
        '{
          "filesystem_policy": {
            "paths": [
              {"path": "/workspace", "access": "read_write"},
              {"path": "/tmp", "access": "read_write"}
            ]
          },
          "process": {
            "run_as_user": "sandbox",
            "max_processes": 256
          },
          "network_policies": {
            "package-registries": {
              "enforcement": "enforced",
              "endpoints": [
                {"host": "registry.npmjs.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "pypi.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "files.pythonhosted.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "crates.io", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "static.crates.io", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "proxy.golang.org", "port": 443, "protocol": "rest", "access": "allow"},
                {"host": "sum.golang.org", "port": 443, "protocol": "rest", "access": "allow"}
              ]
            }
          }
        }'::jsonb
    )
 ) AS seed(name, description, spec)
ON CONFLICT (environment_id, name) DO NOTHING;

-- ---------------------------------------------------------------------------
-- 2. The policy a new sandbox is assigned
--
-- Its document is the safe-defaults template's, copied rather than retyped: a
-- default that differs from the preset it is named after is a lie in the UI.
-- The assignment happens in the sandbox-create path, not here; there is no
-- sandbox yet.
-- ---------------------------------------------------------------------------

INSERT INTO policies (environment_id, name, scope, spec)
SELECT t.environment_id, t.name, 'sandbox', t.spec
  FROM policy_templates t
 WHERE t.name = 'Safe defaults (best practice)'
ON CONFLICT (environment_id, name) DO NOTHING;

-- A policy without a first revision cannot be rolled back to, so the seed writes
-- one. An existing policy keeps its own history untouched.
INSERT INTO policy_versions (policy_id, version, spec)
SELECT p.id, p.version, p.spec
  FROM policies p
 WHERE p.name = 'Safe defaults (best practice)'
   AND p.scope = 'sandbox'
   AND NOT EXISTS (
        SELECT 1 FROM policy_versions v WHERE v.policy_id = p.id
   );

COMMENT ON TABLE policy_templates IS
  'The preset library: documents with a name, per environment. Presets are data (ADR 0020), so editing one needs no deploy.';
