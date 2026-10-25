# Control Plane Architecture
Status: planned contract; public site currently offers only local seed generation.
Services: React Admin web-console, FastAPI control-api, policy-service, gateway, OpenShell, sandbox-runtime, image-builder and event-forwarder.
API: sandboxes, model-sandboxes, policies, mounts, tool-catalog, events and artifacts are versioned under /v1.
Invariant: no mount, tool, MCP capability or egress route exists without an explicit grant and audit event.
