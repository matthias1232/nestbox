// The host's own load, live: CPU, memory, disk, load average, and how much the
// running sandboxes have been promised. The runner reads the host's /proc, so
// these are the machine's numbers, not a container's.
import { Show, createSignal } from 'solid-js';
import * as deck from '../api/deck.js';
import Meter, { formatBytes } from './Meter.jsx';
import { usePolling } from './Poll.js';
import '../styles/deck.css';

export default function HostLoad() {
  const [load, setLoad] = createSignal(null);
  const [error, setError] = createSignal(null);
  usePolling(() => deck.hostLoad(), { everyMs: 5000, onData: (d) => { setLoad(d); setError(null); }, onError: (e) => setError(e.message) });

  return (
    <section class="host-load">
      <Show when={load()} fallback={<span class="muted small">{error() || 'Reading the host…'}</span>}>
        {(data) => (
          <>
            <Meter label="Host CPU" value={data().cpu_percent ?? 0} max={100}
              text={`${data().cpu_percent ?? '—'}% of ${data().cpus} cores`}
              promised={data().cpus ? (data().promised.cpus / data().cpus) * 100 : null}
              title="The marker is the CPU the running sandboxes have been promised." />
            <Meter label="Host memory" value={data().memory.used} max={data().memory.total}
              text={`${formatBytes(data().memory.used)} of ${formatBytes(data().memory.total)}`}
              promised={data().promised.memory_bytes}
              title="The marker is the memory the running sandboxes have been promised." />
            <Show when={data().disk}>
              <Meter label="Disk" value={data().disk.used} max={data().disk.total}
                text={`${formatBytes(data().disk.free)} free`} />
            </Show>
            <span class="muted small">
              load {data().load_average?.join(' ') ?? '—'} · {data().sandboxes.running}/{data().sandboxes.total} sandboxes running
            </span>
          </>
        )}
      </Show>
    </section>
  );
}
