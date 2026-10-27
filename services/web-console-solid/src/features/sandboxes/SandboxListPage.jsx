// The sandbox inventory.
//
// Density is deliberate: an operator scanning for a problem wants many rows and
// few pixels. Detail lives in the drawer.

import { For, Show, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import Empty from '../../components/Empty.jsx';
import Drawer from '../../components/Drawer.jsx';
import { useSandboxes } from './useSandboxes.js';
import SandboxDetail from './SandboxDetail.jsx';
import CreateSandboxForm from './CreateSandboxForm.jsx';
import { createSignal } from 'solid-js';

export default function SandboxListPage(props) {
  const state = useSandboxes();
  const [openId, setOpenId] = createSignal(null);
  const [creating, setCreating] = createSignal(false);
  const [created, setCreated] = createSignal(null);

  onMount(() => state.load({ cursor: null }));

  function confirmDestroy(sandbox) {
    if (window.confirm(`Delete sandbox "${sandbox.name}"? Its container and its state are removed.`)) {
      state.destroy(sandbox.id);
    }
  }

  async function createdSandbox(sandbox) {
    setCreating(false);
    setCreated(
      `Sandbox “${sandbox.name}” created on ${sandbox.node_name || 'the local node'}.`,
    );
    await state.load({ cursor: null });
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Sandboxes</h1>
          <p class="muted">
            {state.total()} total · {state.items().length} shown
            {state.status() ? ` · filtered by ${state.status()}` : ''}
          </p>
        </div>
        <div class="controls">
          <button class="primary" onClick={() => { setCreated(null); setCreating(true); }}>
            New sandbox
          </button>
          <input
            type="search"
            placeholder="Search by name…"
            value={state.search()}
            onInput={(event) => state.updateSearch(event.currentTarget.value)}
          />
          <button class="secondary" onClick={() => state.load({ cursor: null })} disabled={state.loading()}>
            {state.loading() ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </header>

      <Show when={created()}>
        <p class="notice">{created()}</p>
      </Show>

      <Show when={state.error()}>
        <p class="error">{state.error()}</p>
      </Show>

      <div class="facets">
        <For each={Object.entries(state.facetsStatus())}>
          {([value, count]) => (
            <button
              class={state.status() === value ? 'chip active' : 'chip'}
              onClick={() => state.filterByStatus(value)}
            >
              {value} <b>{count}</b>
            </button>
          )}
        </For>
      </div>

      <Show when={state.selected().size > 0}>
        <div class="bulkbar">
          <span>{state.selected().size} selected</span>
          <button onClick={() => state.bulk('start')}>Start</button>
          <button onClick={() => state.bulk('stop')}>Stop</button>
          <button onClick={() => state.bulk('restart')}>Restart</button>
          <button class="ghost" onClick={() => state.toggleAll()}>Clear</button>
        </div>
      </Show>

      <Show
        when={state.items().length > 0}
        fallback={
          <Empty
            title="No sandboxes yet"
            hint="Create one here, choosing a node and its share of that node's memory, CPU, and PIDs."
          />
        }
      >
        <div class="table-scroll">
          <table>
            <thead>
              <tr>
                <th class="narrow">
                  <input
                    type="checkbox"
                    checked={state.selected().size === state.items().length && state.items().length > 0}
                    onChange={() => state.toggleAll()}
                    aria-label="Select all"
                  />
                </th>
                <th>Name</th>
                <th>Status</th>
                <th>Health</th>
                <th>Pack</th>
                <th>Address</th>
                <th class="narrow">Uptime</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={state.items()}>
                {(sandbox) => (
                  <tr class={openId() === sandbox.id ? 'selected' : ''}>
                    <td class="narrow">
                      <input
                        type="checkbox"
                        checked={state.selected().has(sandbox.id)}
                        onChange={() => state.toggle(sandbox.id)}
                        aria-label={`Select ${sandbox.name}`}
                      />
                    </td>
                    <td>
                      {/* The name opens the Admin Center, because that is
                          where the questions about a sandbox are answered. The
                          drawer stays one click away for a quick look. */}
                      <button
                        class="link"
                        onClick={() =>
                          props.onOpenAdmin ? props.onOpenAdmin(sandbox.id) : setOpenId(sandbox.id)
                        }
                      >
                        {sandbox.name}
                      </button>
                    </td>
                    <td><StatusBadge status={sandbox.status} /></td>
                    <td><StatusBadge status={sandbox.health} /></td>
                    <td class="mono small">{sandbox.pack_key}</td>
                    <td class="mono small">{sandbox.address || '—'}</td>
                    <td class="narrow mono small">{shortUptime(sandbox)}</td>
                    <td class="actions">
                      <button class="tiny" onClick={() => setOpenId(sandbox.id)}>Details</button>
                      <button class="tiny" disabled={!state.canStart(sandbox)}
                              onClick={() => state.act(sandbox.id, 'start')}>Start</button>
                      <button class="tiny" disabled={!state.canStop(sandbox)}
                              onClick={() => state.act(sandbox.id, 'stop')}>Stop</button>
                      <button class="tiny" disabled={!sandbox.status.includes('running')}
                              onClick={() => state.act(sandbox.id, 'restart')}>Restart</button>
                      <button class="tiny danger" onClick={() => confirmDestroy(sandbox)}>Delete</button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>

        <Show when={state.nextCursor()}>
          <div class="pager">
            <button class="secondary"
                    onClick={() => state.load({ cursor: state.nextCursor(), append: true })}
                    disabled={state.loading()}>
              Load more
            </button>
          </div>
        </Show>
      </Show>

      <SandboxDetail
        sandboxId={openId()}
        onClose={() => setOpenId(null)}
        onChanged={() => state.load({ cursor: null })}
      />

      {/* The form is mounted only while the drawer is open, so every open reads
          the node budgets again instead of showing the ones from last time. */}
      <Drawer
        open={creating()}
        title="New sandbox"
        subtitle="Its limits are checked against the node it lands on."
        onClose={() => setCreating(false)}
      >
        <Show when={creating()}>
          <CreateSandboxForm
            onCancel={() => setCreating(false)}
            onCreated={createdSandbox}
          />
        </Show>
      </Drawer>
    </section>
  );
}

function shortUptime(sandbox) {
  const seconds = sandbox.uptime_seconds;
  if (!seconds) return '—';
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}
