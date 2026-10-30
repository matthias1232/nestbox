# ADR 0039 — Media rendering is a governed capability, not a provider key in the sandbox

Status: Accepted
Date: 2026-10-29

## Context

A text model cannot draw a picture, cut a clip or write a track. An agent in a
sandbox needs a way to ask for one. Three shapes were possible:

1. **Give the agent a provider key** and let it call the image API directly. The
   key would live in the sandbox, the model would be whatever the agent chose,
   and the appliance would learn about the spend from the provider's bill.
2. **Let the agent call the provider through the egress proxy with a model name
   in the tool arguments.** Policy could allow the host, but not the model, and a
   prompt could name any model the deployment's key can reach.
3. **Put a small MCP server in front and keep the decision in the control
   plane.** The agent calls `render_image`; the control plane resolves which
   model that means for *this* sandbox, calls the provider with a credential the
   sandbox never sees, and stores the asset.

Options 1 and 2 fail the product rule that a sandbox holds no long-lived secret
and that what an agent may use is an operator decision. They also make "which
model drew this?" unanswerable after the fact.

## Decision

1. **Three MCP tools, one server.** `render_image`, `render_video` and
   `render_music` are advertised by `media-mcp`, an HTTP MCP server on the
   sandbox network. HTTP because a call through the proxy is a logged traffic
   event with a tool name (ADR 0026); stdio would be invisible.
2. **The model is never an argument.** A tool takes a prompt, and optionally a
   size or a duration. Which model answers comes from two settings rows:
   `media_settings` (the environment default and the allow-list) and
   `sandbox_media_settings` (one sandbox's own choice). A NULL kind inherits, so
   changing the default reaches every sandbox that did not override it.
3. **The credential stays in the control plane.** `media-mcp` holds only a
   bridge token; the provider key is resolved at use by `media_service`, the same
   way an inference credential is (ADR 0016).
4. **Two tokens, not one.** The agent authenticates to `media-mcp` with
   `NESTBOX_MEDIA_TOKEN`; `media-mcp` authenticates to the control plane with
   `NESTBOX_MEDIA_BRIDGE_TOKEN`. An agent that reads its own token therefore
   cannot call the bridge route directly.
5. **The attachment is the grant.** The bridge refuses a render for a sandbox
   that does not have `nestbox-media` attached and enabled, even when a model is
   configured. Detaching the server removes the tool from the agent's config on
   the next re-apply, so the console and the agent cannot disagree.
6. **The sandbox is named by its config, not by the call.**
   `X-Nestbox-Sandbox` is written into the agent's MCP entry by the control plane;
   a prompt cannot render on another sandbox's quota.
7. **Every render is a row.** `media_renders` records the prompt, the model, the
   provider, the status, the duration and the asset locator. The asset lives on a
   volume under `NESTBOX_MEDIA_DIR`, named by the render id, and is served through
   the API after a tenant-scoped lookup.
8. **An admin sets the default and the allow-list; an operator narrows their own
   sandbox.** `MediaConfigure` is admin (it decides what every sandbox inherits
   and what may be billed); a sandbox's own choice is `SandboxWrite` and is
   refused unless the environment allows the provider.
9. **Graphify joins the registry as a stdio server.** Attaching it writes the
   `graphify --mcp` entry into the agent's config, pointing at the persistent
   knowledge graph (`/home/nestbox/brain/graphify-out/graph.json`). It is stdio,
   so its calls are not egress — the registry and the console say so rather than
   implying coverage.

## Consequences

* A sandbox needs two MCP attachments to use media: the server, and a model
  configured for the kind it wants. Both failures name the missing half.
* A render is synchronous from the agent's point of view and bounded by a
  three-minute client timeout. A provider that takes longer fails the render with
  its reason; there is no queue yet.
* Assets are removed with their render row: `POST /media/purge` deletes rows and
  files older than a number of days and audits the count, and
  `DELETE /media/renders/{id}` removes one. A deployment schedules the purge; the
  appliance does not run it on a timer of its own yet.
* The provider protocol is OpenAI-shaped plus three documented paths
  (`/images/generations`, `/videos/generations`, `/audio/music`), overridable per
  provider with `labels.media_path_image` and friends. A provider that speaks a
  different shape needs an adapter, not a second default.

## Not decided here

* **Per-user model visibility** — decided in ADR 0040 (`user_media_grants`):
  the environment allow-list is the outer bound, a person's release narrows it.
* **Async rendering and progress.** A job table would make a long video
  observable while it renders. The render row already carries the states
  (`queued`, `running`, `done`, `error`); nothing yet moves them in the
  background.
