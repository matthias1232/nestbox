# Phase 1 review — what is proven, what is partial, what is not done

Written after the implementation, against the acceptance criteria in the
phase-1 plan and the M1 rebuild plan. Each line names the evidence; a claim
without evidence is listed as not done.

Verified on the appliance VM200 (`192.168.55.50`), branch `feat/m1-rebuild`,
2026-10-27.

## The reference environment ("Fertig" criteria of the phase-1 plan)

| # | Criterion | State | Evidence |
|---|---|---|---|
| 1 | Runtime, packs, tool profile and bindings configurable via API/GUI only | **Partial** | Packs resolve image + command as data (`pack_defaults`); the provider binding (Hermes → model endpoint) is a `provider_profiles` row edited in the console. The tool catalogue/profile module and a dedicated bindings table are not built; the tool set is still the image's Dockerfile. |
| 2 | GUI shows the validated plan before deploy (images, digests, tools, ports, bindings, order) | **Not done** | There is no deploy-plan screen. A create is one request; its inputs are visible afterwards in the drawer. |
| 3 | One deployment job creates gateway sandbox → waits for health → creates Hermes → configures its gateway binding | **Not done** | Sandboxes are created individually. The jobs table and API exist; no orchestrating job kind implements this sequence. |
| 4 | Real adapter, real runtime IDs; a simulated adapter cannot mark success | **Done for Docker, not for OpenShell** | `/api/v1/runtime` reports `docker-development`; container IDs, IPs and uptime are read from Docker. The official NVIDIA OpenShell gateway is not integrated; `gateway-01` is a registered row with a compose container behind it, not an OpenShell control path. |
| 5 | Smoke test reaches Hermes through the intended path and the GUI shows the result | **Done (direct path)** | `POST /sandboxes/{id}/exec` with `hermes -z "Reply with exactly: NESTBOX-E2E-OK"` returns `NESTBOX-E2E-OK`, exit 0, recorded in `audit_events`. The console's Shell tab shows the same (`UI-SHELL-OK`, screenshot). The path is sandbox → OmniRoute directly; there is no OpenClaw gateway hop. |
| 6 | `dev-01` creatable via GUI/API, Ubuntu 24.04, resolved default tool profile | **Done (minus profile)** | Created through the API; `cat /etc/os-release` → Ubuntu 24.04.5; `git`, `jq`, `rg` present. The profile is baked into the image, not resolved from a catalogue. |
| 7 | Stop, start, logs, delete for every sandbox; stable error codes | **Done** | Stop/start/restart/delete verified through the API and the row buttons in a real browser. Invalid transitions answer `sandbox.invalid_state`; unpinned images `request.validation_failed`; missing CSRF `auth.csrf_required`. |

## The admin interface (the user's stated goals)

| Goal | State | Evidence |
|---|---|---|
| Sandboxes listed with status, uptime, IP | Done | List returns `address`, `uptime_seconds`, `container_name`, `runtime_available` from one runner call per page. Browser screenshot shows both rows. |
| Click a sandbox, see details | Done | Drawer: stored row + runtime facts (container, IP, gateway, uptime, exit code, memory/CPU/PID limits, interfaces). |
| Logs in the overlay | Done | Logs tab reads `docker logs`; an empty tail explains why instead of looking broken. |
| Start/stop/restart/delete from the UI | Done | Row buttons, bulk bar; verified by clicking in Chromium. |
| Shell access from the GUI | Done (request/response) | Shell tab runs `bash -lc <line>` via exec; not an interactive PTY. |
| Network / topology view: which sandbox, which gateway, where traffic goes | Done | Cluster → OpenShell gateway → sandbox → egress, with placement edges (solid) and traffic edges (dashed). 4 nodes, 3+2 edges on the appliance. |
| Hermes runs and answers | Done | Plain `hermes -z` inside the sandbox answers; the model/provider default is baked into the image, endpoint and key arrive from the provider profile. |
| Visible at http://192.168.55.50:8080/ | Done | Every commit is deployed there; `feat/m1-rebuild` is pushed. |

## The model catalog and hardware fit

| Goal | State | Evidence |
|---|---|---|
| Broad catalog (NVIDIA, DeepSeek, Qwen, Llama, Gemma, Phi, CPU-only) | Done | 53 rows in `0013_model_catalog.sql`, each with publisher, licence, runtime, format, and min/recommended figures. |
| See own CPU, RAM, VRAM, unified/Dynamic RAM | Done | Probe reads `/proc/cpuinfo`, `/proc/meminfo`, `nvidia-smi`; the page shows CPU model and cores, system RAM, GPU memory or the unified pool, per-card list, and the source of the unified figure. |
| Two bars per resource: have versus need | Done | `fit.bars` is server-computed (`domain::model::resource_bars`); the page renders "have" and "need" tracks with minimum and recommended. |
| Live filter "can install / cannot" after Probe | Done | Probe switches the filter to "fits this machine" and shows the count; chips show live counts, and family/publisher/runtime/CPU-only filters narrow further. |
| Models that run without a GPU are installable on the appliance | Done | VM200 has no GPU; CPU and hybrid models classify as `cpu_installable` by RAM and cores instead of `no_gpu`. |
| Unified memory (DGX Spark / GB10) shown explicitly | Partial | Modelled and displayed, with the source note; no GB10 machine has been probed, so the path is unit-tested, not field-tested. |
| Install the weights | **Not done** | The button creates a job and records intent. No artefact is downloaded; the page says so. Phase 2. |

## Nodes, installs, and the builder

