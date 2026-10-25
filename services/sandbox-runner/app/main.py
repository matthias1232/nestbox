import os
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
from docker.types import Mount
import docker
app = FastAPI(title="Nestbox Sandbox Runner")
ALLOWED_TOOL_PROFILES = set(os.getenv("NESTBOX_ALLOWED_TOOL_PROFILES", "git,github-mcp,web-research").split(","))
ALLOWED_IMAGES = set(os.getenv("NESTBOX_ALLOWED_IMAGES", "alpine:3.20").split(","))
client = docker.from_env()
class SandboxRequest(BaseModel):
    name: str = Field(pattern="^[a-z0-9-]{1,63}$")
    image: str = "alpine:3.20"
    command: list[str] = ["sleep", "infinity"]
    runtime: str = "custom"
    policy_id: str | None = None
    volume: str | None = None
    mode: str = "read-write"
    memory_limit: str = "512m"
    cpu_limit: float = 1.0
    pids_limit: int = 128
    tool_profiles: list[str] = []
def container_name(name): return "nestbox-sandbox-" + name
def volume_name(name): return "nestbox-volume-" + name
@app.get("/healthz")
def healthz(): return {"status":"ok","service":"sandbox-runner"}
@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: SandboxRequest):
    if body.image not in ALLOWED_IMAGES: raise HTTPException(422, "image is not approved")
    if any(profile not in ALLOWED_TOOL_PROFILES for profile in body.tool_profiles): raise HTTPException(422, "tool profile is not approved")
    if body.volume and (not body.volume.replace("-", "").isalnum() or body.mode not in {"read-only", "read-write"}): raise HTTPException(422, "invalid mount capability")
    name = container_name(body.name)
    try: container = client.containers.get(name); return {"id":container.id,"name":name,"status":container.status}
    except docker.errors.NotFound: pass
    mounts = []
    if body.volume:
        client.volumes.create(name=volume_name(body.volume), labels={"nestbox.managed":"true"})
        mounts = [Mount(target="/workspace", source=volume_name(body.volume), type="volume", read_only=body.mode == "read-only")]
    try: container = client.containers.run(body.image, body.command, name=name, detach=True, network_disabled=True, read_only=True, cap_drop=["ALL"], security_opt=["no-new-privileges:true"], mem_limit=body.memory_limit, nano_cpus=int(body.cpu_limit * 1000000000), pids_limit=body.pids_limit, mounts=mounts, labels={"nestbox.managed":"true","nestbox.runtime":body.runtime,"nestbox.policy":body.policy_id or "none","nestbox.tools":";".join(body.tool_profiles)})
    except docker.errors.DockerException as error: raise HTTPException(400, str(error))
    return {"id":container.id,"name":name,"status":container.status}
@app.post("/v1/sandboxes/{name}/{action}")
def lifecycle(name: str, action: str):
    if action not in {"start", "stop", "restart", "remove"}: raise HTTPException(422, "unsupported action")
    try: container = client.containers.get(container_name(name))
    except docker.errors.NotFound: raise HTTPException(404, "sandbox not found")
    getattr(container, action)(force=True) if action == "remove" else getattr(container, action)()
    return {"id":container.id,"name":container.name,"action":action}
@app.get("/v1/sandboxes/{name}/logs")
def logs(name: str, tail: int = 200):
    if tail < 1 or tail > 1000: raise HTTPException(422, "tail must be 1..1000")
    try: container = client.containers.get(container_name(name))
    except docker.errors.NotFound: raise HTTPException(404, "sandbox not found")
    if container.labels.get("nestbox.managed") != "true": raise HTTPException(403, "unmanaged container")
    return {"name":container.name,"logs":container.logs(tail=tail).decode(errors="replace")}
