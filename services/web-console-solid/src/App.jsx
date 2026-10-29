// The shell: navigation, environment switcher, and the route switch.
//
// Routing is a signal rather than a router library. The console is one page with
// seven views and no deep links that need to survive a reload; a router would add
// a dependency and a concept without buying anything yet. If deep links become a
// requirement, this is the single file that changes.

import { Show, For, createSignal, onMount, onCleanup } from 'solid-js';
import { useSession } from './state/session.jsx';
import * as auth from './api/auth.js';
import LoginPage from './features/auth/LoginPage.jsx';
import SandboxListPage from './features/sandboxes/SandboxListPage.jsx';
import TrafficPage from './features/traffic/TrafficPage.jsx';
import PoliciesPage from './features/policies/PoliciesPage.jsx';
import ClusterPage from './features/cluster/ClusterPage.jsx';
import ProvidersPage from './features/providers/ProvidersPage.jsx';
import ModelsPage from './features/models/ModelsPage.jsx';
import NodesPage from './features/nodes/NodesPage.jsx';
import RecipesPage from './features/recipes/RecipesPage.jsx';
import PersistentPage from './features/persistent/PersistentPage.jsx';
import UsersPage from './features/users/UsersPage.jsx';
import LogsPage from './features/logs/LogsPage.jsx';
import McpPage from './features/mcp/McpPage.jsx';
import BuilderPage from './features/builder/BuilderPage.jsx';
import JobsPage from './features/jobs/JobsPage.jsx';
import SkillsPage from './features/skills/SkillsPage.jsx';
import CudaXPage from './features/cudax/CudaXPage.jsx';
import AdminCenter from './features/sandboxes/AdminCenter.jsx';

// The Admin Center is the only view with an address of its own: an operator
// pastes `/sandboxes/<id>` into a chat and expects the same screen. Everything
// else stays a signal, because adding a router for seven views would be a
// dependency without a benefit.
const ADMIN_PATH = /^\/sandboxes\/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\/?$/;

function adminFromLocation() {
  const match = ADMIN_PATH.exec(window.location.pathname);
  return match ? match[1] : null;
}

const ROUTES = [
  { id: 'sandboxes', label: 'Sandboxes' },
  { id: 'traffic', label: 'Live traffic' },
  { id: 'logs', label: 'Live logs' },
  { id: 'policies', label: 'Policies' },
  { id: 'cluster', label: 'Cluster' },
  { id: 'nodes', label: 'Nodes' },
  { id: 'providers', label: 'Providers' },
  { id: 'mcp', label: 'MCP servers' },
  { id: 'models', label: 'Models' },
  { id: 'skills', label: 'Skills' },
  { id: 'cudax', label: 'CUDA-X' },
  { id: 'builder', label: 'Build image' },
  { id: 'recipes', label: 'Recipes' },
  { id: 'persistent', label: 'Persistent data' },
  { id: 'operations', label: 'Operations' },
  // Only an admin manages who may sign in; the page and the API both refuse anyone else.
  { id: 'users', label: 'Users', adminOnly: true },
];

export default function App() {
  const session = useSession();
  const [route, setRoute] = createSignal('sandboxes');
  const [checked, setChecked] = createSignal(false);
  const [adminId, setAdminId] = createSignal(adminFromLocation());

  function openAdmin(id) {
    window.history.pushState({}, '', `/sandboxes/${id}`);
    setAdminId(id);
    setRoute('sandboxes');
  }

  function closeAdmin() {
    window.history.pushState({}, '', '/');
    setAdminId(null);
  }

  // The back button must close the Admin Center again, otherwise the address bar
  // and the screen disagree.
  function onPop() {
    setAdminId(adminFromLocation());
  }
  window.addEventListener('popstate', onPop);
  onCleanup(() => window.removeEventListener('popstate', onPop));

  // A reload must not show the login form to someone who is still signed in.
  // The session cookie is HttpOnly, so the only way to know is to ask the
  // server; a 401 here is the normal "not signed in" answer, not a failure.
  onMount(async () => {
    try {
      const data = await auth.me();
      // A page reload loses the CSRF token, which is intentional: it lives in
      // memory only. The next login restores it. Without it a mutating action
      // fails with a clear 403 rather than silently succeeding.
      session.restore(data);
      // The token is memory-only and a reload loses it: ask for a new one, so changes keep
      // working without a second sign-in.
      await session.refreshCsrf();
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
              <For each={ROUTES.filter((item) => !item.adminOnly || session.identity()?.role === 'admin')}>
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
            <Show
              when={adminId()}
              fallback={
                <>
                  <Show when={route() === 'sandboxes'}>
                    <SandboxListPage
                      onOpenAdmin={openAdmin}
                      onOpenNode={() => setRoute('nodes')}
                    />
                  </Show>
                  <Show when={route() === 'traffic'}><TrafficPage /></Show>
                  <Show when={route() === 'policies'}><PoliciesPage /></Show>
                  <Show when={route() === 'cluster'}><ClusterPage /></Show>
                  <Show when={route() === 'nodes'}>
                    <NodesPage onOpenSandbox={openAdmin} />
                  </Show>
                  <Show when={route() === 'providers'}><ProvidersPage /></Show>
                  <Show when={route() === 'models'}><ModelsPage /></Show>
                  <Show when={route() === 'skills'}><SkillsPage /></Show>
                  <Show when={route() === 'cudax'}><CudaXPage /></Show>
                  <Show when={route() === 'builder'}><BuilderPage /></Show>
                  <Show when={route() === 'recipes'}><RecipesPage /></Show>
                  <Show when={route() === 'persistent'}><PersistentPage /></Show>
                  <Show when={route() === 'operations'}><JobsPage /></Show>
                  <Show when={route() === 'users'}><UsersPage /></Show>
                  <Show when={route() === 'mcp'}><McpPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'logs'}><LogsPage role={session.identity()?.role} /></Show>
                </>
              }
            >
              <AdminCenter sandboxId={adminId()} onClose={closeAdmin} />
            </Show>
          </main>
        </div>
      </Show>
    </Show>
  );
}
