# AI firewall for agent traffic

A sandbox without a network policy is a machine with your API keys and an open route to anywhere.
Nestbox gives every nest a policy document with four sections, and shows them as one firewall.

## The four sections

| Section | Fixed at | What it controls |
|---|---|---|
| `filesystem_policy` | nest creation | which paths are readable, writable, or both (`include_workdir`) |
| `landlock` | nest creation | how strictly Landlock is applied (`strict` or `best_effort`) |
| `process` | nest creation | the user and group the workload runs as |
| `network_policies` | **live** | which binaries may reach which endpoints, with which methods |

Only the network section is hot-reloadable. That is a property of the runtime, not a limitation we
invented: filesystem and process boundaries cannot change under a running process without
restarting it. The UI says so instead of pretending otherwise.

## A rule

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

- `enforcement: enforced` blocks; `audit` records the attempt and allows it while you are
  building a policy.
- `binaries` is the allow-list: only these executables may use the endpoint. A shell script that
  calls `curl` is not the binary, and is therefore not allowed.
- `access: deny` is useful to carve exceptions out of a broad allow block.

## The workflow the UI gives you

1. Open a policy, edit the rule in a structured form or as YAML.
2. **Validate** against the schema before anything is sent.
3. **Diff** against the effective policy the gateway currently enforces.
4. **Apply** — the gateway reloads the network section; existing keep-alive tunnels are closed at
   the reload boundary.
5. **Roll back** to any earlier revision. Every save creates one.

The global policy is an administrator override: while active it replaces per-nest policies and
blocks further per-nest edits. Deleting it restores normal selection.
