// "How a request travels": pick a real connection this sandbox made and watch the five steps
// it took, with the real values. Meant for learning the system: who does what, in which order.

import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import * as deck from '../../api/deck.js';
import { journeySteps } from './journey.js';

export default function RequestJourney(props) {
  const [events, setEvents] = createSignal([]);
  const [chosen, setChosen] = createSignal(null);
  const [lit, setLit] = createSignal(5);
  const [error, setError] = createSignal(null);
  let timer = null;

  async function load() {
    try {
      const page = await deck.traffic(props.sandbox.id, { window: '1h', limit: 60 });
      // Distinct destinations first: ten identical lines teach nothing.
      const seen = new Set();
      const distinct = (page.items || []).filter((item) => {
        const key = `${item.decision}|${item.host}|${item.port}|${item.mcp_tool || ''}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
      setEvents(distinct);
      if (!chosen() && distinct.length) setChosen(distinct[0]);
      setError(null);
    } catch (failure) {
      setError(failure.message);
    }
  }
  onMount(load);
  onCleanup(() => timer && clearInterval(timer));

  function play() {
    if (timer) clearInterval(timer);
    setLit(0);
    timer = setInterval(() => {
      setLit(lit() + 1);
      if (lit() >= 5) clearInterval(timer);
    }, 900);
  }
  const steps = () => (chosen() ? journeySteps(chosen()) : []);

  return (
    <section class="journey">
      <h3>How a request travels</h3>
      <p class="muted small">
        Pick a connection this sandbox really made. The five steps below show what happened to it, in order,
        with the real program, destination and rule.
      </p>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={events().length > 0} fallback={<p class="muted small">No connection in the last hour. Use the agent (or run a command) and come back.</p>}>
        <div class="logs-controls">
          <select onChange={(e) => { setChosen(events()[Number(e.currentTarget.value)]); setLit(5); }}>
            <For each={events()}>
              {(item, index) => (
                <option value={index()}>
                  {item.decision === 'deny' ? '⛔' : '✅'} {item.host}:{item.port}{item.mcp_tool ? ` · ${item.mcp_tool}` : ''} · {String(item.ts).slice(11, 19)}
                </option>
              )}
            </For>
          </select>
          <button class="primary tiny" onClick={play}>▶ Play the steps</button>
          <button class="secondary tiny" onClick={load}>Refresh</button>
        </div>
        <ol class="journey-steps">
          <For each={steps()}>
            {(step) => (
              <li class={`journey-step ${lit() >= step.n ? 'lit' : 'dim'} ${step.n === 4 ? (chosen().decision === 'deny' ? 'stop' : 'go') : ''}`}>
                <span class="num">{step.n}</span>
                <div>
                  <div class="who">{step.icon} {step.who}</div>
                  <div class="title">{step.title}</div>
                  <div class="says muted small">{step.says}</div>
                </div>
              </li>
            )}
          </For>
        </ol>
      </Show>
    </section>
  );
}
