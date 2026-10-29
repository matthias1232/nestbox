// A log, read the way an operator reads one: newest at the bottom, problems in colour,
// a filter, and it follows the end unless you scroll away.
//
// Takes plain text, one entry per line, `<iso time> [<source>] <message>`; a line without a
// time is shown as it is.

import { For, Show, createEffect, createMemo, createSignal, onMount } from 'solid-js';

const LEVELS = [
  [/\b(denied|refused|refuse|error|failed|fatal|panic|unhealthy)\b/i, 'bad'],
  [/\b(warn|warning|deny|audit|degraded)\b/i, 'warn'],
  [/\b(allowed|connected|ready|started|accepted)\b/i, 'good'],
];

export function classify(line) {
  for (const [pattern, level] of LEVELS) if (pattern.test(line)) return level;
  return 'plain';
}

export function splitLine(line) {
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.\d+)?Z\s+(?:\[([^\]]+)\]\s+)?(.*)$/.exec(line);
  if (!match) return { time: '', source: '', text: line };
  return { time: match[2], source: match[3] || '', text: match[4] };
}

export default function LogView(props) {
  // props: text, filter (string), follow (bool), onlyProblems (bool)
  let box;
  const [pinned, setPinned] = createSignal(true);
  const lines = createMemo(() => {
    const needle = (props.filter || '').trim().toLowerCase();
    return (props.text || '')
      .split('\n')
      .filter((line) => line.trim())
      .filter((line) => !needle || line.toLowerCase().includes(needle))
      .filter((line) => !props.onlyProblems || classify(line) !== 'plain' && classify(line) !== 'good')
      .map((line) => ({ ...splitLine(line), level: classify(line) }));
  });

  createEffect(() => {
    lines();
    if (props.follow !== false && pinned() && box) box.scrollTop = box.scrollHeight;
  });
  onMount(() => {
    if (box) box.scrollTop = box.scrollHeight;
  });

  return (
    <div
      class="logview"
      ref={box}
      onScroll={() => setPinned(box.scrollHeight - box.scrollTop - box.clientHeight < 40)}
    >
      <Show when={lines().length > 0} fallback={<p class="muted small logview-empty">{props.empty || 'Nothing to show yet.'}</p>}>
        <For each={lines()}>
          {(line) => (
            <div class={`logline ${line.level}`}>
              <span class="lt">{line.time}</span>
              <Show when={line.source}><span class="ls">{line.source}</span></Show>
              <span class="lm">{line.text}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
