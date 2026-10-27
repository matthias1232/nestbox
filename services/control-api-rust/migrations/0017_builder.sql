-- 0017_builder — base operating systems, agent packs, and the tool catalog as
-- data.
--
-- Until now a pack was a `match` arm in the sandbox service and a tool was a
-- Dockerfile line. Both are catalog data of the same kind as the model
-- catalog (ADR 0013): a new agent should be a row, not a code change, and the
-- console should be able to list what exists without a deployment.

-- ---------------------------------------------------------------------------
-- Base operating systems a recipe can start from.
-- ---------------------------------------------------------------------------

CREATE TABLE baselines (
    key             text PRIMARY KEY,
    name            text NOT NULL,
    recommended     boolean NOT NULL DEFAULT false,
    base_image      text NOT NULL,
    -- apt or dnf; decides which package name the tool catalog resolves to.
    package_manager text NOT NULL,
    default_tools   text[] NOT NULL DEFAULT '{}',
    openshell_ready boolean NOT NULL DEFAULT false,
    notes           text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT baselines_pm_valid CHECK (package_manager IN ('apt', 'dnf'))
);

INSERT INTO baselines (key, name, recommended, base_image, package_manager, default_tools, openshell_ready, notes) VALUES
    ('ubuntu-24.04', 'Ubuntu 24.04 LTS', true,  'ubuntu:24.04',    'apt',
     ARRAY['git','nano','ripgrep','curl','ca-certificates'], true,
     'Supported by NVIDIA OpenShell; the recommended choice.'),
    ('debian-12',    'Debian 12',        false, 'debian:12-slim',  'apt',
     ARRAY['git','nano','ripgrep','curl','ca-certificates'], false,
     'Smaller base, same package names as Ubuntu.'),
    ('rocky-9',      'Rocky Linux 9',    false, 'rockylinux:9',    'dnf',
     ARRAY['git','nano','curl','ca-certificates'], false,
     'Enterprise base; fewer tools packaged, dnf names differ.');

-- ---------------------------------------------------------------------------
-- Agent packs: what goes into a sandbox and how.
-- ---------------------------------------------------------------------------

CREATE TABLE agent_packs (
    key             text PRIMARY KEY,
    name            text NOT NULL,
    vendor          text,
    description     text,
    -- Base OS keys this pack supports; empty means all.
    base_os         text[] NOT NULL DEFAULT '{}',
    -- script | git | npm | pip | binary | image | none
    install_kind    text NOT NULL DEFAULT 'none',
    -- The URL, package, or image the install uses.
    install_ref     text,
    -- Paths under the agent's home that must persist across rebuilds.
    home_paths      text[] NOT NULL DEFAULT '{}',
    requires_gpu    boolean NOT NULL DEFAULT false,
    needs_inference boolean NOT NULL DEFAULT false,
    -- The image and command the pack runs with when no recipe is involved.
    default_image   text NOT NULL,
    default_command text[] NOT NULL DEFAULT '{}',
    ports           integer[] NOT NULL DEFAULT '{}',
    env_slots       text[] NOT NULL DEFAULT '{}',
    enabled         boolean NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT agent_packs_install_kind_valid CHECK (install_kind IN (
        'script', 'git', 'npm', 'pip', 'binary', 'image', 'none'
    ))
);

