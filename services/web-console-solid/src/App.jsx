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
import ProxiesPage from './features/proxies/ProxiesPage.jsx';
import ModelsPage from './features/models/ModelsPage.jsx';
import NodesPage from './features/nodes/NodesPage.jsx';
import RecipesPage from './features/recipes/RecipesPage.jsx';
import PersistentPage from './features/persistent/PersistentPage.jsx';
import UsersPage from './features/users/UsersPage.jsx';
import LogsPage from './features/logs/LogsPage.jsx';
import McpPage from './features/mcp/McpPage.jsx';
import MediaPage from './features/media/MediaPage.jsx';
import PrivacyPage from './features/privacy/PrivacyPage.jsx';
import MasterControlPage from './features/master/MasterControlPage.jsx';
import UsagePage from './features/usage/UsagePage.jsx';
import LivePage from './features/live/LivePage.jsx';
import BuilderPage from './features/builder/BuilderPage.jsx';
import JobsPage from './features/jobs/JobsPage.jsx';
import SkillsPage from './features/skills/SkillsPage.jsx';
import CudaXPage from './features/cudax/CudaXPage.jsx';
import SystemPage from './features/system/SystemPage.jsx';
import AdminCenter from './features/sandboxes/AdminCenter.jsx';
import HostingPage from './features/hosting/HostingPage.jsx';
import MovesPage from './features/moves/MovesPage.jsx';
import ImagesPage from './features/images/ImagesPage.jsx';
import TenantsPage from './features/tenants/TenantsPage.jsx';
import ContextBar from './features/tenants/ContextBar.jsx';
import WorkboxPage from './features/workbox/WorkboxPage.jsx';
import MyBoxesPage from './features/workbox/MyBoxesPage.jsx';
import TerminalPopout from './features/workbox/TerminalPopout.jsx';
import DesktopPopout from './features/workbox/DesktopPopout.jsx';
import { menuFor } from './navigation.js';
import { navigate, parsePath } from './routing.js';
import { adoptOpenerCsrf } from './api/client.js';

// Three views have an address of their own: the Admin Center
// (`/sandboxes/<id>`), the Workbox (`/box/<id>`, `/my`) and the pop-out windows
// (`/box/<id>/terminal/<tab>`, `/box/<id>/desktop`). `routing.js` says what a
// path means; this file only renders it.


