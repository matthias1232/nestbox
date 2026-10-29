"""Acceptance for the image-builder catalogues, templates and verified builds.

Runs against the live appliance with a short-lived bearer token inserted into
`api_tokens`. The token row is removed at the end; nothing is left behind.
"""
import hashlib
import json
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:8080/api/v1"
TOKEN = "acc-" + secrets.token_hex(12)


def psql(sql: str) -> None:
    subprocess.run(
        ["docker", "exec", "-i", "nestbox-postgres-1", "psql", "-U", "nestbox", "-d", "nestbox", "-v", "ON_ERROR_STOP=1"],
        input=sql.encode(),
        check=True,
        capture_output=True,
    )


def call(path: str, method: str = "GET", body=None, expect=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(
        BASE + path,
        data=data,
        method=method,
        headers={"authorization": f"Bearer {TOKEN}", "content-type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=900) as answer:
            status, payload = answer.status, answer.read()
    except urllib.error.HTTPError as error:
        status, payload = error.code, error.read()
    if expect is not None and status != expect:
        raise SystemExit(f"{method} {path} -> {status}: {payload[:400]!r}")
    try:
        return status, json.loads(payload or b"null")
    except json.JSONDecodeError:
        return status, payload


def main() -> int:
    digest = hashlib.sha256(TOKEN.encode()).hexdigest()
    psql(
        "INSERT INTO api_tokens (user_id, name, token_hash, expires_at) "
        f"SELECT id, 'acceptance', '{digest}', now() + interval '30 minutes' "
        "FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1;\n"
    )
    try:
        status, catalogue = call("/packages?baseline=ubuntu-24.04&q=ripgrep", expect=200)
        print("catalogue:", catalogue["available"], catalogue["detail"], [hit["name"] for hit in catalogue["items"]][:3])

        status, templates = call("/image-templates", expect=200)
        print("templates listed:", len(templates["items"]))

        name = "acceptance-" + secrets.token_hex(3)
        status, template = call(
            "/image-templates",
            "POST",
            {
                "name": name,
                "description": "acceptance run",
                "baseline_key": "ubuntu-24.04",
                "extra_packages": ["jq"],
                "repositories": [],
                "mcp_server_keys": [],
                "is_default": True,
                "is_shared": True,
            },
            expect=201,
        )
        print("template created:", template["name"], "default:", template["is_default"])

        status, default = call("/image-templates/default", expect=200)
        print("default template:", (default["template"] or {}).get("name"))

        status, recipe = call(
            "/recipes",
            "POST",
            {
                "name": "acc-" + secrets.token_hex(3),
                "baseline_key": "ubuntu-24.04",
                "tools": ["git"],
                "agents": [],
                "resources": {"memory_limit": "1Gi", "cpu_limit": 1, "gpu_count": 0, "pids_limit": 256},
                "project_mounts": [],
                "node": None,
            },
            expect=201,
        )
        print("recipe created:", recipe["name"], "revision", recipe["revision"])

        status, applied = call(
            f"/recipes/{recipe['id']}/apply-template",
            "POST",
            {"template_id": template["id"]},
            expect=200,
        )
        print("template applied: revision", applied["revision"], "packages", applied["extra_packages"])

        status, queued = call(f"/recipes/{recipe['id']}/build", "POST", {})
        if status not in {200, 202}:
            raise SystemExit(f"build -> {status}: {queued}")
        print("build queued:", queued.get("job_id"), "reused:", queued.get("reused"))

        for _ in range(120):
            time.sleep(5)
            status, fresh = call(f"/recipes/{recipe['id']}", expect=200)
            if fresh["state"] in {"built", "failed"}:
                break
        print("build state:", fresh["state"], "image:", fresh["image_ref"], "error:", fresh["build_error"])
        if fresh["state"] != "built":
            return 1

        image = fresh["image_ref"]
        check = subprocess.run(
            ["docker", "run", "--rm", "--entrypoint", "jq", image, "--version"],
            capture_output=True,
            text=True,
        )
        print("jq in image:", check.stdout.strip() or check.stderr.strip())
        # Leave no acceptance rows or images behind on a shared test box.
        subprocess.run(["docker", "image", "rm", image], capture_output=True)
        call(f"/recipes/{recipe['id']}", "DELETE")
        call(f"/image-templates/{template['id']}", "DELETE")
        return 0 if check.returncode == 0 else 1
    finally:
        psql(f"DELETE FROM api_tokens WHERE name = 'acceptance' AND token_hash = '{digest}';\n")


if __name__ == "__main__":
    sys.exit(main())