INSERT INTO agent_packs (key, name, vendor, description, base_os, install_kind, install_ref, home_paths, requires_gpu, needs_inference, default_image, default_command, ports, env_slots) VALUES
    ('ubuntu-dev', 'Ubuntu development shell', 'Nestbox',
     'A plain shell with the baseline tools; the starting point for anything else.',
     '{}', 'none', NULL, ARRAY['.bash_history','.config'], false, false,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], '{}', '{}'),
    ('hermes', 'Hermes agent', 'Nous Research',
     'Agent CLI that talks to an OpenAI-compatible endpoint.',
     ARRAY['ubuntu-24.04','debian-12'], 'script', 'https://hermes-agent.nousresearch.com/install.sh',
     ARRAY['.hermes'], false, true,
     'nestbox-hermes:local', ARRAY['sleep','infinity'], '{}', ARRAY['OPENAI_BASE_URL','OPENAI_API_KEY','HERMES_MODEL']),
    ('openclaw', 'OpenClaw gateway', 'OpenClaw',
     'Gateway sandbox that fronts agents and enforces policy.',
     ARRAY['ubuntu-24.04'], 'image', 'nestbox-openclaw:local',
     ARRAY['.openclaw'], false, false,
     'nestbox-openclaw:local', '{}', ARRAY[8080], '{}'),
    ('git', 'Git tool sandbox', 'Nestbox',
     'Git and nothing else, for repository operations.',
     '{}', 'none', NULL, ARRAY['.gitconfig','.ssh'], false, false,
     'nestbox-tool-git:local', ARRAY['sleep','infinity'], '{}', '{}'),
    ('claude-code', 'Claude Code', 'Anthropic',
     'Anthropic''s agentic coding CLI.',
     ARRAY['ubuntu-24.04','debian-12'], 'npm', '@anthropic-ai/claude-code',
     ARRAY['.claude','.claude.json'], false, true,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], '{}', ARRAY['ANTHROPIC_API_KEY','ANTHROPIC_BASE_URL']),
    ('opencode', 'OpenCode', 'OpenCode',
     'Open-source coding agent with provider routing.',
     ARRAY['ubuntu-24.04','debian-12'], 'script', 'https://opencode.ai/install',
     ARRAY['.config/opencode','.local/share/opencode'], false, true,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], '{}', ARRAY['OPENAI_BASE_URL','OPENAI_API_KEY']),
    ('openchamber', 'OpenChamber', 'OpenChamber',
     'Web front end for OpenCode sessions.',
     ARRAY['ubuntu-24.04','debian-12'], 'npm', 'openchamber',
     ARRAY['.openchamber','.config/opencode'], false, true,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], ARRAY[3000], ARRAY['OPENAI_BASE_URL','OPENAI_API_KEY']),
    ('nemoclaw', 'NemoClaw', 'NVIDIA',
     'NVIDIA''s OpenShell agent runtime.',
     ARRAY['ubuntu-24.04'], 'script', 'https://nemoclaw.nvidia.com/install.sh',
     ARRAY['.nemoclaw'], true, true,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], '{}', ARRAY['NVIDIA_API_KEY']),
    ('antigravity', 'Antigravity', 'Google',
     'Google''s agent IDE runtime.',
     ARRAY['ubuntu-24.04','debian-12'], 'binary', 'https://antigravity.google/download',
     ARRAY['.antigravity'], false, true,
     'nestbox-ubuntu-dev:local', ARRAY['sleep','infinity'], '{}', ARRAY['GEMINI_API_KEY']);

-- ---------------------------------------------------------------------------
-- Tool catalog with a name per package manager.
-- ---------------------------------------------------------------------------

CREATE TABLE tool_catalog (
    key         text PRIMARY KEY,
    name        text NOT NULL,
    "group"     text NOT NULL DEFAULT 'general',
    description text,
    apt_name    text,
    rpm_name    text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    -- A tool must be installable on at least one base.
    CONSTRAINT tool_catalog_some_name CHECK (apt_name IS NOT NULL OR rpm_name IS NOT NULL)
);

INSERT INTO tool_catalog (key, name, "group", description, apt_name, rpm_name) VALUES
    ('git',        'Git',          'vcs',     'Version control',                 'git',             'git'),
    ('nano',       'nano',         'editor',  'Small terminal editor',           'nano',            'nano'),
    ('vim',        'Vim',          'editor',  'Modal terminal editor',           'vim',             'vim-enhanced'),
    ('ripgrep',    'ripgrep',      'search',  'Fast recursive grep',             'ripgrep',         'ripgrep'),
    ('fd',         'fd',           'search',  'Fast file finder',                'fd-find',         'fd-find'),
    ('jq',         'jq',           'data',    'JSON processor',                  'jq',              'jq'),
    ('curl',       'curl',         'network', 'HTTP client',                     'curl',            'curl'),
    ('wget',       'wget',         'network', 'File downloader',                 'wget',            'wget'),
    ('htop',       'htop',         'system',  'Process viewer',                  'htop',            'htop'),
    ('tmux',       'tmux',         'system',  'Terminal multiplexer',            'tmux',            'tmux'),
    ('python3',    'Python 3',     'runtime', 'Python interpreter',              'python3',         'python3'),
    ('python3-pip','pip',          'runtime', 'Python package installer',        'python3-pip',     'python3-pip'),
    ('nodejs',     'Node.js',      'runtime', 'JavaScript runtime',              'nodejs',          'nodejs'),
    ('npm',        'npm',          'runtime', 'Node package manager',            'npm',             'npm'),
    ('build-essential','Build tools','build', 'C/C++ compiler and make',         'build-essential', NULL),
    ('gcc',        'GCC',          'build',   'C compiler',                      'gcc',             'gcc'),
    ('make',       'make',         'build',   'Build automation',                'make',            'make'),
    ('openssh-client','SSH client','network', 'ssh, scp',                        'openssh-client',  'openssh-clients'),
    ('ca-certificates','CA certs', 'network', 'TLS trust store',                 'ca-certificates', 'ca-certificates'),
    ('unzip',      'unzip',        'archive', 'Zip extractor',                   'unzip',           'unzip'),
    ('zip',        'zip',          'archive', 'Zip creator',                     'zip',             'zip'),
    ('less',       'less',         'system',  'Pager',                           'less',            'less'),
    ('tree',       'tree',         'system',  'Directory listing',               'tree',            'tree');

-- A tool profile says which package manager it was resolved for.
ALTER TABLE tool_profiles
    ADD COLUMN package_manager text NOT NULL DEFAULT 'apt',
    ADD CONSTRAINT tool_profiles_pm_valid CHECK (package_manager IN ('apt', 'dnf'));
