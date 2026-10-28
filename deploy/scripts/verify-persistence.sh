#!/bin/bash
# The acceptance row nobody had proven: "recreate the sandbox, the file is
# still in the home". A bind mount is fixed at container creation, so the
# proof is delete the sandbox (container goes away), instantiate the same
# recipe again (new container, same root), read the file back.
set -u
B=http://127.0.0.1:8080/api/v1
J=/tmp/jar41.txt; rm -f $J
curl -s -c $J -X POST $B/auth/login -H 'content-type: application/json' \
  -d '{"email":"admin@nestbox.local","password":"08e9e4ded664a0234af4b8e4d3879fc37f9d"}' > /tmp/l41.json
C=$(python3 -c "import json;print(json.load(open('/tmp/l41.json')).get('csrf_token',''))")
api() { curl -s -b $J -H "x-csrf-token: $C" -H 'content-type: application/json' "$@"; }
say() { echo "== $* =="; }
show_exec() { python3 -c "
import json;d=json.load(open('$1'))
print('  output:', repr((d.get('output') or '').strip()))
print('  exit:', d.get('exit_code'), 'truncated:', d.get('truncated'))
" 2>/dev/null; }

say "create a project root"
SLUG="proof-$RANDOM"
api -X POST "$B/persistent-roots" \
  -d "{\"kind\":\"project\",\"slug\":\"$SLUG\"}" -o /tmp/root41.json -w 'root %{http_code}\n'
ROOT=$(python3 -c "import json;print(json.load(open('/tmp/root41.json')).get('id',''))")
python3 -c "
import json;d=json.load(open('/tmp/root41.json'))
print('  id  ', d.get('id')); print('  host', d.get('host_path'))
" 2>/dev/null

say "create a recipe that mounts it at /workspace"
api -X POST "$B/recipes" \
  -d "{\"name\":\"$SLUG\",\"baseline_key\":\"ubuntu-24.04\",\"tools\":[\"git\"],\"agents\":[],\"resources\":{\"memory_limit\":\"512Mi\",\"cpu_limit\":1,\"gpu_count\":0,\"pids_limit\":256},\"project_mounts\":[{\"root_id\":\"$ROOT\",\"mount_path\":\"/workspace\",\"read_only\":false}],\"node\":\"local\"}" \
  -o /tmp/rec41.json -w 'recipe %{http_code}\n'
REC=$(python3 -c "import json;print(json.load(open('/tmp/rec41.json')).get('id',''))")

say "build it"
api -X POST "$B/recipes/$REC/build" -d '{}' -o /tmp/b41.json -w 'build %{http_code}\n'
for i in $(seq 1 40); do
  sleep 5
  api "$B/recipes/$REC" -o /tmp/rr41.json
  st=$(python3 -c "import json;print(json.load(open('/tmp/rr41.json')).get('state','?'))" 2>/dev/null)
  [ "$st" = "built" ] && break
  [ "$st" = "failed" ] && { echo "build failed"; python3 -c "import json;print(json.load(open('/tmp/rr41.json')).get('build_error'))"; exit 1; }
done
echo "  state=$st image=$(python3 -c "import json;print(json.load(open('/tmp/rr41.json')).get('image_ref'))" 2>/dev/null)"

say "first instantiate"
api -X POST "$B/recipes/$REC/instantiate" -d "{\"name\":\"$SLUG-1\"}" -o /tmp/sb41.json -w 'sandbox %{http_code}\n'
SB1=$(python3 -c "import json;print(json.load(open('/tmp/sb41.json')).get('id',''))")
for i in $(seq 1 30); do
  sleep 4
  api "$B/sandboxes/$SB1" -o /tmp/s42.json
  st=$(python3 -c "import json;print(json.load(open('/tmp/s42.json')).get('status','?'))" 2>/dev/null)
  [ "$st" = "running" ] && break
done
echo "  status=$st"

say "is the mount there at all"
api -X POST "$B/sandboxes/$SB1/exec" \
  -d '{"command":["bash","-lc","ls -la /workspace; mount | grep workspace"],"timeout_seconds":60}' \
  -o /tmp/e42.json -w 'exec %{http_code}\n'
show_exec /tmp/e42.json

say "write the proof file from inside the container"
api -X POST "$B/sandboxes/$SB1/exec" \
  -d '{"command":["bash","-lc","echo \"proof $(date -u +%FT%TZ)\" > /workspace/proof.txt; cat /workspace/proof.txt"],"timeout_seconds":60}' \
  -o /tmp/e43.json -w 'exec %{http_code}\n'
show_exec /tmp/e43.json

say "delete the sandbox - the container goes away"
api -X DELETE "$B/sandboxes/$SB1" -o /dev/null -w 'delete %{http_code}\n'
api "$B/sandboxes/$SB1" -o /dev/null -w 'get-after-delete %{http_code}\n'

say "instantiate the same recipe again - new container, same root"
api -X POST "$B/recipes/$REC/instantiate" -d "{\"name\":\"$SLUG-2\"}" -o /tmp/sb42.json -w 'sandbox %{http_code}\n'
SB2=$(python3 -c "import json;print(json.load(open('/tmp/sb42.json')).get('id',''))")
for i in $(seq 1 30); do
  sleep 4
  api "$B/sandboxes/$SB2" -o /tmp/s43.json
  st=$(python3 -c "import json;print(json.load(open('/tmp/s43.json')).get('status','?'))" 2>/dev/null)
  [ "$st" = "running" ] && break
done
echo "  status=$st"

say "read the file back from the new container"
api -X POST "$B/sandboxes/$SB2/exec" \
  -d '{"command":["bash","-lc","cat /workspace/proof.txt"],"timeout_seconds":60}' \
  -o /tmp/e44.json -w 'exec %{http_code}\n'
show_exec /tmp/e44.json
python3 -c "
import json;d=json.load(open('/tmp/e44.json'))
out=(d.get('output') or '').strip()
print('  PERSISTED' if out.startswith('proof ') else '  NOT PERSISTED')
" 2>/dev/null

say "clean up"
api -X DELETE "$B/sandboxes/$SB2" -o /dev/null -w 'delete sandbox2 %{http_code}\n'
api -X DELETE "$B/recipes/$REC" -o /dev/null -w 'delete recipe %{http_code}\n'
api -X DELETE "$B/persistent-roots/$ROOT" -o /dev/null -w 'delete root %{http_code}\n'
