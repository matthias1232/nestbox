import json, os
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field
app = FastAPI(title="Nestbox Control API", version="0.2.0")
STATE_PATH = Path(os.getenv("NESTBOX_STATE_PATH", "/data/state.json"))
RUNNER_URL = os.getenv("NESTBOX_RUNNER_URL", "http://sandbox-runner:9000")
DEFAULT = {"sandboxes": {}, "policies": {}, "events": []}
def load_state():
    try: return json.loads(STATE_PATH.read_text())
    except json.JSONDecodeError: return {"sandboxes": {}, "policies": {}, "events": []}
    except FileNotFoundError: return {"sandboxes": {}, "policies": {}, "events": []}
state = load_state()
def save_state():
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    temporary = STATE_PATH.with_suffix(".tmp")
    temporary.write_text(json.dumps(state))
    temporary.replace(STATE_PATH)
def now(): return datetime.now(UTC).isoformat()
def audit(kind, subject, detail=None):
    event = {"id": str(uuid4()), "time": now(), "kind": kind, "subject": subject, "detail": detail or {}}
    state["events"].append(event); save_state(); return event
def runner(method, path, payload=None):
    data = json.dumps(payload).encode() if payload else None
    request = Request(RUNNER_URL + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=15) as response: return json.loads(response.read())
    except (HTTPError, URLError) as error: raise HTTPException(502, str(error))
class Sandbox(BaseModel):
    name: str = Field(pattern="^[a-z0-9-]{1,63}$")
    runtime: str = "custom"
    template: str = "custom"
    policy_id: str | None = None
    volume: str | None = None
    mode: str = "read-write"
class Action(BaseModel):
    action: str = Field(pattern="^(start|stop|restart|remove)$")
class Policy(BaseModel):
    name: str
    allowed_destinations: list[str] = []
    proxy_observation: bool = False
    tls_inspection: bool = False
@app.get("/healthz")
def healthz(): return {"status":"ok","mode":"control-plane"}
@app.get("/v1/sandboxes")
def list_sandboxes(): return list(state["sandboxes"].values())
@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: Sandbox):
    if body.policy_id and body.policy_id not in state["policies"]: raise HTTPException(404,"policy not found")
    result = runner("POST", "/v1/sandboxes", {"name":body.name,"runtime":body.runtime})
    value = body.model_dump() | {"id":result["id"],"status":result["status"],"created_at":now()}
    state["sandboxes"][value["id"]]=value; audit("sandbox.created",value["id"],{"runtime":body.runtime}); return value
@app.post("/v1/sandboxes/{sandbox_id}/lifecycle")
def lifecycle(sandbox_id: str, body: Action):
    value=state["sandboxes"].get(sandbox_id)
    if not value: raise HTTPException(404,"sandbox not found")
    runner("POST","/v1/sandboxes/"+value["name"]+"/"+body.action)
    value["status"]="running" if body.action in {"start","restart"} else "stopped"
    audit("sandbox."+body.action,sandbox_id,{"status":value["status"]}); return value
@app.get("/v1/policies")
def list_policies(): return list(state["policies"].values())
@app.post("/v1/policies", status_code=201)
def create_policy(body: Policy):
    if body.tls_inspection: raise HTTPException(422,"TLS inspection is disabled")
    ident=str(uuid4()); value=body.model_dump()|{"id":ident,"version":1,"created_at":now()}
    state["policies"][ident]=value; audit("policy.created",ident); return value
@app.get("/v1/events")
def list_events(): return state["events"]
@app.get("/v1/egress-events")
def list_egress_events():
    path=Path("/data/gateway/access.json")
    return [json.loads(line) for line in path.read_text().splitlines() if line] if path.exists() else []
@app.get("/v1/sandboxes/{sandbox_id}/logs")
def sandbox_logs(sandbox_id: str, tail: int = 200):
    value = state["sandboxes"].get(sandbox_id)
    if not value: raise HTTPException(404, "sandbox not found")
    return runner("GET", "/v1/sandboxes/" + value["name"] + "/logs?tail=" + str(tail))
