import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "services" / "control-api"))
from fastapi.testclient import TestClient
from app.main import app
client = TestClient(app)
def test_health_and_catalog():
    assert client.get("/healthz").status_code == 200
    assert client.get("/v1/tool-catalog").status_code == 200
def test_sandbox_policy_mount_and_artifact_flow():
    policy = client.post("/v1/policies", json={"name":"research"})
    assert policy.status_code == 201
    sandbox = client.post("/v1/sandboxes", json={"name":"research-01","policy_id":policy.json()["id"]})
    assert sandbox.status_code == 201
    ident = sandbox.json()["id"]
    assert client.post(f"/v1/sandboxes/{ident}/lifecycle", json={"action":"start"}).json()["status"] == "running"
    assert client.post("/v1/mounts", json={"sandbox_id":ident,"volume":"project","mode":"read-write"}).status_code == 201
    assert client.post("/v1/artifacts/seed-bundles", json={"hostname":"lab"}).status_code == 201
