# ADR 0014 — OpenShell CLI feature parity

Status: Accepted
Date: 2026-10-26

## Context

The requirement is that everything the OpenShell CLI can do, the web console can
do too. Without a checklist, "we support OpenShell" means whatever was
implemented first, and gaps are discovered by users.

The CLI surface is known and stable enough to enumerate from
`crates/openshell-cli/src/main.rs`.

## Decision

The CLI is the checklist, and the mapping is documented and tested.

| CLI | Capability | Console surface |
|---|---|---|
| `sandbox create/get/list/delete/stop/start` | Lifecycle | Sandbox list and detail |
| `sandbox exec` | Run a command | Command runner in the detail drawer |
| `sandbox connect` (SSH proxy) | Interactive shell | Web terminal over WebSocket |
| `sandbox upload/download` | File transfer | Upload and download in the drawer |
| `sandbox ssh-config` | SSH client config | Generated config with a copy button |
| `sandbox provider attach/detach/status` | Provider binding | Provider tab on the sandbox |
| `sandbox template create/get/list/delete` | Reusable templates | Templates page |
| `logs` | Logs with tail, since, source, level | Log viewer with the same filters |
| `policy set/update/get/list/delete` | Policy lifecycle | Policy editor |
| `policy get --global`, `set --global`, `delete --global` | Admin override | Global policy panel |
| `rule get/approve/reject/approve-all/clear/history` | Rule approval | Rule draft queue |
| `settings get/set/delete` | Settings, sandbox or global scope | Settings panel with scope selector |
| `provider create/get/list/update/delete` | Provider lifecycle | Providers page |
| `provider refresh` (status/configure/rotate/delete) | Credential refresh | Refresh panel per provider |
| `provider list-profiles`, `profile *` | Profile catalog, import/export/lint | Profile catalog page |
| `workspace create/get/list/delete`, `member *` | Workspaces and membership | Environment members |
| `gateway add/remove/login/logout/select/info/list` | Gateway registration | Gateways page |
| `status`, `whoami` | Health and identity | System page |
| `doctor check` | Prerequisite diagnosis | Diagnostics panel |
| `forward start/stop/list`, `service expose/list/get/delete` | Port and service exposure | Ingress page |

Two CLI capabilities are deliberately not exposed as-is:

- `term` (the CLI TUI) is replaced by the web console itself.
- `completions` is meaningless for a GUI.

## Alternatives rejected

**Implement what is asked for and treat parity as a goal.** Rejected: goals
without a checklist become claims without evidence. The table is the checklist.

**Wrap the CLI in a shell.** Rejected: shelling out from a web API is a command
injection surface, loses structured errors, and inherits CLI session state.
The control plane talks to the gateway API, as the CLI does.

**Expose only a subset and document the rest as "CLI only".** Rejected: it
contradicts the stated requirement. Where a capability is genuinely
GUI-inappropriate (`term`, `completions`) it is named and justified, not
silently omitted.

## Consequences

- The permission model needs to cover the newly exposed capabilities, notably
  `exec` and `terminal`, which get their own permission bit and a dedicated
  security test.
- A CLI capability that appears later needs a row in this table and a console
  surface, or an explicit decision to exclude it.
- Some capabilities map to more than one screen (a provider appears both in the
  provider list and in the sandbox's provider tab). The table names the primary
  surface.

## What would invalidate this

- OpenShell releasing a capability with no API equivalent, which would force a
  documented exception rather than a CLI shell-out.
