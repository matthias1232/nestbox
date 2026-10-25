from datetime import datetime, UTC
from uuid import uuid4
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
app = FastAPI(title="Nestbox Control API", version="0.1.0")
state = {"sandboxes": {}, "models": {}, "policies": {}, "mounts": {}, "events": [], "artifacts": {}}
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
    syslog_enabled: bool = False
class ModelSandbox(BaseModel):
    name: str = Field(min_length=1, max_length=80)
    profile: str = Field(min_length=1, max_length=120)
    hardware_target: str = "local-gpu"

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
    if body.tls_inspection: raise HTTPException(422, "TLS inspection is disabled in the development preview")
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
@app.get("/v1/artifacts")
def list_artifacts(): return list(state["artifacts"].values())
@app.post("/v1/artifacts/seed-bundles", status_code=201)
def create_seed_bundle(body: dict):
    ident=str(uuid4()); artifact={"id":ident,"kind":"seed-bundle","status":"rendered","created_at":stamp(),"recipe":body}
    state["artifacts"][ident]=artifact; audit("artifact.seed_bundle.rendered",ident); return artifact
@app.get("/v1/model-sandboxes")
def list_models(): return list(state["models"].values())
@app.post("/v1/model-sandboxes", status_code=201)
def create_model(body: ModelSandbox):
    ident=str(uuid4()); value=body.model_dump()|{"id":ident,"status":"stopped","created_at":stamp()}
    state["models"][ident]=value; audit("model_sandbox.created",ident,{"profile":body.profile}); return value
@app.post("/v1/model-sandboxes/{model_id}/lifecycle")
def model_lifecycle(model_id: str, body: Action):
    value=state["models"].get(model_id)
    if not value: raise HTTPException(404,"model sandbox not found")
    value["status"]="running" if body.action in {"start","restart"} else "stopped"
    audit("model_sandbox."+body.action,model_id,{"status":value["status"]}); return value
