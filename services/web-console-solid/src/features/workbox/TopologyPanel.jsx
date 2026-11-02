// The sandbox's live topology: who talks to what, right now, with blocked and
// would-be-blocked targets highlighted.
//
// The diagram itself is the one the sandbox deck draws (`features/sandboxes`),
// so there is only one truth about what the stack is and what state each piece
// is in. This panel only re-reads it when telemetry for this sandbox arrives,
// at most every two seconds, so a busy agent does not cause a burst of reads.
//
// It also flashes the card of every host an event names: the diagram shows what
// is wired, the pulse shows what was used, and the two are read together.
import { createSignal, onCleanup, onMount } from 'solid-js';
import { stream } from '../../api/telemetry.js';
import StackTopology from '../sandboxes/StackTopology.jsx';

const DEBOUNCE_MS = 2000;
// How long a host stays lit after its last event. Long enough to see the card
// flash at 600 ms and read which one it was, short enough to mean "just now".
const PULSE_MS = 2000;

// The panel: the reason it redraws, then the stack diagram for this sandbox.
export default function TopologyPanel(props) {
  const [rev, setRev] = createSignal(0);
  const [pulse, setPulse] = createSignal(new Set());
  let timer = null;
  // One timer per lit host, restarted by the next event, so a burst keeps the
  // card lit until the burst is over instead of blinking per packet.
  const lit = new Map();

  function bump() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      setRev(rev() + 1);
    }, DEBOUNCE_MS);
  }

  function light(host) {
    if (!host) return;
    setPulse((current) => new Set(current).add(host));
    if (lit.has(host)) clearTimeout(lit.get(host));
    lit.set(
      host,
      setTimeout(() => {
        lit.delete(host);
        setPulse((current) => {
          const next = new Set(current);
          next.delete(host);
          return next;
        });
      }, PULSE_MS),
    );
  }

  onMount(() => {
    const stop = stream(null, {
      onEvent: (event) => {
        if (event.sandbox_id && event.sandbox_id === props.sandbox?.id) {
          // A connection names its destination, a name lookup its hostname; both
          // are the host a destination card is drawn for.
          light(event.destination || event.hostname);
          bump();
        }
      },
    });
    onCleanup(stop);
  });
  onCleanup(() => {
    if (timer) clearTimeout(timer);
    for (const running of lit.values()) clearTimeout(running);
    lit.clear();
  });

  return (
    <section>
      <div class="controls">
        <h3 style="margin:0">Live topology</h3>
        <span class="muted small">re-read while this sandbox talks</span>
      </div>
      <StackTopology sandbox={props.sandbox} revision={rev()} pulse={pulse()} />
    </section>
  );
}
