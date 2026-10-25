from datetime import datetime, UTC
from uuid import uuid4
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
app = FastAPI(title="Nestbox Control API", version="0.1.0")
state = {"sandboxes": {}, "policies": {}, "mounts": {}, "events": []}
tools = [{"id":"git","kind":"package"},{"id":"github","kind":"mcp"},{"id":"web-research","kind":"mcp"}]
def stamp(): return datetime.now(UTC).isoformat()
def audit(kind, subject, detail=None):
    value={"id":str(uuid4()),"time":stamp(),"kind":kind,"subject":subject,"detail":detail or {}}
    state["events"].append(value); return value
class Sandbox(BaseModel):
    name: str = Field(min_length=1, max_length=80)
    runtime: str = "custom"
    template: str = "custom"
    tools: list[str] = []
    policy_id: str | None = None
class Policy(BaseModel):
    name: str = Field(min_length=1, max_length=80)
    allowed_destinations: list[str] = []
    proxy_observation: bool = False
    tls_inspection: bool = False
class Mount(BaseModel):
    sandbox_id: str
    volume: str
    mode: str = Field(pattern="^(read-only|read-write)$")
class Action(BaseModel):
    action: str = Field(pattern="^(start|stop|restart)$")
@app.get("/healthz")
def healthz(): return {"status":"ok","mode":"development-preview","time":stamp()}
@app.get("/v1/sandboxes")
def list_sandboxes(): return list(state["sandboxes"].values())
@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: Sandbox):
    ident=str(uuid4()); value=body.model_dump()|{"id":ident,"status":"stopped","created_at":stamp()}
    state["sandboxes"][ident]=value; audit("sandbox.created",ident,{"runtime":body.runtime}); return value
@app.get("/v1/policies")
def list_policies(): return list(state["policies"].values())
@app.post("/v1/policies", status_code=201)
def create_policy(body: Policy):
    ident=str(uuid4()); value=body.model_dump()|{"id":ident,"version":1,"created_at":stamp()}
    state["policies"][ident]=value; audit("policy.created",ident); return value
@app.get("/v1/tool-catalog")
def tool_catalog(): return tools
@app.get("/v1/events")
def list_events(): return state["events"]
@app.get("/v1/mounts")
def list_mounts(): return list(state["mounts"].values())
@app.post("/v1/mounts", status_code=201)
def create_mount(body: Mount):
    if body.sandbox_id not in state["sandboxes"]: raise HTTPException(404,"sandbox not found")
    ident=str(uuid4()); value=body.model_dump()|{"id":ident,"created_at":stamp()}; state["mounts"][ident]=value; audit("mount.attached",ident,value); return value

@app.post("/v1/sandboxes/{sandbox_id}/lifecycle")
def lifecycle(sandbox_id: str, body: Action):
    value = state["sandboxes"].get(sandbox_id)
    if not value:
        raise HTTPException(404, "sandbox not found")
    value["status"] = "running" if body.action in {"start", "restart"} else "stopped"
    audit("sandbox." + body.action, sandbox_id, {"status": value["status"]})
    return value
