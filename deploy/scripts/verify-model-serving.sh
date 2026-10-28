#!/bin/bash
# Phase 2 acceptance: an install becomes an answering model server.
#
# Everything here runs on the appliance. The proof is a chat completion from a
# container on the runtime network, not the runner's word that a container is
# up: "running" and "answering" are different states, which is why the install
# has two of them.
set -u
B=http://127.0.0.1:8080/api/v1
J=/tmp/jar-p2.txt; rm -f $J
curl -s -c $J -X POST $B/auth/login -H 'content-type: application/json' \
  -d '{"email":"admin@nestbox.local","password":"08e9e4ded664a0234af4b8e4d3879fc37f9d"}' > /tmp/lp2.json
C=$(python3 -c "import json;print(json.load(open('/tmp/lp2.json')).get('csrf_token',''))")
api() { curl -s -b $J -H "x-csrf-token: $C" -H 'content-type: application/json' "$@"; }
code() { curl -s -o /tmp/p2-last.json -w '%{http_code}' -b $J -H "x-csrf-token: $C" -H 'content-type: application/json' "$@"; }
say() { echo; echo "== $* =="; }

say "clean start"
api -X POST "$B/models/qwen3-1.7b/uninstall" -d '{"node":"local"}' -o /dev/null -w 'uninstall %{http_code}\n'
docker rm -f model-qwen3-1-7b >/dev/null 2>&1 || true
api "$B/models?node=local" | python3 -c "
import json,sys
b=json.load(sys.stdin)['budget']
print('  free %.2f GiB, reserved %.2f GiB, installs %s' % (b['ram_free_bytes']/2**30, b['ram_reserved_bytes']/2**30, b['installs']))
"

say "a model with no artefact is refused before anything is reserved"
code -X POST "$B/models/llama-3.2-1b/install" -d '{"node":"local"}'; echo
python3 -c "import json;d=json.load(open('/tmp/p2-last.json'));print('  ',d.get('error'))"
api "$B/models?node=local" | python3 -c "
import json,sys
b=json.load(sys.stdin)['budget']
print('  still reserved %.2f GiB' % (b['ram_reserved_bytes']/2**30))
"

say "install qwen3-1.7b"
code -X POST "$B/models/qwen3-1.7b/install" -d '{"node":"local"}'; echo
python3 -c "
import json;d=json.load(open('/tmp/p2-last.json'))
print('  install', d.get('install_id'), 'state', d.get('state'), 'artifact', d.get('artifact'))
print('  job', d.get('job_id'), 'reserved', d.get('reserved_ram_bytes'))
"
JOB=$(python3 -c "import json;print(json.load(open('/tmp/p2-last.json')).get('job_id',''))")

