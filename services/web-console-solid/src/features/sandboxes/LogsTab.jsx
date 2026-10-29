// Logs tab of the sandbox Admin Center: the sandbox's whole story, live.
//
// For an OpenShell sandbox that is three voices in one list, ordered by time: the agent's own
// output, the supervisor (every connection it allowed or refused, every SSH session, every
// policy it loaded) and the gateway. A sandbox whose agent prints nothing is not silent: the
// supervisor still says what happened to it.

import { createSignal, onCleanup, onMount } from 'solid-js';
import * as api from '../../api/sandboxes.js';
import LogView from '../../components/LogView.jsx';

const TAIL = 500;
const POLL_MS = 2500;

export default function LogsTab(props) {
  const [content, setContent] = createSignal('');
  const [readAt, setReadAt] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [following, setFollowing] = createSignal(true);
  const [filter, setFilter] = createSignal('');
  const [onlyProblems, setOnlyProblems] = createSignal(false);
  let timer = null;

  async function read() {
    const sandbox = props.sandbox;
    if (!sandbox?.id) return;
    try {
      const answer = await api.logs(sandbox.id, TAIL);
      setContent(answer.content || '');
      setReadAt(new Date());
      setError(null);
    } catch (failure) {
      setError(failure.message || 'The log could not be read.');
    }
  }
  function schedule() {
    if (timer) clearInterval(timer);
    timer = following() ? setInterval(read, POLL_MS) : null;
  }
  onMount(() => { read(); schedule(); });
  onCleanup(() => timer && clearInterval(timer));
  const toggle = () => { setFollowing(!following()); schedule(); };

  return (
    <section class="tab-panel">
      <p class="muted small">
        Everything this sandbox did, oldest first: <b>agent</b> is what the agent printed,
        <b> sandbox</b> is the supervisor (connections allowed or refused, sessions, policy loaded),
        <b> gateway</b> is OpenShell managing it. Red lines are refusals and errors.
      </p>
      <div class="logs-controls">
        <button class={following() ? 'primary tiny' : 'secondary tiny'} onClick={toggle}>{following() ? 'Live' : 'Paused'}</button>
        <button class="secondary tiny" onClick={read}>Read now</button>
        <input type="search" placeholder="Filter lines…" value={filter()} onInput={(e) => setFilter(e.currentTarget.value)} />
        <label class="small"><input type="checkbox" checked={onlyProblems()} onChange={(e) => setOnlyProblems(e.currentTarget.checked)} /> only problems</label>
        <span class="muted small">{readAt() ? `last ${TAIL} lines, read ${readAt().toLocaleTimeString()}` : 'not read yet'}</span>
      </div>
      {error() && <p class="error">{error()}</p>}
      <LogView text={content()} filter={filter()} onlyProblems={onlyProblems()} follow={following()}
               empty="No log lines yet. Run something in the sandbox and it appears here." />
    </section>
  );
}
