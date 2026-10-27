// The shell: navigation, environment switcher, and the route switch.
//
// Routing is a signal rather than a router library. The console is one page with
// seven views and no deep links that need to survive a reload; a router would add
// a dependency and a concept without buying anything yet. If deep links become a
// requirement, this is the single file that changes.

import { Show, For, createSignal, onMount } from 'solid-js';
import { useSession } from './state/session.jsx';
import LoginPage from './features/auth/LoginPage.jsx';
import SandboxListPage from './features/sandboxes/SandboxListPage.jsx';
import TrafficPage from './features/traffic/TrafficPage.jsx';
import PoliciesPage from './features/policies/PoliciesPage.jsx';
import ClusterPage from './features/cluster/ClusterPage.jsx';
import ModelsPage from './features/models/ModelsPage.jsx';
import JobsPage from './features/jobs/JobsPage.jsx';

const ROUTES = [
  { id: 'sandboxes', label: 'Sandboxes' },
  { id: 'traffic', label: 'Live traffic' },
  { id: 'policies', label: 'Policies' },
  { id: 'cluster', label: 'Cluster' },
  { id: 'models', label: 'Models' },
  { id: 'operations', label: 'Operations' },
];

export default function App() {
  const session = useSession();
  const [route, setRoute] = createSignal('sandboxes');
  const [checked, setChecked] = createSignal(false);

  // A reload must not show the login form to someone who is still signed in.
  // The session cookie is the truth; `me` asks the server.
  onMount(async () => {
    try {
      const identity = await fetch('/api/v1/auth/me', { credentials: 'same-origin' });
      if (identity.ok) {
        const data = await identity.json();
        // A page reload loses the CSRF token, which is intentional: it lives in
        // memory only. The next login restores it. Without it a mutating action
        // would fail with a clear 403 rather than silently succeed.
        session.restore?.(data);
      }
    } catch {
      /* not signed in */
    } finally {
      setChecked(true);
    }
  });

  return (
    <Show when={checked()} fallback={<div class="login"><p class="muted">Loading…</p></div>}>
      <Show when={session.identity()} fallback={<LoginPage />}>
        <div class="shell">
          <aside class="sidebar">
            <div class="brand">
              <strong>Nestbox</strong>
              <span>control</span>
            </div>

            <nav class="nav">
              <For each={ROUTES}>
                {(item) => (
                  <button
                    aria-current={route() === item.id ? 'page' : undefined}
                    onClick={() => setRoute(item.id)}
                  >
                    {item.label}
                  </button>
                )}
              </For>
            </nav>

            <footer>
              <Show when={session.environments().length > 1}>
                <div class="env-picker">
                  <label for="env">Environment</label>
                  <select
                    id="env"
                    value={session.activeEnvironment()}
                    onChange={(event) => session.switchEnvironment(event.currentTarget.value)}
                  >
                    <For each={session.environments()}>
                      {(env) => <option value={env.id}>{env.name} ({env.role})</option>}
                    </For>
                  </select>
                </div>
              </Show>

              <p class="muted small">
                {session.identity()?.display_name} · {session.identity()?.role}
              </p>
              <button class="secondary" onClick={session.signOut}>Sign out</button>
            </footer>
          </aside>

          <main>
            <Show when={route() === 'sandboxes'}><SandboxListPage /></Show>
            <Show when={route() === 'traffic'}><TrafficPage /></Show>
            <Show when={route() === 'policies'}><PoliciesPage /></Show>
            <Show when={route() === 'cluster'}><ClusterPage /></Show>
            <Show when={route() === 'models'}><ModelsPage /></Show>
            <Show when={route() === 'operations'}><JobsPage /></Show>
          </main>
        </div>
      </Show>
    </Show>
  );
}
