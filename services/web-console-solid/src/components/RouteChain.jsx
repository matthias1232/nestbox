// The way one request took, or would have taken: sandbox, proxy, check, upstream,
// router, model.
import { For, Show } from 'solid-js';
import '../styles/dlp.css';

export default function RouteChain(props) {
  const hops = () => props.route || [];
  return (
    <Show when={hops().length}>
      <span class={`route-chain ${props.compact ? 'compact' : ''}`}>
        <For each={hops()}>
          {(hop, index) => (
            <>
              <Show when={index() > 0}>
                <span class="route-arrow">→</span>
              </Show>
              <span
                class={`route-pill ${hop.state || 'passed'}`}
                title={hop.detail || ''}
              >
                {hop.state === 'would' ? 'hätte: ' : ''}
                {hop.label}
              </span>
            </>
          )}
        </For>
      </span>
    </Show>
  );
}