say "watch the state machine"
last=""
for i in $(seq 1 90); do
  sleep 4
  state=$(api "$B/models/installs?node=local" | python3 -c "
import json,sys
rows=[r for r in json.load(sys.stdin)['items'] if r['model_key']=='qwen3-1.7b']
print(rows[0]['state'] if rows else 'gone')
")
  if [ "$state" != "$last" ]; then echo "  t=$((i*4))s state=$state"; last="$state"; fi
  [ "$state" = "installed" ] && break
  [ "$state" = "failed" ] && break
done

say "what the job recorded"
api "$B/jobs/$JOB/logs" | python3 -c "
import json,sys
for row in json.load(sys.stdin).get('items',[]):
    print('  [%s] %s' % (row['level'], row['message'][:180]))
"

if [ "$last" != "installed" ]; then
  say "install did not finish; the row says why"
  api "$B/models/installs?node=local" | head -c 600; echo
  docker logs --tail 30 model-qwen3-1-7b 2>&1 | tail -20
  exit 0
fi

say "the container the node started"
docker inspect model-qwen3-1-7b --format '  name={{.Name}} status={{.State.Status}} image={{.Config.Image}}
  memory={{.HostConfig.Memory}} nanocpus={{.HostConfig.NanoCpus}} pids={{.HostConfig.PidsLimit}}
  capdrop={{.HostConfig.CapDrop}} networks={{range $k,$v := .NetworkSettings.Networks}}{{$k}} {{end}}
  mount={{range .Mounts}}{{.Source}}->{{.Destination}} ro={{not .RW}} {{end}}'

say "the provider profile the sandboxes will use"
api "$B/providers" | python3 -c "
import json,sys
for p in json.load(sys.stdin)['items']:
    if p.get('model_install_id'):
        print('  ', p['name'], p['endpoint'], p['status'])
"

say "a real chat completion on the runtime network"
docker exec -i nestbox-sandbox-runner-1 python3 - <<'PY'
import json, urllib.request
request = urllib.request.Request(
    "http://model-qwen3-1-7b:8080/v1/chat/completions",
    data=json.dumps({
        "model": "qwen3",
        "messages": [{"role": "user", "content": "Reply with the single word: ready"}],
        # Enough for Qwen3's thinking tokens plus the word asked for; the
        # answer may arrive in `reasoning_content` when thinking is on.
        "max_tokens": 96,
        "temperature": 0,
    }).encode(),
    headers={"content-type": "application/json"},
)
with urllib.request.urlopen(request, timeout=120) as response:
    body = json.load(response)
message = body["choices"][0]["message"]
print("  HTTP", response.status)
print("  answer:", repr((message.get("content") or "")[:80]))
print("  thinking:", repr((message.get("reasoning_content") or "")[:80]))
print("  model:", body.get("model"), "tokens:", body.get("usage", {}).get("completion_tokens"))
PY

say "the artefact on the node"
docker exec nestbox-sandbox-runner-1 sh -c 'ls -l /var/lib/nestbox/models/qwen3-1.7b/'

say "uninstall stops the server and releases the memory"
api -X POST "$B/models/qwen3-1.7b/uninstall" -d '{"node":"local"}' -o /tmp/p2-un.json -w 'uninstall %{http_code}\n'
python3 -c "import json;d=json.load(open('/tmp/p2-un.json'));print('  released', d.get('released_ram_bytes'), 'server_stopped', d.get('server_stopped'))"
docker inspect model-qwen3-1-7b >/dev/null 2>&1 && echo "  container STILL EXISTS" || echo "  container gone"
api "$B/models?node=local" | python3 -c "
import json,sys
b=json.load(sys.stdin)['budget']
print('  reserved %.2f GiB, free %.2f GiB' % (b['ram_reserved_bytes']/2**30, b['ram_free_bytes']/2**30))
"

say "install again: the artefact is reused, not downloaded twice"
code -X POST "$B/models/qwen3-1.7b/install" -d '{"node":"local"}'; echo
JOB=$(python3 -c "import json;print(json.load(open('/tmp/p2-last.json')).get('job_id',''))")
for i in $(seq 1 60); do
  sleep 3
  state=$(api "$B/models/installs?node=local" | python3 -c "
import json,sys
rows=[r for r in json.load(sys.stdin)['items'] if r['model_key']=='qwen3-1.7b']
print(rows[0]['state'] if rows else 'gone')
")
  [ "$state" = "installed" ] && break
  [ "$state" = "failed" ] && break
done
echo "  state=$state after ~$((i*3))s"
api "$B/jobs/$JOB/logs" | python3 -c "
import json,sys
for row in json.load(sys.stdin).get('items',[]):
    if 'Artefact' in row['message']:
        print('  ', row['message'][:160])
"

say "leave the appliance as it was"
api -X POST "$B/models/qwen3-1.7b/uninstall" -d '{"node":"local"}' -o /dev/null -w 'uninstall %{http_code}\n'
docker inspect model-qwen3-1-7b >/dev/null 2>&1 && echo "  container STILL EXISTS" || echo "  container gone"
