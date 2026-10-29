// Give a sandbox more or less memory, CPU and processes, live.
//
// Memory, CPU and the process limit change on a running container without a
// restart. The answer says which happened: applied now, saved for the next start,
// or saved but the runner could not apply it.
//
// Every control the sandbox cannot have is shown disabled, with the reason, and
// not silently hidden: an operator comparing two nodes must be able to see what
// the machine does not offer. GPU and VRAM are that case today — the node may
// have a card, but a sandbox on the OpenShell runtime cannot be given a
// guaranteed share of it yet, so the sliders are grey rather than a promise.
import { Show, createSignal, createResource, onMount } from 'solid-js';
import * as deck from '../../api/deck.js';
import * as models from '../../api/models.js';
import { formatBytes } from '../../components/Meter.jsx';

// The spec stores bytes as "805306368b"; show what a person reads.
function readable(limit) {
  const match = String(limit || '').match(/^(\d+)b$/);
  return match ? formatBytes(Number(match[1])) : limit || '—';
}

const SAID = {
  applied: 'Applied to the running sandbox.',
  saved_applies_on_start: 'Saved. The sandbox is not running; it starts with these limits.',
  saved_not_applied: 'Saved, but the runner could not change the running container. Restart the sandbox to apply it.',
};

// Docker's default weight. 1024 means "normal"; below is quieter under
// contention, above is greedier.
const NORMAL_SHARES = 1024;

export default function ResourcesPanel(props) {
  const spec = () => props.sandbox?.spec || {};
  const [memory, setMemory] = createSignal('');
  const [reservation, setReservation] = createSignal('');
  const [cpus, setCpus] = createSignal('');
  const [shares, setShares] = createSignal(String(spec().cpu_shares ?? NORMAL_SHARES));
  const [pids, setPids] = createSignal('');
  const [said, setSaid] = createSignal(null);
  const [failure, setFailure] = createSignal(null);

  // The node's own facts decide what is offerable. Nothing here is assumed.
  const [hardware] = createResource(() => models.hardware(false));
  const gpu = () => hardware()?.gpus?.[0];
  const gpuOffered = () => Boolean(hardware()?.has_gpu);
  const gpuEnforced = () => Boolean(hardware()?.container_gpu);

  const changed = () =>
    Boolean(memory().trim() || reservation().trim() || cpus().trim() || pids().trim()
      || Number(shares()) !== Number(spec().cpu_shares ?? NORMAL_SHARES));

  async function apply(event) {
    event.preventDefault();
    setFailure(null);
    setSaid(null);
    const body = {};
    if (memory().trim()) body.memory_limit = memory().trim();
    if (reservation().trim()) body.memory_reservation = reservation().trim();
    if (cpus().trim()) body.cpu_limit = Number(cpus());
    if (pids().trim()) body.pids_limit = Number(pids());
    const wantedShares = Number(shares());
    if (wantedShares !== Number(spec().cpu_shares ?? NORMAL_SHARES)) body.cpu_shares = wantedShares;
    try {
      const answer = await deck.setResources(props.sandbox.id, body);
      setSaid(SAID[answer.applied] || 'Saved.');
      setMemory(''); setReservation(''); setCpus(''); setPids('');
      props.reload?.();
    } catch (error) {
      setFailure(error.message);
    }
  }

  return (
    <form class="resource-form" onSubmit={apply}>
      <label>
        Memory limit (now {readable(spec().memory_limit)})
        <input placeholder="e.g. 2g or 512Mi" value={memory()} onInput={(e) => setMemory(e.currentTarget.value)} />
        <small class="muted">The hard ceiling. The sandbox is stopped above it, never swapped.</small>
      </label>
      <label>
        Memory reservation (now {readable(spec().memory_reservation)})
        <input placeholder="e.g. 1g" value={reservation()} onInput={(e) => setReservation(e.currentTarget.value)} />
        <small class="muted">
          The soft target below the ceiling: what this sandbox is guaranteed under pressure. Cannot be
          larger than the limit, and a smaller reservation lets an idle sandbox give memory back.
        </small>
      </label>
      <label>
        CPU cores (now {spec().cpu_limit ?? '—'})
        <input type="number" step="0.1" min="0.1" max="64" placeholder="e.g. 1.5" value={cpus()} onInput={(e) => setCpus(e.currentTarget.value)} />
        <small class="muted">A hard cap: 0.5 means half a core even when the host is idle.</small>
      </label>
      <label>
        CPU priority (now {spec().cpu_shares ?? NORMAL_SHARES} shares)
        <input type="range" min="2" max="4096" step="2" value={shares()} onInput={(e) => setShares(e.currentTarget.value)} />
        <small class="muted">
          Only matters when cores are contended. {NORMAL_SHARES} is normal; lower yields to other
          sandboxes, higher takes a larger share. This is a weight, not a cap.
        </small>
      </label>
      <label>
        Processes (now {spec().pids_limit ?? '—'})
        <input type="number" min="32" max="65536" placeholder="e.g. 512" value={pids()} onInput={(e) => setPids(e.currentTarget.value)} />
        <small class="muted">The ceiling on processes and threads; the guard against a fork bomb.</small>
      </label>

      <label class="muted">
        GPU (now {gpuOffered() ? gpu()?.name || 'reported' : 'none reported'})
        <input type="range" min="0" max="1" step="1" value="0" disabled />
        <small class="muted">
          {!gpuOffered()
            ? 'This node reports no NVIDIA GPU, so there is nothing to assign.'
            : gpuEnforced()
              ? `The node has ${gpu()?.name || 'a GPU'}${gpu()?.memory_bytes ? ` with ${formatBytes(gpu().memory_bytes)}` : ''}, but a guaranteed per-sandbox GPU share is not enforced by this runtime yet. The slider stays disabled until it is.`
              : 'The node has a GPU, but a container cannot be given it (no container GPU runtime), so it is not capacity for a sandbox.'}
        </small>
      </label>
      <label class="muted">
        VRAM reservation (now none)
        <input type="range" min="0" max="100" step="1" value="0" disabled />
        <small class="muted">
          Earning a VRAM guarantee needs MIG or a vGPU profile the node actually offers. Until the node
          reports one, a reservation here would be a number nobody enforces — so it is shown, not offered.
        </small>
      </label>

      <button class="primary" type="submit" disabled={!changed()}>Apply</button>
      <Show when={said()}><span class="notice">{said()}</span></Show>
      <Show when={failure()}><span class="error">{failure()}</span></Show>
    </form>
  );
}
