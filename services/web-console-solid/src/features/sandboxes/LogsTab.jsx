// Logs tab of the sandbox Admin Center.
//
// The container's own output, followed: a shell that printed nothing and a
// sandbox that died look the same in a one-shot read, so the panel polls while
// it is open and says when it last read. There is no streaming transport here
// on purpose — the runner's log call is a `docker logs --tail`, and a WebSocket
// for text that changes once a minute would be a second way to read the same
// file.

import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import * as api from '../../api/sandboxes.js';

const TAIL = 400;
const POLL_MS = 2000;

export default function LogsTab(props) {
  const [content, setContent] = createSignal('');
  const [readAt, setReadAt] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [following, setFollowing] = createSignal(true);

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
      // A dead container is a normal answer here, not a reason to keep asking
      // every two seconds forever.
      setFollowing(false);
    }
  }

  function schedule() {
    if (timer) clearInterval(timer);
    timer = null;
    if (!following()) return;
    timer = setInterval(read, POLL_MS);
  }

  onMount(read);
  onMount(schedule);
  onCleanup(() => {
    if (timer) clearInterval(timer);
  });

  function toggleFollow() {
    setFollowing(!following());
    schedule();
  }

  return (
    <section class="tab-panel">
      <div class="controls">
        <button class={following() ? 'primary' : 'secondary'} onClick={toggleFollow}>
          {following() ? 'Following' : 'Follow'}
        </button>
        <button class="secondary" onClick={read}>Read now</button>
        <span class="muted small">
          <Show when={readAt()} fallback="not read yet">
            last {TAIL} lines, read {readAt().toLocaleTimeString()}
          </Show>
        </span>
      </div>

      <Show when={error()}><p class="error">{error()}</p></Show>

      <Show
        when={content()}
        fallback={
          <p class="muted">
            The container has printed nothing since it started. A sandbox whose
            entrypoint only waits (the supervisor with an idle agent) is silent
            by design; the Processes tab shows what is running.
          </p>
        }
      >
        <pre class="logs mono small">{content()}</pre>
      </Show>
    </section>
  );
}