| Goal | State | Evidence on VM200 (2026-10-27) |
|---|---|---|
| A node is the unit a sandbox and a model live on | Done | `GET /nodes` answers one `local` node, online, 7.8 GiB, 4 cores, no GPU, 3 sandboxes; `sandboxes.node_id` is NOT NULL. |
| The budget is measurement minus reservations | Done | Installing `qwen3-1.7b` reserved 5.00 GiB: free fell 7.76 → 2.76 GiB. A second model needing 12 GiB answered `422 node.insufficient_resources`: "5.0 GiB of this node's 7.8 GiB are reserved by qwen3-1.7b. 2.8 GiB are free, short by 9.2 GiB." |
| Uninstall releases the reservation | Done | `released_ram_bytes: 5368709120`; free returned to 7.76 GiB. |
| A sandbox is refused when it cannot fit | Done | `32Gi` on the 7.8 GiB node → `422 node.insufficient_resources`, "The node has 7.8 GiB of RAM free; the request needs 32.0 GiB." `512Mi` → `201`, `node_name: local`. |
| An install creates a provider profile | Done | `local/qwen3-1.7b@local`, status `pending`, endpoint `http://model-qwen3-1-7b:8000/v1`. The inference environment filters on `reachable`. |
| The catalog is built from the repositories | Done | `POST /models/sync` and the daily `catalog-sync` schedule both run it: Hugging Face +27, Ollama +16, NGC +24 → 152 rows. A second run reports 0 added, 67 updated; the 85 curated rows are untouched (all `pinned`). |
| Baselines, packs, and tools are data | Done | 3 baselines (Ubuntu recommended), 9 packs, 23 tools with apt/rpm names; `pack_defaults` reads the table. |
| Agents inside a sandbox | Done (API) | Adding `hermes` answered the agent row and the generated supervisor; removing with `keep_persistent=false` answered `204`. The console tab exists. |
| Recipes and builds | Partial | Recipe CRUD, revision, Dockerfile generation, and `409 recipe.not_built` are proven. A real image build through the runner was queued (`202`) but not completed and observed on VM200. |
| Persistent roots and mounts | Done (API) | CRUD, one read-write mount per root enforced by index, mounts passed to the runtime from the database. The "rebuild the sandbox, the file is still there" proof needs a rebuilt container and was not run. |
| Builder UI, recipes screen, persistent-data screen | **Not done** | Three console packages (builder wizard, per-node models view beyond the budget line, recipes/persistence) did not land. The models page does show the node selector, the budget with reservations, the installs list, and the sync button. |
| Download weights, start a model server | **Not done** | Phase 2. The install reserves memory, creates the pending profile, and says so in its own response. |

## Bugs found by testing on the appliance, and fixed

Each of these would have shipped if the code had only been compiled:

- Redaction looped forever on a `Bearer` token.
- Quoted JSON keys were not redacted.
- Unified-memory hosts reported 0 GPUs.
- Sandbox list: ambiguous `updated_at` after the gateway join; mismatched
  cursor parameter types.
- RLS migration filtered on partitioned tables only; blanket policy hid the
  shared model catalogue; policies assumed every table has `environment_id`.
- `migrations/` missing from the image build; secrets ownership and service
  ordering in compose.
- "2Gi" rejected by Docker — memory limits are now normalised to bytes.
- The sandbox total showed 0 (planner estimate before ANALYZE).
- The drawer stayed on "loading…" (wrong response shape assumed).
- Create and restart derived health differently; the badge flipped.
- A redeploy during a stop left a sandbox in `stopping` forever; reads now
  settle a transitional row from the runtime.
- Exec route shadowed by the lifecycle catch-all in the runner.
- Hermes image build failed on `libatomic1`; plain `hermes -z` used the
  vendor's default model, which the key is not allowed to use.

## What is not done, stated plainly

1. **NVIDIA OpenShell as the runtime.** The adapter boundary exists and is
   the only thing that would change, but the real gateway is not wired. The
   topology's "OpenShell gateway" column is the registered gateway row, backed
   today by Docker.
2. **OpenClaw gateway sandbox and the Hermes → OpenClaw binding.** The
   `openclaw` pack has an image and a default command but was not deployed or
   proven; Hermes talks to the model endpoint directly.
3. **Tool catalogue and tool profiles** as data with APT search. Tools are in
   Dockerfiles.
4. **Deployment plan and the orchestrating deploy job.**
5. **Interactive terminal (PTY).** Exec is request/response.
6. **Ingress renderer, certificates, address assignment UI.** Schema and API
   surface exist from the M1 plan; no console screens and no execution path.
7. **Live traffic from a real gateway.** The stream, buffer, pause, and draft
   workflow work with generated samples; no gateway publishes real events yet.
8. **Interactive policy enforcement.** Policies, rules, drafts, and windows
   are stored and versioned; nothing applies them to a container.
9. **Model artefact download.** The catalog, the requirements, and the fit are
   real; an install job does not fetch weights yet, so no model becomes usable
   from the Models page. This is the largest honest gap in the model feature.

## Suggested order for phase 2

1. Integrate the official OpenShell gateway behind the existing adapter trait
   and make `gateway-01` real. Everything else in the topology gets truthful
   at once.
2. Deploy the `openclaw` pack as a sandbox, prove the Hermes → OpenClaw path,
   and store it as a binding row.
3. The deploy job: validate → gateway sandbox → health → Hermes → handshake,
   logged per step, visible in Operations.
4. Tool catalogue/profiles as data, rendering into the image build.
5. Telemetry from the real gateway into the existing stream.
6. Policy enforcement against the real gateway.

Items 1–3 are what turn the current "working development reference" into the
"Phase-1 reference environment" as the plan defines it.
