// The master-switch panel: markup only. The state and the calls live in
// `useWindows`, so this file stays a description of what an operator can do.
//
// Every switch is a duration. Nothing here is "off until further notice": that
// is what a policy is for, and a switch that can be left open is the one that
// stays open over a weekend.

import { For, Show } from 'solid-js';
import { ALWAYS_ON, CHANNELS, DURATIONS } from './useWindows.js';

function remaining(expiresAt) {
  const ms = new Date(expiresAt).getTime() - Date.now();
  if (!Number.isFinite(ms) || ms <= 0) return 'expired';
  const minutes = Math.ceil(ms / 60000);
  return minutes >= 60 ? `${(minutes / 60).toFixed(1)} h` : `${minutes} min`;
}

function terms(window) {
  if (!window) return { tone: 'muted', word: 'enforcing' };
  if (window.mode === 'audit') return { tone: 'warn', word: `observing · ${remaining(window.expires_at)}` };
  return { tone: 'ok', word: `open · ${remaining(window.expires_at)}` };
}

export default function MasterSwitches(props) {
  const switches = props.switches;
  return (
    <section class="master-switches">
      <header class="controls">
        <div>
          <h2>Master switches</h2>
          <p class="muted small">
            A switch opens one firewall for a fixed time, then reverts on its own. Observation
            mode lets everything through <em>and</em> records what would have been blocked.
          </p>
        </div>
        <button class="secondary tiny" onClick={switches.load} disabled={switches.loading()}>
          {switches.loading() ? 'Reading…' : 'Refresh'}
        </button>
      </header>

      <Show when={switches.observing()}>
        <p class="warn-note">
          <b>Observation mode is active.</b> Requests are allowed and recorded as blocks, so the
          traffic list shows what a strict policy would have stopped. Nothing is being enforced
          for the channel below while this lasts.
        </p>
      </Show>
      <Show when={switches.error()}>
        <p class="error">{switches.error()}</p>
      </Show>
      <Show when={switches.notice()}>
        <p class="notice">{switches.notice()}</p>
      </Show>

      <div class="table-scroll">
        <table class="dense">
          <thead>
            <tr>
              <th>Firewall</th>
              <th class="narrow">State</th>
              <th>Allow everything for</th>
              <th class="narrow" />
            </tr>
          </thead>
          <tbody>
            <For each={CHANNELS}>
              {(channel) => {
                const current = () => switches.stateFor(channel.id);
                return (
                  <tr>
                    <td>
                      <b>{channel.label}</b>
                      <small class="muted block">{channel.description}</small>
                    </td>
                    <td class="narrow">
                      <span class={`badge ${terms(current()).tone}`}>
                        <i />
                        {terms(current()).word}
                      </span>
                      <Show when={current()}>
                        <small class="muted block">{current().reason}</small>
                      </Show>
                    </td>
                    <td>
                      <div class="actions">
                        <For each={DURATIONS}>
                          {(minutes) => (
                            <button
                              class="tiny"
                              disabled={switches.busy()}
                              onClick={() => switches.flip(channel.id, minutes, 'bypass')}
                            >
                              {minutes} min
                            </button>
                          )}
                        </For>
                        <button
                          class="tiny secondary"
                          disabled={switches.busy()}
                          title="Allow and record what would have been blocked"
                          onClick={() => switches.flip(channel.id, 30, 'audit')}
                        >
                          Observe 30 min
                        </button>
                      </div>
                    </td>
                    <td class="narrow">
                      <Show when={current()}>
                        <button
                          class="tiny danger"
                          disabled={switches.busy()}
                          onClick={() => switches.revoke(current().id)}
                        >
                          Revoke
                        </button>
                      </Show>
                    </td>
                  </tr>
                );
              }}
            </For>

            <tr>
              <td>
                <b>{ALWAYS_ON.label}</b>
                <small class="muted block">{ALWAYS_ON.description}</small>
              </td>
              <td class="narrow">
                <span class="badge ok"><i />always on</span>
              </td>
              <td colspan="2" class="muted small">
                Not switchable by design: it is the path the sandbox’s tools and model calls take.
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  );
}
