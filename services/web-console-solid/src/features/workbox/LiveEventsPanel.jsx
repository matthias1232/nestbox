// Proxy, OpenShell and MCP events of this sandbox, live.
//
// One subscription to the telemetry stream, three lists over it: `liveEvents.js`
// decides which list a row belongs to and what it says. The panel only pours the
// rows in. Pause stops taking rows but keeps the subscription open, so the
// connection under it does not change while the reader looks at the list.
import { Show, createEffect, createMemo, createSignal, onCleanup, onMount } from 'solid-js';

import { stream } from '../../api/telemetry.js';
import DataTable from '../../components/DataTable.jsx';
import Segmented from '../../components/Segmented.jsx';
import { RING, SEGMENTS, isSandbox, pushRow, toRow } from './liveEvents.js';

const TONE = { allow: 'ok', audit: 'warn', deny: 'bad' };

function clock(ts) {
  if (!ts) return '';
  const at = new Date(ts);
  return Number.isNaN(at.getTime()) ? String(ts) : at.toLocaleTimeString();
}

export default function LiveEventsPanel(props) {
  const [category, setCategory] = createSignal('proxy');
  const [rows, setRows] = createSignal([]);
  const [paused, setPaused] = createSignal(false);
  const [connected, setConnected] = createSignal(false);

  const sandboxId = () => props.sandbox?.id;

  // A new sandbox is a new story; the rows of the previous one must not stay.
  let shownFor = null;
  createEffect(() => {
    const id = sandboxId();
    if (id !== shownFor) {
      shownFor = id;
      setRows([]);
    }
  });

  onMount(() => {
    const stop = stream(null, {
      onEvent: (event) => {
        if (paused() || !isSandbox(event, sandboxId())) return;
        const row = toRow(event);
        if (!row) return;
        setConnected(true);
        setRows((current) => pushRow(current, row, RING));
      },
      onError: () => setConnected(false),
    });
    onCleanup(stop);
  });

  const view = createMemo(() => rows().filter((row) => row.kind === category()));

  const columns = [
    { key: 'ts', label: 'Zeit', sortable: true, render: (row) => <span class="mono small">{clock(row.ts)}</span> },
    {
      key: 'target',
      label: 'Ziel',
      sortable: true,
      filter: 'select',
      render: (row) => (
        <span class="mono small">
          {row.target || '—'}
          <Show when={row.resolved_ip}>
            <span class="muted"> {row.resolved_ip} (aufgelöst)</span>
          </Show>
        </span>
      ),
    },
    {
      key: 'decision',
      label: 'Entscheidung',
      sortable: true,
      filter: 'select',
      render: (row) => (
        <Show when={row.decision} fallback={<span class="muted small">—</span>}>
          <span class={`badge ${TONE[row.decision] || 'muted'}`}>
            <i />
            {row.decision}
          </span>
        </Show>
      ),
    },
    { key: 'reason', label: 'Grund', sortable: true },
    { key: 'method', label: 'Methode', sortable: true },
    {
      key: 'tool',
      label: 'Werkzeug',
      sortable: true,
      filter: 'select',
      render: (row) => (row.tool ? <span class="mono small">{row.tool}</span> : ''),
    },
    {
      key: 'error',
      label: 'Fehler',
      sortable: true,
      render: (row) => <Show when={row.error}><span class="error small">{row.error}</span></Show>,
    },
  ];

  return (
    <section class="card">
      <div class="controls">
        <h3 style="margin:0">Was diese Sandbox gerade tut</h3>
        <Segmented options={SEGMENTS} value={category()} onChange={setCategory} />
        <button
          type="button"
          class={paused() ? 'secondary tiny' : 'primary tiny'}
          aria-pressed={paused()}
          onClick={() => setPaused(!paused())}
        >
          {paused() ? 'Weiter' : 'Pause'}
        </button>
        <span class="muted small">{view().length} Ereignisse</span>
        <Show when={!connected()}>
          <span class="badge warn" title="Der Ereignisstrom ist unterbrochen und verbindet sich neu.">
            <i />
            keine Verbindung
          </span>
        </Show>
      </div>

      <Show when={paused()}>
        <p class="muted small">
          Angehalten. Ereignisse, die jetzt eintreffen, werden nicht aufgenommen, damit die Liste
          stehen bleibt, während du liest.
        </p>
      </Show>

      <DataTable
        rows={view()}
        columns={columns}
        rowKey={(row) => row.id}
        storageKey={`workbox-events-${category()}`}
        live={!paused()}
        emptyText={
          category() === 'mcp'
            ? 'Keine MCP-Aufrufe in dieser Sandbox seit dem Öffnen der Seite.'
            : 'Keine Ereignisse in dieser Sandbox seit dem Öffnen der Seite.'
        }
      />
    </section>
  );
}
