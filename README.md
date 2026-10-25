<p align="center"><img src="assets/brand/operations-center.jpg" alt="Nestbox private AI operations center" width="100%"></p>

<h1 align="center">Nestbox</h1>

<p align="center"><b>Your private AI operations center.</b><br>Open-source, self-hosted infrastructure for governed AI agents, local models and protected OCI applications.</p>

<p align="center"><a href="https://nestbox.mmbits.com/">🌐 Visit the Nestbox website</a> · <a href="https://buymeacoffee.com/matthias1232">☕ Buy me a coffee</a> · <a href="#capabilities">Capabilities</a> · <a href="#quickstart">Quickstart</a> · <a href="docs/">Docs</a></p>

> **Built in public.** Nestbox started at Dev Community Day Berlin. The local recipe and seed-bundle flow works today; the Control Center, policy editor, model lifecycle and remote builds are active development work.
>
> **Support the project.** Star the repository and follow development on the [Nestbox website](https://nestbox.mmbits.com/). GitHub Sponsors / Buy Me a Coffee will be linked here when the maintainer configures a support destination.

---

## What Nestbox is

Nestbox is an open-source, self-hosted control plane for AI agents, local LLMs and protected OCI applications. Deploy a workload into an isolated sandbox, attach only the required project volume, assign reusable policy, and see resulting state from one operations surface.

Nestbox is designed around **OpenShell**, the NVIDIA safe runtime for controlled AI-agent execution. It adds policy, project mounts, local model sandboxes, approved tools, MCP capabilities, image recipes, gateway observability and deployment artifacts.

> **Independent project.** NVIDIA, OpenShell, NemoClaw, Nemotron, DGX Spark, GB10 Grace Blackwell, NIM and NeMo are NVIDIA product names or trademarks. Nestbox is not affiliated with or endorsed by NVIDIA.

## Capabilities

| Capability | Direction |
| --- | --- |
| **Agent sandboxes** | Run OpenShell, NemoClaw, OpenClaw, OpenCode, OpenChamber, Hermes, Claude Code, OpenAI-compatible and custom runtimes in isolated workspaces. |
| **Local model sandboxes** | Select validated NVIDIA Nemotron profiles with lifecycle, health, logs and controlled consumers. |
| **Policy editor** | Validate, version, diff, assign, revoke and roll back policy for mounts, tools, DNS, TLS and destinations. |
| **Project mounts** | Explicit read-only or read-write grants; detach and reassign without a global shared filesystem. |
| **Tools and MCP** | Choose Git, approved GitHub access, developer tools, CLI packages and MCP capabilities per sandbox. |
| **Gateway observability** | See allowed and blocked destinations, route decisions, status and latency per sandbox. |
| **OCI application sandboxes** | Plan protected image deployments from Docker Hub, private OCI registries and approved image sources. |
## Visual product tour
<p><img src="assets/brand/model-deployment.jpg" alt="Local model deployment into an isolated sandbox" width="49%"> <img src="assets/brand/tool-template.jpg" alt="Agent workspace with controlled tools" width="49%"></p>
**Local model sandboxes:** choose a validated Nemotron profile, deploy a dedicated runtime, then start, stop, inspect health and read logs.
**Agent templates and tools:** choose a runtime, preinstalled tools, MCP capabilities, mounts and policy before deployment.
<p><img src="assets/brand/security-flow.jpg" alt="Policy gateway controls sandbox traffic" width="49%"> <img src="assets/brand/deployment-targets.jpg" alt="Deployment targets for Nestbox" width="49%"></p>
**Policy and egress:** forward proxy is the intended default. Transparent proxy and TLS inspection are explicit opt-in, sandbox-scoped and disabled by default.
**Deploy where compute lives:** generate inspectable seed bundles for bare metal, Proxmox, a cloud VM, WSL and local GPU hardware.
## Architecture
React Admin web console → FastAPI control API → gateway and policy enforcement → OpenShell runtime → isolated agent, model, tool-enabled or OCI application sandbox.
Each project mount, tool, MCP capability and egress route is an explicit capability grant.
## Secure network traffic
The governed forward proxy is the intended default: operators can inspect destinations, policy decisions, status and latency without TLS decryption. Transparent proxy observation is planned as explicit opt-in for an authorized sandbox. A sandbox-scoped trust CA is issued only after approval and can be revoked when that mode is disabled.
## Local AI, NVIDIA cloud and DGX Spark-class hardware
Nestbox targets local GPU workstations, DGX Spark-class GB10 Grace Blackwell systems, provider-neutral GPU VMs and NVIDIA cloud resources. NVIDIA Nemotron, NIM, NeMo, NemoClaw, OpenShell and NVIDIA AI Enterprise can be part of a workload stack; Nestbox focuses on the policy-first operations layer around it.
Hardware and model fit depend on quantisation, context length, inference backend and concurrent workloads. A 120B-class model must be validated on the exact target configuration; Nestbox does not make blanket performance claims.
## Quickstart
```bash
git clone https://github.com/matthias1232/nestbox.git
cd nestbox
docker compose up -d --build
```
See [docs](docs/) and the [Nestbox website](https://nestbox.mmbits.com/) for installation guidance and the product tour.
## Project origin and thanks
Nestbox began during the Dev Community Day Berlin hackathon. The early project concept, website and presentation were accelerated with OpenAI credits. Thank you to OpenAI for helping make the first public project foundation possible.
## License
Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
