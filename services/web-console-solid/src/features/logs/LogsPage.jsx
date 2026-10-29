// Live logs across everything you may see.
//
// One screen for "what is happening": the logs of your sandboxes merged by time, one sandbox on
// its own, or (admins) a platform component such as the control API, the runner or the egress
// proxy. Each line says who said it. It polls while Live is on; nothing is stored here.

import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import * as sandboxApi from '../../api/sandboxes.js';
import * as systemApi from '../../api/system.js';
import LogView from '../../components/LogView.jsx';

const POLL_MS = 3000;

export default function LogsPage(props) {
  const [sandboxes, setSandboxes] = createSignal([]);
  const [components, setComponents] = createSignal([]);
  const [source, setSource] = createSignal('all');
  const [text, setText] = createSignal('');
  const [error, setError] = createSignal(null);
  const [readAt, setReadAt] = createSignal(null);
  const [live, setLive] = createSignal(true);
  const [filter, setFilter] = createSignal('');
  const [onlyProblems, setOnlyProblems] = createSignal(false);
  let timer = null;

  const isAdmin = () => props.role === 'admin';

  async function loadSources() {
    try {
      const page = await sandboxApi.list({ limit: 100 });
      setSandboxes((page.items || []).filter((item) => item.status === 'running'));
    } catch (failure) {
      setError(failure.message);
    }
    if (isAdmin()) {
      try {
        const answer = await systemApi.components();
        const rows = Array.isArray(answer) ? answer : answer.components || answer.items || [];
        setComponents(rows.map((item) => item.service));
      } catch { /* the component list is a convenience */ }
    }
  }

  const stamp = (line, tag) => {
    // Lines already carry `<iso> [source] text`; the sandbox name is put in front of the source.
    const match = /^(\S+Z)\s+(?:\[([^\]]+)\]\s+)?(.*)$/.exec(line);
    return match ? `${match[1]} [${tag}${match[2] ? ' ' + match[2] : ''}] ${match[3]}` : line;
  };

  async function read() {
    try {
      const chosen = source();
      let merged = [];
      if (chosen === 'all' || chosen.startsWith('sandbox:')) {
        const targets = chosen === 'all' ? sandboxes() : sandboxes().filter((item) => `sandbox:${item.id}` === chosen);
        const tail = chosen === 'all' ? 80 : 500;
        const results = await Promise.allSettled(targets.map((item) => sandboxApi.logs(item.id, tail)));
        results.forEach((result, index) => {
          if (result.status !== 'fulfilled') return;
          const name = targets[index].name;
          (result.value.content || '').split('\n').filter(Boolean).forEach((line) => merged.push(stamp(line, name)));
        });
      } else if (chosen.startsWith('system:')) {
        const service = chosen.slice(7);
        const answer = await systemApi.componentLogs(service, 500);
        merged = (answer.content || '').split('\n').filter(Boolean).map((line) => (/^\d{4}-\d\d-\d\dT/.test(line) ? stamp(line, service) : line));
      }
      merged.sort();
      setText(merged.slice(-1500).join('\n'));
      setReadAt(new Date());
      setError(null);
    } catch (failure) {
      setError(failure.message || 'The log could not be read.');
    }
  }

  function schedule() {
    if (timer) clearInterval(timer);
    timer = live() ? setInterval(read, POLL_MS) : null;
  }
  onMount(async () => { await loadSources(); await read(); schedule(); });
  onCleanup(() => timer && clearInterval(timer));
  const toggle = () => { setLive(!live()); schedule(); };
  const choose = (value) => { setSource(value); setText(''); read(); };

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Live logs</h1>
          <p class="muted">
            What is happening right now. Your sandboxes' logs are merged by time and each line says who
            said it. Red lines are refusals and errors.
          </p>
        </div>
      </header>
      <div class="logs-controls">
        <label class="small">Source{' '}
          <select value={source()} onChange={(e) => choose(e.currentTarget.value)}>
            <option value="all">All my running sandboxes ({sandboxes().length})</option>
            <For each={sandboxes()}>{(item) => <option value={`sandbox:${item.id}`}>Sandbox {item.name}</option>}</For>
            <Show when={isAdmin() && components().length > 0}>
              <For each={components()}>{(name) => <option value={`system:${name}`}>Platform: {name}</option>}</For>
            </Show>
          </select>
        </label>
        <button class={live() ? 'primary tiny' : 'secondary tiny'} onClick={toggle}>{live() ? 'Live' : 'Paused'}</button>
        <button class="secondary tiny" onClick={() => { loadSources(); read(); }}>Refresh</button>
        <input type="search" placeholder="Filter lines…" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
        <label class="small"><input type="checkbox" checked={onlyProblems()} onChange={(e) => setOnlyProblems(e.currentTarget.checked)} /> only problems</label>
        <span class="muted small">{readAt() ? `read ${readAt().toLocaleTimeString()}` : 'reading…'}</span>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <LogView text={text()} filter={filter()} onlyProblems={onlyProblems()} follow={live()}
               empty="No log lines yet. Start or use a sandbox and its log appears here." />
    </section>
  );
}
