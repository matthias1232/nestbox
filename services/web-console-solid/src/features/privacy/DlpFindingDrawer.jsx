// One finding in full, and the way to allow it on purpose.
//
// The drawer is where a blocked request becomes a decision the user can revisit:
// what fired, what happened, where the request was going — and a button that says
// "allow this value here for an hour", so the next attempt goes through.
import { For, Show, createSignal } from 'solid-js';
import * as privacy from '../../api/privacy.js';
import DlpBadge from '../../components/DlpBadge.jsx';
import RouteChain from '../../components/RouteChain.jsx';

const DURATIONS = [
  { label: '15 min', minutes: 15 },
  { label: '1 h', minutes: 60 },
  { label: '24 h', minutes: 1440 },
  { label: '7 Tage', minutes: 10080 },
  { label: 'Immer', minutes: null },
];
const ACTIONS = [
  { value: 'allow', label: 'erlauben' },
  { value: 'replace', label: 'ersetzen statt blockieren' },
  { value: 'pseudonymize', label: 'pseudonymisieren' },
];

function field(label, value) {
  return (
    <Show when={value}>
      <div class="small"><span class="muted">{label}: </span><span class="mono">{value}</span></div>
    </Show>
  );
}

export default function DlpFindingDrawer(props) {
  const incident = () => props.incident?.incident || props.incident;
  const wrapper = () => props.incident || {};
  const [action, setAction] = createSignal('allow');
  const [minutes, setMinutes] = createSignal(60);
  const [sandboxOnly, setSandboxOnly] = createSignal(true);
  const [scope, setScope] = createSignal('value');
  const [reason, setReason] = createSignal('');
  const [message, setMessage] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  async function allow() {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await privacy.createException({
        incident_id: incident().id,
        scope: scope(),
        sandbox_only: sandboxOnly(),
        action: action(),
        minutes: minutes(),
        reason: reason(),
      });
      setMessage('Ausnahme angelegt. Sie gilt ab der nächsten Anfrage.');
      props.onChanged?.();
    } catch (failure) {
      setError(failure.message || 'Die Ausnahme konnte nicht angelegt werden.');
    } finally {
      setBusy(false);
    }
  }

  const route = () => {
    const item = wrapper();
    return [
      { kind: 'sandbox', label: item.sandbox_name || 'Sandbox', state: 'passed' },
      { kind: 'egress', label: 'Egress', detail: item.channel || '', state: 'passed' },
      {
        kind: 'dlp',
        label: `DLP: ${incident()?.action || ''}`,
        detail: incident()?.requested_action ? `gewünscht: ${incident().requested_action}` : '',
        state: incident()?.action === 'blocked' ? 'blocked' : 'passed',
      },
      {
        kind: 'upstream',
        label: item.upstream_ip ? `${incident()?.destination} (${item.upstream_ip})` : (incident()?.destination || ''),
        state: incident()?.action === 'blocked' ? 'would' : 'passed',
      },
      {
        kind: 'model',
        label: item.served_model || incident()?.model || '',
        state: incident()?.action === 'blocked' ? 'would' : 'passed',
      },
    ];
  };

  return (
    <Show when={props.open}>
      <div class="card" style="position:fixed;inset:0;z-index:40;overflow:auto;padding:1rem;background:color-mix(in srgb, currentColor 4%, var(--background))">
        <div class="controls">
          <DlpBadge state={incident()?.action} />
          <b class="mono">{incident()?.detector}</b>
          <span class="muted small">{String(incident()?.created_at || '').replace('T', ' ').slice(0, 19)}</span>
          <Show when={(incident()?.occurrences || 1) > 1}>
            <span class="muted small">×{incident().occurrences}</span>
          </Show>
          <button class="tiny secondary" onClick={props.onClose}>Schließen</button>
        </div>

        <div class="small">
          {field('Sandbox', wrapper().sandbox_name)}
          {field('Nutzer', wrapper().owner_name)}
          {field('Agent', wrapper().agent)}
          {field('Konto', wrapper().account_kind)}
          {field('Richtung', incident()?.direction)}
          {field('Quelle', incident()?.source)}
          {field('Anfrage', `${incident()?.method || ''} ${incident()?.path || ''}`)}
          {field('Anbieter', wrapper().provider)}
          {field('Modell', wrapper().served_model || incident()?.model)}
          {field('Ziel', incident()?.destination)}
          {field('IP', wrapper().upstream_ip || (wrapper().upstream_ip_would ? `hätte: ${wrapper().upstream_ip_would}` : ''))}
          {field('maskierter Wert', incident()?.masked_value)}
          {field('Ersatz', incident()?.replacement)}
          {field('Fingerabdruck', (incident()?.value_fingerprint || '').slice(0, 16))}
          {field('Ausnahme', incident()?.exception_id)}
        </div>

        <RouteChain route={route()} />

        <Show when={incident()?.value_fingerprint}>
          <div class="card">
            <b>Diese Anfrage erlauben</b>
            <div class="controls">
              <select value={scope()} onChange={(e) => setScope(e.currentTarget.value)}>
                <option value="value">nur diesen Wert</option>
                <option value="detector">jeden Wert dieses Detektors</option>
              </select>
              <select value={action()} onChange={(e) => setAction(e.currentTarget.value)}>
                <For each={ACTIONS}>{(entry) => <option value={entry.value}>{entry.label}</option>}</For>
              </select>
              <select value={String(minutes())} onChange={(e) => setMinutes(e.currentTarget.value === 'null' ? null : Number(e.currentTarget.value))}>
                <For each={DURATIONS}>{(entry) => <option value={String(entry.minutes)}>{entry.label}</option>}</For>
              </select>
              <label class="small">
                <input type="checkbox" checked={sandboxOnly()} onChange={(e) => setSandboxOnly(e.currentTarget.checked)} /> nur diese Sandbox
              </label>
              <input class="input" placeholder="Grund (optional)" value={reason()} onInput={(e) => setReason(e.currentTarget.value)} />
              <button class="primary tiny" disabled={busy()} onClick={allow}>Ausnahme anlegen</button>
            </div>
            <Show when={message()}><p class="notice small">{message()}</p></Show>
            <Show when={error()}><p class="error small">{error()}</p></Show>
          </div>
        </Show>
      </div>
    </Show>
  );
}
