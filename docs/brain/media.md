# Media rendering and MCP tools

An agent that writes text cannot draw. Nestbox gives it three tools —
`render_image`, `render_video`, `render_music` — and keeps the decision about
*which model* answers outside the sandbox.

## The shape

```
agent ──MCP──▶ media-mcp ──bridge token──▶ control API ──provider key──▶ provider
                  │                            │
                  │                            └── media_renders row + asset on a volume
                  └── X-Nestbox-Sandbox: <name> (written by the control plane)
```

* `media-mcp` (`services/tool-images/media-mcp`) advertises the three tools. It
  holds no provider credential and never names a model.
* The control plane resolves the model: the sandbox's own choice first, the
  environment default second (`sandbox_media_settings` over `media_settings`).
* The provider key is resolved at use, exactly like an inference credential
  (ADR 0016), and never enters the sandbox.
* Every render is a row, and the asset is a file named by the render id under
  `NESTBOX_MEDIA_DIR`.

## Who may use it

Three gates, and all three must pass:

1. **Attached.** `nestbox-media` has to be attached to the sandbox. Attaching is
   also what compiles the allow rule for the server's host and port
   (`domain::policy::rules_from_mcp`), so the sandbox cannot reach the endpoint
   without the grant that the console shows.
2. **Allowed.** A sandbox's choice must be on the environment's allow-list; an
   empty list means every provider in the environment. An admin sets the default
   and the list (`media.configure`); an operator narrows their own sandbox
   (`sandbox.write`).
3. **Configured.** The kind must resolve to a model. An unconfigured kind fails
   with a sentence that names the missing half — "no image model is configured
   for this sandbox or its environment".

## Why the model is not a tool argument

If the agent could pass `model`, the operator's choice would be advisory, the
bill would follow the prompt, and "which model drew this?" would be unanswerable
later. The same reasoning as policy: the sandbox does not decide what it may do.

## Graphify

`graphify` is in the MCP registry as a **stdio** server: `graphify --mcp --graph
/home/nestbox/brain/graphify-out/graph.json`. Attaching it writes that entry into
the agent's config on the persistent home, so the agent queries the local
knowledge graph instead of re-reading the repository. stdio means no egress and
therefore no traffic log — the registry says so rather than implying coverage.

## Agent configuration

Attaching or detaching a server rewrites the agent's configuration immediately
(`sandbox_service::reconfigure_agent`). The `mcpServers` list the control plane
writes is authoritative: a detached server disappears. Everything else in the
file — a login, history — is merged and survives.

A server that needs a token gets a **reference** (`${NESTBOX_MEDIA_TOKEN}` or
`{env:…}`), never the value: the file lives on a persistent home.

## Honest limits

* Rendering is synchronous with a three-minute client timeout; there is no queue.
* Assets are removed with their row by `POST /media/purge` (admin) or one at a
  time by `DELETE /media/renders/{id}`; scheduling the purge is a deployment
  decision.
* The provider protocol is OpenAI-shaped plus three documented paths, overridable
  per provider via `labels.media_path_image`, `media_path_video`,
  `media_path_music`.
* A person's release (`user_media_grants`) narrows the environment allow-list and
  can make the choice read-only; an admin is never restricted (ADR 0040).
