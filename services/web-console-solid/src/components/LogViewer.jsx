// One log viewer for everything that has a log.
//
// A sandbox, a supervisor (a filtered view of its sandbox's output) and an
// appliance component all read through the same panel, so "follow", "filter" and
// "copy" behave the same wherever an operator opens a log. The caller passes a
// `load` function that returns the text; this component owns polling, the filter
// and the way a failure is said.
//
// Polling, not streaming: the runner answers `docker logs --tail`, and a socket
// for text that changes once a minute would be a second way to read the same
// file. Following stops after a failed read, because a dead container is a
// normal answer and not a reason to ask every two seconds forever.

import { Show, createEffect, createSignal, onCleanup, onMount } from 'solid-js';

const POLL_MS = 3000;

// Keep only the lines that contain `needle`. Pure, so it can be tested.
export function filterLines(text, needle) {
  const wanted = (needle || '').trim().toLowerCase();
  if (!wanted) return text;
  return text
    .split('\n')
    .filter((line) => line.toLowerCase().includes(wanted))
    .join('\n');
}

export default function LogViewer(props) {
  const [content, setContent] = createSignal('');
  const [readAt, setReadAt] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [following, setFollowing] = createSignal(true);
  const [needle, setNeedle] = createSignal(props.filter || '');
  let box;
  let timer = null;

  async function read() {
    try {
      const text = await props.load();
      setContent(text || '');
      setReadAt(new Date());
      setError(null);
    } catch (failure) {
      setError(failure.message || 'The log could not be read.');
      setFollowing(false);
    }
  }

  function schedule() {
    if (timer) clearInterval(timer);
    timer = following() ? setInterval(read, POLL_MS) : null;
  }

  onMount(() => {
    read();
    schedule();
  });
  onCleanup(() => timer && clearInterval(timer));

  // A different source (the operator clicked another card) starts a fresh read.
  createEffect(() => {
    props.source;
    setNeedle(props.filter || '');
    setFollowing(true);
    read();
  });
  createEffect(() => {
    following();
    schedule();
  });

  const shown = () => filterLines(content(), needle());
  createEffect(() => {
    shown();
    if (box && following()) box.scrollTop = box.scrollHeight;
  });

  return (
    <section class="log-viewer">
      <div class="controls">
        <button class={following() ? 'primary tiny' : 'secondary tiny'} onClick={() => setFollowing(!following())}>
          {following() ? 'Following' : 'Follow'}
        </button>
        <button class="secondary tiny" onClick={read}>Read now</button>
        <button class="secondary tiny" onClick={() => navigator.clipboard?.writeText(shown())}>Copy</button>
        <input
          class="log-filter"
          placeholder="filter lines"
          value={needle()}
          onInput={(event) => setNeedle(event.currentTarget.value)}
        />
        <span class="muted small">
          <Show when={readAt()} fallback="not read yet">read {readAt().toLocaleTimeString()}</Show>
        </span>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show
        when={shown()}
        fallback={
          <p class="muted small">
            {needle()
              ? 'No line matches the filter.'
              : 'Nothing has been printed yet. A component that only waits is silent by design.'}
          </p>
        }
      >
        <pre class="logs mono small" ref={box}>{shown()}</pre>
      </Show>
    </section>
  );
}
