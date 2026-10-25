import os
import tempfile
import sys
os.environ["NESTBOX_STATE_PATH"] = tempfile.mktemp(prefix="nestbox-test-", suffix=".json")
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
def test_model_sandbox_and_tls_guard():
    model = client.post("/v1/model-sandboxes", json={"name":"nemotron-local","profile":"nemotron-nano"})
    assert model.status_code == 201
    model_id = model.json()["id"]
    result = client.post("/v1/model-sandboxes/" + model_id + "/lifecycle", json={"action":"start"})
    assert result.json()["status"] == "running"
    assert client.post("/v1/policies", json={"name":"inspect","tls_inspection":True}).status_code == 422
def test_registry_and_image_plan():
    registry = client.post("/v1/registries", json={"name":"Docker Hub","url":"https://registry-1.docker.io","kind":"docker-hub"})
    assert registry.status_code == 201
    registry_id = registry.json()["id"]
    plan = client.post("/v1/image-deployments", json={"registry_id":registry_id,"image":"library/nginx:latest","sandbox_name":"protected-app"})
    assert plan.status_code == 202
    assert plan.json()["protect_with_openshell"] is True
def test_egress_events_empty_without_gateway_log():
    assert client.get("/v1/egress-events").status_code == 200
