# Policies

## The document

A policy is a YAML document with four sections:

| Section | Scope in time | What it controls |
|---|---|---|
| `filesystem_policy` | Fixed at start | Which paths are readable, writable, or both |
| `landlock` | Fixed at start | How strictly Landlock is applied |
| `process` | Fixed at start | The user and group the workload runs as |
| `network_policies` | Hot | Which binaries reach which endpoints, with which methods |

Only the network section reloads into a running sandbox. The others are fixed
when the workload starts. This is a property of the runtime, not a limitation
invented here, and the console says so at the point of editing (ADR 0008).

## The scope

Two scopes, mirroring the runtime exactly (ADR 0007):

- **Sandbox**: the policy assigned to one sandbox.
- **Global**: one policy that replaces every sandbox policy and blocks further
  sandbox-level edits until deleted.

There is no cluster policy and no gateway policy. A gateway's configuration is
operator settings, not policy (ADR 0012).

## A network rule

```yaml
network_policies:
  model-api:
    endpoints:
      - host: api.anthropic.com
        port: 443
        protocol: rest
        enforcement: enforced
        access: allow
        methods: [GET, POST]
        binaries:
          - path: /usr/local/bin/claude
```

Two fields carry most of the meaning:

- `enforcement: enforced` blocks. `audit` records the attempt and allows it,
  which is how a policy is developed without breaking the workload.
- `binaries` is an allowlist of executables. A shell script calling `curl` is
  not `curl`, and is therefore not allowed. This is the field that makes the
  policy about the workload rather than the destination.

## Resolution

The effective policy for a sandbox is:

```
if global policy exists:
    effective = global            # replaces everything, blocks edits
else:
    effective = sandbox policy
    + provider-composed entries   # from the attached provider profiles
```

Provider entries are composed by the runtime, not by Nestbox. That is why the
effective policy must be read from the gateway rather than computed locally:
only the gateway knows what a provider profile contributes.

## Change semantics

Every change is validated twice: by the gateway before saving, and by the
sandbox before loading. The second validation matters because the sandbox also
accounts for provider rules that arrive at the same time.

| Change | Effect on a running sandbox |
|---|---|
| Network rules | Loaded; connections opened under the old rules close |
| Network middleware | Loaded, if the service is already registered |
| External middleware registration | Rejected; needs gateway config and restart |
| Added filesystem path | Saved, not effective until recreate |
| Removed path, or Landlock/process change | Rejected after start |

When network rules change, keep-alive connections, tunnels, and WebSockets are
closed at the reload boundary. The console warns about this, because a user
watching a stream will see it drop.

## Templates

A template is a reusable document fragment that **produces** a policy. It is a
Nestbox convenience for composition, not a fourth runtime scope. A policy
created from a template is a normal policy afterwards, and editing the template
does not retroactively change policies made from it.

## Rule drafts

A rule created from a live event is a draft first (ADR 0011). The draft carries
the proposed rule, the diff against the enforced policy, and a state:
`pending`, `approved`, `rejected`. Approval creates or modifies the rule and
triggers a rollout. This mirrors the runtime's own approve/reject model.

## Where this lives in the code

| Concern | File |
|---|---|
| Document types and validation | `services/control-api-rust/src/domain/policy.rs` |
| Resolution and precedence | `services/control-api-rust/src/services/policy_resolution.rs` |
| Rollout in waves | `services/control-api-rust/src/services/policy_rollout.rs` |
| Draft to rule | `services/control-api-rust/src/services/rule_drafts.rs` |
| SQL | `services/control-api-rust/src/store/postgres/policies.rs` |
