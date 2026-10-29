// Pure helpers for the policy forms: no JSX, no network. The server owns what is
// valid (`/policy-schema` says which sections and fields exist, and every save is
// validated again there); this file only turns form state into the documents the
// API takes and back, so the forms can be tested without a browser.

// "a, b ,c" -> ["a","b","c"]; empty input -> [].
export function splitList(text) {
  return String(text ?? '')
    .split(/[\n,]/)
    .map((part) => part.trim())
    .filter(Boolean);
}

export function joinList(values) {
  return (values || []).join(', ');
}

// A value typed into a field, converted to what the schema says the field holds.
// An empty optional field is removed instead of being sent as "" or NaN.
export function coerce(field, raw) {
  if (field.type === 'integer') {
    if (raw === '' || raw === null || raw === undefined) return undefined;
    const number = Number(raw);
    return Number.isFinite(number) ? Math.trunc(number) : undefined;
  }
  if (field.type === 'string-list' || field.type === 'path-list') {
    const list = Array.isArray(raw) ? raw : splitList(raw);
    return list.length ? list : undefined;
  }
  if (field.type === 'enum-list') {
    return Array.isArray(raw) && raw.length ? raw : undefined;
  }
  const text = String(raw ?? '').trim();
  return text === '' ? undefined : text;
}

// Set or delete one key without mutating the original.
export function withField(object, name, value) {
  const next = { ...(object || {}) };
  if (value === undefined) delete next[name];
  else next[name] = value;
  return next;
}

export function blankSpec() {
  return {
    filesystem_policy: { paths: [{ path: '/workspace', access: 'read_write' }] },
    network_policies: {},
    process: { run_as_user: 'sandbox', max_processes: 64 },
  };
}

export function blankEndpoint() {
  return { host: '', port: 443, protocol: 'rest', access: 'allow' };
}

// A name for a new network group that does not collide with an existing one.
export function freshGroupName(groups, base = 'group') {
  const taken = new Set(Object.keys(groups || {}));
  if (!taken.has(base)) return base;
  let n = 2;
  while (taken.has(`${base}-${n}`)) n += 1;
  return `${base}-${n}`;
}

// Rename a group while keeping its position and content.
export function renameGroup(groups, from, to) {
  const clean = String(to).trim();
  if (!clean || clean === from || (groups || {})[clean]) return groups;
  const out = {};
  for (const [name, value] of Object.entries(groups || {})) out[name === from ? clean : name] = value;
  return out;
}

// The matcher kinds a rule can have, with what each one asks for.
export const MATCHER_KINDS = [
  { kind: 'host', label: 'Exact host', placeholder: 'api.example.com' },
  { kind: 'host_suffix', label: 'Host suffix', placeholder: '.example.com' },
  { kind: 'cidr', label: 'IP range', placeholder: '10.0.0.0/8' },
  { kind: 'port', label: 'Port or range', placeholder: '443 or 8000-8100' },
  { kind: 'protocol', label: 'Protocol', placeholder: 'https' },
  { kind: 'method', label: 'HTTP method', placeholder: 'GET' },
  { kind: 'binary', label: 'Executable', placeholder: '/usr/bin/curl' },
  { kind: 'any', label: 'Any (no condition)', placeholder: '' },
];

export function blankRuleForm() {
  return {
    name: '',
    action: 'allow',
    enforcement: 'enforced',
    priority: 100,
    enabled: true,
    // Empty means "any protocol"; the other conditions are matchers.
    protocol: '',
    matchers: [{ kind: 'host', value: '' }],
    method: '',
    binary: '',
  };
}

// The body `POST /network-rules` takes. The flat `host`/`port` fields are what
// older readers use; they are derived from the matchers so the two cannot
// disagree. The ids and version are placeholders the server replaces.
export function ruleToPayload(form) {
  // The server evaluates the matchers (all of them must hold) and derives the flat
  // fields from them, so protocol, method and executable are matchers too.
  const matchers = (form.matchers || [])
    .map((m) => ({ kind: m.kind, value: m.kind === 'any' ? undefined : String(m.value ?? '').trim() }))
    .filter((m) => m.kind === 'any' || m.value);
  const extra = [
    ['protocol', form.protocol],
    ['method', String(form.method ?? '').trim().toUpperCase()],
    ['binary', String(form.binary ?? '').trim()],
  ];
  for (const [kind, value] of extra) if (value) matchers.push({ kind, value });
  const conditions = matchers.filter((m) => m.kind !== 'any');
  const first = (kind) => conditions.find((m) => m.kind === kind)?.value;
  const portText = first('port');
  const port = portText && /^\d+$/.test(portText) ? Number(portText) : null;
  return {
    id: '00000000-0000-0000-0000-000000000000',
    environment_id: '00000000-0000-0000-0000-000000000000',
    policy_id: form.policy_id || null,
    sandbox_id: form.sandbox_id || null,
    name: String(form.name).trim(),
    action: form.action,
    host: first('host') || first('host_suffix') || '*',
    port,
    protocol: first('protocol') || 'tcp',
    methods: first('method') ? [first('method')] : [],
    binaries: first('binary') ? [first('binary')] : [],
    matchers: conditions.length ? conditions : matchers,
    enforcement: form.enforcement,
    priority: Number(form.priority) || 100,
    enabled: !!form.enabled,
    version: 0,
  };
}

// What is wrong with a rule form, in a sentence, before it is sent. The server
// checks again; this only saves a round trip for the obvious mistakes.
export function ruleProblem(form) {
  if (!String(form.name).trim()) return 'Give the rule a name.';
  const real = (form.matchers || []).filter((m) => m.kind === 'any' || String(m.value ?? '').trim());
  if (real.length === 0) return 'Add at least one condition, or choose "Any" to match everything.';
  return null;
}

export function dnsToPayload(form) {
  return {
    id: '00000000-0000-0000-0000-000000000000',
    environment_id: '00000000-0000-0000-0000-000000000000',
    policy_id: form.policy_id || null,
    sandbox_id: form.sandbox_id || null,
    action: form.action,
    pattern: String(form.pattern).trim(),
    priority: Number(form.priority) || 100,
    enabled: true,
    version: 0,
  };
}

export function dnsProblem(form) {
  const pattern = String(form.pattern ?? '').trim();
  if (!pattern) return 'Enter a name or a .suffix.';
  if (/\s/.test(pattern)) return 'A DNS pattern has no spaces.';
  return null;
}
