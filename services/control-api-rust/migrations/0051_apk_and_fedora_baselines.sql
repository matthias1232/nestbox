-- 0048_apk_and_fedora_baselines — cover the distributions operators actually run.
--
-- Alpine was not a baseline because the schema only knew apt and dnf. It is a
-- real target for small sandboxes, and a package catalogue that lists Alpine
-- packages while a recipe cannot use them would be a promise the builder does
-- not keep. Fedora is the same argument on the dnf side: the tool exists, the
-- baseline did not.
--
-- `apk_name` is a separate column rather than a fallback to `apt_name`: most
-- names match, and the ones that do not (py3-pip, openssh) would fail at image
-- build time with a message about a missing package.

ALTER TABLE tool_catalog ADD COLUMN apk_name text;

UPDATE tool_catalog SET apk_name = apt_name WHERE key IN (
    'git', 'nano', 'vim', 'ripgrep', 'jq', 'curl', 'wget', 'htop', 'tmux',
    'python3', 'nodejs', 'npm', 'gcc', 'make', 'ca-certificates', 'unzip',
    'zip', 'less', 'tree'
);
UPDATE tool_catalog SET apk_name = 'fd' WHERE key = 'fd';
UPDATE tool_catalog SET apk_name = 'py3-pip' WHERE key = 'python3-pip';
UPDATE tool_catalog SET apk_name = 'openssh-client' WHERE key = 'openssh-client';
UPDATE tool_catalog SET apk_name = 'build-base' WHERE key = 'build-essential';

ALTER TABLE baselines DROP CONSTRAINT baselines_pm_valid;
ALTER TABLE baselines ADD CONSTRAINT baselines_pm_valid
    CHECK (package_manager IN ('apt', 'dnf', 'apk'));

ALTER TABLE tool_profiles DROP CONSTRAINT tool_profiles_pm_valid;
ALTER TABLE tool_profiles ADD CONSTRAINT tool_profiles_pm_valid
    CHECK (package_manager IN ('apt', 'dnf', 'apk'));

INSERT INTO baselines (key, name, recommended, base_image, package_manager, default_tools, openshell_ready, notes) VALUES
    ('fedora-41', 'Fedora 41', false, 'fedora:41', 'dnf',
     ARRAY['git','nano','curl','ca-certificates'], false,
     'Current Fedora; the same dnf catalog as Rocky with newer packages.'),
    ('alpine-3.21', 'Alpine Linux 3.21', false, 'alpine:3.21', 'apk',
     ARRAY['git','nano','curl','ca-certificates'], false,
     'Smallest base; musl libc and a different package name for some tools.')
ON CONFLICT (key) DO NOTHING;
