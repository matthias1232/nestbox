import os
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
import docker
app = FastAPI(title="Nestbox Sandbox Runner")
client = docker.from_env()
class SandboxRequest(BaseModel):
    name: str = Field(pattern="^[a-z0-9-]{1,63}$")
    image: str = "alpine:3.20"
    command: list[str] = ["sleep", "infinity"]
    runtime: str = "custom"
def container_name(name): return "nestbox-sandbox-" + name
@app.get("/healthz")
def healthz(): return {"status":"ok","service":"sandbox-runner"}
@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: SandboxRequest):
    name = container_name(body.name)
    try: container = client.containers.get(name); return {"id":container.id,"name":name,"status":container.status}
    except docker.errors.NotFound: pass
    try: container = client.containers.run(body.image, body.command, name=name, detach=True, network_disabled=True, read_only=True, cap_drop=["ALL"], security_opt=["no-new-privileges:true"], labels={"nestbox.managed":"true","nestbox.runtime":body.runtime})
    except docker.errors.DockerException as error: raise HTTPException(400, str(error))
    return {"id":container.id,"name":name,"status":container.status}
@app.post("/v1/sandboxes/{name}/{action}")
def lifecycle(name: str, action: str):
    if action not in {"start", "stop", "restart", "remove"}: raise HTTPException(422, "unsupported action")
    try: container = client.containers.get(container_name(name))
    except docker.errors.NotFound: raise HTTPException(404, "sandbox not found")
    getattr(container, action)(force=True) if action == "remove" else getattr(container, action)()
    return {"id":container.id,"name":container.name,"action":action}
