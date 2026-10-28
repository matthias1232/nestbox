# hermes — the agent image, and the one MCP server it gets by default

## Purpose

Hermes Agent in a sandbox, with the inference default pinned to the appliance and
the sandbox's own MCP hub (`mcp-hub`) configured. The image is the pin: the
control plane records `nestbox-hermes:local`, and the build log holds the version
the installer reported.

## What the image decides, and why

| Decision | Why |
|---|---|
| `model.default` / `model.provider` are set at build | A fresh sandbox answers `hermes -z "..."` without `hermes setup`; the failure mode without it is a vendor model the local key may not use, which looks like a broken agent |
| The endpoint and the key are **not** baked | They arrive as environment variables from the provider profile, so a rotated key needs no rebuild (ADR 0005) |
| `mcp==2.3.0` is installed into Hermes' own interpreter | Hermes' MCP client imports `mcp.client.streamable_http`. The standalone installer does not install its `mcp` extra, so without this step a remote HTTP server can be configured and is then unusable at connect time |
| `mcp_servers.nestbox-hub` uses `${NESTBOX_MCP_URL}` and `${NESTBOX_MCP_TOKEN}` | Hermes interpolates `${VAR}` from the environment at load time. The credential stays out of the image and out of a file on disk, and an unset variable makes Hermes skip the server with a warning instead of calling it unauthenticated |
| The hub is reached over HTTP on the sandbox network | Every call then crosses the egress proxy and is visible in traffic with a tool name (ADR 0026). The stdio servers a pack adds later stay invisible, and the console says so |

`NESTBOX_MCP_URL` defaults to `http://mcp-hub:8080/mcp` so only the token has to
be provided. A per-node hub gets its own URL from the environment; the config
does not change.

## entrypoint.sh

Runs before the container's command and does one thing twice-safely:

- If an egress CA is mounted (`NESTBOX_EGRESS_CA`, default
  `/secrets/egress-ca.crt`), append it to the `certifi` bundle of Hermes'
  interpreter and export `SSL_CERT_FILE` / `REQUESTS_CA_BUNDLE` /
  `CURL_CA_BUNDLE` for login shells. The system store is not writable as the
  sandbox user, and the CA is generated per deployment, so it cannot be baked.

It writes no MCP configuration: that is static in `config.yaml`, so a restart
cannot half-write it and a credential is never written to disk by this script.

## Rules

1. The agent runs as `nestbox`, never root, and the image adds no capability.
2. No credential in the image, in `config.yaml`, or in a layer: only `${VAR}`
   references and the variable **names** the registry records.
3. The hub is configured here, but whether it is *reachable* is the network's
   answer: a policy without an allow rule for `mcp-hub` makes `hermes mcp test`
   fail, and that failure is real, not a configuration bug.
