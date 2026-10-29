// Give a sandbox more or less memory, CPU and processes, live.
//
// Memory, CPU and the process limit change on a running container without a
// restart. The answer says which happened: applied now, saved for the next start,
// or saved but the runner could not apply it.
import { Show, createSignal } from 'solid-js';
import * as deck from '../../api/deck.js';

const SAID = {
  applied: 'Applied to the running sandbox.',
  saved_applies_on_start: 'Saved. The sandbox is not running; it starts with these limits.',
  saved_not_applied: 'Saved, but the runner could not change the running container. Restart the sandbox to apply it.',
};

export default function ResourcesPanel(props) {
  const spec = () => props.sandbox?.spec || {};
  const [memory, setMemory] = createSignal('');
  const [cpus, setCpus] = createSignal('');
  const [pids, setPids] = createSignal('');
  const [said, setSaid] = createSignal(null);
  const [failure, setFailure] = createSignal(null);

  async function apply(event) {
    event.preventDefault();
    setFailure(null);
    setSaid(null);
    const body = {};
    if (memory().trim()) body.memory_limit = memory().trim();
    if (cpus().trim()) body.cpu_limit = Number(cpus());
    if (pids().trim()) body.pids_limit = Number(pids());
    try {
      const answer = await deck.setResources(props.sandbox.id, body);
      setSaid(SAID[answer.applied] || 'Saved.');
      setMemory(''); setCpus(''); setPids('');
      props.reload?.();
    } catch (error) {
      setFailure(error.message);
    }
  }

  return (
    <form class="resource-form" onSubmit={apply}>
      <label>Memory (now {spec().memory_limit || '—'})
        <input placeholder="e.g. 2g or 512Mi" value={memory()} onInput={(e) => setMemory(e.currentTarget.value)} />
      </label>
      <label>CPU cores (now {spec().cpu_limit ?? '—'})
        <input type="number" step="0.1" min="0.1" max="64" placeholder="e.g. 1.5" value={cpus()} onInput={(e) => setCpus(e.currentTarget.value)} />
      </label>
      <label>Processes (now {spec().pids_limit ?? '—'})
        <input type="number" min="32" max="65536" placeholder="e.g. 512" value={pids()} onInput={(e) => setPids(e.currentTarget.value)} />
      </label>
      <button class="primary" type="submit" disabled={!memory().trim() && !cpus().trim() && !pids().trim()}>Apply</button>
      <Show when={said()}><span class="notice">{said()}</span></Show>
      <Show when={failure()}><span class="error">{failure()}</span></Show>
    </form>
  );
}