export default function App() {
  const session = useSession();
  const [route, setRoute] = createSignal('sandboxes');
  const [checked, setChecked] = createSignal(false);
  const [place, setPlace] = createSignal(parsePath(window.location.pathname));
  // The Admin Center is addressed by its path; a menu click leaves the path behind.
  const adminId = () => (place().kind === 'admin' ? place().id : null);

  function openAdmin(id) {
    navigate(`/sandboxes/${id}`);
    setRoute('sandboxes');
  }

  function closeAdmin() {
    navigate('/');
  }

  // The back button must move the screen, otherwise the address bar and the
  // screen disagree.
  function onPop() {
    setPlace(parsePath(window.location.pathname));
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
      // working without a second sign-in. A pop-out window shares its opener's token
      // instead: asking for a new one would invalidate the token the main window holds.
      const popout = place().kind === 'terminal' || place().kind === 'desktop';
      if (!(popout && adoptOpenerCsrf())) {
        await session.refreshCsrf();
      }
      // A member's day starts with their own sandboxes, not with the operator's list.
      if (session.identity()?.role === 'member' && window.location.pathname === '/') {
        navigate('/my');
        setPlace(parsePath(window.location.pathname));
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
        {/* A popped-out terminal or desktop owns its window: no sidebar, no menu. */}
        <Show when={place().kind === 'terminal' || place().kind === 'desktop'}>
          <Show when={place().kind === 'terminal'} fallback={<DesktopPopout sandboxId={place().id} />}>
            <TerminalPopout sandboxId={place().id} tab={place().tab} />
          </Show>
        </Show>
        <Show when={place().kind !== 'terminal' && place().kind !== 'desktop'}>
        <div class="shell">
          <aside class="sidebar">
            <div class="brand">
              <strong>Nestbox</strong>
              <span>control</span>
            </div>

            <ContextBar />

            <nav class="nav">
              <For each={menuFor(session.identity()?.role)}>
                {(group) => (
                  <section class="nav-group">
                    <h2 class="nav-heading">{group.label}</h2>
                    <For each={group.routes}>
                      {(item) => (
                        <button
                          aria-current={
                            item.id === 'my' ? (place().kind === 'my' ? 'page' : undefined)
                              : route() === item.id && !place().kind ? 'page' : undefined
                          }
                          onClick={() => {
                            if (item.id === 'my') {
                              navigate('/my');
                              setPlace(parsePath('/my'));
                              return;
                            }
                            if (place().kind) {
                              navigate('/');
                              setPlace(parsePath('/'));
                            }
                            setRoute(item.id);
                          }}
                        >
                          {item.label}
                        </button>
                      )}
                    </For>
                  </section>
                )}
              </For>
            </nav>

            <footer>

              <p class="muted small">
                {session.identity()?.display_name} · {session.identity()?.role}
              </p>
              <button class="secondary" onClick={session.signOut}>Sign out</button>
            </footer>
          </aside>

          <main>
            {/* Keyed: choosing another data center remounts the page, so every screen reloads its data. */}
            <Show when={session.activeEnvironment()} keyed>
              {() => (
            <Show
              when={adminId()}
              fallback={
                <>
                  <Show when={place().kind === 'box'}>
                    <WorkboxPage sandboxId={place().id} />
                  </Show>
                  <Show when={place().kind === 'my'}>
                    <MyBoxesPage />
                  </Show>
                  <Show when={!place().kind}>
                  <Show when={route() === 'sandboxes'}>
                    <SandboxListPage
                      onOpenAdmin={openAdmin}
                      onOpenNode={() => setRoute('nodes')}
                    />
                  </Show>
                  <Show when={route() === 'traffic'}><TrafficPage /></Show>
                  <Show when={route() === 'policies'}><PoliciesPage /></Show>
                  <Show when={route() === 'cluster'}><ClusterPage /></Show>
                  <Show when={route() === 'system'}><SystemPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'nodes'}>
                    <NodesPage onOpenSandbox={openAdmin} role={session.identity()?.role} />
                  </Show>
                  <Show when={route() === 'providers'}><ProvidersPage /></Show>
                  <Show when={route() === 'proxies'}><ProxiesPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'models'}><ModelsPage /></Show>
                  <Show when={route() === 'skills'}><SkillsPage /></Show>
                  <Show when={route() === 'cudax'}><CudaXPage /></Show>
                  <Show when={route() === 'builder'}><BuilderPage /></Show>
                  <Show when={route() === 'recipes'}><RecipesPage /></Show>
                  <Show when={route() === 'persistent'}><PersistentPage /></Show>
                  <Show when={route() === 'operations'}><JobsPage /></Show>
                  <Show when={route() === 'users'}><UsersPage /></Show>
                  <Show when={route() === 'hosting'}><HostingPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'moves'}><MovesPage /></Show>
                  <Show when={route() === 'images'}><ImagesPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'tenants'}><TenantsPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'mcp'}><McpPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'media'}><MediaPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'privacy'}><PrivacyPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'master'}><MasterControlPage role={session.identity()?.role} /></Show>
                  <Show when={route() === 'live'}><LivePage /></Show>
                  <Show when={route() === 'usage'}><UsagePage /></Show>
                  <Show when={route() === 'logs'}><LogsPage role={session.identity()?.role} /></Show>
                  </Show>
                </>
              }
            >
              <AdminCenter sandboxId={adminId()} onClose={closeAdmin} onFollow={openAdmin} />
            </Show>
              )}
            </Show>
          </main>
        </div>
        </Show>
      </Show>
    </Show>
  );
}
