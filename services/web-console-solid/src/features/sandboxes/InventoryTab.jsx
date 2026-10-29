// What is installed inside: OS, agents, tool versions, listening ports, mounts.
import { For, Show, createSignal, onMount } from 'solid-js';
import * as deck from '../../api/deck.js';

function Block(props) {
  return (
    <Show when={props.value != null && (typeof props.value !== 'object' || Object.keys(props.value).length > 0)}>
      <div class="deck-card"><h3>{props.title}</h3><pre class="mono small">{typeof props.value === 'string' ? props.value : JSON.stringify(props.value, null, 2)}</pre></div>
    </Show>
  );
}

export default function InventoryTab(props) {
  const [data, setData] = createSignal(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const load = async () => { try { setData(await deck.inventory(props.sandbox.id)); setError(null); } catch (e) { setError(e.message); } };
  async function refresh() {
    setBusy(true);
    try { await deck.refreshInventory(props.sandbox.id); await load(); } catch (e) { setError(e.message); } finally { setBusy(false); }
  }
  onMount(load);
  const inv = () => data()?.inventory;
  return (
    <section class="tab-panel">
      <div class="controls">
        <button class="primary tiny" disabled={busy()} onClick={refresh}>{busy() ? 'Reading…' : 'Take a fresh snapshot'}</button>
        <span class="muted small">{data()?.age_seconds != null ? `snapshot is ${Math.round(data().age_seconds)} s old` : 'no snapshot yet'}</span>
      </div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={inv()} fallback={<p class="muted">No snapshot has been taken. The sandbox must be running; press the button.</p>}>
        <div class="deck-grid">
          <Block title="Operating system" value={inv().os} />
          <Block title="Agents" value={inv().agents} />
          <Block title="Tool versions" value={inv().tool_versions} />
          <Block title="Listening ports" value={inv().listening} />
          <Block title="Mounts" value={inv().mounts} />
          <Block title="Environment variable names" value={inv().env_keys} />
          <Block title="Image" value={inv().image} />
          <Block title="Packages" value={inv().packages} />
        </div>
        <Show when={inv().note}><p class="muted small">{inv().note}</p></Show>
      </Show>
    </section>
  );
}
