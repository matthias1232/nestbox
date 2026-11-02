// The findings allowed on purpose, and when they stop being allowed.
import { For, Show, createMemo, createSignal } from 'solid-js';
import * as privacy from '../../api/privacy.js';
import DataTable from '../../components/DataTable.jsx';
import Countdown from '../../components/Countdown.jsx';
import { usePolling } from '../../components/Poll.js';

export default function DlpExceptionsTable(props) {
  const [items, setItems] = createSignal([]);
  const [error, setError] = createSignal(null);
  const poll = usePolling(
    async () => {
      const answer = await privacy.exceptions(props.sandboxId);
      setItems(Array.isArray(answer) ? answer : answer.items || []);
      setError(null);
    },
    { everyMs: 30000, onError: (e) => setError(e.message) },
  );
  const rows = createMemo(() =>
    items().map((item) => ({
      id: item.id,
      detector: item.detector,
      scope: item.value_fingerprint ? 'ein Wert' : 'ganzer Detektor',
      where: item.sandbox_id ? 'diese Sandbox' : 'Umgebung',
      action: item.action,
      expires: item.expires_at,
      reason: item.reason,
    })),
  );

  async function remove(id) {
    try {
      await privacy.deleteException(id);
      poll.refresh();
    } catch (failure) {
      setError(failure.message || 'Die Ausnahme konnte nicht gelöscht werden.');
    }
  }

  return (
    <div>
      <Show when={error()}><p class="error">{error()}</p></Show>
      <DataTable
        rows={rows()}
        columns={[
          { key: 'detector', label: 'Detektor', sortable: true, filter: 'select' },
          { key: 'scope', label: 'Bereich', sortable: true, filter: 'select' },
          { key: 'where', label: 'Gilt für', sortable: true, groupable: true, filter: 'select' },
          { key: 'action', label: 'Aktion', sortable: true, filter: 'select' },
          {
            key: 'expires',
            label: 'Ablauf',
            sortable: true,
            render: (r) => (r.expires
              ? <Countdown until={r.expires} onExpire={poll.refresh} />
              : <span class="muted">nie</span>),
          },
          { key: 'reason', label: 'Grund', sortable: false },
          {
            key: 'remove',
            label: '',
            sortable: false,
            render: (r) => <button class="tiny secondary" onClick={() => remove(r.id)}>Löschen</button>,
          },
        ]}
        rowKey={(r) => r.id}
        defaultSort={{ key: 'expires', dir: 'asc' }}
        emptyText="Keine Ausnahme. Jede Anfrage wird nach der Richtlinie geprüft."
      />
    </div>
  );
}
